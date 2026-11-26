use super::*;
use crate::config::Appearance;
use windows_sys::Win32::UI::{
    Controls::{
        CDDS_PREPAINT, CDRF_DODEFAULT, CDRF_SKIPDEFAULT, EM_SETSEL, TB_THUMBTRACK,
        TBM_GETCHANNELRECT, TBM_GETTHUMBRECT, TBM_SETPOS,
    },
    Input::KeyboardAndMouse::{GetFocus, VK_RETURN},
    Shell::{DefSubclassProc, RemoveWindowSubclass},
};

const TBM_GETPOS: u32 = WM_USER;

pub(super) fn apply(style: Appearance, persist: bool) -> io::Result<()> {
    if persist && DATA.with_borrow(|s| s.appearance != style) {
        if let Err(error) = collection::update(|s| s.appearance = style) {
            let saved = DATA.with_borrow(|s| s.appearance);
            APPEARANCE_STYLE.set(saved);
            island::appearance_changed();
            let hwnd = SETTINGS.get();
            if !hwnd.is_null() {
                send(hwnd, MATERIAL, CB_SETCURSEL, usize::from(!saved.frosted), 0);
                display(hwnd, saved.transparency);
            }
            return Err(error);
        }
        DATA.with_borrow_mut(|s| s.appearance = style);
    }
    if APPEARANCE_STYLE.replace(style) != style {
        island::appearance_changed();
    }
    Ok(())
}

fn position_slider(hwnd: HWND, percent: u8) {
    let slider = child(hwnd, SLIDER);
    if slider.is_null() {
        return;
    }
    unsafe {
        if SendMessageW(slider, TBM_GETPOS, 0, 0) != percent as isize {
            SendMessageW(slider, TBM_SETPOS, 1, percent as isize);
        }
        // ComCtl32 invalidates its native thumb. Our focus ring and filled track
        // extend beyond that rectangle, including when the mouse is captured.
        RedrawWindow(
            slider,
            null_mut(),
            null_mut(),
            RDW_INVALIDATE | RDW_UPDATENOW,
        );
    }
}

pub(super) fn display(hwnd: HWND, percent: u8) {
    position_slider(hwnd, percent);
    let value = percent.to_string();
    if field(hwnd, TRANSPARENCY) != value {
        caption(child(hwnd, TRANSPARENCY), &value);
    }
}

pub(super) fn scroll(hwnd: HWND, event: u32) -> io::Result<()> {
    let mut style = APPEARANCE_STYLE.get();
    style.transparency = send(hwnd, SLIDER, TBM_GETPOS, 0, 0).clamp(0, 100) as u8;
    apply(style, event != TB_THUMBTRACK)?;
    display(hwnd, style.transparency);
    Ok(())
}

pub(super) fn edit(hwnd: HWND, event: u32) -> io::Result<()> {
    let mut style = APPEARANCE_STYLE.get();
    let value = field(hwnd, TRANSPARENCY).trim().parse::<u16>().ok();
    if event == EN_CHANGE {
        // Empty or incomplete input can remain in the editor until focus leaves.
        if let Some(value @ 0..=100) = value
            && value as u8 != style.transparency
        {
            style.transparency = value as u8;
            apply(style, true)?;
            position_slider(hwnd, value as u8);
        }
    } else {
        if event == EN_KILLFOCUS {
            if let Some(value) = value {
                style.transparency = value.min(100) as u8;
            }
            apply(style, true)?;
            display(hwnd, style.transparency);
        }
        unsafe {
            InvalidateRect(hwnd, &area(X + 392, 209, X + W, 249), 0);
        }
    }
    Ok(())
}

pub(super) unsafe extern "system" fn edit_proc(
    hwnd: HWND,
    msg: u32,
    wp: WPARAM,
    lp: LPARAM,
    id: usize,
    _: usize,
) -> LRESULT {
    unsafe {
        match msg {
            WM_GETDLGCODE if lp != 0 && (*(lp as *const MSG)).wParam == VK_RETURN as usize => {
                return DefSubclassProc(hwnd, msg, wp, lp) | DLGC_WANTMESSAGE as isize;
            }
            WM_KEYDOWN if wp == VK_RETURN as usize => {
                if let Err(error) = edit(GetParent(hwnd), EN_KILLFOCUS) {
                    report(&error);
                }
                SendMessageW(hwnd, EM_SETSEL, 0, -1);
                return 0;
            }
            WM_CHAR if wp == VK_RETURN as usize => return 0,
            WM_NCDESTROY => {
                RemoveWindowSubclass(hwnd, Some(edit_proc), id);
            }
            _ => {}
        }
        DefSubclassProc(hwnd, msg, wp, lp)
    }
}

