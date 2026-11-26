use super::*;

// Draw the symbol and its transient hover surface on the same subpixel grid.
const SCALE: i32 = 6;
const WHITE: u32 = 0xffffff;

#[derive(Clone, Copy)]
pub(in crate::ui) enum Symbol {
    Category(crate::collection::Category),
    More,
    Less,
    Up,
    Down,
    Left,
    Right,
}

#[derive(Clone, Copy)]
pub(super) struct Icon<'a> {
    pub symbol: Symbol,
    pub file: Option<&'a std::path::Path>,
}

pub(super) fn mix(a: u32, b: u32, t: f32) -> u32 {
    let channel = |shift: u32| {
        let from = ((a >> shift) & 255) as f32;
        let to = ((b >> shift) & 255) as f32;
        (from + (to - from) * t).round() as u32
    };
    channel(0) | (channel(8) << 8) | (channel(16) << 16)
}

fn px(value: f32) -> i32 {
    (value * SCALE as f32).round() as i32
}

fn rounded(dc: HDC, bounds: [f32; 4], radius: f32, color: u32) {
    let [x, y, w, h] = bounds;
    unsafe {
        let region = CreateRoundRectRgn(
            px(x),
            px(y),
            px(x + w) + 1,
            px(y + h) + 1,
            px(2. * radius),
            px(2. * radius),
        );
        if !region.is_null() {
            let brush = CreateSolidBrush(color);
            if !brush.is_null() {
                FillRgn(dc, region, brush);
                DeleteObject(brush);
            }
            DeleteObject(region);
        }
    }
}

fn pen(width: f32, color: u32) -> HPEN {
    let brush = LOGBRUSH {
        lbStyle: BS_SOLID,
        lbColor: color,
        lbHatch: 0,
    };
    unsafe {
        ExtCreatePen(
            (PS_GEOMETRIC | PS_SOLID | PS_ENDCAP_ROUND | PS_JOIN_ROUND) as u32,
            px(width) as u32,
            &brush,
            0,
            null_mut(),
        )
    }
}

// Rotate only the icon in its local coordinates. The caller owns the saved DC.
fn tilt_about(dc: HDC, cx: f32, cy: f32, radians: f32) {
    if radians == 0. || unsafe { SetGraphicsMode(dc, GM_ADVANCED) } == 0 {
        return;
    }
    let (sin, cos) = radians.sin_cos();
    let rotation = XFORM {
        eM11: cos,
        eM12: sin,
        eM21: -sin,
        eM22: cos,
        eDx: (1. - cos) * cx + sin * cy,
        eDy: (1. - cos) * cy - sin * cx,
    };
    unsafe { ModifyWorldTransform(dc, &rotation, MWT_LEFTMULTIPLY) };
}

