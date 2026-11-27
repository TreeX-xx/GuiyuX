use super::*;
use windows_sys::Win32::System::SystemServices::{MK_LBUTTON, MK_MBUTTON, MK_RBUTTON};

pub(super) const LEAVE_TIMER: usize = 5;
thread_local! {
    static WAIT_FOR_EXIT: Cell<bool> = const { Cell::new(false) };
    static PREVIOUS_FRAME: Cell<Option<RECT>> = const { Cell::new(None) };
    pub(super) static IN_MENU: Cell<bool> = const { Cell::new(false) };
}

fn cursor() -> Option<POINT> {
    let mut point = POINT::default();
    (unsafe { GetCursorPos(&mut point) } != 0).then_some(point)
}

fn pointer_target(hwnd: HWND, point: POINT) -> Option<HWND> {
    unsafe {
        let hit = WindowFromPoint(point);
        (hit == hwnd || IsChild(hwnd, hit) != 0).then_some(hit)
    }
}

pub(super) fn reset(hwnd: HWND) {
    PREVIOUS_FRAME.set(None);
    WAIT_FOR_EXIT.set(false);
    unsafe { KillTimer(hwnd, LEAVE_TIMER) };
}

pub(super) fn dismiss(hwnd: HWND) {
    PREVIOUS_FRAME.set(None);
    if nav::dock().mode == DockMode::AutoHide {
        // A click on the collapse arrow or Esc must work even over the tab.
        WAIT_FOR_EXIT.set(cursor().and_then(|p| pointer_target(hwnd, p)).is_some());
        if WAIT_FOR_EXIT.get() {
            check_later();
        }
    }
}

pub(super) fn changing_view() {
    if nav::dock().mode == DockMode::AutoHide && !drag::active() {
        let mut frame = RECT::default();
        if unsafe { GetWindowRect(ISLAND.get(), &mut frame) } != 0 {
            PREVIOUS_FRAME.set(Some(frame));
        }
    }
}

// See .agents/notes/module.md#灵动岛与菜单外观 for the edge hover interaction.
pub(super) fn pointer_move(hwnd: HWND, keys: WPARAM) {
    if nav::dock().mode != DockMode::AutoHide {
        return;
    }
    motion::track_leave(hwnd);
    if TRANSITION.get().is_none() {
        PREVIOUS_FRAME.set(None);
    }
    if WAIT_FOR_EXIT.get()
        || IN_MENU.get()
        || drag::active()
        || keys & (MK_LBUTTON | MK_RBUTTON | MK_MBUTTON) as usize != 0
    {
        return;
    }
    if !EXPANDED.get() || TRANSITION.get().is_some_and(|m| m.closing) {
        expand_home();
    }
}

pub(super) fn pointer_left() {
    let hwnd = ISLAND.get();
    if nav::dock().mode != DockMode::AutoHide {
        return;
    }
    if WAIT_FOR_EXIT.get()
        && !IN_MENU.get()
        && !drag::active()
        && cursor().and_then(|p| pointer_target(hwnd, p)).is_none()
    {
        WAIT_FOR_EXIT.set(false);
    }
    check_later();
}

// Child-to-child leaves, a resized view, and the end of a menu or a drag all
// get a delayed hit test.
pub(super) fn check_later() {
    if nav::dock().mode == DockMode::AutoHide && (EXPANDED.get() || WAIT_FOR_EXIT.get()) {
        unsafe { SetTimer(ISLAND.get(), LEAVE_TIMER, 260, None) };
    }
}

pub(super) fn check(hwnd: HWND) {
    check_at(hwnd, cursor());
}

