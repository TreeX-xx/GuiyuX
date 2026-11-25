use super::*;

// Composite the native buttons and background into a single premultiplied BGRA frame.
pub(super) fn present(hwnd: HWND) -> io::Result<()> {
    unsafe {
        let rect = FRAME_RECT.get();
        let mut current = RECT::default();
        GetWindowRect(hwnd, &mut current);
        layer(hwnd, rect, 12, CONTENT_ALPHA.get(), |dc, local| {
            paint_background(dc, local);
            for id in BUTTONS {
                let child = GetDlgItem(hwnd, id);
                if GetWindowLongW(child, GWL_STYLE) as u32 & WS_VISIBLE == 0 {
                    continue;
                }
                let mut bounds = RECT::default();
                GetWindowRect(child, &mut bounds);
                let state = SaveDC(dc);
                SetViewportOrgEx(
                    dc,
                    bounds.left - current.left,
                    bounds.top - current.top,
                    null_mut(),
                );
                IntersectClipRect(
                    dc,
                    0,
                    0,
                    bounds.right - bounds.left,
                    bounds.bottom - bounds.top,
                );
                SendMessageW(child, WM_PRINTCLIENT, dc as usize, PRF_CLIENT as isize);
                RestoreDC(dc, state);
            }
            drag::paint_line(dc);
        })
    }
}

pub(super) fn layer(
    hwnd: HWND,
    rect: RECT,
    radius: i32,
    content_alpha: u32,
    paint: impl FnOnce(HDC, RECT),
) -> io::Result<()> {
    let (w, h) = (rect.right - rect.left, rect.bottom - rect.top);
    if w <= 0 || h <= 0 {
        return Ok(());
    }
    let mut bitmap = Bitmap::new(w, h)?;
    let dc = bitmap.dc;
    paint(
        dc,
        RECT {
            left: 0,
            top: 0,
            right: w,
            bottom: h,
        },
    );
    let pixels = bitmap.pixels();
    let alpha = content_alpha;
    if alpha < 255 {
        // DIB channels are BGR in memory, COLORREF stores red in the low byte.
        let base = [(BG >> 16) & 255, (BG >> 8) & 255, BG & 255];
        for pixel in pixels.iter_mut() {
            let mut result = 0;
            for (channel, bg) in base.iter().enumerate() {
                let shift = 8 * channel;
                result |=
                    ((((*pixel >> shift) & 255) * alpha + bg * (255 - alpha) + 127) / 255) << shift;
            }
            *pixel = result;
        }
    }
    round_edges(pixels, w, h, radius);
    bitmap.present(
        hwnd,
        Some(POINT {
            x: rect.left,
            y: rect.top,
        }),
        SIZE { cx: w, cy: h },
    )
}
