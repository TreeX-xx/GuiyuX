// Collection design: .agents/notes/module.md#收藏分类与自动整理
mod classify;
use crate::{
    config::{self, Settings},
    language::Language,
};
use serde::{Deserialize, Serialize};
use std::{
    io,
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    Office,
    Assistant,
    Developer,
    Design,
    Utilities,
    Documents,
    Images,
    Media,
    Archives,
    Folders,
    Other,
}

impl Category {
    pub const ALL: [Self; 11] = [
        Self::Office,
        Self::Assistant,
        Self::Developer,
        Self::Design,
        Self::Utilities,
        Self::Documents,
        Self::Images,
        Self::Media,
        Self::Archives,
        Self::Folders,
        Self::Other,
    ];
    pub fn name(self, lang: Language) -> &'static str {
        lang.text().groups[self as usize]
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Group {
    Builtin(Category),
    Folder(u64),
}
pub const OTHER: Group = Group::Builtin(Category::Other);
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Shortcut {
    Group(Group),
    Item(PathBuf),
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Item {
    pub path: PathBuf,
    pub group: Group,
    pub manual: bool,
    #[serde(default)]
    pub directory: bool,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Folder {
    pub id: u64,
    pub name: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuleKind {
    Extension,
    Name,
    Application,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Rule {
    pub kind: RuleKind,
    pub pattern: String,
    pub category: Category,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Placement {
    Collect,
    InGroup(Group, Option<PathBuf>),
    Pin(usize),
}

pub fn file_name(path: &Path) -> String {
    path.file_name()
        .unwrap_or(path.as_os_str())
        .to_string_lossy()
        .into_owned()
}

pub const BAR_CAPACITY: usize = 6;

pub fn default_bar() -> Vec<Shortcut> {
    [
        Category::Office,
        Category::Developer,
        Category::Documents,
        Category::Folders,
        Category::Assistant,
        Category::Design,
    ]
    .map(|g| Shortcut::Group(Group::Builtin(g)))
    .into()
}

impl Settings {
    pub fn group_name(&self, group: Group, lang: Language) -> String {
        match group {
            Group::Builtin(c) => c.name(lang).to_owned(),
            Group::Folder(id) => self
                .folders
                .iter()
                .find(|f| f.id == id)
                .map_or(String::new(), |f| f.name.clone()),
        }
    }
    pub fn title(&self, entry: &Shortcut, lang: Language) -> String {
        match entry {
            Shortcut::Group(g) => self.group_name(*g, lang),
            Shortcut::Item(p) => file_name(p),
        }
    }
    pub fn groups(&self) -> impl Iterator<Item = Group> + '_ {
        Category::ALL
            .into_iter()
            .filter(|&g| g != Category::Other)
            .map(Group::Builtin)
            .chain(self.folders.iter().map(|f| Group::Folder(f.id)))
            .chain(Some(OTHER))
    }
    pub fn count(&self, group: Group) -> usize {
        self.items.iter().filter(|i| i.group == group).count()
    }
    pub fn pin(&mut self, entry: Shortcut, position: usize) {
        let mut at = position.min(self.bar.len());
        if let Some(old) = self.bar.iter().position(|s| s == &entry) {
            self.bar.remove(old);
            if old < at {
                at -= 1;
            }
        }
        self.bar.insert(at, entry);
    }
    pub fn add_folder(&mut self, name: &str) -> Group {
        let id = self.folders.iter().map(|f| f.id).max().unwrap_or(0) + 1;
        self.folders.push(Folder {
            id,
            name: name.trim().to_owned(),
        });
        let group = Group::Folder(id);
        self.pin(Shortcut::Group(group), 0);
        group
    }
    pub fn delete_folder(&mut self, id: u64) {
        let group = Group::Folder(id);
        self.folders.retain(|f| f.id != id);
        self.bar.retain(|s| *s != Shortcut::Group(group));
        for item in &mut self.items {
            if item.group == group {
                item.group = OTHER;
                item.manual = false;
            }
        }
    }
    pub fn place(&mut self, paths: &[PathBuf], placement: &Placement) {
        // Insert the batch against one stable anchor, retaining Explorer's selection order.
        let mut unique = Vec::new();
        for p in paths {
            if !unique.contains(&p) {
                unique.push(p);
            }
        }
        let mut insertion = match placement {
            Placement::InGroup(_, Some(before)) => self
                .items
                .iter()
                .position(|i| &i.path == before)
                .unwrap_or(self.items.len()),
            Placement::InGroup(_, None) => self.items.len(),
            _ => 0,
        };
        let mut pin_at = match placement {
            Placement::Pin(at) => *at,
            _ => 0,
        };
        for path in unique {
            if let Placement::InGroup(_, Some(before)) = placement
                && path == before
            {
                continue;
            }
            let old = self.items.iter().position(|i| &i.path == path);
            if let Placement::Pin(_) = placement {
                let key = Shortcut::Item(path.clone());
                self.pin(key.clone(), pin_at);
                pin_at = self.bar.iter().position(|s| s == &key).unwrap() + 1;
            }
            if let Placement::InGroup(group, _) = placement {
                let directory = old
                    .map(|i| self.items[i].directory)
                    .unwrap_or_else(|| path.is_dir());
                if let Some(old) = old {
                    self.items.remove(old);
                    if old < insertion {
                        insertion -= 1;
                    }
                }
                self.items.insert(
                    insertion,
                    Item {
                        path: path.clone(),
                        group: *group,
                        manual: true,
                        directory,
                    },
                );
            } else if old.is_none() {
                let group = if self.auto_sort {
                    Group::Builtin(classify::classify(path, &self.rules))
                } else {
                    OTHER
                };
                self.items.insert(
                    insertion,
                    Item {
                        path: path.clone(),
                        group,
                        manual: false,
                        directory: path.is_dir(),
                    },
                );
            } else {
                continue;
            }
            insertion += 1;
        }
    }
    pub fn follow(&mut self, path: &Path) {
        if let Some(item) = self.items.iter_mut().find(|it| it.path == path) {
            item.manual = false;
            if self.auto_sort {
                item.group = Group::Builtin(classify::classify(path, &self.rules));
            }
        }
    }
    pub fn changes(&self, rules: &[Rule]) -> Vec<(usize, Group)> {
        self.items
            .iter()
            .enumerate()
            .filter(|(_, item)| !item.manual)
            .filter_map(|(index, item)| {
                let group = Group::Builtin(classify::classify(&item.path, rules));
                (item.group != group).then_some((index, group))
            })
            .collect()
    }
    pub fn organize(&mut self) -> usize {
        let changes = self.changes(&self.rules);
        for &(index, group) in &changes {
            self.items[index].group = group;
        }
        changes.len()
    }
}

pub fn current() -> io::Result<Settings> {
    config::load(&config::path()?)
}
pub fn update<T>(apply: impl FnOnce(&mut Settings) -> T) -> io::Result<T> {
    Ok(transaction(apply)?.0)
}

pub fn transaction<T>(apply: impl FnOnce(&mut Settings) -> T) -> io::Result<(T, bool)> {
    let path = config::path()?;
    let mut data = config::load(&path)?;
    let before = data.clone();
    let result = apply(&mut data);
    let changed = data != before;
    if changed {
        config::save(&path, &data)?;
    }
    Ok((result, changed))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn old_inbox_is_upgraded_once_in_recent_order() {
        let dir = tempdir().unwrap();
        let db = dir.path().join("settings.json");
        let new = dir.path().join("新文件.PDF");
        let old = dir.path().join("notes.md");
        fs::write(
            &db,
            serde_json::to_vec(&serde_json::json!({"language":"en","inbox":[old,new]})).unwrap(),
        )
        .unwrap();
        let first = config::load(&db).unwrap();
        assert_eq!(first.language, Language::English);
        assert_eq!(
            first.items.iter().map(|it| &it.path).collect::<Vec<_>>(),
            vec![&new, &old]
        );
        assert!(
            first
                .items
                .iter()
                .all(|it| it.group == Group::Builtin(Category::Documents))
        );
        let migrated = fs::read(&db).unwrap();
        assert_eq!(config::load(&db).unwrap(), first);
        assert_eq!(fs::read(&db).unwrap(), migrated);
    }

    #[test]
    fn rule_order_manual_drop_and_pins_survive_bulk_sort_and_restart() {
        use Category::*;
        let mut s = Settings::default();
        s.auto_sort = false;
        let a = PathBuf::from("mock.DRAWIO");
        let b = PathBuf::from("second.drawio");
        let c = PathBuf::from("old.PDF");
        s.place(&[a.clone(), b.clone(), c.clone()], &Placement::Collect);
        assert_eq!(s.count(OTHER), 3);
        let folder = s.add_folder("设计评审");
        s.place(std::slice::from_ref(&a), &Placement::InGroup(folder, None));
        s.place(
            std::slice::from_ref(&c),
            &Placement::InGroup(folder, Some(a.clone())),
        );
        s.place(std::slice::from_ref(&a), &Placement::Pin(1));
        s.place(std::slice::from_ref(&a), &Placement::Pin(1));
        s.rules = vec![
            Rule {
                kind: RuleKind::Name,
                pattern: "SECOND.drawio".into(),
                category: Developer,
            },
            Rule {
                kind: RuleKind::Extension,
                pattern: "*.drawio".into(),
                category: Design,
            },
        ];
        let bar = s.bar.clone();
        assert_eq!(s.organize(), 1);
        assert_eq!(s.organize(), 0);
        assert_eq!(s.bar, bar);
        assert_eq!(s.count(Group::Builtin(Developer)), 1);
        assert_eq!(
            s.items
                .iter()
                .filter(|it| it.group == folder)
                .map(|it| &it.path)
                .collect::<Vec<_>>(),
            vec![&c, &a]
        );
        let restored: Settings = serde_json::from_slice(&serde_json::to_vec(&s).unwrap()).unwrap();
        assert_eq!(s, restored);
        s.auto_sort = true;
        s.follow(&a);
        assert_eq!(s.count(Group::Builtin(Design)), 1);
        let Group::Folder(id) = folder else {
            unreachable!()
        };
        s.delete_folder(id);
        assert_eq!(s.count(OTHER), 1);
        assert!(s.bar.contains(&Shortcut::Item(a)));
        assert_eq!(s.items.len(), 3);
    }

    #[test]
    fn resolved_shell_targets_and_exact_url_hosts_drive_classification() {
        use std::os::windows::ffi::OsStrExt;
        use windows::{
            Win32::{
                System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance, IPersistFile},
                UI::Shell::{IShellLinkW, ShellLink},
            },
            core::{Interface, PCWSTR},
        };
        let _apartment = crate::shell::Apartment::open().unwrap();
        let dir = tempdir().unwrap();
        let doc = dir.path().join("ChatGPT.docx");
        fs::write(&doc, []).unwrap();
        let link = dir.path().join("飞书.lnk");
        let wide = |p: &Path| {
            p.as_os_str()
                .encode_wide()
                .chain(Some(0))
                .collect::<Vec<_>>()
        };
        unsafe {
            let lnk: IShellLinkW =
                CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).unwrap();
            lnk.SetPath(PCWSTR(wide(&doc).as_ptr())).unwrap();
            lnk.cast::<IPersistFile>()
                .unwrap()
                .Save(PCWSTR(wide(&link).as_ptr()), true)
                .unwrap();
        }
        assert_eq!(classify::classify(&link, &[]), Category::Documents);
        fs::remove_file(&doc).unwrap();
        assert_eq!(classify::classify(&link, &[]), Category::Other);
        let web = dir.path().join("问答.url");
        for (url, expected) in [
            ("https://CHATGPT.com/c/example", Category::Assistant),
            ("https://chatgpt.com.example.org/", Category::Other),
            ("https://example.org/chatgpt.com", Category::Other),
        ] {
            fs::write(&web, format!("[InternetShortcut]\r\nURL={url}")).unwrap();
            assert_eq!(classify::classify(&web, &[]), expected);
        }
        assert_eq!(classify::classify(dir.path(), &[]), Category::Folders);
        for (name, cat) in [
            ("Weixin.exe", Category::Office),
            ("ChatGPT.exe", Category::Assistant),
            ("Cursor.exe", Category::Developer),
            ("Figma.exe", Category::Design),
            ("7zFM.exe", Category::Utilities),
            ("frame.SVG", Category::Images),
            ("demo.mp4", Category::Media),
            ("backup.tar.gz", Category::Archives),
            ("random.exe", Category::Other),
        ] {
            assert_eq!(classify::classify(&dir.path().join(name), &[]), cat);
        }
    }
}
