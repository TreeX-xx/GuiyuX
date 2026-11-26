use super::*;
use std::{
    ffi::OsString,
    os::windows::ffi::{OsStrExt, OsStringExt},
    path::PathBuf,
};
use windows_sys::Win32::UI::Shell::*;

use std::cell::RefCell;
use windows_sys::Win32::UI::Controls::{DRAWITEMSTRUCT, ODS_FOCUS, ODS_SELECTED, WM_MOUSELEAVE};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent,
};

mod actions;
pub(super) mod art;
mod autohide;
mod backdrop;
mod capsule;
mod drag;
mod item_icon;
mod layout;
mod nav;
use crate::collection::{self, Group, Placement, Shortcut};
use crate::config::{Dock, DockEdge, DockMode, Settings};
use collection::BAR_CAPACITY;
use nav::{VIEW, View};
pub(super) mod motion;
mod surface;
mod tiles;
mod tips;

const PRESENT: u32 = WM_APP + 1;
const ISLAND_RADIUS: i32 = 12;
const RESIZE_TIMER: usize = 3;
const HOVER_TIMER: usize = 2;
const HEADER: i32 = 101;
const FIRST: i32 = 200;
const BAR_SLOTS: usize = BAR_CAPACITY + 1;
use tiles::PAGE_SIZE as GRID_PAGE;
const SLOTS: usize = BAR_SLOTS + GRID_PAGE;
const PREV: i32 = 300;
const NEXT: i32 = 301;
const BUTTONS: [i32; SLOTS + 3] = {
    let mut ids = [0; SLOTS + 3];
    ids[0] = HEADER;
    ids[1] = PREV;
    ids[2] = NEXT;
    let mut slot = 0;
    while slot < SLOTS {
        ids[slot + 3] = FIRST + slot as i32;
        slot += 1;
    }
    ids
};
#[derive(Clone, Copy)]
struct Resize {
    started: std::time::Instant,
    from: (i32, i32),
    alpha: f32,
    closing: bool,
}

thread_local! {
    static KEYBOARD:Cell<bool>=const{Cell::new(false)};
    static TRANSITION: Cell<Option<Resize>> = const { Cell::new(None) };
    static CONTENT_ALPHA: Cell<u32> = const { Cell::new(255) };
    static FRAME_RECT: Cell<RECT> = const { Cell::new(RECT { left: 0, top: 0, right: 0, bottom: 0 }) };
    static PENDING: Cell<bool> = const { Cell::new(false) };
    static PAGE: Cell<usize> = const { Cell::new(0) };
    static FEEDBACK: Cell<Option<&'static str>> = const { Cell::new(None) };
}

pub(super) fn input_mode(msg: u32) {
    let keyboard = match msg {
        WM_KEYDOWN | WM_SYSKEYDOWN => true,
        WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN | WM_POINTERDOWN => false,
        _ => return,
    };
    if KEYBOARD.replace(keyboard) != keyboard {
        schedule_frame(ISLAND.get());
    }
}

pub(super) fn grid_key(msg: &MSG) -> bool {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        SetFocus, VK_DOWN, VK_LEFT, VK_NEXT, VK_PRIOR, VK_RIGHT, VK_UP,
    };
    if msg.message != WM_KEYDOWN || !EXPANDED.get() || VIEW.get() == View::Home {
        return false;
    }
    let first = nav::content_first();
    let id = unsafe { GetDlgCtrlID(msg.hwnd) };
    let count = nav::len();
    if !(first..first + GRID_PAGE as i32).contains(&id) || count == 0 {
        return false;
    }
    let index = PAGE.get() * GRID_PAGE + (id - first) as usize;
    let next = match msg.wParam as u16 {
        VK_LEFT => index.saturating_sub(1),
        VK_RIGHT => (index + 1).min(count - 1),
        VK_UP => index.checked_sub(tiles::COLUMNS).unwrap_or(index),
        VK_DOWN if index / tiles::COLUMNS < (count - 1) / tiles::COLUMNS => {
            (index + tiles::COLUMNS).min(count - 1)
        }
        VK_DOWN => index,
        VK_PRIOR => {
            if PAGE.get() > 0 {
                index - GRID_PAGE
            } else {
                index
            }
        }
        VK_NEXT => {
            if (PAGE.get() + 1) * GRID_PAGE < count {
                (index + GRID_PAGE).min(count - 1)
            } else {
                index
            }
        }
        _ => return false,
    };
    if PAGE.replace(next / GRID_PAGE) != next / GRID_PAGE {
        tips::dismiss();
        motion::clear_hover();
        refresh();
    }
    unsafe { SetFocus(GetDlgItem(ISLAND.get(), first + (next % GRID_PAGE) as i32)) };
    true
}

