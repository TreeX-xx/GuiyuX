use super::{Group, OTHER, Rule, Settings, classify};
use crate::config;
use std::{
    cmp::Ordering,
    fs, io,
    os::windows::ffi::OsStrExt,
    os::windows::fs::MetadataExt,
    path::{Path, PathBuf},
};
use windows::Win32::UI::Shell::{
    FOLDERID_Desktop, FOLDERID_PublicDesktop, KF_FLAG_DONT_VERIFY, SHELLSTATEA,
    SHGetKnownFolderPath, SHGetSetSettings, SSF_SHOWALLOBJECTS, SSF_SHOWSUPERHIDDEN,
};
use windows_sys::Win32::{
    Globalization::{CSTR_EQUAL, CompareStringOrdinal},
    Storage::FileSystem::{FILE_ATTRIBUTE_HIDDEN, FILE_ATTRIBUTE_SYSTEM},
};

mod view;
use view::DesktopView;

#[cfg(test)]
mod tests;

// Desktop collection and restoration: .agents/notes/module.md#收藏分类与自动整理.
fn path_key(path: &Path) -> Vec<u16> {
    path.components()
        .collect::<PathBuf>()
        .as_os_str()
        .encode_wide()
        .collect()
}

fn compare_keys(a: &[u16], b: &[u16]) -> Ordering {
    let result =
        unsafe { CompareStringOrdinal(a.as_ptr(), a.len() as i32, b.as_ptr(), b.len() as i32, 1) };
    if result == 0 {
        a.cmp(b)
    } else {
        result.cmp(&CSTR_EQUAL)
    }
}

pub(crate) fn files() -> io::Result<Vec<PathBuf>> {
    let folder = |id| {
        let name = unsafe { SHGetKnownFolderPath(id, KF_FLAG_DONT_VERIFY, None) }
            .map_err(io::Error::other)?;
        Ok::<_, io::Error>(PathBuf::from(crate::shell::owned_string(name)))
    };
    let mut state = SHELLSTATEA::default();
    unsafe {
        SHGetSetSettings(
            Some(&mut state),
            SSF_SHOWALLOBJECTS | SSF_SHOWSUPERHIDDEN,
            false,
        )
    };
    // SHELLSTATE exposes fShowAllObjects and fShowSuperHidden as SDK bitfields.
    scan(
        &folder(&FOLDERID_Desktop)?,
        &folder(&FOLDERID_PublicDesktop)?,
        state._bitfield1 & 1 != 0,
        state._bitfield1 & (1 << 15) != 0,
    )
}

fn scan(
    personal: &Path,
    shared: &Path,
    show_hidden: bool,
    show_protected: bool,
) -> io::Result<Vec<PathBuf>> {
    let mut paths = Vec::new();
    for (root, optional) in [(personal, false), (shared, true)] {
        if optional && compare_keys(&path_key(personal), &path_key(shared)).is_eq() {
            continue;
        }
        let entries = match fs::read_dir(root) {
            Err(e) if optional && e.kind() == io::ErrorKind::NotFound => continue,
            other => other?,
        };
        for entry in entries {
            let entry = entry?;
            if entry.file_name().eq_ignore_ascii_case("desktop.ini") {
                continue;
            }
            let attributes = match entry.metadata() {
                Ok(m) => m.file_attributes(),
                Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
                Err(e) => return Err(e),
            };
            if attributes & FILE_ATTRIBUTE_HIDDEN == 0
                || (show_hidden && (attributes & FILE_ATTRIBUTE_SYSTEM == 0 || show_protected))
            {
                paths.push(entry.path());
            }
        }
    }
    paths.sort();
    Ok(paths)
}

fn new_paths<'a>(s: &Settings, files: &'a [PathBuf]) -> impl Iterator<Item = &'a PathBuf> {
    let mut saved: Vec<_> = s.items.iter().map(|item| path_key(&item.path)).collect();
    saved.sort_unstable_by(|a, b| compare_keys(a, b));
    files.iter().filter(move |p| {
        let key = path_key(p);
        saved
            .binary_search_by(|existing| compare_keys(existing, &key))
            .is_err()
    })
}

pub(crate) struct Organization {
    pub added: usize,
    pub moved: usize,
    pub other: usize,
    pub view_error: Option<io::Error>,
}

pub(crate) fn organize<T>(
    edit: impl FnOnce(&mut Settings) -> io::Result<T>,
) -> io::Result<(T, Organization)> {
    let files = files()?;
    let view = DesktopView::connect()?;
    organize_at(&config::path()?, &files, view.hidden()?, edit, || {
        view.set_hidden(true)
    })
}

fn organize_at<T>(
    path: &Path,
    files: &[PathBuf],
    hidden: bool,
    edit: impl FnOnce(&mut Settings) -> io::Result<T>,
    hide: impl FnOnce() -> io::Result<()>,
) -> io::Result<(T, Organization)> {
    let ((value, mut report), _) = super::transaction_at(path, |s| {
        let value = edit(s)?;
        let moved = s.organize();
        let incoming: Vec<_> = new_paths(s, files)
            .map(|p| s.automatic_item(p, true))
            .collect();
        let added = incoming.len();
        s.items.splice(0..0, incoming);
        // Commit both the original paths and the recovery marker before touching the shell view.
        s.restore_desktop_on_exit |= !hidden;
        Ok((
            value,
            Organization {
                added,
                moved,
                other: s.count(OTHER),
                view_error: None,
            },
        ))
    })?;
    if !hidden {
        report.view_error = hide().err();
    }
    Ok((value, report))
}

pub(crate) fn restore() -> io::Result<()> {
    restore_at(&config::path()?, false, || {
        DesktopView::connect()?.set_hidden(false)
    })
}

pub(crate) fn restore_on_exit() -> io::Result<()> {
    restore_at(&config::path()?, true, || {
        DesktopView::connect()?.set_hidden(false)
    })
}

fn restore_at(
    path: &Path,
    only_if_owned: bool,
    show: impl FnOnce() -> io::Result<()>,
) -> io::Result<()> {
    if only_if_owned && !config::load(path)?.restore_desktop_on_exit {
        return Ok(());
    }
    show()?;
    super::transaction_at(path, |s| {
        s.restore_desktop_on_exit = false;
        Ok(())
    })
    .map_err(|e| {
        io::Error::other(format!(
            "{} {e}",
            crate::language().pick(
                "桌面图标已恢复，但未能保存恢复状态。",
                "Desktop icons restored, but the recovery state could not be saved."
            )
        ))
    })?;
    Ok(())
}

pub(crate) fn preview(
    s: &Settings,
    files: &[PathBuf],
    rules: &[Rule],
) -> Vec<(PathBuf, Option<Group>, Group)> {
    let mut changes: Vec<_> = s
        .changes(rules)
        .into_iter()
        .map(|(i, to)| {
            let item = &s.items[i];
            (item.path.clone(), Some(item.group), to)
        })
        .collect();
    changes.extend(new_paths(s, files).map(|path| {
        let group = Group::Builtin(classify::classify(path, rules));
        (path.clone(), None, group)
    }));
    changes
}
