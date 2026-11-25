use super::*;
use std::time::Instant;

const HOVER_SECONDS: f32 = 0.14;

#[derive(Clone, Copy)]
struct Hover {
    target: i32,
    from: [f32; BUTTONS.len()],
    started: Option<Instant>,
}
impl Hover {
    fn value(self, slot: usize, now: Instant) -> f32 {
        let target = if self.target == BUTTONS[slot] {
            1.0
        } else {
            0.0
        };
        let Some(start) = self.started else {
            return target;
        };
        let t = (now.duration_since(start).as_secs_f32() / HOVER_SECONDS).min(1.0);
        let eased = t * t * (3.0 - 2.0 * t);
        self.from[slot] + (target - self.from[slot]) * eased
    }
}
thread_local! {
    static HOVER: Cell<Hover> = const { Cell::new(Hover {
        target:0, from:[0.0; BUTTONS.len()], started:None
    }) };
}

pub(super) fn clear_hover() {
    HOVER.set(Hover {
        target: 0,
        from: [0.0; BUTTONS.len()],
        started: None,
    });
    unsafe {
        KillTimer(ISLAND.get(), HOVER_TIMER);
    }
}

pub(in crate::ui) fn animation_enabled() -> bool {
    let mut enabled = 1i32;
    unsafe {
        SystemParametersInfoW(
            SPI_GETCLIENTAREAANIMATION,
            0,
            (&mut enabled as *mut i32).cast(),
            0,
        );
    }
    enabled != 0
}

pub(in crate::ui) const SWAY_SECONDS: f32 = 0.32;
pub(in crate::ui) fn icon_sway(elapsed: f32) -> f32 {
    let t = (elapsed / SWAY_SECONDS).clamp(0., 1.);
    (std::f32::consts::TAU * t).sin() * (1. - t) * 2.6
}
pub(super) fn sway(id: i32) -> f32 {
    let hover = HOVER.get();
    if hover.target == id
        && id >= FIRST
        && id < FIRST + SLOTS as i32
        && let Some(start) = hover.started
    {
        icon_sway(start.elapsed().as_secs_f32())
    } else {
        0.
    }
}

pub(super) fn restore_pointer() {
    let mut p = POINT::default();
    unsafe {
        if GetCursorPos(&mut p) == 0 {
            return;
        }
        let hit = WindowFromPoint(p);
        if GetParent(hit) == ISLAND.get() {
            let id = GetDlgCtrlID(hit);
            if BUTTONS.contains(&id) && HOVER.get().target != id {
                track(hit, id);
            }
        } else if HOVER.get().target != 0 {
            target(0);
        }
    }
}
fn track(hwnd: HWND, id: i32) {
    let mut tracking = TRACKMOUSEEVENT {
        cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
        dwFlags: TME_LEAVE,
        hwndTrack: hwnd,
        dwHoverTime: 0,
    };
    if unsafe { TrackMouseEvent(&mut tracking) } != 0 {
        target(id);
    }
}

pub(super) fn strength(id: i32) -> f32 {
    BUTTONS
        .iter()
        .position(|button| *button == id)
        .map_or(0.0, |slot| HOVER.get().value(slot, Instant::now()))
}

fn redraw(hwnd: HWND) {
    if EXPANDED.get() || FEEDBACK.get().is_some() {
        schedule_frame(hwnd);
    } else {
        refresh();
    }
}

fn target(id: i32) {
    let hwnd = ISLAND.get();
    let now = Instant::now();
    let old = HOVER.get();
    if old.target == id {
        return;
    }
    let from = std::array::from_fn(|slot| old.value(slot, now));
    let animate = animation_enabled() && unsafe { SetTimer(hwnd, HOVER_TIMER, 15, None) } != 0;
    HOVER.set(Hover {
        target: id,
        from,
        started: animate.then_some(now),
    });
    if !animate {
        unsafe {
            KillTimer(hwnd, HOVER_TIMER);
        }
    }
    redraw(hwnd);
}

pub(super) fn tick(hwnd: HWND) {
    let mut hover = HOVER.get();
    if hover.started.is_none_or(|t| {
        t.elapsed().as_secs_f32()
            >= if (FIRST..FIRST + SLOTS as i32).contains(&hover.target) {
                SWAY_SECONDS
            } else {
                HOVER_SECONDS
            }
    }) {
        hover.started = None;
        HOVER.set(hover);
        unsafe {
            KillTimer(hwnd, HOVER_TIMER);
        }
    }
    redraw(hwnd);
}

pub(super) unsafe extern "system" fn input(
    hwnd: HWND,
    msg: u32,
    wp: WPARAM,
    lp: LPARAM,
    id: usize,
    _: usize,
) -> LRESULT {
    let id = id as i32;
    if drag::button_input(hwnd, id, msg, wp, lp) {
        return 0;
    }
    match msg {
        WM_MOUSEMOVE if HOVER.get().target != id => {
            track(hwnd, id);
        }
        WM_MOUSELEAVE if HOVER.get().target == id => target(0),
        _ => {}
    }
    unsafe { DefSubclassProc(hwnd, msg, wp, lp) }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hover_nudge_returns_to_rest() {
        assert_eq!(icon_sway(0.), 0.);
        assert!(icon_sway(SWAY_SECONDS * 0.25) > 1.);
        assert!(icon_sway(SWAY_SECONDS * 0.75) < -0.5);
        assert_eq!(icon_sway(SWAY_SECONDS), 0.);
        assert_eq!(icon_sway(5.), 0.);
    }
}
