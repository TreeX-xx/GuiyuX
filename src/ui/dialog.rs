use super::*;
use windows_sys::Win32::{
    Graphics::Dwm::*,
    UI::{
        Controls::WM_MOUSELEAVE,
        Input::KeyboardAndMouse::{TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent},
    },
};

thread_local! {
    pub(super) static HOT: Cell<HWND> = const { Cell::new(null_mut()) };
}

pub(super) fn position(hwnd: HWND, owner: HWND, width: i32, height: i32) -> io::Result<()> {
    unsafe {
        let mut frame = area(0, 0, width, height);
        let mut island = RECT::default();
        let mut monitor = MONITORINFO {
            cbSize: size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        let style = GetWindowLongW(hwnd, GWL_STYLE) as u32;
        let extended = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
        if AdjustWindowRectEx(&mut frame, style, 0, extended) == 0
            || GetWindowRect(owner, &mut island) == 0
            || GetMonitorInfoW(
                MonitorFromWindow(owner, MONITOR_DEFAULTTONEAREST),
                &mut monitor,
            ) == 0
        {
            return Err(io::Error::last_os_error());
        }
        let work = monitor.rcWork;
        let w = frame.right - frame.left;
        let h = frame.bottom - frame.top;
        let x = ((island.left + island.right - w) / 2)
            .min(work.right - w)
            .max(work.left);
        let y = (work.top + 50).min(work.bottom - h).max(work.top);
        if SetWindowPos(hwnd, HWND_TOP, x, y, w, h, SWP_NOACTIVATE) == 0 {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(())
}

pub(super) fn style_caption(hwnd: HWND) {
    let custom = !high_contrast();
    let color = |normal| if custom { normal } else { DWMWA_COLOR_DEFAULT };
    for (attr, value) in [
        (DWMWA_USE_IMMERSIVE_DARK_MODE, u32::from(custom)),
        (DWMWA_CAPTION_COLOR, color(BG)),
        (DWMWA_TEXT_COLOR, color(MUTED)),
        (DWMWA_BORDER_COLOR, color(EDGE)),
        (DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUNDSMALL as u32),
    ] {
        frame_attribute(hwnd, attr, value);
    }
}

pub(super) fn track_hover(hwnd: HWND, msg: u32) {
    match msg {
        WM_MOUSEMOVE if HOT.get() != hwnd => {
            let mut event = TRACKMOUSEEVENT {
                cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
                dwFlags: TME_LEAVE,
                hwndTrack: hwnd,
                ..Default::default()
            };
            if unsafe { TrackMouseEvent(&mut event) } != 0 {
                set_hot(hwnd);
            }
        }
        WM_MOUSELEAVE if HOT.get() == hwnd => set_hot(null_mut()),
        WM_NCDESTROY if HOT.get() == hwnd => HOT.set(null_mut()),
        _ => {}
    }
}

fn set_hot(hwnd: HWND) {
    let old = HOT.replace(hwnd);
    if old != hwnd {
        for row in [old, hwnd] {
            if !row.is_null() {
                unsafe { InvalidateRect(row, null_mut(), 0) };
            }
        }
    }
}
