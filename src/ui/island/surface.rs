use super::*;

pub(super) fn sync_backdrop(hwnd: HWND) {
    if ISLAND.get() != hwnd {
        return;
    }
    let mut bounds = RECT::default();
    unsafe { GetWindowRect(hwnd, &mut bounds) };
    let style = APPEARANCE_STYLE.get();
    backdrop::sync(
        hwnd,
        bounds,
        style.frosted && style.transparency > 0 && !high_contrast(),
    );
}

// Render controls on the surface and text on a transparent, pixel-aligned plane.
pub(super) fn present(hwnd: HWND) -> io::Result<()> {
    let rect = FRAME_RECT.get();
    let (w, h) = (rect.right - rect.left, rect.bottom - rect.top);
    if w <= 0 || h <= 0 {
        return Ok(());
    }
    let mut face = Bitmap::new(w, h)?;
    let mut ink = Bitmap::new(w, h)?;
    ink.pixels().fill(0);
    paint_background(face.dc, ink.dc, area(0, 0, w, h));
    unsafe {
        let mut current = RECT::default();
        GetWindowRect(hwnd, &mut current);
        for id in BUTTONS {
            let child = GetDlgItem(hwnd, id);
            if GetWindowLongW(child, GWL_STYLE) as u32 & WS_VISIBLE == 0 {
                continue;
            }
            let mut bounds = RECT::default();
            GetWindowRect(child, &mut bounds);
            let local = area(0, 0, bounds.right - bounds.left, bounds.bottom - bounds.top);
            for dc in [face.dc, ink.dc] {
                SaveDC(dc);
                SetViewportOrgEx(
                    dc,
                    bounds.left - current.left,
                    bounds.top - current.top,
                    null_mut(),
                );
                IntersectClipRect(dc, 0, 0, local.right, local.bottom);
            }
            let pressed = SendMessageW(child, BM_GETSTATE, 0, 0) & BST_PUSHED as isize != 0;
            let item = DRAWITEMSTRUCT {
                CtlID: id as u32,
                hwndItem: child,
                hDC: face.dc,
                rcItem: local,
                itemState: if pressed { ODS_SELECTED } else { 0 },
                ..Default::default()
            };
            draw(&item, ink.dc);
            for dc in [face.dc, ink.dc] {
                RestoreDC(dc, -1);
            }
        }
    }
    drag::paint_line(face.dc);

    let mut style = APPEARANCE_STYLE.get();
    if high_contrast() {
        style.transparency = 0;
    }
    finish(&mut face, &mut ink, w, h, style, CONTENT_ALPHA.get());
    face.present(
        hwnd,
        Some(POINT {
            x: rect.left,
            y: rect.top,
        }),
        SIZE { cx: w, cy: h },
    )?;
    backdrop::sync(hwnd, rect, style.frosted && style.transparency > 0);
    Ok(())
}

// Source-over for premultiplied BGRA, including the expand/collapse opacity.
fn over(dst: u32, src: u32, opacity: u32) -> u32 {
    if src == 0 || opacity == 0 {
        return dst;
    }
    if opacity == 255 && src >> 24 == 255 {
        return src;
    }
    let rest = 255 - ((src >> 24) * opacity + 127) / 255;
    let channel = |shift: u32| {
        ((((src >> shift) & 255) * opacity + ((dst >> shift) & 255) * rest + 127) / 255).min(255)
    };
    channel(24) << 24 | channel(16) << 16 | channel(8) << 8 | channel(0)
}

