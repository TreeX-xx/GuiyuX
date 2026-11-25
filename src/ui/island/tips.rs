use super::*;
use windows_sys::Win32::UI::Controls::*;

// v2 TOOLINFO, accepted by both classic and themed common controls.
const TOOL_SIZE: u32 = std::mem::offset_of!(TTTOOLINFOW, lpReserved) as u32;

thread_local! { static TIP: Cell<HWND> = const {Cell::new(null_mut())}; }

pub(super) fn install(owner: HWND) -> io::Result<()> {
    unsafe {
        InitCommonControls();
        let tip = window(
            "tooltips_class32",
            "",
            WS_POPUP | TTS_ALWAYSTIP | TTS_NOPREFIX | TTS_NOANIMATE | TTS_NOFADE,
            WS_EX_TOPMOST | WS_EX_NOACTIVATE | WS_EX_LAYERED | WS_EX_TRANSPARENT,
            owner,
            0,
        )?;
        SendMessageW(tip, TTM_SETDELAYTIME, TTDT_INITIAL as usize, 400);
        for id in std::iter::once(HEADER).chain(FIRST..FIRST + SLOTS as i32) {
            let tool = TTTOOLINFOW {
                cbSize: TOOL_SIZE,
                uFlags: TTF_IDISHWND | TTF_SUBCLASS,
                hwnd: owner,
                uId: GetDlgItem(owner, id) as usize,
                lpszText: -1isize as *mut u16,
                ..Default::default()
            };
            if SendMessageW(tip, TTM_ADDTOOLW, 0, &tool as *const _ as isize) == 0 {
                return Err(io::Error::other("Could not attach hover tip"));
            }
        }
        TIP.set(tip);
    }
    Ok(())
}

fn hint(anchor: HWND) -> String {
    let id = unsafe { GetDlgCtrlID(anchor) };
    let t = crate::language().text();
    if !EXPANDED.get() {
        return String::new();
    }
    if id == HEADER {
        return if nav::toolbar() { t.collapse } else { t.back }.into();
    }
    nav::button_row(id).map_or_else(String::new, |r| r.title)
}

fn show(tip: HWND) -> bool {
    unsafe {
        let mut tool = TTTOOLINFOW {
            cbSize: TOOL_SIZE,
            ..Default::default()
        };
        if SendMessageW(tip, TTM_GETCURRENTTOOLW, 0, &mut tool as *mut _ as isize) == 0 {
            return false;
        }
        let anchor = tool.uId as HWND;
        let text = hint(anchor);
        if text.is_empty() {
            return false;
        }
        let dc = GetDC(anchor);
        let font = interface_font(&text, POPUP_TEXT_SIZE, 1);
        let old = SelectObject(dc, font);
        let mut size = RECT::default();
        DrawTextW(
            dc,
            wide(&text).as_ptr(),
            -1,
            &mut size,
            DT_CALCRECT | DT_SINGLELINE | DT_NOPREFIX,
        );
        SelectObject(dc, old);
        DeleteObject(font);
        ReleaseDC(anchor, dc);
        let mut button = RECT::default();
        GetWindowRect(anchor, &mut button);
        let w = size.right + 16;
        let h = size.bottom + 8;
        let x = (button.left + button.right - w) / 2;
        let y = button.bottom + 6;
        let bounds = RECT {
            left: x,
            top: y,
            right: x + w,
            bottom: y + h,
        };
        // TTN_SHOW supplies the visible native tooltip and its accessibility.
        // Commit its artwork and anchor position together before it is shown.
        surface::layer(tip, bounds, ROW_RADIUS, 255, |dc, rect| {
            fill(dc, &rect, POPUP_BG);
            interface_text(dc, rect, &text, POPUP_TEXT_SIZE, INK, DT_CENTER);
        })
        .is_ok()
    }
}

pub(super) fn notify(lp: LPARAM) -> Option<LRESULT> {
    unsafe {
        let hdr = &*(lp as *const NMHDR);
        if hdr.hwndFrom != TIP.get() {
            return None;
        }
        match hdr.code {
            TTN_GETDISPINFOW => {
                let text = hint(hdr.idFrom as HWND);
                let info = &mut *(lp as *mut NMTTDISPINFOW);
                info.szText.fill(0);
                for (slot, unit) in info.szText[..79].iter_mut().zip(text.encode_utf16()) {
                    *slot = unit;
                }
                info.lpszText = info.szText.as_mut_ptr();
                Some(0)
            }
            TTN_SHOW => Some(isize::from(show(hdr.hwndFrom))),
            _ => None,
        }
    }
}

pub(super) fn dismiss() {
    unsafe {
        SendMessageW(TIP.get(), TTM_POP, 0, 0);
    }
}
