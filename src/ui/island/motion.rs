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
        let elapsed = now.duration_since(start).as_secs_f32();
        let eased = if BUTTONS[slot] == HEADER && capsule::active() {
            let remaining = (target - self.from[slot]).abs();
            if remaining == 0. {
                return target;
            }
            capsule::ease_out(elapsed * 1000. / (capsule::STYLE.slide_ms as f32 * remaining))
        } else {
            let t = (elapsed / HOVER_SECONDS).min(1.);
            t * t * (3. - 2. * t)
        };
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

// The icon pivots in place. See .agents/notes/module.md#当前工程.
const TILT_SECONDS: f32 = 0.32;
fn tilt_radians(elapsed: f32) -> f32 {
    let t = (elapsed / TILT_SECONDS).clamp(0., 1.);
    (std::f32::consts::TAU * t).sin() * (1. - t) * 9.0_f32.to_radians()
}
pub(super) fn icon_tilt(id: i32) -> f32 {
    let hover = HOVER.get();
    if hover.target == id
        && id >= FIRST
        && id < FIRST + SLOTS as i32
        && let Some(start) = hover.started
    {
        tilt_radians(start.elapsed().as_secs_f32())
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
    if track_leave(hwnd) {
        target(id);
    }
}

pub(super) fn track_leave(hwnd: HWND) -> bool {
    let mut tracking = TRACKMOUSEEVENT {
        cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
        dwFlags: TME_LEAVE,
        hwndTrack: hwnd,
        dwHoverTime: 0,
    };
    unsafe { TrackMouseEvent(&mut tracking) != 0 }
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

pub(super) fn target(id: i32) {
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
                TILT_SECONDS
            } else if capsule::active() {
                capsule::STYLE.slide_ms as f32 / 1000.
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
        WM_MOUSEMOVE => {
            if !capsule::active() && HOVER.get().target != id {
                track(hwnd, id);
            }
            autohide::pointer_move(hwnd, wp);
        }
        WM_MOUSELEAVE => {
            if !capsule::active() && HOVER.get().target == id {
                target(0);
            }
            autohide::pointer_left();
        }
        _ => {}
    }
    unsafe { DefSubclassProc(hwnd, msg, wp, lp) }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hover_tilt_dampens_and_returns_upright() {
        assert_eq!(tilt_radians(0.), 0.);
        let first = tilt_radians(TILT_SECONDS * 0.25).to_degrees();
        let rebound = tilt_radians(TILT_SECONDS * 0.75).to_degrees();
        assert!((5.0..9.0).contains(&first));
        assert!((-first..0.).contains(&rebound));
        assert_eq!(tilt_radians(TILT_SECONDS), 0.);
        assert_eq!(tilt_radians(5.), 0.);
    }
}
