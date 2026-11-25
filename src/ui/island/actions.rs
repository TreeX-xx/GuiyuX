use super::*;
use nav::Action;

pub(super) fn context(hwnd: HWND, wp: WPARAM, lp: LPARAM) -> io::Result<()> {
    let mut pt = POINT {
        x: lp as i16 as i32,
        y: (lp >> 16) as i16 as i32,
    };
    let anchor = if lp == -1 {
        wp as HWND
    } else {
        unsafe { WindowFromPoint(pt) }
    };
    let id = unsafe { GetDlgCtrlID(anchor) };
    let entry = nav::button_row(id)
        .filter(|_| EXPANDED.get())
        .and_then(|r| match r.action {
            Action::Open(k) => Some(k),
            _ => None,
        });
    let Some(entry) = entry else {
        return menu::show(hwnd, lp);
    };
    if lp == -1 {
        let mut r = RECT::default();
        unsafe {
            GetWindowRect(anchor, &mut r);
        }
        pt = POINT {
            x: r.left,
            y: r.bottom,
        };
    }
    let data = collection::current()?;
    let lang = crate::language();
    let pinned = data.bar.contains(&entry);
    let root = unsafe { CreatePopupMenu() };
    if root.is_null() {
        return Err(io::Error::last_os_error());
    }
    let groups: Vec<_> = data.groups().collect();
    let command = unsafe {
        AppendMenuW(
            root,
            MF_STRING,
            1,
            wide(if pinned {
                lang.pick("从快捷栏移除", "Unpin from bar")
            } else {
                lang.pick("固定到快捷栏", "Pin to bar")
            })
            .as_ptr(),
        );
        if let Shortcut::Item(_) = entry {
            let submenu = CreatePopupMenu();
            for (i, g) in groups.iter().enumerate() {
                AppendMenuW(
                    submenu,
                    MF_STRING,
                    100 + i,
                    wide(&data.group_name(*g, lang)).as_ptr(),
                );
            }
            AppendMenuW(
                root,
                MF_POPUP,
                submenu as usize,
                wide(lang.pick("移至分类或文件夹", "Move to group or folder")).as_ptr(),
            );
            AppendMenuW(
                root,
                MF_STRING,
                2,
                wide(lang.pick("跟随规则", "Follow rules")).as_ptr(),
            );
        }
        AppendMenuW(root, MF_SEPARATOR, 0, null_mut());
        AppendMenuW(root, MF_STRING, 3, wide(lang.text().settings).as_ptr());
        let command = TrackPopupMenu(
            root,
            TPM_RETURNCMD | TPM_RIGHTBUTTON,
            pt.x,
            pt.y,
            0,
            hwnd,
            null_mut(),
        );
        DestroyMenu(root);
        command
    };
    match command {
        0 => return Ok(()),
        3 => return settings::open(),
        1 => collection::update(|s| {
            if pinned {
                s.bar.retain(|k| k != &entry)
            } else {
                s.pin(entry, 0)
            }
        })?,
        2 => {
            if let Shortcut::Item(p) = entry {
                collection::update(|s| s.follow(&p))?;
            }
        }
        i if i >= 100 => {
            if let (Some(g), Shortcut::Item(p)) = (groups.get((i - 100) as usize), entry) {
                return drag::place_paths(&[p], Placement::InGroup(*g, None));
            }
        }
        _ => {}
    }
    settings::notify();
    collection_changed()
}
