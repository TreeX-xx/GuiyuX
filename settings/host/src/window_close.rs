// Windows can pick Tao's visible, zero-size event target as this process's
// main window while the WebView is starting. Let Tauri finish edits and close
// the real window before its event target goes away. See .agents/notes/module.md.
use std::{cell::Cell, ptr::null_mut};
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, WPARAM},
    System::Threading::GetCurrentThreadId,
    UI::{
        Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass},
        WindowsAndMessaging::{
            EnumThreadWindows, GetClassNameW, PostMessageW, WM_CLOSE, WM_NCDESTROY,
        },
    },
};

thread_local! {
    static SETTINGS: Cell<HWND> = const { Cell::new(null_mut()) };
    static CLOSE_PENDING: Cell<bool> = const { Cell::new(false) };
}

pub(super) fn install() {
    unsafe { EnumThreadWindows(GetCurrentThreadId(), Some(find_dispatcher), 0) };
}

unsafe extern "system" fn find_dispatcher(hwnd: HWND, _: LPARAM) -> i32 {
    let mut class = [0; 64];
    let len = unsafe { GetClassNameW(hwnd, class.as_mut_ptr(), class.len() as i32) } as usize;
    if class[..len]
        .iter()
        .copied()
        .eq("Tao Thread Event Target".encode_utf16())
    {
        unsafe { SetWindowSubclass(hwnd, Some(dispatcher), 1, 0) };
        return 0;
    }
    1
}

pub(super) fn presented(hwnd: HWND) {
    SETTINGS.set(hwnd);
    if CLOSE_PENDING.replace(false) {
        unsafe { PostMessageW(hwnd, WM_CLOSE, 0, 0) };
    }
}

unsafe extern "system" fn dispatcher(
    hwnd: HWND,
    msg: u32,
    wp: WPARAM,
    lp: LPARAM,
    id: usize,
    _: usize,
) -> LRESULT {
    if msg == WM_CLOSE {
        let settings = SETTINGS.get();
        if settings.is_null() {
            CLOSE_PENDING.set(true);
        } else {
            unsafe { PostMessageW(settings, WM_CLOSE, wp, lp) };
        }
        return 0;
    }
    if msg == WM_NCDESTROY {
        unsafe { RemoveWindowSubclass(hwnd, Some(dispatcher), id) };
    }
    unsafe { DefSubclassProc(hwnd, msg, wp, lp) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows_sys::Win32::UI::{
        Shell::GetWindowSubclass,
        WindowsAndMessaging::{
            CreateWindowExW, DestroyWindow, IsWindow, MSG, PM_REMOVE, PeekMessageW, SendMessageW,
            WS_POPUP,
        },
    };

    #[test]
    fn closing_the_event_target_before_or_after_present_reaches_the_settings_window() {
        // Build the installed Tao runtime so a change to its event target is detected.
        let _app = tauri::Builder::default()
            .any_thread()
            .build(tauri::generate_context!())
            .unwrap();
        install();
        unsafe extern "system" fn installed(hwnd: HWND, out: LPARAM) -> i32 {
            if unsafe { GetWindowSubclass(hwnd, Some(dispatcher), 1, &mut 0) } != 0 {
                unsafe { *(out as *mut HWND) = hwnd };
                return 0;
            }
            1
        }
        unsafe {
            let mut target: HWND = null_mut();
            EnumThreadWindows(
                GetCurrentThreadId(),
                Some(installed),
                (&mut target as *mut HWND) as LPARAM,
            );
            assert!(!target.is_null());
            SendMessageW(target, WM_CLOSE, 0, 0);
            assert_ne!(IsWindow(target), 0);
            let settings = CreateWindowExW(
                0,
                windows_sys::w!("STATIC"),
                std::ptr::null(),
                WS_POPUP,
                0,
                0,
                0,
                0,
                null_mut(),
                null_mut(),
                null_mut(),
                std::ptr::null(),
            );
            assert!(!settings.is_null());
            presented(settings);
            let take_close =
                || PeekMessageW(&mut MSG::default(), settings, WM_CLOSE, WM_CLOSE, PM_REMOVE);
            assert_ne!(take_close(), 0);
            SendMessageW(target, WM_CLOSE, 0, 0);
            assert_ne!(take_close(), 0);
            assert_ne!(IsWindow(target), 0);
            DestroyWindow(settings);
        }
    }
}