pub(super) fn open(settings: Settings) -> io::Result<()> {
    let hwnd = window(
        "GuiyuX.Island",
        "GuiyuX",
        WS_POPUP | WS_CLIPCHILDREN,
        WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_LAYERED,
        null_mut(),
        0,
    )?;
    ISLAND.set(hwnd);
    unsafe {
        DragAcceptFiles(hwnd, 1);
    }
    for id in BUTTONS {
        control(hwnd, "BUTTON", WS_TABSTOP | BS_OWNERDRAW as u32, id, [0; 4])?;
        if unsafe { SetWindowSubclass(GetDlgItem(hwnd, id), Some(motion::input), id as usize, 0) }
            == 0
        {
            return Err(io::Error::other("Could not attach island input handler"));
        }
    }
    tips::install(hwnd)?;
    drag::register(hwnd)?;
    nav::set_data(settings);
    refresh();
    unsafe {
        ShowWindow(hwnd, SW_SHOWNOACTIVATE);
    }
    Ok(())
}

pub(super) fn collection_changed() -> io::Result<()> {
    let old = nav::dock();
    nav::reload()?;
    if nav::dock() != old {
        let hwnd = ISLAND.get();
        tips::dismiss();
        autohide::reset(hwnd);
        unsafe { KillTimer(hwnd, RESIZE_TIMER) };
        TRANSITION.set(None);
        FEEDBACK.set(None);
        finish_collapse();
    }
    refresh();
    Ok(())
}

pub(super) fn show_group(group: Group) -> io::Result<()> {
    nav::reload()?;
    EXPANDED.set(true);
    VIEW.set(View::Home);
    nav::show(View::Group(group));
    unsafe {
        SetForegroundWindow(ISLAND.get());
    }
    Ok(())
}

fn finish_collapse() {
    // The trailing arrow and the idle capsule share a native button, not a hover state.
    motion::clear_hover();
    KEYBOARD.set(false);
    EXPANDED.set(false);
    VIEW.set(View::Home);
    PAGE.set(0);
}

fn expand_home() {
    EXPANDED.set(true);
    nav::show(View::Home);
    autohide::check_later();
}

fn start_transition(hwnd: HWND, closing: bool, keep_content: bool) {
    unsafe {
        if motion::animation_enabled() && SetTimer(hwnd, RESIZE_TIMER, 15, None) != 0 {
            let mut rect = RECT::default();
            GetWindowRect(hwnd, &mut rect);
            TRANSITION.set(Some(Resize {
                started: std::time::Instant::now(),
                from: (rect.right - rect.left, rect.bottom - rect.top),
                alpha: if keep_content {
                    1.0
                } else if closing || TRANSITION.get().is_some_and(|m| m.closing) {
                    CONTENT_ALPHA.get() as f32 / 255.0
                } else {
                    0.0
                },
                closing,
            }));
        } else {
            TRANSITION.set(None);
            KillTimer(hwnd, RESIZE_TIMER);
            if closing {
                finish_collapse();
            }
        }
    }
}

fn collapse(hwnd: HWND) {
    if !EXPANDED.get() || TRANSITION.get().is_some_and(|m| m.closing) {
        return;
    }
    autohide::dismiss(hwnd);
    tips::dismiss();
    start_transition(hwnd, true, false);
    refresh();
}

fn place(hwnd: HWND, id: i32, name: &str, rect: [i32; 4], visible: bool) {
    unsafe {
        let child = GetDlgItem(hwnd, id);
        if visible {
            caption(child, name);
            MoveWindow(child, rect[0], rect[1], rect[2], rect[3], 0);
        }
        ShowWindow(child, if visible { SW_SHOWNA } else { SW_HIDE });
    }
}