fn frost(pixel: u32, x: i32, y: i32, w: i32, h: i32) -> u32 {
    let transmission = 255 - (pixel >> 24);
    if transmission == 0 {
        return pixel;
    }
    let top = (h - 1 - y) as u32 * 255 / h.max(1) as u32;
    // A cool diffuse top light and one-pixel reflection follow the capsule,
    // including its semicircular ends while the window is animating.
    let edge_x = x.min(w - 1 - x);
    let edge_y = y.min(h - 1 - y);
    let radius = (ISLAND_RADIUS as f32).min(w.min(h) as f32 * 0.5);
    let rim = if edge_x == 0 || edge_y == 0 {
        1.0
    } else if (edge_x.min(edge_y) as f32) < radius && (edge_x.max(edge_y) as f32) < radius {
        let dx = radius - edge_x as f32 - 0.5;
        let dy = radius - edge_y as f32 - 0.5;
        (dx.hypot(dy) - radius + 1.5).clamp(0., 1.)
    } else {
        0.0
    };
    let film = over(
        pixel,
        0xff18222b,
        (15 + (255 - top) / 10) * transmission / 255,
    );
    let sheen = 3 + top * 34 / 255 + (rim * (60 + top / 3) as f32) as u32;
    over(film, 0xffd5e2ed, sheen * transmission / 255)
}

fn finish(
    face: &mut Bitmap,
    ink: &mut Bitmap,
    w: i32,
    h: i32,
    style: crate::config::Appearance,
    opacity: u32,
) {
    let base = 0xff000000 | (BG & 255) << 16 | (BG & 0xff00) | BG >> 16;
    for pixel in face.pixels() {
        *pixel = over(base, *pixel | 0xff000000, opacity);
    }
    glass::blend(face, w, h, BG, style.transparency, None);
    let labels = ink.pixels();
    let pixels = face.pixels();
    let shadow = u32::from(style.transparency.saturating_sub(50)) * 180 / 50;
    for (i, pixel) in pixels.iter_mut().enumerate() {
        let (x, y) = ((i % w as usize) as i32, (i / w as usize) as i32);
        if style.frosted && style.transparency > 0 {
            *pixel = frost(*pixel, x, y, w, h);
        }
        // At high transmission, a one-pixel dark edge retains legibility over
        // light windows. The glyph coverage itself stays untouched.
        if shadow > 0 && x > 0 && x + 1 < w && y > 0 && y + 1 < h {
            let side = (labels[i - 1].max(labels[i + 1]) >> 24) * 3 / 4;
            let above = (labels[i + w as usize] >> 24) / 2;
            let below = labels[i - w as usize] >> 24;
            let coverage = below.max(side).max(above);
            *pixel = over(
                *pixel,
                0xff11151a,
                coverage * shadow * opacity / (255 * 255),
            );
        }
        *pixel = over(*pixel, labels[i], opacity);
    }
    clip_edges(pixels, w, h, ISLAND_RADIUS);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn material_changes_leave_glyph_coverage_on_the_foreground() {
        let (w, h) = (112, 36);
        let r = area(0, 0, w, h);
        let mut face = Bitmap::new(w, h).unwrap();
        let mut ink = Bitmap::new(w, h).unwrap();
        ink.pixels().fill(0);
        island_text(ink.dc, r, "工具 83%", 16, INK, DT_CENTER);
        let glyphs = ink.pixels().to_vec();
        assert!(glyphs.iter().any(|p| p >> 24 == 255));
        assert!(glyphs.iter().any(|p| (1..255).contains(&(p >> 24))));
        for frosted in [false, true] {
            for transparency in [0, 37, 100] {
                let style = crate::config::Appearance {
                    frosted,
                    transparency,
                };
                let mut empty = Bitmap::new(w, h).unwrap();
                empty.pixels().fill(0);
                let mut material = Bitmap::new(w, h).unwrap();
                fill(material.dc, &r, BG);
                finish(&mut material, &mut empty, w, h, style, 255);
                fill(face.dc, &r, BG);
                finish(&mut face, &mut ink, w, h, style, 255);
                let actual = face.pixels();
                let background = material.pixels();
                assert_eq!(actual[0], 0);
                assert_ne!(background[(w * (h / 2) + 8) as usize] >> 24, 0);
                for (i, &glyph) in glyphs.iter().enumerate().filter(|(_, p)| **p >> 24 != 0) {
                    if transparency <= 50 || glyph >> 24 == 255 {
                        assert_eq!(actual[i], over(background[i], glyph, 255));
                    }
                }
            }
        }
    }
}
