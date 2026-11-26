// The settings preview and the desktop capsule share assets/dock-capsule.json.
// See .agents/notes/module.md#灵动岛与菜单外观.
use super::*;
use serde::Deserialize;
use std::sync::LazyLock;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Style {
    length: i32,
    depth: i32,
    side_length: i32,
    side_depth: i32,
    peek: i32,
    fill: [u8; 3],
    border_alpha: u32,
}

pub(super) static STYLE: LazyLock<Style> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../../../assets/dock-capsule.json"))
        .expect("dock capsule style")
});

impl Style {
    pub(super) fn tucked_size(&self, edge: DockEdge) -> (i32, i32) {
        if edge.vertical() {
            (self.peek, self.side_length)
        } else {
            (self.length, self.peek)
        }
    }
}

pub(super) fn active() -> bool {
    !EXPANDED.get() && FEEDBACK.get().is_none() && nav::dock().mode == DockMode::AutoHide
}

// Only the outer arc of the pill is visible at the work-area edge. Both the
// layered pixels and DWM use this outline in client coordinates.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct Outline {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub diameter: i32,
}

impl Outline {
    fn sliced(edge: DockEdge, width: i32, height: i32) -> Self {
        let (w, h) = if edge.vertical() {
            (STYLE.side_depth, height)
        } else {
            (width, STYLE.depth)
        };
        Self {
            x: if edge == DockEdge::Left { width - w } else { 0 },
            y: if edge == DockEdge::Top { height - h } else { 0 },
            w,
            h,
            diameter: w.min(h),
        }
    }

    pub(super) fn current(w: i32, h: i32) -> Self {
        if active() {
            Self::sliced(nav::dock().edge, w, h)
        } else {
            Self {
                x: 0,
                y: 0,
                w,
                h,
                diameter: (2 * ISLAND_RADIUS).min(w).min(h),
            }
        }
    }
}

pub(super) fn paint(
    bitmap: &mut Bitmap,
    width: i32,
    outline: Outline,
    vertical: bool,
    transparency: u8,
    high_contrast: bool,
    focused: bool,
) {
    let rgb = |[r, g, b]: [u8; 3]| u32::from_be_bytes([255, r, g, b]);
    let (fill, rim) = if high_contrast {
        let system = |index| {
            let [r, g, b, _] = unsafe { GetSysColor(index) }.to_le_bytes();
            rgb([r, g, b])
        };
        (system(COLOR_WINDOW), system(COLOR_WINDOWTEXT))
    } else {
        (rgb(STYLE.fill), 0xffffffff)
    };
    let alpha = if high_contrast {
        255
    } else {
        (255 * u32::from(100u8.saturating_sub(transparency)) / 100).max(1)
    };
    let base = surface::over(0, fill, alpha);
    let rim_alpha = if focused || high_contrast {
        255
    } else {
        STYLE.border_alpha
    };
    let radius = outline.diameter as f32 / 2.;
    let cx = outline.x as f32 + outline.w as f32 / 2.;
    let cy = outline.y as f32 + outline.h as f32 / 2.;
    let straight = (outline.w - outline.h).abs() as f32 / 2.;
    let coverage = |distance: f32| (0.5 - distance).clamp(0., 1.);
    for (i, pixel) in bitmap.pixels().iter_mut().enumerate() {
        let x = (i % width as usize) as f32 + 0.5 - cx;
        let y = (i / width as usize) as f32 + 0.5 - cy;
        let (along, across) = if vertical { (y, x) } else { (x, y) };
        let distance = ((along.abs() - straight).max(0.)).hypot(across) - radius;
        let outer = coverage(distance);
        if outer == 0. {
            *pixel = 0;
            continue;
        }
        let border = 1. - coverage(distance + 1.);
        let color = surface::over(base, rim, (border * rim_alpha as f32).round() as u32);
        *pixel = surface::over(0, color, (outer * 255.).round() as u32);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tucked_edges_keep_the_arc_and_border_of_the_capsule() {
        for edge in [
            DockEdge::Bottom,
            DockEdge::Top,
            DockEdge::Left,
            DockEdge::Right,
        ] {
            let (w, h) = if edge.vertical() {
                (STYLE.side_depth, STYLE.side_length)
            } else {
                (STYLE.length, STYLE.depth)
            };
            let mut full = Bitmap::new(w, h).unwrap();
            let render = |surface: &mut Bitmap, w, h| {
                paint(
                    surface,
                    w,
                    Outline::sliced(edge, w, h),
                    edge.vertical(),
                    50,
                    false,
                    false,
                );
            };
            render(&mut full, w, h);
            let full = full.pixels();
            let at = |x, y| full[(y * w + x) as usize];
            assert_eq!(at(0, 0), 0);
            assert!(at(w / 2, 0) >> 24 > at(w / 2, h / 2) >> 24);
            let (vw, vh) = STYLE.tucked_size(edge);
            let mut frame = Bitmap::new(vw, vh).unwrap();
            render(&mut frame, vw, vh);
            let x0 = if edge == DockEdge::Left { w - vw } else { 0 };
            let y0 = if edge == DockEdge::Top { h - vh } else { 0 };
            for (y, line) in frame.pixels().chunks_exact(vw as usize).enumerate() {
                let offset = (y0 as usize + y) * w as usize + x0 as usize;
                assert_eq!(line, &full[offset..offset + vw as usize]);
            }
        }
    }
}
