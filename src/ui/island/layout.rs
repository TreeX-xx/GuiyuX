use super::*;

pub(super) use tiles::BAR_CELL as CELL;
use tiles::BAR_DEPTH;

pub(super) fn work_area() -> RECT {
    let mut work = RECT::default();
    unsafe {
        if SystemParametersInfoW(SPI_GETWORKAREA, 0, (&mut work as *mut RECT).cast(), 0) == 0 {
            work = area(
                0,
                0,
                GetSystemMetrics(SM_CXSCREEN),
                GetSystemMetrics(SM_CYSCREEN),
            );
        }
    }
    work
}

impl Dock {
    pub(super) fn idle_size(self, hover: f32, feedback: bool) -> (i32, i32) {
        if feedback {
            return (178, 36);
        }
        if self.mode == DockMode::AutoHide {
            return capsule::STYLE.tucked_size(self.edge);
        }
        self.edge.orient(
            100 + (hover * 15.).round() as i32,
            18 + (hover * 2.).round() as i32,
        )
    }

    pub(super) fn frame(self, work: RECT, w: i32, h: i32) -> RECT {
        let gap = if self.mode == DockMode::AutoHide {
            0
        } else {
            8
        };
        let w = w.clamp(1, (work.right - work.left - 2 * gap).max(1));
        let h = h.clamp(1, (work.bottom - work.top - 2 * gap).max(1));
        let center_x = work.left + (work.right - work.left - w) / 2;
        let center_y = work.top + (work.bottom - work.top - h) / 2;
        let (x, y) = match self.edge {
            DockEdge::Top => (center_x, work.top + gap),
            DockEdge::Bottom => (center_x, work.bottom - gap - h),
            DockEdge::Left => (work.left + gap, center_y),
            DockEdge::Right => (work.right - gap - w, center_y),
        };
        area(x, y, x + w, y + h)
    }
}

impl DockEdge {
    pub(super) fn vertical(self) -> bool {
        matches!(self, Self::Left | Self::Right)
    }

    fn orient(self, length: i32, depth: i32) -> (i32, i32) {
        if self.vertical() {
            (depth, length)
        } else {
            (length, depth)
        }
    }

    // The toolbar stays against its dock edge. Grid, feedback and paging share
    // the remaining rectangle, in normal reading orientation on every edge.
    pub(super) fn split(self, w: i32, h: i32) -> (RECT, RECT) {
        let dx = w.min(BAR_DEPTH);
        let dy = h.min(BAR_DEPTH);
        match self {
            Self::Top => (area(0, 0, w, dy), area(0, dy, w, h)),
            Self::Bottom => (area(0, h - dy, w, h), area(0, 0, w, h - dy)),
            Self::Left => (area(0, 0, dx, h), area(dx, 0, w, h)),
            Self::Right => (area(w - dx, 0, w, h), area(0, 0, w - dx, h)),
        }
    }

    pub(super) fn tip(self, button: RECT, w: i32, h: i32, work: RECT) -> POINT {
        let center_x = (button.left + button.right - w) / 2;
        let center_y = (button.top + button.bottom - h) / 2;
        let (x, y) = match self {
            Self::Top => (center_x, button.bottom + 6),
            Self::Bottom => (center_x, button.top - h - 6),
            Self::Left => (button.right + 6, center_y),
            Self::Right => (button.left - w - 6, center_y),
        };
        POINT {
            x: x.clamp(work.left, (work.right - w).max(work.left)),
            y: y.clamp(work.top, (work.bottom - h).max(work.top)),
        }
    }
}

pub(super) struct Toolbar {
    edge: DockEdge,
    pub length: i32,
}

impl Toolbar {
    pub(super) fn new(edge: DockEdge, slots: usize) -> Self {
        Self {
            edge,
            length: 16 + CELL * slots as i32 + 32,
        }
    }

    pub(super) fn size(&self, board: bool, count: usize, footer: bool) -> (i32, i32) {
        if board {
            let grid_height = tiles::height(count, footer);
            let grid_width = tiles::width(count);
            if self.edge.vertical() {
                (BAR_DEPTH + grid_width, self.length.max(grid_height))
            } else {
                (self.length.max(grid_width), BAR_DEPTH + grid_height)
            }
        } else {
            let extra = if !footer {
                0
            } else if self.edge.vertical() {
                178
            } else {
                26
            };
            self.edge.orient(self.length, BAR_DEPTH + extra)
        }
    }

    pub(super) fn slot(&self, strip: RECT, at: i32, len: i32) -> [i32; 4] {
        let (w, h) = (strip.right - strip.left, strip.bottom - strip.top);
        let (along, depth) = self.edge.orient(w, h);
        let span = along.min(self.length);
        let pos = |n| (along - span) / 2 + (n * span + self.length / 2) / self.length;
        let inset = (depth + 5) / 10;
        let (x, y) = self.edge.orient(pos(at), inset);
        let (w, h) = self.edge.orient(pos(at + len) - pos(at), depth - 2 * inset);
        [strip.left + x, strip.top + y, w, h]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tab_and_toolbar_share_the_dock_edge_while_more_opens_inward() {
        let work = area(60, 30, 1260, 830);
        for edge in [
            DockEdge::Top,
            DockEdge::Bottom,
            DockEdge::Left,
            DockEdge::Right,
        ] {
            for mode in [DockMode::AlwaysVisible, DockMode::AutoHide] {
                let dock = Dock { edge, mode };
                let idle = match (mode, edge.vertical()) {
                    (DockMode::AutoHide, true) => (5, 96),
                    (DockMode::AutoHide, false) => (120, 5),
                    (DockMode::AlwaysVisible, true) => (18, 100),
                    (DockMode::AlwaysVisible, false) => (100, 18),
                };
                assert_eq!(dock.idle_size(0., false), idle);
                let bar = Toolbar::new(edge, BAR_SLOTS);
                let frames = [
                    idle,
                    bar.size(false, 0, false),
                    bar.size(true, GRID_PAGE, true),
                ];
                for (w, h) in frames {
                    let frame = dock.frame(work, w, h);
                    let gap = match edge {
                        DockEdge::Top => frame.top - work.top,
                        DockEdge::Bottom => work.bottom - frame.bottom,
                        DockEdge::Left => frame.left - work.left,
                        DockEdge::Right => work.right - frame.right,
                    };
                    assert_eq!(gap, if mode == DockMode::AutoHide { 0 } else { 8 });
                }
                let slot = |(w, h), at| {
                    let frame = dock.frame(work, w, h);
                    let (strip, _) = edge.split(w, h);
                    let [x, y, w, h] = bar.slot(strip, at, CELL);
                    (frame.left + x, frame.top + y, w, h)
                };
                for i in 0..BAR_SLOTS {
                    let at = 8 + i as i32 * CELL;
                    assert_eq!(slot(frames[1], at), slot(frames[2], at));
                }
            }
        }
    }
}