fn mark(dc: HDC, icon: Icon<'_>, cx: f32, cy: f32, size: f32, color: u32, tilt: f32) {
    use crate::collection::Category::*;
    let point = |x: f32, y: f32| POINT {
        x: px(cx + (x - 12.0) * size / 24.0),
        y: px(cy + (y - 12.0) * size / 24.0),
    };
    let line = |coords: &[(i32, i32)]| {
        let pts: Vec<_> = coords
            .iter()
            .map(|&(x, y)| point(x as f32, y as f32))
            .collect();
        unsafe {
            Polyline(dc, pts.as_ptr(), pts.len() as i32);
        }
    };
    let rect = |l, t, r, b, radius: f32| {
        let a = point(l, t);
        let b = point(r, b);
        let round = px(radius * size / 24.0);
        unsafe {
            RoundRect(dc, a.x, a.y, b.x, b.y, round, round);
        }
    };
    unsafe {
        let state = SaveDC(dc);
        if state == 0 {
            return;
        }
        tilt_about(dc, cx * SCALE as f32, cy * SCALE as f32, tilt);
        let side = size.min(26.);
        if side >= 8.
            && let Some(path) = icon.file
            && item_icon::draw(dc, path, px(cx - side / 2.), px(cy - side / 2.), px(side))
        {
            RestoreDC(dc, state);
            return;
        }
        let stroke = pen(1.7 * size / 24.0, color);
        SelectObject(dc, stroke);
        SelectObject(dc, GetStockObject(NULL_BRUSH));
        match icon.symbol {
            Symbol::Category(Office) => line(&[
                (4, 4),
                (20, 4),
                (20, 15),
                (11, 15),
                (6, 20),
                (6, 15),
                (4, 15),
                (4, 4),
            ]),
            Symbol::Category(Assistant) => line(&[
                (12, 2),
                (15, 9),
                (22, 12),
                (15, 15),
                (12, 22),
                (9, 15),
                (2, 12),
                (9, 9),
                (12, 2),
            ]),
            Symbol::Category(Developer) => {
                line(&[(7, 7), (2, 12), (7, 17)]);
                line(&[(14, 4), (10, 20)]);
                line(&[(17, 7), (22, 12), (17, 17)]);
            }
            Symbol::Category(Design) => {
                line(&[(4, 20), (5, 15), (17, 3), (21, 7), (9, 19), (4, 20)]);
                line(&[(14, 6), (18, 10)]);
            }
            Symbol::Category(Utilities) => {
                line(&[(3, 8), (21, 8)]);
                line(&[(8, 5), (8, 11)]);
                line(&[(3, 16), (21, 16)]);
                line(&[(16, 13), (16, 19)]);
            }
            Symbol::Category(Documents) => {
                line(&[(5, 3), (14, 3), (19, 8), (19, 21), (5, 21), (5, 3)]);
                line(&[(14, 3), (14, 8), (19, 8)]);
            }
            Symbol::Category(Images) => {
                rect(5., 4., 9., 8., 4.);
                line(&[(2, 19), (9, 12), (13, 16), (17, 10), (22, 16)]);
            }
            Symbol::Category(Media) => line(&[(7, 3), (21, 12), (7, 21), (7, 3)]),
            Symbol::Category(Archives) => {
                rect(3., 4., 21., 8., 1.);
                line(&[(5, 9), (5, 20), (19, 20), (19, 9)]);
            }
            Symbol::Category(Folders) => {
                line(&[(3, 5), (9, 5), (12, 8), (21, 8), (21, 19), (3, 19), (3, 5)])
            }
            Symbol::Category(Other) => {
                line(&[
                    (3, 13),
                    (7, 6),
                    (17, 6),
                    (21, 13),
                    (21, 19),
                    (3, 19),
                    (3, 13),
                    (8, 13),
                    (10, 15),
                    (14, 15),
                    (16, 13),
                    (21, 13),
                ]);
            }
            Symbol::More => {
                for x in [4., 10.5, 17.] {
                    rect(x, 11., x + 2., 13., 2.);
                }
            }
            Symbol::Less => line(&[(5, 12), (19, 12)]),
            Symbol::Up => line(&[(6, 15), (12, 9), (18, 15)]),
            Symbol::Down => line(&[(6, 9), (12, 15), (18, 9)]),
            Symbol::Left => line(&[(15, 6), (9, 12), (15, 18)]),
            Symbol::Right => line(&[(9, 6), (15, 12), (9, 18)]),
        }
        RestoreDC(dc, state);
        DeleteObject(stroke);
    }
}

fn button(
    dc: HDC,
    w: f32,
    h: f32,
    icon: Icon<'_>,
    hover: f32,
    focused: bool,
    pressed: bool,
    tilt: f32,
) {
    let active = if focused || pressed { 1.0 } else { hover };
    if active > 0.0 && w > 4. && h > 2. {
        let shade = mix(BG, if focused { FOCUS_BG } else { HOVER_BG }, active);
        rounded(dc, [1.5, 1., w - 3., h - 2.], ROW_RADIUS as f32, shade);
    }
    let nav = matches!(
        icon.symbol,
        Symbol::Up | Symbol::Down | Symbol::Left | Symbol::Right
    );
    let size = (if nav { 17.0_f32 } else { 24.0_f32 })
        .min(w - 4.)
        .min(h - 4.);
    if size > 1.0 {
        mark(
            dc,
            icon,
            w / 2.,
            h / 2.
                + if pressed && icon.file.is_none() {
                    0.6
                } else {
                    0.
                },
            size,
            mix(if nav { MUTED } else { SILVER }, WHITE, active),
            tilt,
        );
    }
}

// Average actual coverage in the 32-bit image. A display-dependent HALFTONE
// stretch can introduce a dither pattern across low-contrast hover surfaces.
fn resolve(samples: &[u32], w: usize, h: usize) -> Vec<u32> {
    let scale = SCALE as usize;
    let stride = w * scale;
    let count = (scale * scale) as u32;
    let mut pixels = vec![0; w * h];
    for (rows, output) in samples
        .chunks_exact(stride * scale)
        .zip(pixels.chunks_exact_mut(w))
    {
        for (x, pixel) in output.iter_mut().enumerate() {
            let (mut r, mut g, mut b) = (0, 0, 0);
            for row in rows.chunks_exact(stride) {
                for color in &row[x * scale..(x + 1) * scale] {
                    r += (color >> 16) & 255;
                    g += (color >> 8) & 255;
                    b += color & 255;
                }
            }
            *pixel = (((r + count / 2) / count) << 16)
                | (((g + count / 2) / count) << 8)
                | ((b + count / 2) / count);
        }
    }
    pixels
}

pub(super) fn draw(
    target: HDC,
    rect: RECT,
    icon: Icon<'_>,
    hover: f32,
    focused: bool,
    pressed: bool,
    tilt: f32,
) {
    paint_control(target, rect, |dc, w, h| {
        button(dc, w, h, icon, hover, focused, pressed, tilt)
    });
}

