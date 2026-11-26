mod dialog;
mod glass;
mod island;
mod menu;
mod paint;
use paint::*;
mod settings;

use std::{cell::Cell, io, ptr::null_mut};
use windows_sys::Win32::{
    Foundation::*, Graphics::Gdi::*, System::LibraryLoader::GetModuleHandleW,
    UI::WindowsAndMessaging::*,
};

thread_local! {
    pub(crate) static APPEARANCE_STYLE: Cell<crate::config::Appearance> = Cell::default();
    static ISLAND: Cell<HWND> = const { Cell::new(null_mut()) };
    static SETTINGS: Cell<HWND> = const { Cell::new(null_mut()) };
    static EXPANDED: Cell<bool> = const { Cell::new(false) };
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

pub fn report(error: &io::Error) {
    let msg = format!("{}\n{error}", crate::language().text().error);
    unsafe {
        MessageBoxW(
            null_mut(),
            wide(&msg).as_ptr(),
            wide("GuiyuX").as_ptr(),
            MB_ICONERROR,
        );
    }
}

fn window(
    class: &str,
    title: &str,
    style: u32,
    ex: u32,
    owner: HWND,
    id: usize,
) -> io::Result<HWND> {
    let hwnd = unsafe {
        CreateWindowExW(
            ex,
            wide(class).as_ptr(),
            wide(title).as_ptr(),
            style,
            0,
            0,
            0,
            0,
            owner,
            id as HMENU,
            GetModuleHandleW(null_mut()),
            null_mut(),
        )
    };
    if hwnd.is_null() {
        Err(io::Error::last_os_error())
    } else {
        Ok(hwnd)
    }
}

fn caption(hwnd: HWND, text: &str) {
    unsafe {
        SetWindowTextW(hwnd, wide(text).as_ptr());
    }
}

fn control(parent: HWND, class: &str, style: u32, id: i32, rect: [i32; 4]) -> io::Result<()> {
    let hwnd = window(class, "", WS_CHILD | style, 0, parent, id as usize)?;
    let [x, y, w, h] = rect;
    unsafe {
        MoveWindow(hwnd, x, y, w, h, 1);
        SendMessageW(
            hwnd,
            WM_SETFONT,
            GetStockObject(DEFAULT_GUI_FONT) as usize,
            1,
        );
    }
    Ok(())
}

fn register(class: &str, proc: WNDPROC) -> io::Result<()> {
    let name = wide(class);
    unsafe {
        let wc = WNDCLASSW {
            lpfnWndProc: proc,
            hInstance: GetModuleHandleW(null_mut()),
            hCursor: LoadCursorW(null_mut(), IDC_ARROW),
            hbrBackground: (COLOR_BTNFACE + 1) as _,
            lpszClassName: name.as_ptr(),
            ..Default::default()
        };
        if RegisterClassW(&wc) == 0 {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(())
}

pub fn run() -> io::Result<()> {
    register("GuiyuX.Island", Some(island::procedure))?;
    register("GuiyuX.Settings", Some(settings::procedure))?;
    island::open()?;
    unsafe {
        let mut msg = MSG::default();
        loop {
            match GetMessageW(&mut msg, null_mut(), 0, 0) {
                -1 => return Err(io::Error::last_os_error()),
                0 => return Ok(()),
                _ => {
                    let root = GetAncestor(msg.hwnd, GA_ROOT);
                    if root == ISLAND.get() && !root.is_null() {
                        island::input_mode(msg.message);
                        if menu::context_key(root, &msg) {
                            continue;
                        }
                    }
                    if !root.is_null() && IsDialogMessageW(root, &msg) != 0 {
                        continue;
                    }
                    TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }
        }
    }
}