pub(super) fn refresh() {
    let hwnd = ISLAND.get();
    let transition = TRANSITION.get();
    let progress = transition.map_or(1.0, |m| {
        let duration = if m.closing { 0.16 } else { 0.20 };
        (m.started.elapsed().as_secs_f32() / duration).min(1.0)
    });
    if progress == 1.0
        && let Some(m) = transition
    {
        TRANSITION.set(None);
        unsafe {
            KillTimer(hwnd, RESIZE_TIMER);
        }
        if m.closing {
            finish_collapse();
        }
    }
    let hover = motion::strength(HEADER);
    let expanded = EXPANDED.get();
    let text = crate::language().text();
    {
        let view = VIEW.get();
        let count = nav::len();
        let size = GRID_PAGE;
        let start = PAGE.get() * size;
        let rows = count.saturating_sub(start).min(size);
        let paging = count > size;
        let category_bar = expanded && nav::toolbar();
        let board = expanded && view == View::More;
        let dock = nav::dock();
        let bar = layout::Toolbar::new(dock.edge, nav::bar_len());
        let feedback = FEEDBACK.get().is_some();
        let footer = paging || feedback;
        let (w, h) = if category_bar {
            bar.size(board, count, footer)
        } else if expanded {
            (40 + tiles::width(count), tiles::height(count, footer))
        } else {
            dock.idle_size(hover, feedback)
        };
        let (w, h) = if let Some(m) = TRANSITION.get() {
            let t = progress;
            let (w, h, blend, alpha) = if m.closing {
                let eased = t * t * (3.0 - 2.0 * t);
                let (w, h) = dock.idle_size(0., feedback);
                (w, h, eased, m.alpha * (1.0 - t * 1.6).max(0.0))
            } else {
                (
                    w,
                    h,
                    1.0 - (1.0 - t).powi(3),
                    m.alpha + (1.0 - m.alpha) * ((t - 0.18) / 0.82).clamp(0.0, 1.0),
                )
            };
            CONTENT_ALPHA.set((255.0 * alpha).round() as u32);
            let between = |from: i32, to: i32| from + ((to - from) as f32 * blend).round() as i32;
            (between(m.from.0, w), between(m.from.1, h))
        } else {
            CONTENT_ALPHA.set(255);
            (w, h)
        };
        // Commit bounds with the finished layered bitmap. Resizing the live
        // window first can expose its previous frame around the moving edge.
        let frame = dock.frame(layout::work_area(), w, h);
        FRAME_RECT.set(frame);
        let (w, h) = (frame.right - frame.left, frame.bottom - frame.top);
        let (strip, body) = dock.edge.split(w, h);
        let body = if category_bar { body } else { area(0, 0, w, h) };
        place(
            hwnd,
            HEADER,
            if category_bar {
                text.collapse
            } else if expanded {
                text.back
            } else {
                text.categories
            },
            if category_bar {
                bar.slot(strip, bar.length - 38, 32)
            } else if expanded {
                [4, 8, 34, 44]
            } else {
                [0, 0, w, h]
            },
            true,
        );
        for slot in 0..SLOTS {
            let id = FIRST + slot as i32;
            let row = nav::button_row(id);
            let rect = if category_bar && slot < BAR_SLOTS {
                bar.slot(strip, 8 + slot as i32 * layout::CELL, layout::CELL)
            } else if board && slot >= BAR_SLOTS {
                tiles::cell(body, count, slot - BAR_SLOTS)
            } else {
                tiles::cell(area(40, 0, w, h), count, slot)
            };
            place(
                hwnd,
                id,
                row.as_ref()
                    .map(nav::Row::full_name)
                    .as_deref()
                    .unwrap_or(""),
                rect,
                expanded && row.is_some(),
            );
        }
        place(
            hwnd,
            PREV,
            "‹",
            [body.left + 12, body.bottom - 38, 48, 32],
            expanded && paging && start > 0,
        );
        place(
            hwnd,
            NEXT,
            "›",
            [body.right - 60, body.bottom - 38, 48, 32],
            expanded && paging && start + rows < count,
        );
        schedule_frame(hwnd);
        if expanded && progress == 1.0 && transition.is_some_and(|m| !m.closing) {
            motion::restore_pointer();
        }
    }
}

