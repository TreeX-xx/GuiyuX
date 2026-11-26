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

struct Frame {
    menu: HMENU,
    items: Rc<[Item]>,
    palette: &'static Palette,
    window: Cell<HWND>,
    added_layer: Cell<bool>,
    disabled: Cell<bool>,
    selected: Cell<u32>,
    backdrop: std::cell::RefCell<Option<glass::Backdrop>>,
    last: Cell<Option<(i32, i32, u32)>>,
}

thread_local! {
    static FRAMES: std::cell::RefCell<Vec<Rc<Frame>>> = const { std::cell::RefCell::new(Vec::new()) };
    static RESTORE_SHADOW: Cell<bool> = const { Cell::new(false) };
}

fn find(menu: HMENU) -> Option<Rc<Frame>> {
    FRAMES.with_borrow(|frames| frames.iter().find(|f| f.menu == menu).cloned())
}

fn register_popup(popup: &Popup, frames: &mut Vec<Rc<Frame>>) {
    frames.push(Rc::new(Frame {
        menu: popup.handle,
        items: popup.items.clone(),
        palette: popup.palette,
        window: Cell::default(),
        added_layer: Cell::default(),
        disabled: Cell::default(),
        selected: Cell::default(),
        backdrop: Default::default(),
        last: Cell::default(),
    }));
    for child in popup.items.iter().filter_map(|item| item.submenu.as_ref()) {
        register_popup(child, frames);
    }
}

pub(super) struct Watch {
    hook: HHOOK,
    start: usize,
}

impl Watch {
    pub(super) fn start(popup: &Popup) -> Self {
        let start = FRAMES.with_borrow_mut(|frames| {
            let start = frames.len();
            register_popup(popup, frames);
            start
        });
        // Scope the pre-show handler to this menu on our own UI thread.
        unsafe {
            Self {
                start,
                hook: SetWindowsHookExW(
                    WH_CALLWNDPROC,
                    Some(before_show),
                    null_mut(),
                    GetCurrentThreadId(),
                ),
            }
        }
    }
}

impl Drop for Watch {
    fn drop(&mut self) {
        if !self.hook.is_null() {
            unsafe { UnhookWindowsHookEx(self.hook) };
        }
        for frame in FRAMES.with_borrow_mut(|frames| frames.split_off(self.start)) {
            let hwnd = frame.window.take();
            if !hwnd.is_null() && unsafe { IsWindow(hwnd) } != 0 {
                restore(hwnd, &frame);
            }
        }
    }
}

