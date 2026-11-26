use super::*;
use windows_sys::Win32::UI::{
    Controls::{COMBOBOXINFO, GetComboBoxInfo, ODS_COMBOBOXEDIT, ODS_SELECTED, SetWindowTheme},
    Shell::{DefSubclassProc, RemoveWindowSubclass},
};

pub(super) const RADIUS: i32 = 6;

thread_local! {
    // Native list tracking can change the hot row before the user commits it.
    static OPEN: Cell<Option<(HWND, isize)>> = const { Cell::new(None) };
}

pub(super) fn install(combo: HWND) -> io::Result<()> {
    let mut info = COMBOBOXINFO {
        cbSize: size_of::<COMBOBOXINFO>() as u32,
        ..Default::default()
    };
    unsafe {
        if GetComboBoxInfo(combo, &mut info) == 0
            || SetWindowSubclass(combo, Some(appearance::combo_proc), 1, 0) == 0
            || SetWindowSubclass(info.hwndList, Some(list_proc), 1, 0) == 0
        {
            return Err(io::Error::last_os_error());
        }
        if !high_contrast() {
            SetWindowTheme(
                info.hwndList,
                wide("DarkMode_Explorer").as_ptr(),
                null_mut(),
            );
        }
        SendMessageW(combo, CB_SETITEMHEIGHT, usize::MAX, 30);
    }
    Ok(())
}

pub(super) fn notify(combo: HWND, opening: bool) {
    OPEN.set(opening.then(|| (combo, unsafe { SendMessageW(combo, CB_GETCURSEL, 0, 0) })));
    unsafe {
        InvalidateRect(combo, null_mut(), 0);
    }
}

pub(super) fn initial(combo: HWND) -> Option<isize> {
    OPEN.get()
        .filter(|(owner, _)| *owner == combo)
        .map(|(_, selected)| selected)
}

pub(super) fn row(d: &DRAWITEMSTRUCT) {
    let hot = d.itemState & ODS_SELECTED != 0;
    let field = d.itemState & ODS_COMBOBOXEDIT != 0;
    let hc = high_contrast();
    let (face, ink) = if hc {
        unsafe {
            (
                GetSysColor(if hot { COLOR_HIGHLIGHT } else { COLOR_WINDOW }),
                GetSysColor(if hot {
                    COLOR_HIGHLIGHTTEXT
                } else {
                    COLOR_WINDOWTEXT
                }),
            )
        }
    } else {
        (ROW_BG, INK)
    };
    let r = d.rcItem;
    fill(d.hDC, &r, face);
    if d.itemID == u32::MAX {
        return;
    }
    if !hc && hot && !field {
        rounded_fill(
            d.hDC,
            area(r.left + 4, r.top + 2, r.right - 4, r.bottom - 2),
            ROW_RADIUS,
            HOVER_BG,
        );
    }
    let index = d.itemID as usize;
    let combo = d.hwndItem;
    let len = unsafe { SendMessageW(combo, CB_GETLBTEXTLEN, index, 0) };
    if len < 0 {
        return;
    }
    let mut buf = vec![0; len as usize + 1];
    let n = unsafe { SendMessageW(combo, CB_GETLBTEXT, index, buf.as_mut_ptr() as isize) };
    if n < 0 {
        return;
    }
    let current =
        initial(combo).unwrap_or_else(|| unsafe { SendMessageW(combo, CB_GETCURSEL, 0, 0) });
    if !field && current == index as isize {
        glyph(
            d.hDC,
            area(r.left + 7, r.top, r.left + 28, r.bottom),
            "\u{e73e}",
            12,
            ink,
        );
    }
    interface_text(
        d.hDC,
        area(
            r.left + if field { 10 } else { 32 },
            r.top,
            r.right - 8,
            r.bottom,
        ),
        &String::from_utf16_lossy(&buf[..n as usize]),
        13,
        ink,
        DT_LEFT,
    );
}

fn bounds(hwnd: HWND) -> RECT {
    let mut r = RECT::default();
    unsafe { GetWindowRect(hwnd, &mut r) };
    area(0, 0, r.right - r.left, r.bottom - r.top)
}