pub(super) fn draw_slider(d: &NMCUSTOMDRAW) -> LRESULT {
    if d.dwDrawStage != CDDS_PREPAINT {
        return CDRF_DODEFAULT as isize;
    }
    let hwnd = d.hdr.hwndFrom;
    let mut channel = RECT::default();
    let mut thumb = RECT::default();
    let mut bounds = RECT::default();
    unsafe {
        GetClientRect(hwnd, &mut bounds);
        SendMessageW(
            hwnd,
            TBM_GETCHANNELRECT,
            0,
            (&mut channel as *mut RECT) as isize,
        );
        SendMessageW(
            hwnd,
            TBM_GETTHUMBRECT,
            0,
            (&mut thumb as *mut RECT) as isize,
        );
    }
    fill(d.hdc, &bounds, BG);
    let y = bounds.bottom / 2;
    let x = (thumb.left + thumb.right) / 2;
    let half = (thumb.right - thumb.left) / 2;
    let start = channel.left + half;
    let end = channel.right - half;
    rounded_fill(d.hdc, area(start, y - 2, end, y + 2), 2, EDGE);
    if x > start {
        rounded_fill(d.hdc, area(start, y - 2, x, y + 2), 2, SILVER);
    }
    if unsafe { GetFocus() } == hwnd {
        rounded_fill(d.hdc, area(x - 10, y - 10, x + 10, y + 10), 10, SILVER);
        rounded_fill(d.hdc, area(x - 9, y - 9, x + 9, y + 9), 9, BG);
    }
    rounded_fill(d.hdc, area(x - 7, y - 7, x + 7, y + 7), 7, INK);
    CDRF_SKIPDEFAULT as isize
}

#[cfg(test)]
mod tests {
    use super::*;

    unsafe extern "system" fn observe_paint(
        hwnd: HWND,
        msg: u32,
        wp: WPARAM,
        lp: LPARAM,
        _: usize,
        data: usize,
    ) -> LRESULT {
        unsafe {
            if msg == WM_PAINT {
                GetUpdateRect(hwnd, data as *mut RECT, 0);
            }
            DefSubclassProc(hwnd, msg, wp, lp)
        }
    }

    #[test]
    fn thumb_tracking_repaints_the_whole_slider_at_its_current_position() {
        unsafe {
            windows_sys::Win32::UI::Controls::InitCommonControls();
            let host = window("STATIC", "", WS_POPUP, WS_EX_TOOLWINDOW, null_mut(), 0).unwrap();
            let slider = window(
                "msctls_trackbar32",
                "",
                WS_CHILD | WS_VISIBLE | TBS_NOTICKS,
                0,
                host,
                SLIDER as usize,
            )
            .unwrap();
            SetWindowPos(
                host,
                null_mut(),
                -20000,
                -20000,
                382,
                34,
                SWP_NOACTIVATE | SWP_NOZORDER | SWP_SHOWWINDOW,
            );
            MoveWindow(slider, 0, 0, 382, 34, 1);
            SendMessageW(slider, TBM_SETPOS, 1, 45);
            UpdateWindow(slider);
            let mut painted = RECT::default();
            SetWindowSubclass(
                slider,
                Some(observe_paint),
                1,
                (&mut painted as *mut RECT) as usize,
            );
            // A native drag notification arrives after the thumb has already moved.
            InvalidateRect(slider, &area(168, 2, 179, 26), 0);
            position_slider(host, 45);
            RemoveWindowSubclass(slider, Some(observe_paint), 1);
            DestroyWindow(host);
            assert_eq!(
                (painted.left, painted.top, painted.right, painted.bottom),
                (0, 0, 382, 34)
            );
        }
    }
}