fn check_at(hwnd: HWND, point: Option<POINT>) {
    unsafe { KillTimer(hwnd, LEAVE_TIMER) };
    if nav::dock().mode != DockMode::AutoHide || IN_MENU.get() || drag::active() {
        return;
    }
    let hit = point.and_then(|p| pointer_target(hwnd, p));
    // Keep the route from the tucked tab to an inset toolbar reachable.
    if hit.is_none()
        && EXPANDED.get()
        && !TRANSITION.get().is_some_and(|m| m.closing)
        && APPEARANCE_STYLE.get().edge_margin.unwrap_or(0) > 0
        && let Some(p) = point
    {
        let work = layout::work_area();
        let mut gap = FRAME_RECT.get();
        match nav::dock().edge {
            DockEdge::Top => {
                gap.bottom = gap.top;
                gap.top = work.top;
            }
            DockEdge::Bottom => {
                gap.top = gap.bottom;
                gap.bottom = work.bottom;
            }
            DockEdge::Left => {
                gap.right = gap.left;
                gap.left = work.left;
            }
            DockEdge::Right => {
                gap.left = gap.right;
                gap.right = work.right;
            }
        }
        if unsafe { PtInRect(&gap, p) != 0 } {
            check_later();
            return;
        }
    }
    if let Some(frame) = PREVIOUS_FRAME.get()
        && (TRANSITION.get().is_some()
            || (hit.is_none() && point.is_some_and(|p| unsafe { PtInRect(&frame, p) != 0 })))
    {
        // The new view can be smaller than the grid the user just clicked.
        // Keep it reachable while the pointer crosses the vacated area.
        check_later();
        return;
    }
    PREVIOUS_FRAME.set(None);
    if let Some(hit) = hit {
        // Resizing may have moved a stationary pointer from a button to the
        // background (or vice versa), without a mouse move on its new HWND.
        motion::track_leave(hit);
    } else {
        WAIT_FOR_EXIT.set(false);
        if EXPANDED.get() {
            collapse(hwnd);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetFocus;

    #[test]
    fn edge_hover_reveals_the_toolbar_without_stealing_focus() {
        let _desktop = crate::test_desktop::TestDesktop::enter().unwrap();
        let _apartment = crate::shell::Apartment::open().unwrap();
        crate::ui::register("GuiyuX.Island", Some(procedure)).unwrap();
        let mut settings = Settings::default();
        settings.dock.mode = DockMode::AlwaysVisible;
        APPEARANCE_STYLE.set(crate::config::Appearance {
            edge_margin: Some(0),
            ..Default::default()
        });
        open(settings.clone()).unwrap();
        let hwnd = ISLAND.get();
        unsafe {
            // The private desktop has no physical pointer. Deliver enter/leave to
            // the real child and parent and inspect the very next layered frame.
            ShowWindow(hwnd, SW_HIDE);
            let button = GetDlgItem(hwnd, HEADER);
            SendMessageW(button, WM_MOUSEMOVE, 0, 0);
            assert!(!EXPANDED.get());
            motion::clear_hover();
            settings.dock.mode = DockMode::AutoHide;
            settings.dock.edge = DockEdge::Bottom;
            nav::set_data(settings);
            refresh();
            let assert_frame = |open: bool| {
                surface::present(hwnd).unwrap();
                let mut frame = RECT::default();
                assert_ne!(GetWindowRect(hwnd, &mut frame), 0);
                assert_eq!(frame.bottom, layout::work_area().bottom);
                let size = if open {
                    layout::Toolbar::new(DockEdge::Bottom, nav::bar_len()).size(false, 0, false)
                } else {
                    (120, 5)
                };
                assert_eq!((frame.right - frame.left, frame.bottom - frame.top), size);
                let shortcut = GetWindowLongW(GetDlgItem(hwnd, FIRST), GWL_STYLE) as u32;
                assert_eq!(shortcut & WS_VISIBLE != 0, open);
                assert_eq!(EXPANDED.get(), open);
                assert!(TRANSITION.get().is_none());
                assert_eq!(CONTENT_ALPHA.get(), 255);
            };
            SendMessageW(button, WM_MOUSEMOVE, MK_RBUTTON as usize, 0);
            assert_frame(false);
            let settle = || {
                std::thread::sleep(std::time::Duration::from_millis(220));
                refresh();
                surface::present(hwnd).unwrap();
            };
            let focus = GetFocus();
            SendMessageW(button, WM_MOUSEMOVE, 0, 0);
            if motion::animation_enabled() {
                let mut animation = TRANSITION.get().expect("animated hover reveal");
                assert_eq!(animation.from, (120, 5));
                animation.started =
                    std::time::Instant::now() - std::time::Duration::from_millis(95);
                TRANSITION.set(Some(animation));
                SendMessageW(hwnd, WM_TIMER, RESIZE_TIMER, 0);
                surface::present(hwnd).unwrap();
                let mut frame = RECT::default();
                assert_ne!(GetWindowRect(hwnd, &mut frame), 0);
                assert_eq!(frame.bottom, layout::work_area().bottom);
                assert!((6..tiles::BAR_DEPTH).contains(&(frame.bottom - frame.top)));
                assert!((1..255).contains(&CONTENT_ALPHA.get()));
                settle();
            }
            assert_frame(true);
            assert_eq!(GetFocus(), focus);

            // Click a category in the top row of More. The shorter category
            // window no longer covers the original button on a bottom dock.
            let more = FIRST + nav::bar_len() as i32 - 1;
            SendMessageW(GetDlgItem(hwnd, more), BM_CLICK, 0, 0);
            settle();
            let category = GetDlgItem(hwnd, nav::content_first());
            let mut tile = RECT::default();
            assert_ne!(GetWindowRect(category, &mut tile), 0);
            let click = POINT {
                x: (tile.left + tile.right) / 2,
                y: (tile.top + tile.bottom) / 2,
            };
            SendMessageW(category, BM_CLICK, 0, 0);
            settle();
            assert!(matches!(VIEW.get(), View::Group(_)));
            assert_eq!(PtInRect(&FRAME_RECT.get(), click), 0);
            SendMessageW(category, WM_MOUSELEAVE, 0, 0);
            check_at(hwnd, Some(click));
            assert!(EXPANDED.get() && TRANSITION.get().is_none());
            SendMessageW(button, BM_CLICK, 0, 0);
            assert!(VIEW.get() == View::More);
            settle();
            // Moving beyond both views still dismisses the island.
            check_at(
                hwnd,
                Some(POINT {
                    x: tile.left,
                    y: tile.top - 1000,
                }),
            );
            settle();
            assert_frame(false);
            SendMessageW(button, WM_MOUSEMOVE, 0, 0);
            settle();
            assert_frame(true);
            SendMessageW(hwnd, WM_ENTERMENULOOP, 1, 0);
            SendMessageW(button, WM_MOUSELEAVE, 0, 0);
            SendMessageW(hwnd, WM_TIMER, LEAVE_TIMER, 0);
            assert_frame(true);
            SendMessageW(hwnd, WM_EXITMENULOOP, 1, 0);
            SendMessageW(hwnd, WM_TIMER, LEAVE_TIMER, 0);
            SendMessageW(hwnd, WM_MOUSEMOVE, 0, 0);
            settle();
            assert_frame(true);
            SendMessageW(hwnd, WM_MOUSELEAVE, 0, 0);
            SendMessageW(hwnd, WM_TIMER, LEAVE_TIMER, 0);
            std::thread::sleep(std::time::Duration::from_millis(180));
            refresh();
            assert_frame(false);
            APPEARANCE_STYLE.set(crate::config::Appearance {
                edge_margin: Some(32),
                bar_padding: 20,
                item_gap: 14,
                corner_radius: 0,
                ..Default::default()
            });
            SendMessageW(button, WM_MOUSEMOVE, 0, 0);
            settle();
            let bounds = |window| {
                let mut rect = RECT::default();
                assert_ne!(GetWindowRect(window, &mut rect), 0);
                rect
            };
            let frame = bounds(hwnd);
            let first = bounds(GetDlgItem(hwnd, FIRST));
            let second = bounds(GetDlgItem(hwnd, FIRST + 1));
            assert_eq!(layout::work_area().bottom - frame.bottom, 32);
            assert_eq!(first.left - frame.left, 20);
            assert_eq!(second.left - first.right, 14);
            assert_eq!(
                capsule::Outline::current(frame.right - frame.left, frame.bottom - frame.top)
                    .diameter,
                0
            );
            let tab = POINT {
                x: (frame.left + frame.right) / 2,
                y: frame.bottom + 31,
            };
            check_at(hwnd, Some(tab));
            assert!(EXPANDED.get() && TRANSITION.get().is_none());
            check_at(
                hwnd,
                Some(POINT {
                    x: frame.left - 10,
                    y: tab.y,
                }),
            );
            settle();
            assert_frame(false);
            DestroyWindow(hwnd);
        }
    }
}