fn draw(d: &DRAWITEMSTRUCT, ink: HDC) {
    let id = d.CtlID as i32;
    let r = d.rcItem;
    let dc = d.hDC;
    let expanded = EXPANDED.get();
    let focused = KEYBOARD.get()
        && (d.itemState & ODS_FOCUS != 0
            || unsafe {
                windows_sys::Win32::UI::Input::KeyboardAndMouse::GetFocus() == d.hwndItem
            });
    let pressed = d.itemState & ODS_SELECTED != 0;
    let area = |left, top, right, bottom| RECT {
        left,
        top,
        right,
        bottom,
    };
    let hover = if drag::highlighted(id) {
        1.0
    } else {
        motion::strength(id)
    };
    if id == HEADER && !expanded {
        // The auto-hide capsule is composited as one clipped surface.
        if capsule::active() {
            return;
        }
        let shade = if pressed {
            0x484139
        } else if focused {
            0x423d38
        } else {
            art::mix(BG, 0x34302d, hover)
        };
        fill(dc, &r, shade);
        if let Some(message) = FEEDBACK.get() {
            island_text(ink, r, message, 16, INK, DT_CENTER);
        }
        return;
    }

    let tilt = motion::icon_tilt(id);
    let row = nav::button_row(id);
    let file = match row.as_ref().map(|row| &row.action) {
        Some(nav::Action::Open(Shortcut::Item(path))) => Some(path.as_path()),
        _ => None,
    };
    let symbol = match id {
        HEADER if nav::toolbar() => Some(match nav::dock().edge {
            DockEdge::Top => art::Symbol::Up,
            DockEdge::Bottom => art::Symbol::Down,
            DockEdge::Left => art::Symbol::Left,
            DockEdge::Right => art::Symbol::Right,
        }),
        HEADER | PREV => Some(art::Symbol::Left),
        NEXT => Some(art::Symbol::Right),
        _ => None,
    };
    if let Some(symbol) = symbol {
        art::draw(
            dc,
            r,
            art::Icon { symbol, file },
            hover,
            focused,
            pressed,
            tilt,
        );
        return;
    }
    if let Some(row) = row.as_ref() {
        let shade = art::tile_background(dc, r, hover, focused, pressed);
        let label_height = 18;
        let icon_side = (r.bottom - label_height - 2).min(34).min(r.right).max(0);
        let x = (r.right - icon_side) / 2;
        let y = (r.bottom - label_height - icon_side) / 2;
        art::emblem(
            dc,
            area(x, y, x + icon_side, y + icon_side),
            art::Icon {
                symbol: row.symbol,
                file,
            },
            shade,
            SILVER,
            tilt,
        );
        island_text(
            ink,
            area(2, r.bottom - label_height, r.right - 2, r.bottom),
            &row.title,
            12,
            INK,
            DT_CENTER,
        );
    }
}

fn drop_files(handle: HDROP) -> Vec<PathBuf> {
    let mut items = Vec::new();
    unsafe {
        for i in 0..DragQueryFileW(handle, u32::MAX, null_mut(), 0) {
            let len = DragQueryFileW(handle, i, null_mut(), 0);
            let mut buf = vec![0; len as usize + 1];
            let n = DragQueryFileW(handle, i, buf.as_mut_ptr(), len + 1);
            if n > 0 {
                items.push(PathBuf::from(OsString::from_wide(&buf[..n as usize])));
            }
        }
    }
    items
}
fn receive_drop(handle: HDROP) -> io::Result<()> {
    let items = drop_files(handle);
    let mut at = POINT::default();
    unsafe {
        DragQueryPoint(handle, &mut at);
        ClientToScreen(ISLAND.get(), &mut at);
        DragFinish(handle);
    }
    drag::place_paths(&items, drag::placement(at))
}

fn open_item(hwnd: HWND, item: &std::path::Path) -> io::Result<()> {
    unsafe {
        let path: Vec<_> = item.as_os_str().encode_wide().chain(Some(0)).collect();
        let result = ShellExecuteW(
            hwnd,
            wide("open").as_ptr(),
            path.as_ptr(),
            null_mut(),
            null_mut(),
            SW_SHOWNORMAL,
        );
        if result as isize <= 32 {
            return Err(io::Error::other(format!(
                "ShellExecuteW ({})",
                result as usize
            )));
        }
    }
    Ok(())
}

fn schedule_frame(hwnd: HWND) {
    if !PENDING.replace(true) {
        unsafe {
            PostMessageW(hwnd, PRESENT, 0, 0);
        }
    }
}

pub(super) fn appearance_changed() {
    let hwnd = ISLAND.get();
    if !hwnd.is_null() {
        schedule_frame(hwnd);
    }
}

