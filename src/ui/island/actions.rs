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
    let groups: Vec<_> = data.groups().collect();
    let mut rows = vec![menu::Row::new(
        1,
        if pinned {
            lang.pick("从快捷栏移除", "Unpin from bar")
        } else {
            lang.pick("固定到快捷栏", "Pin to bar")
        },
        if pinned { "\u{e77a}" } else { "\u{e718}" },
    )];
    if let Shortcut::Item(_) = entry {
        let destinations = groups
            .iter()
            .enumerate()
            .map(|(i, group)| {
                menu::Row::new(100 + i as u32, data.group_name(*group, lang), "\u{e8b7}")
            })
            .collect();
        rows.push(menu::Row::submenu(
            4,
            lang.pick("移至分类或文件夹", "Move to group or folder"),
            "\u{e8de}",
            destinations,
        ));
        rows.push(menu::Row::new(
            2,
            lang.pick("跟随规则", "Follow rules"),
            "\u{e8a5}",
        ));
    }
    rows.push(menu::Row::separator());
    rows.push(menu::Row::new(3, lang.text().settings, "\u{e713}"));
    let command = menu::track(hwnd, pt, rows, TPM_RETURNCMD)?;
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