pub(super) fn tile_background(
    target: HDC,
    rect: RECT,
    hover: f32,
    focused: bool,
    pressed: bool,
) -> u32 {
    if hover <= 0.0 && !focused && !pressed {
        fill(target, &rect, BG);
        return BG;
    }
    let strength = if pressed || focused { 1.0 } else { hover };
    let shade = mix(BG, if focused { FOCUS_BG } else { HOVER_BG }, strength);
    paint_control(target, rect, |dc, w, h| {
        rounded(dc, [0.0, 1.0, w, h - 2.0], ROW_RADIUS as f32, shade)
    });
    shade
}

pub(super) fn emblem(target: HDC, rect: RECT, icon: Icon<'_>, base: u32, ink: u32, tilt: f32) {
    paint_on(target, rect, base, |dc, w, h| {
        mark(dc, icon, w / 2., h / 2., w.min(h) * 0.74, ink, tilt)
    });
}

fn paint_control(target: HDC, rect: RECT, paint: impl FnOnce(HDC, f32, f32)) {
    paint_on(target, rect, BG, paint);
}

fn paint_on(target: HDC, rect: RECT, base: u32, paint: impl FnOnce(HDC, f32, f32)) {
    let (w, h) = (rect.right - rect.left, rect.bottom - rect.top);
    if w <= 0 || h <= 0 {
        return;
    }
    unsafe {
        let (sw, sh) = (w * SCALE, h * SCALE);
        let Ok(mut bitmap) = Bitmap::new(sw, sh) else {
            return;
        };
        let dc = bitmap.dc;
        fill(
            dc,
            &RECT {
                left: 0,
                top: 0,
                right: sw,
                bottom: sh,
            },
            base,
        );
        paint(dc, w as f32, h as f32);
        let pixels = resolve(bitmap.pixels(), w as usize, h as usize);
        SetDIBitsToDevice(
            target,
            rect.left,
            rect.top,
            w as u32,
            h as u32,
            0,
            0,
            0,
            h as u32,
            pixels.as_ptr().cast(),
            &bitmap_info(w, h),
            DIB_RGB_COLORS,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toolbar_and_grid_icons_tilt_around_their_center() {
        const W: usize = 48;
        const H: usize = 40;
        let rect = area(0, 0, W as i32, H as i32);
        let mut frame = Bitmap::new(W as i32, H as i32).unwrap();
        let icon = Icon {
            symbol: Symbol::Less,
            file: None,
        };
        for grid in [false, true] {
            for degrees in [-7.0_f32, 7.0] {
                let tilt = degrees.to_radians();
                if grid {
                    emblem(frame.dc, rect, icon, BG, WHITE, tilt);
                } else {
                    draw(frame.dc, rect, icon, 1., false, false, tilt);
                }
                // A horizontal line tilts in opposite directions about the same midpoint.
                let (mut mass, mut sum_x, mut sum_y, mut sum_xy) = (0., 0., 0., 0.);
                for (i, pixel) in frame.pixels().iter().enumerate() {
                    let weight = (*pixel as u8).saturating_sub(128) as f32;
                    let x = (i % W) as f32 + 0.5 - W as f32 / 2.;
                    let y = (i / W) as f32 + 0.5 - H as f32 / 2.;
                    mass += weight;
                    sum_x += x * weight;
                    sum_y += y * weight;
                    sum_xy += x * y * weight;
                }
                assert!(mass > 0.);
                assert!((sum_x / mass).abs() < 0.25 && (sum_y / mass).abs() < 0.25);
                assert!(degrees.signum() * sum_xy / mass > 1.);
            }
        }
    }

    #[test]
    fn hover_plate_has_partial_coverage_and_returns_to_idle() {
        let rect = RECT {
            left: 0,
            top: 0,
            right: 48,
            bottom: 40,
        };
        let mut frame = Bitmap::new(48, 40).unwrap();
        let render = |frame: &mut Bitmap, hover| {
            draw(
                frame.dc,
                rect,
                Icon {
                    symbol: Symbol::Category(crate::collection::Category::Media),
                    file: None,
                },
                hover,
                false,
                false,
                0.,
            );
            frame.pixels().to_vec()
        };
        let idle = render(&mut frame, 0.0);
        let levels =
            [0.0, 0.25, 0.5, 0.75, 1.0].map(|hover| render(&mut frame, hover)[20 * 48 + 4] & 255);
        assert!(levels.windows(2).all(|p| p[0] < p[1]));

        let hot = render(&mut frame, 1.0);
        // The plate is flat inside and blends with the background along the small arc.
        assert_eq!(hot[10 * 48 + 4], hot[30 * 48 + 4]);
        assert!(hot[48..48 + 8].iter().any(|pixel| {
            let value = pixel & 255;
            levels[0] < value && value < levels[4]
        }));
        assert_eq!(hot[0], idle[0]);
        assert_eq!(render(&mut frame, 0.0), idle);
    }
}
