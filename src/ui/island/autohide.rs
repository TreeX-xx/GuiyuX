use super::*;
use windows_sys::Win32::System::SystemServices::{MK_LBUTTON, MK_MBUTTON, MK_RBUTTON};

pub(super) const LEAVE_TIMER: usize = 5;
thread_local! {
    static WAIT_FOR_EXIT: Cell<bool> = const { Cell::new(false) };
    pub(super) static IN_MENU: Cell<bool> = const { Cell::new(false) };
}

fn pointer_target(hwnd: HWND) -> Option<HWND> {
    unsafe {
        let mut point = POINT::default();
        if GetCursorPos(&mut point) == 0 {
            return None;
        }
        let hit = WindowFromPoint(point);
        (hit == hwnd || IsChild(hwnd, hit) != 0).then_some(hit)
    }
}

pub(super) fn reset(hwnd: HWND) {
    WAIT_FOR_EXIT.set(false);
    unsafe { KillTimer(hwnd, LEAVE_TIMER) };
}

pub(super) fn dismiss(hwnd: HWND) {
    if nav::dock().mode == DockMode::AutoHide {
        // A click on the collapse arrow or Esc must work even over the tab.
        WAIT_FOR_EXIT.set(pointer_target(hwnd).is_some());
        if WAIT_FOR_EXIT.get() {
            check_later();
        }
    }
}

pub(super) fn pointer_move(hwnd: HWND, keys: WPARAM) {
    if nav::dock().mode != DockMode::AutoHide {
        return;
    }
    motion::track_leave(hwnd);
    if WAIT_FOR_EXIT.get()
        || IN_MENU.get()
        || drag::active()
        || keys & (MK_LBUTTON | MK_RBUTTON | MK_MBUTTON) as usize != 0
    {
        return;
    }
    if !EXPANDED.get() {
        motion::target(HEADER);
    } else if TRANSITION.get().is_some_and(|m| m.closing) {
        expand_home();
    }
}

pub(super) fn pointer_left() {
    let hwnd = ISLAND.get();
    if nav::dock().mode != DockMode::AutoHide {
        return;
    }
    if pointer_target(hwnd).is_none() && !IN_MENU.get() && !drag::active() {
        WAIT_FOR_EXIT.set(false);
        if !EXPANDED.get() {
            motion::target(0);
        }
    }
    check_later();
}

// Child-to-child leaves, a resized view, and the end of a menu or a drag all
// get one delayed hit test. A stationary island needs no polling timer.
pub(super) fn check_later() {
    if nav::dock().mode == DockMode::AutoHide
        && (EXPANDED.get() || WAIT_FOR_EXIT.get() || motion::strength(HEADER) > 0.)
    {
        unsafe { SetTimer(ISLAND.get(), LEAVE_TIMER, 260, None) };
    }
}

pub(super) fn check(hwnd: HWND) {
    unsafe { KillTimer(hwnd, LEAVE_TIMER) };
    if nav::dock().mode != DockMode::AutoHide || IN_MENU.get() || drag::active() {
        return;
    }
    if let Some(hit) = pointer_target(hwnd) {
        // Resizing may have moved a stationary pointer from a button to the
        // background (or vice versa), without a mouse move on its new HWND.
        motion::track_leave(hit);
    } else {
        WAIT_FOR_EXIT.set(false);
        if EXPANDED.get() {
            collapse(hwnd);
        } else {
            motion::target(0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hover_reveals_the_capsule_and_the_menu_can_hold_it_open() {
        let _desktop = crate::test_desktop::TestDesktop::enter().unwrap();
        let _apartment = crate::shell::Apartment::open().unwrap();
        crate::ui::register("GuiyuX.Island", Some(procedure)).unwrap();
        let mut settings = Settings::default();
        open(settings.clone()).unwrap();
        let hwnd = ISLAND.get();
        unsafe {
            // Exercise actual parent/child handlers with the pointer guaranteed outside.
            ShowWindow(hwnd, SW_HIDE);
            let button = GetDlgItem(hwnd, HEADER);
            SendMessageW(button, WM_MOUSEMOVE, 0, 0);
            assert!(!EXPANDED.get());
            motion::clear_hover();
            settings.dock.mode = DockMode::AutoHide;
            settings.dock.edge = DockEdge::Bottom;
            nav::set_data(settings);
            SendMessageW(button, WM_MOUSEMOVE, 0, 0);
            assert!(!EXPANDED.get());
            let settle = || {
                std::thread::sleep(std::time::Duration::from_millis(
                    u64::from(capsule::STYLE.slide_ms) + 10,
                ));
                refresh();
                surface::present(hwnd).unwrap();
                let mut frame = RECT::default();
                assert_ne!(GetWindowRect(hwnd, &mut frame), 0);
                assert_eq!(frame.bottom, layout::work_area().bottom);
                assert_eq!(frame.right - frame.left, 120);
                assert_eq!(
                    GetWindowLongW(GetDlgItem(hwnd, FIRST), GWL_STYLE) as u32 & WS_VISIBLE,
                    0
                );
                frame.bottom - frame.top
            };
            assert_eq!(settle(), 37);
            SendMessageW(hwnd, WM_ENTERMENULOOP, 1, 0);
            SendMessageW(button, WM_MOUSELEAVE, 0, 0);
            SendMessageW(hwnd, WM_TIMER, LEAVE_TIMER, 0);
            assert_eq!(settle(), 37);
            SendMessageW(hwnd, WM_EXITMENULOOP, 1, 0);
            SendMessageW(hwnd, WM_TIMER, LEAVE_TIMER, 0);
            assert_eq!(settle(), 5);
            SendMessageW(hwnd, WM_MOUSEMOVE, 0, 0);
            assert_eq!(settle(), 37);
            assert!(!EXPANDED.get());
            DestroyWindow(hwnd);
        }
    }
}
