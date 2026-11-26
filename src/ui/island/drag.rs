use super::*;
use nav::{Action, View};
use windows::Win32::{
    Foundation::{HWND as ComHwnd, POINTL},
    System::{Com::IDataObject, Ole::*, SystemServices::MODIFIERKEYS_FLAGS},
    UI::Shell::{IShellItemArray, SHCreateShellItemArrayFromDataObject, SIGDN_FILESYSPATH},
};
use windows::core::{Ref, Result as ComResult, implement};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetCapture, ReleaseCapture, SetCapture};

pub(super) const HOVER_OPEN: usize = 4;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Travel {
    Expand,
    More,
    Back,
    Page(i32),
}
struct Hit {
    place: Placement,
    id: i32,
    line: Option<RECT>,
    travel: Option<Travel>,
}
impl Default for Hit {
    fn default() -> Self {
        Self {
            place: Placement::Collect,
            id: 0,
            line: None,
            travel: None,
        }
    }
}
thread_local! {
    static EXTERNAL: Cell<bool> = const { Cell::new(false) };
    static PRESS: RefCell<Option<(HWND,POINT,Shortcut)>> = const { RefCell::new(None) };
    static INTERNAL: RefCell<Option<Shortcut>> = const { RefCell::new(None) };
    static HIGHLIGHT: Cell<i32> = const {Cell::new(0)};
    static LINE: Cell<Option<RECT>> = const {Cell::new(None)};
    static AUTO_EXPANDED: Cell<bool> = const {Cell::new(false)};
    static TRAVEL: Cell<Option<Travel>> = const {Cell::new(None)};
}

fn hit(point: POINT) -> Hit {
    if !EXPANDED.get() {
        return Hit {
            travel: Some(Travel::Expand),
            ..Hit::default()
        };
    }
    let hwnd = ISLAND.get();
    for id in BUTTONS {
        let mut r = RECT::default();
        unsafe {
            let button = GetDlgItem(hwnd, id);
            if IsWindowVisible(button) == 0
                || GetWindowRect(button, &mut r) == 0
                || PtInRect(&r, point) == 0
            {
                continue;
            }
        }
        if matches!(id, HEADER | PREV | NEXT) {
            let travel = match id {
                HEADER if VIEW.get() != View::Home => Some(Travel::Back),
                PREV | NEXT => Some(Travel::Page(id)),
                _ => None,
            };
            return Hit {
                travel,
                ..Hit::default()
            };
        }
        let Some(row) = nav::button_row(id) else {
            continue;
        };
        if nav::bar_slot(id) {
            let slot = (id - FIRST) as usize;
            let after = point.x >= r.right - 8;
            if after
                || point.x < r.left + 8
                || matches!(row.action, Action::Open(Shortcut::Item(_)))
            {
                let edge = if after { r.right - 3 } else { r.left + 1 };
                return Hit {
                    place: Placement::Pin((slot + usize::from(after)).min(BAR_CAPACITY)),
                    line: Some(area(edge, r.top + 7, edge + 2, r.bottom - 7)),
                    ..Hit::default()
                };
            }
        }
        return match row.action {
            Action::More => Hit {
                id,
                travel: (VIEW.get() == View::Home).then_some(Travel::More),
                ..Hit::default()
            },
            Action::Open(Shortcut::Group(g)) => Hit {
                id,
                place: Placement::InGroup(g, None),
                ..Hit::default()
            },
            Action::Open(Shortcut::Item(path)) => {
                if let View::Group(g) = VIEW.get() {
                    let after = point.x >= (r.left + r.right) / 2;
                    let index = PAGE.get() * GRID_PAGE + (id - FIRST) as usize;
                    let before = if after {
                        nav::row(index + 1).and_then(|r| match r.action {
                            Action::Open(Shortcut::Item(p)) => Some(p),
                            _ => None,
                        })
                    } else {
                        Some(path)
                    };
                    let x = if after { r.right - 2 } else { r.left };
                    Hit {
                        place: Placement::InGroup(g, before),
                        line: Some(area(x, r.top + 4, x + 2, r.bottom - 4)),
                        ..Hit::default()
                    }
                } else {
                    Hit::default()
                }
            }
        };
    }
    if let View::Group(g) = VIEW.get() {
        return Hit {
            place: Placement::InGroup(g, None),
            ..Hit::default()
        };
    }
    Hit::default()
}

