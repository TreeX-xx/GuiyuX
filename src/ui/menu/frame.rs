use super::*;
use windows_sys::Win32::{
    Graphics::Dwm::{
        DWMWA_BORDER_COLOR, DWMWA_COLOR_DEFAULT, DWMWA_COLOR_NONE, DWMWA_WINDOW_CORNER_PREFERENCE,
        DWMWCP_DEFAULT, DWMWCP_DONOTROUND,
    },
    System::Threading::GetCurrentThreadId,
    UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass},
};

const MENU_CLASS_ATOM: u32 = 0x8000; // Win32 popup menu class #32768.

thread_local! {
    static MENU: Cell<HMENU> = const { Cell::new(null_mut()) };
    static WINDOW: Cell<HWND> = const { Cell::new(null_mut()) };
    static ADDED_LAYER: Cell<bool> = const { Cell::new(false) };
    static RESTORE_SHADOW: Cell<bool> = const { Cell::new(false) };
    static SELECTED: Cell<u32> = const { Cell::new(0) };
    static LAST: Cell<Option<(i32, i32, u32)>> = const { Cell::new(None) };
}

pub(super) struct Watch(HHOOK);

impl Watch {
    pub(super) fn start(menu: HMENU) -> Self {
        MENU.set(menu);
        SELECTED.set(0);
        LAST.set(None);
        // Scope the pre-show handler to this menu on our own UI thread.
        unsafe {
            Self(SetWindowsHookExW(
                WH_CALLWNDPROC,
                Some(before_show),
                null_mut(),
                GetCurrentThreadId(),
            ))
        }
    }
}

impl Drop for Watch {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { UnhookWindowsHookEx(self.0) };
        }
        MENU.set(null_mut());
        let hwnd = WINDOW.replace(null_mut());
        if !hwnd.is_null() && unsafe { IsWindow(hwnd) } != 0 {
            restore(hwnd);
        }
        LAST.set(None);
    }
}

unsafe extern "system" fn before_show(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        if code >= 0 && !MENU.get().is_null() && WINDOW.get().is_null() {
            let m = &*(lp as *const CWPSTRUCT);
            if m.message == WM_WINDOWPOSCHANGING
                && GetClassLongW(m.hwnd, GCW_ATOM) == MENU_CLASS_ATOM
                && SendMessageW(m.hwnd, MN_GETHMENU, 0, 0) as HMENU == MENU.get()
                && SetWindowSubclass(m.hwnd, Some(procedure), 1, 0) != 0
            {
                WINDOW.set(m.hwnd);
                let style = GetWindowLongW(m.hwnd, GWL_EXSTYLE);
                ADDED_LAYER.set(style & WS_EX_LAYERED as i32 == 0);
                SetWindowLongW(m.hwnd, GWL_EXSTYLE, style | WS_EX_LAYERED as i32);
                frame_attribute(m.hwnd, DWMWA_BORDER_COLOR, DWMWA_COLOR_NONE);
                frame_attribute(
                    m.hwnd,
                    DWMWA_WINDOW_CORNER_PREFERENCE,
                    DWMWCP_DONOTROUND as u32,
                );
            }
        }
        CallNextHookEx(null_mut(), code, wp, lp)
    }
}

fn restore_shadow(hwnd: HWND) {
    if RESTORE_SHADOW.take() {
        unsafe {
            let style = GetClassLongW(hwnd, GCL_STYLE);
            SetClassLongW(hwnd, GCL_STYLE, (style | CS_DROPSHADOW) as i32);
        }
    }
}

fn restore(hwnd: HWND) {
    restore_shadow(hwnd);
    unsafe {
        RemoveWindowSubclass(hwnd, Some(procedure), 1);
        if ADDED_LAYER.replace(false) {
            let style = GetWindowLongW(hwnd, GWL_EXSTYLE);
            SetWindowLongW(hwnd, GWL_EXSTYLE, style & !(WS_EX_LAYERED as i32));
        }
    }
    frame_attribute(hwnd, DWMWA_BORDER_COLOR, DWMWA_COLOR_DEFAULT);
    frame_attribute(hwnd, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_DEFAULT as u32);
}

pub(super) fn select(menu: HMENU, id: u32) {
    if !menu.is_null() && menu == MENU.get() && SELECTED.replace(id) != id {
        repaint(WINDOW.get());
    }
}

fn artwork(w: i32, h: i32, selected: u32) -> io::Result<Bitmap> {
    let mut bitmap = Bitmap::new(w, h)?;
    let dc = bitmap.dc;
    fill(dc, &area(0, 0, w, h), BORDER);
    rounded_fill(dc, area(1, 1, w - 1, h - 1), PANEL_RADIUS - 1, POPUP_BG);
    let rows = COMMANDS.len() as i32;
    for (index, id) in COMMANDS.into_iter().enumerate() {
        let row = index as i32;
        let top = INSET.max(h * row / rows + 1);
        let bottom = (h - INSET).min(h * (row + 1) / rows - 1);
        draw_item(dc, area(INSET, top, w - INSET, bottom), id, selected == id);
    }
    round_edges(bitmap.pixels(), w, h, PANEL_RADIUS);
    Ok(bitmap)
}

