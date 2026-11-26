use super::*;
use crate::collection::{Category, Placement, RuleKind};
use std::cell::Cell;
use windows_sys::Win32::Storage::FileSystem::SetFileAttributesW;

#[test]
fn redirected_and_public_desktops_match_explorer_visibility_without_recursing() {
    let temp = tempfile::tempdir().unwrap();
    let personal = temp.path().join("OneDrive/Desktop");
    let public = temp.path().join("Public/Desktop");
    fs::create_dir_all(personal.join("Folder")).unwrap();
    fs::create_dir_all(&public).unwrap();
    for path in [
        personal.join("Folder/inside.pdf"),
        personal.join("same.lnk"),
        public.join("same.lnk"),
        personal.join("DESKTOP.INI"),
        personal.join("hidden.docx"),
        public.join("protected.bin"),
    ] {
        fs::write(path, []).unwrap();
    }
    for (path, attrs) in [
        (personal.join("hidden.docx"), FILE_ATTRIBUTE_HIDDEN),
        (
            public.join("protected.bin"),
            FILE_ATTRIBUTE_HIDDEN | FILE_ATTRIBUTE_SYSTEM,
        ),
    ] {
        let wide: Vec<_> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        assert_ne!(unsafe { SetFileAttributesW(wide.as_ptr(), attrs) }, 0);
    }
    let normal = scan(&personal, &public, false, false).unwrap();
    assert_eq!(normal.len(), 3);
    for expected in [
        personal.join("Folder"),
        personal.join("same.lnk"),
        public.join("same.lnk"),
    ] {
        assert!(normal.contains(&expected));
    }
    assert_eq!(scan(&personal, &public, true, false).unwrap().len(), 4);
    assert_eq!(scan(&personal, &public, true, true).unwrap().len(), 5);
    assert_eq!(scan(&personal, &personal, false, false).unwrap().len(), 2);
    assert_eq!(
        scan(&personal, &temp.path().join("absent"), false, false)
            .unwrap()
            .len(),
        2
    );
    assert!(scan(&temp.path().join("absent"), &public, false, false).is_err());
}

#[test]
fn explicit_sort_saves_before_hiding_and_matches_the_rule_preview() {
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("settings.json");
    let folder = temp.path().join("Project");
    fs::create_dir(&folder).unwrap();
    let link = temp.path().join("助手.url");
    let doc = temp.path().join("PLAN.PDF");
    let saved_doc = temp.path().join("plan.pdf");
    let existing = temp.path().join("diagram.drawio");
    fs::write(&link, "[InternetShortcut]\nURL=https://chatgpt.com/").unwrap();
    fs::write(&doc, b"original").unwrap();
    fs::write(&existing, []).unwrap();
    let files = vec![folder.clone(), link.clone(), doc.clone(), existing.clone()];
    let mut data = Settings::default();
    data.auto_sort = false;
    let manual = data.add_folder("Pinned");
    data.place(
        std::slice::from_ref(&saved_doc),
        &Placement::InGroup(manual, None),
    );
    data.place(std::slice::from_ref(&existing), &Placement::Collect);
    config::save(&db, &data).unwrap();
    let rule = Rule {
        kind: RuleKind::Extension,
        pattern: ".drawio".into(),
        category: Category::Design,
    };
    let expected = preview(&data, &files, std::slice::from_ref(&rule));
    assert_eq!(expected.len(), 3);
    assert!(expected.contains(&(
        existing.clone(),
        Some(OTHER),
        Group::Builtin(Category::Design)
    )));
    assert!(expected.contains(&(link.clone(), None, Group::Builtin(Category::Assistant))));
    assert!(expected.contains(&(folder.clone(), None, Group::Builtin(Category::Folders))));
    assert_eq!(config::load(&db).unwrap(), data);
    let (_, report) = organize_at(
        &db,
        &files,
        false,
        |s| {
            s.rules.push(rule);
            Ok(())
        },
        || {
            let saved = config::load(&db)?;
            assert!(saved.restore_desktop_on_exit);
            assert_eq!(saved.items.len(), 4);
            Ok(())
        },
    )
    .unwrap();
    assert_eq!((report.added, report.moved, report.other), (2, 1, 0));
    assert!(report.view_error.is_none());
    let organized = config::load(&db).unwrap();
    for (path, _, target) in expected {
        assert!(
            organized
                .items
                .iter()
                .any(|i| i.path == path && i.group == target)
        );
    }
    assert!(
        organized
            .items
            .iter()
            .any(|i| i.path == saved_doc && i.manual && i.group == manual)
    );
    assert_eq!(organized.bar, data.bar);
    let (_, again) =
        organize_at(&db, &files, true, |_| Ok(()), || panic!("already hidden")).unwrap();
    assert_eq!((again.added, again.moved), (0, 0));
    assert_eq!(config::load(&db).unwrap(), organized);
    restore_at(&db, false, || Ok(())).unwrap();
    let restored = config::load(&db).unwrap();
    assert!(!restored.restore_desktop_on_exit);
    assert_eq!(restored.items, organized.items);
    assert!(folder.is_dir() && link.is_file());
    assert_eq!(fs::read(&doc).unwrap(), b"original");
}

#[test]
fn persistence_and_shell_failures_preserve_visibility_and_allow_retry() {
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("settings.json");
    let data_file = temp.path().join("note.txt");
    fs::write(&data_file, []).unwrap();
    let files = [data_file.clone()];
    let fail_view = || Err(io::Error::other("shell unavailable"));
    let contents = br#"{"bar_revision":1}"#;
    fs::write(&db, contents).unwrap();
    let bad_rule = organize_at(
        &db,
        &files,
        false,
        |s| {
            s.auto_sort = false;
            Err::<(), _>(io::Error::other("invalid rule"))
        },
        || panic!("invalid rule must leave icons visible"),
    );
    assert!(bad_rule.is_err());
    assert_eq!(fs::read(&db).unwrap(), contents);
    // Hold the destination against replacement while keeping its original contents readable.
    use std::os::windows::fs::OpenOptionsExt;
    use windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ;
    let lock = fs::File::options()
        .read(true)
        .share_mode(FILE_SHARE_READ)
        .open(&db)
        .unwrap();
    assert!(
        organize_at(
            &db,
            &files,
            false,
            |_| Ok(()),
            || panic!("uncommitted collection")
        )
        .is_err()
    );
    drop(lock);
    assert_eq!(fs::read(&db).unwrap(), contents);
    let (_, partial) = organize_at(&db, &files, false, |_| Ok(()), fail_view).unwrap();
    assert!(partial.view_error.is_some());
    assert_eq!(config::load(&db).unwrap().items[0].path, data_file);
    assert!(restore_at(&db, true, fail_view).is_err());
    assert!(config::load(&db).unwrap().restore_desktop_on_exit);
    restore_at(&db, true, || Ok(())).unwrap();
    assert!(!config::load(&db).unwrap().restore_desktop_on_exit);
    organize_at(&db, &files, true, |_| Ok(()), || panic!("already hidden")).unwrap();
    restore_at(&db, true, || panic!("user had already hidden icons")).unwrap();
    fs::write(&db, b"broken").unwrap();
    let shown = Cell::new(false);
    assert!(
        restore_at(&db, false, || {
            shown.set(true);
            Ok(())
        })
        .is_err()
    );
    assert!(shown.get());
}