fn outline(r: RECT) -> HRGN {
    unsafe { CreateRoundRectRgn(0, 0, r.right + 1, r.bottom + 1, 2 * RADIUS, 2 * RADIUS) }
}

fn paint_frame(dc: HDC, r: RECT) {
    unsafe {
        let shape = outline(r);
        if shape.is_null() {
            return;
        }
        let state = SaveDC(dc);
        if state != 0 {
            // Both corner arcs fit in the side gutters. Leave the native
            // viewport and scrollbar intact, including on a scroll repaint.
            ExcludeClipRect(dc, RADIUS, 1, r.right - RADIUS, r.bottom - 1);
            let brush = GetStockObject(DC_BRUSH);
            SetDCBrushColor(dc, ROW_BG);
            FillRgn(dc, shape, brush);
            SetDCBrushColor(dc, EDGE);
            FrameRgn(dc, shape, brush, 1, 1);
            RestoreDC(dc, state);
        }
        DeleteObject(shape);
    }
}

unsafe extern "system" fn list_proc(
    hwnd: HWND,
    msg: u32,
    wp: WPARAM,
    lp: LPARAM,
    id: usize,
    _: usize,
) -> LRESULT {
    unsafe {
        if msg == WM_NCDESTROY {
            RemoveWindowSubclass(hwnd, Some(list_proc), id);
        }
        let result = DefSubclassProc(hwnd, msg, wp, lp);
        match msg {
            WM_NCCALCSIZE if !high_contrast() => {
                // Both forms of WM_NCCALCSIZE start with the new client RECT.
                let client = &mut *(lp as *mut RECT);
                if client.right - client.left > 2 * (RADIUS - 1) {
                    client.left += RADIUS - 1;
                    client.right -= RADIUS - 1;
                }
            }
            WM_SIZE | WM_THEMECHANGED => {
                if high_contrast() {
                    SetWindowRgn(hwnd, null_mut(), 1);
                } else {
                    let region = outline(bounds(hwnd));
                    if !region.is_null() && SetWindowRgn(hwnd, region, 1) == 0 {
                        DeleteObject(region);
                    }
                }
            }
            WM_NCPAINT | WM_PAINT if !high_contrast() => {
                let dc = GetWindowDC(hwnd);
                if !dc.is_null() {
                    paint_frame(dc, bounds(hwnd));
                    ReleaseDC(hwnd, dc);
                }
            }
            WM_PRINT if lp as i32 & PRF_NONCLIENT != 0 && !high_contrast() => {
                paint_frame(wp as HDC, bounds(hwnd));
            }
            _ => {}
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn popup_outline_connects_all_four_corners_without_covering_the_list() {
        let (w, h) = (155, 98);
        let bitmap = Bitmap::new(w, h).unwrap();
        let r = area(0, 0, w, h);
        fill(bitmap.dc, &r, 0xffffff);
        paint_frame(bitmap.dc, r);
        unsafe {
            let shape = outline(r);
            for y in 0..=RADIUS {
                let first = (0..=RADIUS)
                    .find(|&x| PtInRegion(shape, x, y) != 0)
                    .unwrap();
                for row in [y, h - 1 - y] {
                    assert_eq!(GetPixel(bitmap.dc, first, row), EDGE);
                    assert_eq!(GetPixel(bitmap.dc, w - 1 - first, row), EDGE);
                    if first > 0 {
                        assert_eq!(GetPixel(bitmap.dc, first - 1, row), 0xffffff);
                        assert_eq!(GetPixel(bitmap.dc, w - first, row), 0xffffff);
                    }
                }
            }
            DeleteObject(shape);
            for (x, y) in [(0, h / 2), (w - 1, h / 2), (w / 2, 0), (w / 2, h - 1)] {
                assert_eq!(GetPixel(bitmap.dc, x, y), EDGE);
            }
            assert_eq!(GetPixel(bitmap.dc, w / 2, 1), 0xffffff);
            assert_eq!(GetPixel(bitmap.dc, w - RADIUS - 1, h / 2), 0xffffff);
        }
    }
}