fn repaint(hwnd: HWND) {
    if hwnd.is_null() {
        return;
    }
    unsafe {
        let mut bounds = RECT::default();
        GetWindowRect(hwnd, &mut bounds);
        let (w, h, id) = (
            bounds.right - bounds.left,
            bounds.bottom - bounds.top,
            SELECTED.get(),
        );
        if w <= 0 || h <= 0 || LAST.replace(Some((w, h, id))) == Some((w, h, id)) {
            return;
        }
        if artwork(w, h, id)
            .and_then(|bmp| bmp.present(hwnd, None, SIZE { cx: w, cy: h }))
            .is_err()
        {
            // Restore ordinary native tracking and drawing if composition failed.
            MENU.set(null_mut());
            WINDOW.set(null_mut());
            restore(hwnd);
            RedrawWindow(hwnd, null_mut(), null_mut(), RDW_INVALIDATE | RDW_FRAME);
        }
    }
}

unsafe extern "system" fn procedure(
    hwnd: HWND,
    msg: u32,
    wp: WPARAM,
    lp: LPARAM,
    id: usize,
    _: usize,
) -> LRESULT {
    unsafe {
        match msg {
            WM_NCPAINT | WM_ERASEBKGND => 1,
            WM_PAINT => {
                let mut ps = PAINTSTRUCT::default();
                BeginPaint(hwnd, &mut ps);
                EndPaint(hwnd, &ps);
                repaint(hwnd);
                0
            }
            WM_WINDOWPOSCHANGING if (*(lp as *const WINDOWPOS)).flags & SWP_SHOWWINDOW != 0 => {
                // User32 creates a rectangular SysShadow when it shows the menu.
                // Suppress it for this show, then restore the process-local class style.
                let style = GetClassLongW(hwnd, GCL_STYLE);
                if style & CS_DROPSHADOW != 0
                    && SetClassLongW(hwnd, GCL_STYLE, (style & !CS_DROPSHADOW) as i32) != 0
                {
                    RESTORE_SHADOW.set(true);
                }
                DefSubclassProc(hwnd, msg, wp, lp)
            }
            WM_WINDOWPOSCHANGED => {
                let result = DefSubclassProc(hwnd, msg, wp, lp);
                if (*(lp as *const WINDOWPOS)).flags & SWP_SHOWWINDOW != 0 {
                    restore_shadow(hwnd);
                }
                repaint(hwnd);
                result
            }
            WM_NCDESTROY => {
                restore_shadow(hwnd);
                RemoveWindowSubclass(hwnd, Some(procedure), id);
                WINDOW.set(null_mut());
                DefSubclassProc(hwnd, msg, wp, lp)
            }
            _ => DefSubclassProc(hwnd, msg, wp, lp),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_show_restores_the_original_shadow_style() {
        let class = "GuiyuX.Test.MenuShadow";
        register(class, Some(DefWindowProcW)).unwrap();
        let hwnd = window(class, "", WS_POPUP, 0, null_mut(), 0).unwrap();
        unsafe {
            assert_ne!(SetWindowSubclass(hwnd, Some(procedure), 1, 0), 0);
            let mut pos = WINDOWPOS {
                hwnd,
                flags: SWP_SHOWWINDOW | SWP_NOMOVE | SWP_NOSIZE,
                ..Default::default()
            };
            let lp = &mut pos as *mut WINDOWPOS as LPARAM;
            for style in [CS_DBLCLKS | CS_DROPSHADOW, CS_DBLCLKS] {
                SetClassLongW(hwnd, GCL_STYLE, style as i32);
                SendMessageW(hwnd, WM_WINDOWPOSCHANGING, 0, lp);
                assert_eq!(GetClassLongW(hwnd, GCL_STYLE), CS_DBLCLKS);
                SendMessageW(hwnd, WM_WINDOWPOSCHANGED, 0, lp);
                assert_eq!(GetClassLongW(hwnd, GCL_STYLE), style);
            }
            DestroyWindow(hwnd);
            UnregisterClassW(wide(class).as_ptr(), GetModuleHandleW(null_mut()));
        }
    }

    #[test]
    fn menu_outline_stays_transparent_and_symmetric_as_selection_moves() {
        for (w, h) in [(152, 32 * COMMANDS.len() as i32 + 6), (211, 106)] {
            let mut idle = artwork(w, h, 0).unwrap();
            let pixels = idle.pixels();
            let alpha = |x: i32, y: i32| pixels[(y * w + x) as usize] >> 24;
            assert_eq!(alpha(0, 0), 0);
            assert_eq!(alpha(PANEL_RADIUS, 0), 255);
            assert!((0..PANEL_RADIUS).any(|x| (1..255).contains(&alpha(x, 2))));
            for y in 0..PANEL_RADIUS {
                for x in 0..PANEL_RADIUS {
                    assert_eq!(alpha(x, y), alpha(w - 1 - x, y));
                    assert_eq!(alpha(x, y), alpha(x, h - 1 - y));
                    assert_eq!(alpha(x, y), alpha(w - 1 - x, h - 1 - y));
                }
            }
            for id in COMMANDS {
                let mut hovered = artwork(w, h, id).unwrap();
                assert!(
                    pixels
                        .iter()
                        .zip(hovered.pixels())
                        .all(|(a, b)| a >> 24 == *b >> 24)
                );
            }
        }
    }
}