pub(super) fn placement(at: POINT) -> Placement {
    hit(at).place
}
pub(super) fn highlighted(id: i32) -> bool {
    id != 0 && HIGHLIGHT.get() == id
}
pub(super) fn active() -> bool {
    INTERNAL.with_borrow(Option::is_some) || EXTERNAL.get()
}
pub(super) fn paint_line(dc: HDC) {
    if let Some(r) = LINE.get() {
        let mut origin = POINT::default();
        unsafe {
            ClientToScreen(ISLAND.get(), &mut origin);
        }
        fill(
            dc,
            &area(
                r.left - origin.x,
                r.top - origin.y,
                r.right - origin.x,
                r.bottom - origin.y,
            ),
            SILVER,
        );
    }
}
fn over(point: POINT) {
    let h = hit(point);
    HIGHLIGHT.set(h.id);
    LINE.set(h.line);
    if TRAVEL.replace(h.travel) != h.travel {
        unsafe {
            KillTimer(ISLAND.get(), HOVER_OPEN);
            if h.travel.is_some() {
                SetTimer(ISLAND.get(), HOVER_OPEN, 420, None);
            }
        }
    }
    schedule_frame(ISLAND.get());
}
pub(super) fn hover_open() {
    let hwnd = ISLAND.get();
    unsafe {
        KillTimer(hwnd, HOVER_OPEN);
    }
    if !active() {
        return;
    }
    match TRAVEL.take() {
        Some(Travel::Expand) => {
            if let Err(e) = nav::reload() {
                report(&e);
                return;
            }
            AUTO_EXPANDED.set(true);
            EXPANDED.set(true);
            nav::show(View::Home);
        }
        Some(Travel::Back) => nav::back(),
        Some(Travel::More) => nav::show(View::More),
        Some(Travel::Page(id)) => unsafe {
            SendMessageW(hwnd, WM_COMMAND, id as usize, 0);
        },
        None => {}
    }
    LINE.set(None);
    HIGHLIGHT.set(0);
}
fn reset() {
    EXTERNAL.set(false);
    INTERNAL.take();
    PRESS.take();
    HIGHLIGHT.set(0);
    LINE.set(None);
    TRAVEL.set(None);
    unsafe {
        KillTimer(ISLAND.get(), HOVER_OPEN);
    }
    schedule_frame(ISLAND.get());
}
pub(super) fn cancel() {
    reset();
    unsafe {
        if GetCapture() == ISLAND.get() {
            ReleaseCapture();
        }
    }
}

pub(super) fn place_paths(paths: &[PathBuf], target: Placement) -> io::Result<()> {
    if paths.is_empty() {
        return Ok(());
    }
    let (_, changed) = collection::transaction(|s| s.place(paths, &target))?;
    collection_changed()?;
    settings::notify();
    let lang = crate::language();
    let message = if !changed {
        lang.text().duplicate
    } else {
        match target {
            Placement::Pin(_) => lang.pick("已固定到快捷栏", "Pinned to bar"),
            Placement::Collect if !nav::auto_sort() => lang.pick("已收纳到其他", "Saved to Other"),
            _ => lang.text().saved,
        }
    };
    FEEDBACK.set(Some(message));
    refresh();
    unsafe {
        SetTimer(ISLAND.get(), 1, 1500, None);
    }
    Ok(())
}