fn paint_background(dc: HDC, ink: HDC, mut r: RECT) {
    fill(dc, &r, BG);
    if EXPANDED.get() && nav::toolbar() {
        let edge = nav::dock().edge;
        let (strip, body) = edge.split(r.right, r.bottom);
        if VIEW.get() == View::More {
            let divider = match edge {
                DockEdge::Top => area(12, strip.bottom - 1, r.right - 12, strip.bottom),
                DockEdge::Bottom => area(12, strip.top, r.right - 12, strip.top + 1),
                DockEdge::Left => area(strip.right - 1, 12, strip.right, r.bottom - 12),
                DockEdge::Right => area(strip.left, 12, strip.left + 1, r.bottom - 12),
            };
            fill(dc, &divider, EDGE);
        }
        r = body;
    }
    if EXPANDED.get() && VIEW.get() == View::Home {
        if let Some(message) = FEEDBACK.get() {
            island_text(ink, r, message, 14, SILVER, DT_CENTER);
        }
    } else if EXPANDED.get() {
        let n = nav::len();
        if n == 0 {
            if !nav::toolbar() {
                r.left = 40;
            }
            island_text(
                ink,
                r,
                crate::language().text().empty,
                15,
                SILVER,
                DT_CENTER,
            );
        }
        if let Some(message) = FEEDBACK.get() {
            r.top = r.bottom - 40;
            island_text(ink, r, message, 14, SILVER, DT_CENTER);
        } else if n > GRID_PAGE {
            r.top = r.bottom - 40;
            island_text(
                ink,
                r,
                &format!("{} / {}", PAGE.get() + 1, n.div_ceil(GRID_PAGE)),
                13,
                MUTED,
                DT_CENTER,
            );
        }
    }
}

