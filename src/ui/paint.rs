use super::*;

pub(super) const BG: u32 = 0x211f1f;
pub(super) const POPUP_BG: u32 = 0x2b2929;
pub(super) const POPUP_TEXT_SIZE: i32 = 12;
pub(super) const PANEL_RADIUS: i32 = 8;
pub(super) const ROW_RADIUS: i32 = 4;
pub(super) const ROW_BG: u32 = 0x302e2e;
pub(super) const EDGE: u32 = 0x464343;
pub(super) const HOVER_BG: u32 = 0x403d3d;
pub(super) const FOCUS_BG: u32 = 0x565353;
pub(super) const INK: u32 = 0xf6f5f5;
pub(super) const MUTED: u32 = 0xb9b6b6;
pub(super) const SILVER: u32 = 0xe1dede;

pub(super) fn bitmap_info(w: i32, h: i32) -> BITMAPINFO {
    BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: w,
            biHeight: -h,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB,
            ..Default::default()
        },
        ..Default::default()
    }
}

pub(super) struct Bitmap {
    pub(super) dc: HDC,
    handle: HBITMAP,
    previous: HGDIOBJ,
    bits: *mut u32,
    len: usize,
}

impl Bitmap {
    pub(super) fn new(w: i32, h: i32) -> io::Result<Self> {
        if w <= 0 || h <= 0 {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        unsafe {
            let dc = CreateCompatibleDC(null_mut());
            if dc.is_null() {
                return Err(io::Error::last_os_error());
            }
            let mut bits = null_mut();
            let handle = CreateDIBSection(
                dc,
                &bitmap_info(w, h),
                DIB_RGB_COLORS,
                &mut bits,
                null_mut(),
                0,
            );
            if handle.is_null() {
                let error = io::Error::last_os_error();
                DeleteDC(dc);
                return Err(error);
            }
            let previous = SelectObject(dc, handle);
            if previous.is_null() {
                let error = io::Error::last_os_error();
                DeleteObject(handle);
                DeleteDC(dc);
                return Err(error);
            }
            Ok(Self {
                dc,
                handle,
                previous,
                bits: bits.cast(),
                len: w as usize * h as usize,
            })
        }
    }
    pub(super) fn pixels(&mut self) -> &mut [u32] {
        unsafe {
            GdiFlush();
            std::slice::from_raw_parts_mut(self.bits, self.len)
        }
    }

    pub(super) fn present(&self, hwnd: HWND, origin: Option<POINT>, size: SIZE) -> io::Result<()> {
        let blend = BLENDFUNCTION {
            BlendOp: AC_SRC_OVER as u8,
            BlendFlags: 0,
            SourceConstantAlpha: 255,
            AlphaFormat: AC_SRC_ALPHA as u8,
        };
        let dest = origin.as_ref().map_or(std::ptr::null(), std::ptr::from_ref);
        if unsafe {
            UpdateLayeredWindow(
                hwnd,
                null_mut(),
                dest,
                &size,
                self.dc,
                &POINT::default(),
                0,
                &blend,
                ULW_ALPHA,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
}
impl Drop for Bitmap {
    fn drop(&mut self) {
        unsafe {
            SelectObject(self.dc, self.previous);
            DeleteObject(self.handle);
            DeleteDC(self.dc);
        }
    }
}

pub(super) fn round_edges(pixels: &mut [u32], w: i32, h: i32, radius: i32) {
    for pixel in pixels.iter_mut() {
        *pixel |= 0xff000000;
    }
    clip_edges(pixels, w, h, radius);
}

pub(super) fn clip_edges(pixels: &mut [u32], w: i32, h: i32, radius: i32) {
    // Sixteenths preserve a true semicircle even at odd intermediate heights.
    let radius = (radius * 16).min(w * 8).min(h * 8);
    for (i, pixel) in pixels.iter_mut().enumerate() {
        let x = i as i32 % w;
        let y = i as i32 / w;
        let dx = radius - 16 * x.min(w - 1 - x);
        let dy = radius - 16 * y.min(h - 1 - y);
        if dx <= 0 || dy <= 0 {
            continue;
        }
        // Sample a pixel's area at the curve; mirror the same coverage at all four corners.
        let mut hits = 0;
        for sy in 0..8 {
            for sx in 0..8 {
                let a = (dx - 2 * sx - 1).max(0);
                let b = (dy - 2 * sy - 1).max(0);
                hits += u32::from(a * a + b * b <= radius * radius);
            }
        }
        let channel = |shift: u32| (((*pixel >> shift) & 255) * hits + 32) / 64;
        *pixel = channel(24) << 24 | channel(16) << 16 | channel(8) << 8 | channel(0);
    }
}

pub(super) fn fill(dc: HDC, rect: &RECT, color: u32) {
    unsafe {
        let brush = CreateSolidBrush(color);
        FillRect(dc, rect, brush);
        DeleteObject(brush);
    }
}

fn font(size: i32, weight: i32, family: &str, quality: FONT_QUALITY) -> HFONT {
    unsafe {
        CreateFontW(
            -size,
            0,
            0,
            0,
            weight,
            0,
            0,
            0,
            DEFAULT_CHARSET as u32,
            0,
            0,
            quality as u32,
            0,
            wide(family).as_ptr(),
        )
    }
}

pub(super) fn label_font(size: i32) -> HFONT {
    font(
        size,
        if size >= 16 { 600 } else { 400 },
        "Segoe UI",
        ANTIALIASED_QUALITY,
    )
}

const SINGLE_LINE: u32 = DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS | DT_NOPREFIX;

fn draw_text(dc: HDC, mut rect: RECT, value: &str, face: HFONT, color: u32, flags: u32) {
    unsafe {
        let old = SelectObject(dc, face);
        SetBkMode(dc, TRANSPARENT as i32);
        SetTextColor(dc, color);
        DrawTextW(dc, wide(value).as_ptr(), -1, &mut rect, flags);
        SelectObject(dc, old);
        DeleteObject(face);
    }
}

// Rasterize a grayscale mask so small CJK strokes and symbols keep their edges
// on both an opaque window DC and an offscreen menu DC.
fn smooth_text(
    dc: HDC,
    rect: RECT,
    value: &str,
    color: u32,
    align: u32,
    face: impl Fn(i32) -> HFONT,
) {
    const SCALE: i32 = 3;
    let (w, h) = (rect.right - rect.left, rect.bottom - rect.top);
    let (sw, sh) = (w * SCALE, h * SCALE);
    let (Ok(mut mask), Ok(mut output)) = (Bitmap::new(sw, sh), Bitmap::new(w, h)) else {
        draw_text(dc, rect, value, face(1), color, align);
        return;
    };
    unsafe { PatBlt(mask.dc, 0, 0, sw, sh, BLACKNESS) };
    draw_text(
        mask.dc,
        area(0, 0, sw, sh),
        value,
        face(SCALE),
        0xffffff,
        align,
    );
    let source = mask.pixels();
    for (y, row) in output.pixels().chunks_exact_mut(w as usize).enumerate() {
        for (x, pixel) in row.iter_mut().enumerate() {
            let start = y * SCALE as usize * sw as usize + x * SCALE as usize;
            let mut sum = 0;
            for sy in 0..SCALE as usize {
                let i = start + sy * sw as usize;
                sum += source[i..i + SCALE as usize]
                    .iter()
                    .map(|p| p & 255)
                    .sum::<u32>();
            }
            let samples = (SCALE * SCALE) as u32;
            let alpha = (sum + samples / 2) / samples;
            let channel = |shift: u32| (((color >> shift) & 255) * alpha + 127) / 255;
            *pixel = alpha << 24 | channel(0) << 16 | channel(8) << 8 | channel(16);
        }
    }
    composite(dc, rect, &output);
}

pub(super) fn interface_font(value: &str, size: i32, scale: i32) -> HFONT {
    interface_face(value, size, scale, ANTIALIASED_QUALITY)
}

fn interface_face(value: &str, size: i32, scale: i32, quality: FONT_QUALITY) -> HFONT {
    let family = if value
        .chars()
        .any(|ch| ('\u{3400}'..='\u{9fff}').contains(&ch))
    {
        "Microsoft YaHei UI"
    } else {
        "Segoe UI"
    };
    font(
        size * scale,
        if size >= 18 { 600 } else { 400 },
        family,
        quality,
    )
}

// Rasterize island captions at their final pixel size. The grayscale mask
// carries the glyph's actual coverage, including its antialiased edges.
pub(super) fn island_text(dc: HDC, rect: RECT, value: &str, size: i32, color: u32, align: u32) {
    let (w, h) = (rect.right - rect.left, rect.bottom - rect.top);
    let Ok(mut mask) = Bitmap::new(w, h) else {
        return;
    };
    mask.pixels().fill(0);
    draw_text(
        mask.dc,
        area(0, 0, w, h),
        value,
        interface_face(value, size, 1, CLEARTYPE_NATURAL_QUALITY as FONT_QUALITY),
        0xffffff,
        SINGLE_LINE | align,
    );
    for pixel in mask.pixels() {
        // Reduce subpixel samples to one coverage value before compositing on glass.
        let [b, g, r, _] = pixel.to_le_bytes();
        let a = (u32::from(r) + 2 * u32::from(g) + u32::from(b) + 2) / 4;
        let channel = |shift: u32| (((color >> shift) & 255) * a + 127) / 255;
        *pixel = a << 24 | channel(0) << 16 | channel(8) << 8 | channel(16);
    }
    composite(dc, rect, &mask);
}

pub(super) fn interface_text(dc: HDC, rect: RECT, value: &str, size: i32, color: u32, align: u32) {
    smooth_text(dc, rect, value, color, SINGLE_LINE | align, |scale| {
        interface_font(value, size, scale)
    });
}

pub(super) fn glyph(dc: HDC, rect: RECT, value: &str, size: i32, color: u32) {
    smooth_text(dc, rect, value, color, SINGLE_LINE | DT_CENTER, |scale| {
        font(size * scale, 400, "Segoe MDL2 Assets", ANTIALIASED_QUALITY)
    });
}

pub(super) fn area(left: i32, top: i32, right: i32, bottom: i32) -> RECT {
    RECT {
        left,
        top,
        right,
        bottom,
    }
}

pub(super) fn rounded_fill(dc: HDC, r: RECT, radius: i32, color: u32) {
    let (w, h) = (r.right - r.left, r.bottom - r.top);
    let Ok(mut image) = Bitmap::new(w, h) else {
        return;
    };
    let pixels = image.pixels();
    // The compositor expects a premultiplied BGRA source.
    pixels.fill((color & 255) << 16 | (color & 0xff00) | (color >> 16) & 255);
    round_edges(pixels, w, h, radius);
    composite(dc, r, &image);
}

fn composite(dc: HDC, r: RECT, image: &Bitmap) {
    let (w, h) = (r.right - r.left, r.bottom - r.top);
    unsafe {
        AlphaBlend(
            dc,
            r.left,
            r.top,
            w,
            h,
            image.dc,
            0,
            0,
            w,
            h,
            BLENDFUNCTION {
                BlendOp: AC_SRC_OVER as u8,
                BlendFlags: 0,
                SourceConstantAlpha: 255,
                AlphaFormat: AC_SRC_ALPHA as u8,
            },
        );
    }
}

pub(super) fn frame_attribute(hwnd: HWND, attribute: i32, value: u32) {
    unsafe {
        windows_sys::Win32::Graphics::Dwm::DwmSetWindowAttribute(
            hwnd,
            attribute as u32,
            (&value as *const u32).cast(),
            size_of::<u32>() as u32,
        );
    }
}

pub(super) fn high_contrast() -> bool {
    use windows_sys::Win32::UI::Accessibility::{HCF_HIGHCONTRASTON, HIGHCONTRASTW};
    let mut hc = HIGHCONTRASTW {
        cbSize: size_of::<HIGHCONTRASTW>() as u32,
        ..Default::default()
    };
    unsafe {
        SystemParametersInfoW(
            SPI_GETHIGHCONTRAST,
            hc.cbSize,
            (&mut hc as *mut HIGHCONTRASTW).cast(),
            0,
        ) != 0
            && hc.dwFlags & HCF_HIGHCONTRASTON != 0
    }
}