fn event_point(hwnd: HWND, lp: LPARAM) -> POINT {
    let mut p = POINT {
        x: lp as i16 as i32,
        y: (lp >> 16) as i16 as i32,
    };
    unsafe {
        ClientToScreen(hwnd, &mut p);
    }
    p
}
fn inside(p: POINT) -> bool {
    let mut r = RECT::default();
    unsafe {
        GetWindowRect(ISLAND.get(), &mut r);
        PtInRect(&r, p) != 0
    }
}
pub(super) fn button_input(hwnd: HWND, id: i32, msg: u32, wp: WPARAM, lp: LPARAM) -> bool {
    match msg {
        WM_LBUTTONDOWN => {
            if let Some(nav::Row {
                action: Action::Open(entry),
                ..
            }) = nav::button_row(id)
            {
                PRESS.replace(Some((hwnd, event_point(hwnd, lp), entry)));
            }
        }
        WM_LBUTTONUP | WM_CANCELMODE | WM_CAPTURECHANGED => {
            if PRESS.with_borrow(|p| p.as_ref().is_some_and(|(child, _, _)| *child == hwnd)) {
                PRESS.take();
            }
        }
        WM_MOUSEMOVE if wp & 1 != 0 => {
            let p = event_point(hwnd, lp);
            let candidate = PRESS.with_borrow(|source| {
                source.as_ref().and_then(|(child, down, key)| {
                    let moved = unsafe {
                        (p.x - down.x).abs() >= GetSystemMetrics(SM_CXDRAG)
                            || (p.y - down.y).abs() >= GetSystemMetrics(SM_CYDRAG)
                    };
                    (*child == hwnd && moved).then(|| key.clone())
                })
            });
            if let Some(key) = candidate {
                PRESS.take();
                INTERNAL.replace(Some(key));
                unsafe {
                    SendMessageW(hwnd, WM_CANCELMODE, 0, 0);
                    SetCapture(ISLAND.get());
                }
                over(p);
                return true;
            }
        }
        _ => {}
    }
    false
}
pub(super) fn internal_move(hwnd: HWND, msg: u32, lp: LPARAM) -> bool {
    if !INTERNAL.with_borrow(Option::is_some) {
        return false;
    }
    match msg {
        WM_MOUSEMOVE => {
            let p = event_point(hwnd, lp);
            if inside(p) {
                over(p);
            } else {
                HIGHLIGHT.set(0);
                LINE.set(None);
                TRAVEL.set(None);
                schedule_frame(ISLAND.get());
            }
        }
        WM_LBUTTONUP => {
            let key = INTERNAL.take().unwrap();
            let p = event_point(hwnd, lp);
            let target = placement(p);
            cancel();
            if inside(p) {
                let result = match (key, target) {
                    (key @ Shortcut::Group(_), Placement::Pin(at)) => {
                        collection::update(|s| s.pin(key, at)).and_then(|_| {
                            settings::notify();
                            collection_changed()
                        })
                    }
                    (Shortcut::Item(p), target) => place_paths(&[p], target),
                    _ => Ok(()),
                };
                if let Err(e) = result {
                    report(&e);
                }
            }
        }
        WM_CAPTURECHANGED | WM_CANCELMODE => reset(),
        _ => return false,
    }
    true
}