unsafe extern "system" fn before_show(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        if code >= 0 {
            let m = &*(lp as *const CWPSTRUCT);
            if m.message == WM_WINDOWPOSCHANGING
                && GetClassLongW(m.hwnd, GCW_ATOM) == MENU_CLASS_ATOM
                && let Some(frame) = find(SendMessageW(m.hwnd, MN_GETHMENU, 0, 0) as HMENU)
                && frame.window.get().is_null()
                && !frame.disabled.get()
                && SetWindowSubclass(m.hwnd, Some(procedure), 1, Rc::as_ptr(&frame) as usize) != 0
            {
                frame.window.set(m.hwnd);
                frame.last.set(None);
                frame.backdrop.take();
                let style = GetWindowLongW(m.hwnd, GWL_EXSTYLE);
                frame.added_layer.set(style & WS_EX_LAYERED as i32 == 0);
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

fn restore(hwnd: HWND, frame: &Frame) {
    restore_shadow(hwnd);
    unsafe {
        RemoveWindowSubclass(hwnd, Some(procedure), 1);
        if frame.added_layer.replace(false) {
            let style = GetWindowLongW(hwnd, GWL_EXSTYLE);
            SetWindowLongW(hwnd, GWL_EXSTYLE, style & !(WS_EX_LAYERED as i32));
        }
    }
    frame_attribute(hwnd, DWMWA_BORDER_COLOR, DWMWA_COLOR_DEFAULT);
    frame_attribute(hwnd, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_DEFAULT as u32);
}

pub(super) fn select(menu: HMENU, wp: WPARAM) {
    if let Some(frame) = find(menu) {
        let value = (wp & 0xffff) as u32;
        let id = if (wp >> 16) as u32 & MF_POPUP != 0 {
            frame
                .items
                .get(value as usize)
                .map_or(0, |item| item.row.id)
        } else {
            value
        };
        if frame.selected.replace(id) != id {
            repaint(frame.window.get(), &frame);
        }
    }
}

fn artwork(w: i32, h: i32, selected: u32, items: &[Item], palette: &Palette) -> io::Result<Bitmap> {
    let mut bitmap = Bitmap::new(w, h)?;
    let dc = bitmap.dc;
    fill(dc, &area(0, 0, w, h), palette.border);
    rounded_fill(dc, area(1, 1, w - 1, h - 1), PANEL_RADIUS - 1, palette.face);
    let content_height: i32 = items.iter().map(|item| item.row.height()).sum();
    if content_height > h {
        // Let User32 draw its scroll arrows when a destination list exceeds the work area.
        return Err(io::ErrorKind::Unsupported.into());
    }
    let mut y = (h - content_height) / 2;
    for item in items {
        let top = INSET.max(y + 1);
        y += item.row.height();
        let bottom = (h - INSET).min(y - 1);
        draw_item(
            dc,
            area(INSET, top, w - INSET, bottom),
            item,
            selected == item.row.id,
        );
    }
    round_edges(bitmap.pixels(), w, h, PANEL_RADIUS);
    Ok(bitmap)
}

fn repaint(hwnd: HWND, frame: &Frame) {
    if hwnd.is_null() {
        return;
    }
    unsafe {
        let mut bounds = RECT::default();
        GetWindowRect(hwnd, &mut bounds);
        let (w, h, id) = (
            bounds.right - bounds.left,
            bounds.bottom - bounds.top,
            frame.selected.get(),
        );
        let previous = frame.last.get();
        if w <= 0 || h <= 0 || previous == Some((w, h, id)) {
            return;
        }
        frame.last.set(Some((w, h, id)));
        let style = APPEARANCE_STYLE.get();
        if previous.is_none() && style.frosted && style.transparency > 0 {
            frame.backdrop.replace(glass::Backdrop::capture(bounds));
        }
        if artwork(w, h, id, &frame.items, frame.palette)
            .and_then(|mut bmp| {
                glass::blend(
                    &mut bmp,
                    w,
                    h,
                    frame.palette.face,
                    // Glass keeps a denser tint for legibility. Plain transparency
                    // uses the selected percentage directly.
                    if style.frosted {
                        style.transparency / 2
                    } else {
                        style.transparency
                    },
                    frame.backdrop.borrow().as_ref(),
                );
                bmp.present(hwnd, None, SIZE { cx: w, cy: h })
            })
            .is_err()
        {
            // Restore ordinary native tracking and drawing if composition failed.
            frame.disabled.set(true);
            frame.window.set(null_mut());
            restore(hwnd, frame);
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
    data: usize,
) -> LRESULT {
    unsafe {
        let frame = &*(data as *const Frame);
        match msg {
            WM_NCPAINT | WM_ERASEBKGND => 1,
            WM_PAINT => {
                let mut ps = PAINTSTRUCT::default();
                BeginPaint(hwnd, &mut ps);
                EndPaint(hwnd, &ps);
                repaint(hwnd, frame);
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
                repaint(hwnd, frame);
                result
            }
            WM_NCDESTROY => {
                restore_shadow(hwnd);
                RemoveWindowSubclass(hwnd, Some(procedure), id);
                frame.selected.set(0);
                frame.window.set(null_mut());
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
    fn icon_menu_and_destinations_share_hover_artwork_and_track_selections_independently() {
        let destination = Row::new(100, "Folder", "\u{e8b7}");
        let root = Popup::new(
            vec![
                Row::new(1, "Pin", "\u{e718}"),
                Row::submenu(4, "Move", "\u{e8de}", vec![destination]),
                Row::separator(),
                Row::new(3, "Settings", "\u{e713}"),
            ],
            Some(&palette::LIGHT),
        )
        .unwrap();
        let _watch = Watch::start(&root);
        let child = unsafe { GetSubMenu(root.handle, 1) };
        assert_eq!(unsafe { GetMenuItemID(child, 0) }, 100);
        select(root.handle, ((MF_POPUP as usize) << 16) | 1);
        select(child, 100);
        for (handle, id, height, y) in [(root.handle, 4, 110, 44), (child, 100, 38, 12)] {
            let frame = find(handle).unwrap();
            assert_eq!(frame.selected.get(), id);
            let mut idle = artwork(180, height, 0, &frame.items, frame.palette).unwrap();
            let mut hovered = artwork(180, height, id, &frame.items, frame.palette).unwrap();
            assert_ne!(
                idle.pixels()[y * 180 + 130],
                hovered.pixels()[y * 180 + 130]
            );
            assert_eq!(hovered.pixels()[0], 0);
        }
    }

    #[test]
    fn menu_show_restores_the_original_shadow_style() {
        let class = "GuiyuX.Test.MenuShadow";
        register(class, Some(DefWindowProcW)).unwrap();
        let hwnd = window(class, "", WS_POPUP, 0, null_mut(), 0).unwrap();
        let popup = Popup::new(main_rows(), Some(&palette::DARK)).unwrap();
        let mut frames = Vec::new();
        register_popup(&popup, &mut frames);
        unsafe {
            assert_ne!(
                SetWindowSubclass(hwnd, Some(procedure), 1, Rc::as_ptr(&frames[0]) as usize),
                0
            );
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
        for (w, h, theme) in [(212, 110, &palette::LIGHT), (215, 114, &palette::DARK)] {
            let popup = Popup::new(main_rows(), Some(theme)).unwrap();
            let mut idle = artwork(w, h, 0, &popup.items, theme).unwrap();
            let pixels = idle.pixels();
            let alpha = |x: i32, y: i32| pixels[(y * w + x) as usize] >> 24;
            assert_eq!(alpha(0, 0), 0);
            assert_eq!(alpha(PANEL_RADIUS, 0), 255);
            let rgba = |c: u32| 0xff000000 | (c & 255) << 16 | (c & 0xff00) | c >> 16;
            assert_eq!(pixels[(16 * w + w - 10) as usize], rgba(theme.face));
            assert!((0..PANEL_RADIUS).any(|x| (1..255).contains(&alpha(x, 2))));
            for y in 0..PANEL_RADIUS {
                for x in 0..PANEL_RADIUS {
                    assert_eq!(alpha(x, y), alpha(w - 1 - x, y));
                    assert_eq!(alpha(x, y), alpha(x, h - 1 - y));
                    assert_eq!(alpha(x, y), alpha(w - 1 - x, h - 1 - y));
                }
            }
            for item in popup.items.iter() {
                let mut hovered = artwork(w, h, item.row.id, &popup.items, theme).unwrap();
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