pub(super) unsafe extern "system" fn procedure(
    hwnd: HWND,
    msg: u32,
    wp: WPARAM,
    lp: LPARAM,
) -> LRESULT {
    if drag::internal_move(hwnd, msg, lp) {
        return 0;
    }
    match msg {
        settings::DISPATCH => {
            settings::dispatch(wp);
            0
        }
        WM_NOTIFYFORMAT => NFR_UNICODE as isize,
        WM_NOTIFY => {
            tips::notify(lp).unwrap_or_else(|| unsafe { DefWindowProcW(hwnd, msg, wp, lp) })
        }
        WM_MEASUREITEM if menu::measure(lp) => 1,
        WM_MENUSELECT => {
            menu::select(wp, lp);
            0
        }
        WM_DRAWITEM => {
            let d = unsafe { &*(lp as *const DRAWITEMSTRUCT) };
            if menu::draw(d) {
                return 1;
            }
            draw(d, d.hDC);
            if unsafe { GetObjectType(d.hDC) } != OBJ_MEMDC as u32 {
                schedule_frame(hwnd);
            }
            1
        }
        PRESENT => {
            PENDING.set(false);
            if let Err(error) = surface::present(hwnd) {
                report(&error);
                unsafe {
                    DestroyWindow(hwnd);
                }
            }
            0
        }
        WM_WINDOWPOSCHANGED => {
            let result = unsafe { DefWindowProcW(hwnd, msg, wp, lp) };
            let pos = unsafe { &*(lp as *const WINDOWPOS) };
            if pos.flags & (SWP_HIDEWINDOW | SWP_SHOWWINDOW) != 0 || pos.flags & SWP_NOZORDER == 0 {
                surface::sync_backdrop(hwnd);
            }
            result
        }
        WM_DISPLAYCHANGE | WM_SETTINGCHANGE | WM_THEMECHANGED | WM_DWMCOMPOSITIONCHANGED => {
            if msg == WM_DWMCOMPOSITIONCHANGED {
                backdrop::reset();
            }
            if matches!(msg, WM_DISPLAYCHANGE | WM_SETTINGCHANGE) && !ISLAND.get().is_null() {
                refresh();
            } else {
                appearance_changed();
            }
            unsafe { DefWindowProcW(hwnd, msg, wp, lp) }
        }
        WM_ERASEBKGND => 1,
        WM_PAINT => unsafe {
            let mut ps = PAINTSTRUCT::default();
            BeginPaint(hwnd, &mut ps);
            schedule_frame(hwnd);
            EndPaint(hwnd, &ps);
            0
        },
        WM_CONTEXTMENU => {
            tips::dismiss();
            if let Err(error) = actions::context(hwnd, wp, lp) {
                report(&error);
            }
            0
        }
        WM_ENTERMENULOOP | WM_EXITMENULOOP => {
            autohide::IN_MENU.set(msg == WM_ENTERMENULOOP);
            if msg == WM_EXITMENULOOP {
                autohide::check_later();
            }
            unsafe { DefWindowProcW(hwnd, msg, wp, lp) }
        }
        WM_MOUSEMOVE if nav::dock().mode == DockMode::AutoHide => {
            autohide::pointer_move(hwnd, wp);
            0
        }
        WM_MOUSELEAVE => {
            autohide::pointer_left();
            0
        }
        WM_DROPFILES => {
            if let Err(e) = receive_drop(wp as HDROP) {
                report(&e);
            }
            0
        }
        WM_TIMER if wp == drag::HOVER_OPEN => {
            drag::hover_open();
            0
        }
        WM_TIMER if wp == RESIZE_TIMER => {
            refresh();
            0
        }
        WM_TIMER if wp == autohide::LEAVE_TIMER => {
            autohide::check(hwnd);
            0
        }
        WM_TIMER if wp == HOVER_TIMER => {
            motion::tick(hwnd);
            0
        }
        WM_TIMER if wp == 1 => {
            unsafe {
                KillTimer(hwnd, 1);
            }
            FEEDBACK.set(None);
            refresh();
            0
        }
        WM_ACTIVATE if wp & 0xffff == 0 => {
            if !drag::active() && !autohide::IN_MENU.get() {
                collapse(hwnd);
            }
            0
        }
        WM_COMMAND => {
            tips::dismiss();
            let id = (wp & 0xffff) as i32;
            if lp == 0
                && let Some(result) = menu::command(hwnd, id as u32)
            {
                if let Err(error) = result {
                    report(&error);
                }
                return 0;
            }
            if TRANSITION.get().is_some_and(|m| m.closing) {
                if id != IDCANCEL {
                    VIEW.set(View::Home);
                    PAGE.set(0);
                    nav::rebuild();
                    start_transition(hwnd, false, false);
                    refresh();
                }
                return 0;
            }
            match id {
                HEADER => {
                    if EXPANDED.get() {
                        if nav::toolbar() {
                            collapse(hwnd);
                        } else {
                            nav::back();
                        }
                        return 0;
                    }
                    if let Err(e) = nav::reload() {
                        report(&e);
                        return 0;
                    }
                    unsafe { SetForegroundWindow(hwnd) };
                    expand_home();
                }
                PREV | NEXT => {
                    PAGE.set(if id == PREV {
                        PAGE.get().saturating_sub(1)
                    } else {
                        (PAGE.get() + 1).min(nav::len().saturating_sub(1) / GRID_PAGE)
                    });
                    refresh();
                    unsafe {
                        windows_sys::Win32::UI::Input::KeyboardAndMouse::SetFocus(GetDlgItem(
                            hwnd,
                            nav::content_first(),
                        ));
                    }
                }
                IDCANCEL if drag::active() => drag::cancel(),
                IDCANCEL => nav::back(),
                id if (FIRST..FIRST + SLOTS as i32).contains(&id) => {
                    if let Err(e) = nav::activate(hwnd, id) {
                        report(&e);
                    }
                }
                _ => {}
            }
            0
        }
        WM_DESTROY => {
            settings::shutdown();
            ISLAND.set(null_mut());
            backdrop::reset();
            drag::unregister(hwnd);
            unsafe {
                PostQuitMessage(0);
            }
            0
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wp, lp) },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mouse_focus_does_not_light_the_collapse_button() {
        EXPANDED.set(true);
        VIEW.set(View::Home);
        let mut bitmap = Bitmap::new(32, 40).unwrap();
        let d = DRAWITEMSTRUCT {
            CtlID: HEADER as u32,
            itemState: ODS_FOCUS,
            hDC: bitmap.dc,
            rcItem: area(0, 0, 32, 40),
            ..Default::default()
        };
        KEYBOARD.set(false);
        draw(&d, d.hDC);
        let idle = bitmap.pixels().to_vec();
        assert_eq!(idle[20 * 32 + 4], idle[0]);
        assert_eq!(idle[12 * 32 + 8], idle[0]); // no enclosing tile under the chevron
        KEYBOARD.set(true);
        draw(&d, d.hDC);
        assert_ne!(bitmap.pixels()[20 * 32 + 4], idle[0]);
        KEYBOARD.set(false);
        draw(&d, d.hDC);
        assert_eq!(bitmap.pixels(), idle.as_slice());
    }
}
