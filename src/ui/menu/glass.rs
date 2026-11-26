// 菜单背景设计见 .agents/notes/module.md「当前工程」。
use super::*;

// Sample only this popup's bounds, before its first layered surface is presented.
// The small, averaged image stays in memory for the lifetime of the open menu.
pub(super) struct Backdrop {
    samples: Vec<u32>,
    cols: i32,
    rows: i32,
}

impl Backdrop {
    pub(super) fn capture(bounds: RECT) -> Option<Self> {
        let w = bounds.right - bounds.left;
        let h = bounds.bottom - bounds.top;
        let cols = (w + 15) / 16;
        let rows = (h + 15) / 16;
        let mut small = Bitmap::new(cols, rows).ok()?;
        unsafe {
            let screen = GetDC(null_mut());
            if screen.is_null() {
                return None;
            }
            SetStretchBltMode(small.dc, HALFTONE);
            SetBrushOrgEx(small.dc, 0, 0, null_mut());
            let ok = StretchBlt(
                small.dc,
                0,
                0,
                cols,
                rows,
                screen,
                bounds.left,
                bounds.top,
                w,
                h,
                SRCCOPY | CAPTUREBLT,
            );
            ReleaseDC(null_mut(), screen);
            if ok == 0 {
                return None;
            }
        }
        Some(Self {
            samples: small.pixels().to_vec(),
            cols,
            rows,
        })
    }

    fn color(&self, x: i32, y: i32, w: i32, h: i32) -> u32 {
        // Reconstruct between sample centres so the blurred surface has no tile edges.
        let axis = |p: i32, count: i32, size: i32| {
            let fixed = (((2 * p + 1) as i64 * count as i64 * 128 / size as i64) as i32 - 128)
                .clamp(0, (count - 1) * 256);
            (
                fixed / 256,
                (fixed / 256 + 1).min(count - 1),
                (fixed % 256) as u32,
            )
        };
        let (x0, x1, fx) = axis(x, self.cols, w);
        let (y0, y1, fy) = axis(y, self.rows, h);
        let sample = |x, y| self.samples[(y * self.cols + x) as usize];
        let mix = |a: u32, b: u32, f: u32| (a * (256 - f) + b * f + 128) >> 8;
        let channel = |shift: u32| {
            let line = |y| {
                mix(
                    (sample(x0, y) >> shift) & 255,
                    (sample(x1, y) >> shift) & 255,
                    fx,
                )
            };
            mix(line(y0), line(y1), fy)
        };
        channel(16) << 16 | channel(8) << 8 | channel(0)
    }
}

pub(super) fn blend(bitmap: &mut Bitmap, w: i32, h: i32, percent: u8, backdrop: Option<&Backdrop>) {
    if percent == 0 {
        return;
    }
    for (i, pixel) in bitmap.pixels().iter_mut().enumerate() {
        let a = *pixel >> 24;
        if a == 0 {
            continue;
        }
        let rgb = [(*pixel >> 16) & 255, (*pixel >> 8) & 255, *pixel & 255];
        let base = [POPUP_BG & 255, (POPUP_BG >> 8) & 255, POPUP_BG >> 16];
        // Preserve the brighter foreground strokes and the stronger hover tint.
        let contrast = rgb
            .iter()
            .zip(base)
            .map(|(c, b)| (c * 255 / a).abs_diff(b))
            .max()
            .unwrap();
        let transmission = a * u32::from(percent) * (64 - contrast.min(64)) / 6400;
        if transmission == 0 {
            continue;
        }
        let background = backdrop.map(|b| b.color(i as i32 % w, i as i32 / w, w, h));
        let channel = |index: usize, shift: u32| {
            let light = (background.unwrap_or(0) >> shift) & 255;
            (rgb[index] + light * transmission / 255)
                .saturating_sub(base[index] * transmission / 255)
                .min(a)
        };
        let alpha = if background.is_some() {
            a
        } else {
            a - transmission
        };
        *pixel = alpha << 24 | channel(0, 16) << 16 | channel(1, 8) << 8 | channel(2, 0);
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transparency_changes_the_surface_and_preserves_text_and_corners() {
        // BGRA values on either side of a high-contrast backdrop.
        let backdrop = Backdrop {
            samples: vec![0, 0xffffff],
            cols: 2,
            rows: 1,
        };
        let surface = 0xff000000 | (POPUP_BG & 255) << 16 | (POPUP_BG & 0xff00) | POPUP_BG >> 16;
        let original = [surface, surface, surface, 0xffffffff, 0];
        let mut bmp = Bitmap::new(5, 1).unwrap();
        bmp.pixels().copy_from_slice(&original);
        blend(&mut bmp, 5, 1, 0, Some(&backdrop));
        assert_eq!(bmp.pixels(), original);
        blend(&mut bmp, 5, 1, 30, None);
        assert_eq!(bmp.pixels()[0] >> 24, 179);
        assert_eq!(&bmp.pixels()[3..], &original[3..]);
        bmp.pixels().copy_from_slice(&original);
        blend(&mut bmp, 5, 1, 60, Some(&backdrop));
        let pixels = bmp.pixels();
        assert!(pixels[0] < pixels[1] && pixels[1] < pixels[2]);
        assert_eq!(&pixels[3..], &original[3..]);
    }
}