#[implement(IDropTarget)]
struct FileTarget;
fn format_for(id: u16) -> windows::Win32::System::Com::FORMATETC {
    use windows::Win32::System::Com::{DVASPECT_CONTENT, FORMATETC, TYMED_HGLOBAL};
    FORMATETC {
        cfFormat: id,
        dwAspect: DVASPECT_CONTENT.0,
        lindex: -1,
        tymed: TYMED_HGLOBAL.0 as u32,
        ..Default::default()
    }
}
fn accepts_files(obj: Ref<IDataObject>) -> bool {
    let Ok(data) = obj.ok() else { return false };
    unsafe {
        let shell_list = windows::Win32::System::DataExchange::RegisterClipboardFormatW(
            windows::core::w!("Shell IDList Array"),
        ) as u16;
        [CF_HDROP.0, shell_list]
            .iter()
            .any(|&id| id != 0 && data.QueryGetData(&format_for(id)).is_ok())
    }
}
fn paths(obj: Ref<IDataObject>) -> Vec<PathBuf> {
    use windows::Win32::System::Com::TYMED_HGLOBAL;
    let Ok(data) = obj.ok() else {
        return Vec::new();
    };
    unsafe {
        // Explorer and other desktop file managers both supply CF_HDROP.
        let format = format_for(CF_HDROP.0);
        if let Ok(mut medium) = data.GetData(&format) {
            let files = if medium.tymed == TYMED_HGLOBAL.0 as u32 {
                drop_files(medium.u.hGlobal.0)
            } else {
                Vec::new()
            };
            ReleaseStgMedium(&mut medium);
            if !files.is_empty() {
                return files;
            }
        }
        let Ok(array): ComResult<IShellItemArray> = SHCreateShellItemArrayFromDataObject(data)
        else {
            return Vec::new();
        };
        (0..array.GetCount().unwrap_or(0))
            .filter_map(|i| {
                let name = array
                    .GetItemAt(i)
                    .ok()?
                    .GetDisplayName(SIGDN_FILESYSPATH)
                    .ok()?;
                Some(crate::shell::owned_string(name).into())
            })
            .collect()
    }
}
fn effect(out: *mut DROPEFFECT, accepted: bool) {
    if !out.is_null() {
        unsafe {
            let allowed = *out;
            *out = if !accepted {
                DROPEFFECT_NONE
            } else if allowed.contains(DROPEFFECT_LINK) {
                DROPEFFECT_LINK
            } else if allowed.contains(DROPEFFECT_COPY) {
                DROPEFFECT_COPY
            } else {
                DROPEFFECT_NONE
            };
        }
    }
}
#[allow(non_snake_case)]
impl IDropTarget_Impl for FileTarget_Impl {
    fn DragEnter(
        &self,
        data: Ref<IDataObject>,
        _: MODIFIERKEYS_FLAGS,
        pt: &POINTL,
        out: *mut DROPEFFECT,
    ) -> ComResult<()> {
        EXTERNAL.set(accepts_files(data));
        self.DragOver(MODIFIERKEYS_FLAGS(0), pt, out)
    }
    fn DragOver(&self, _: MODIFIERKEYS_FLAGS, pt: &POINTL, out: *mut DROPEFFECT) -> ComResult<()> {
        effect(out, active());
        if active() {
            over(POINT { x: pt.x, y: pt.y });
        }
        Ok(())
    }
    fn DragLeave(&self) -> ComResult<()> {
        reset();
        if AUTO_EXPANDED.take() {
            collapse(ISLAND.get());
        }
        Ok(())
    }
    fn Drop(
        &self,
        data: Ref<IDataObject>,
        _: MODIFIERKEYS_FLAGS,
        pt: &POINTL,
        out: *mut DROPEFFECT,
    ) -> ComResult<()> {
        effect(out, active());
        let items = if EXTERNAL.get() {
            paths(data)
        } else {
            Vec::new()
        };
        if items.is_empty() {
            effect(out, false);
        }
        let target = placement(POINT { x: pt.x, y: pt.y });
        reset();
        if !out.is_null()
            && unsafe { *out } != DROPEFFECT_NONE
            && let Err(e) = place_paths(&items, target)
        {
            effect(out, false);
            report(&e);
        }
        if AUTO_EXPANDED.take() {
            collapse(ISLAND.get());
        }
        Ok(())
    }
}
pub(super) fn register(hwnd: HWND) -> io::Result<()> {
    let handler: IDropTarget = FileTarget.into();
    unsafe { RegisterDragDrop(ComHwnd(hwnd), &handler) }.map_err(io::Error::other)
}
pub(super) fn unregister(hwnd: HWND) {
    unsafe {
        let _ = RevokeDragDrop(ComHwnd(hwnd));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::windows::process::CommandExt;
    use windows::Win32::UI::Shell::{BHID_DataObject, IShellItem, SHCreateItemFromParsingName};
    use windows::core::PCWSTR;

    #[test]
    fn shell_drop_uses_the_visible_folder_and_pin_targets() {
        // The normal drop entry writes the user's config. Run this case in its
        // own process so even parallel tests never change the process-wide path.
        const CHILD: &str = "GUIYUX_TEST_DROP_CHILD";
        if std::env::var_os(CHILD).is_none() {
            let root = tempfile::tempdir().unwrap();
            let result = std::process::Command::new(std::env::current_exe().unwrap())
                .creation_flags(windows_sys::Win32::System::Threading::CREATE_NO_WINDOW)
                .args([
                    "--exact",
                    "ui::island::drag::tests::shell_drop_uses_the_visible_folder_and_pin_targets",
                    "--nocapture",
                ])
                .env(CHILD, "1")
                .env("LOCALAPPDATA", root.path())
                .output()
                .unwrap();
            assert!(
                result.status.success(),
                "{}{}",
                String::from_utf8_lossy(&result.stdout),
                String::from_utf8_lossy(&result.stderr)
            );
            return;
        }
        let _desktop = crate::test_desktop::TestDesktop::enter().unwrap();
        let _apartment = crate::shell::Apartment::open().unwrap();
        crate::ui::register("GuiyuX.Island", Some(super::super::procedure)).unwrap();
        super::super::open().unwrap();
        let hwnd = ISLAND.get();
        let folder = collection::update(|s| {
            s.auto_sort = false;
            s.add_folder("工作夹")
        })
        .unwrap();
        let path = crate::config::path().unwrap().with_file_name("draw.pdf");
        std::fs::write(&path, []).unwrap();
        let name: Vec<_> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        let item: IShellItem =
            unsafe { SHCreateItemFromParsingName(PCWSTR(name.as_ptr()), None) }.unwrap();
        let data: IDataObject = unsafe { item.BindToHandler(None, &BHID_DataObject) }.unwrap();
        let target: IDropTarget = FileTarget.into();
        let drop_at = |pt| unsafe {
            let mut effect = DROPEFFECT_LINK | DROPEFFECT_COPY;
            target
                .DragEnter(&data, MODIFIERKEYS_FLAGS(1), pt, &mut effect)
                .unwrap();
            assert_eq!(effect, DROPEFFECT_LINK);
            target
                .DragOver(MODIFIERKEYS_FLAGS(1), pt, &mut effect)
                .unwrap();
            target
                .Drop(&data, MODIFIERKEYS_FLAGS(0), pt, &mut effect)
                .unwrap();
            assert_eq!(effect, DROPEFFECT_LINK);
        };
        surface::present(hwnd).unwrap();
        let mut r = RECT::default();
        unsafe {
            GetWindowRect(hwnd, &mut r);
        }
        drop_at(POINTL {
            x: (r.left + r.right) / 2,
            y: (r.top + r.bottom) / 2,
        });
        assert_eq!(collection::current().unwrap().count(collection::OTHER), 1);
        EXPANDED.set(true);
        VIEW.set(View::Home);
        nav::reload().unwrap();
        refresh();
        surface::present(hwnd).unwrap();
        unsafe {
            GetWindowRect(GetDlgItem(hwnd, FIRST), &mut r);
        }
        drop_at(POINTL {
            x: (r.left + r.right) / 2,
            y: (r.top + r.bottom) / 2,
        });
        drop_at(POINTL {
            x: r.left + 3,
            y: (r.top + r.bottom) / 2,
        });
        let state = collection::current().unwrap();
        assert_eq!(state.items.len(), 1);
        assert_eq!(state.items[0].group, folder);
        assert!(state.items[0].manual);
        assert_eq!(state.bar[0], Shortcut::Item(path));

        let files: Vec<_> = (0..GRID_PAGE + 2)
            .map(|i| PathBuf::from(format!("item-{i}.txt")))
            .collect();
        collection::update(|s| {
            s.items.extend(files.iter().map(|path| collection::Item {
                path: path.clone(),
                group: folder,
                directory: false,
                manual: true,
            }));
        })
        .unwrap();
        VIEW.set(View::Group(folder));
        nav::reload().unwrap();
        PAGE.set(1);
        refresh();
        surface::present(hwnd).unwrap();
        unsafe { GetWindowRect(GetDlgItem(hwnd, FIRST), &mut r) };
        for (x, before) in [(r.left + 1, GRID_PAGE - 1), (r.right - 1, GRID_PAGE)] {
            let target = hit(POINT {
                x,
                y: (r.top + r.bottom) / 2,
            });
            assert_eq!(
                target.place,
                Placement::InGroup(folder, Some(files[before].clone()))
            );
            let line = target.line.unwrap();
            assert_eq!(line.right - line.left, 2);
            assert!(line.bottom - line.top > 2);
        }
        VIEW.set(View::Home);
        PAGE.set(0);
        nav::rebuild();
        unsafe {
            DestroyWindow(hwnd);
        }
    }
}
