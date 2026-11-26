use crate::{
    collection::{self, Folder, Item, Placement, Rule, Shortcut},
    language::Language,
};
use serde::{Deserialize, Serialize};
use std::{
    env, fs, io,
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default)]
pub struct Appearance {
    pub frosted: bool,
    pub transparency: u8,
}

impl Default for Appearance {
    fn default() -> Self {
        Self {
            frosted: true,
            transparency: 30,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default)]
pub struct Settings {
    pub language: Language,
    // The island and menus share the appearance saved by menu-only versions.
    #[serde(rename = "menu")]
    pub appearance: Appearance,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    inbox: Vec<PathBuf>,
    pub auto_sort: bool,
    pub restore_desktop_on_exit: bool,
    pub items: Vec<Item>,
    pub rules: Vec<Rule>,
    pub folders: Vec<Folder>,
    pub bar: Vec<Shortcut>,
    #[serde(default)]
    bar_revision: u8,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            language: Language::System,
            appearance: Appearance::default(),
            inbox: vec![],
            items: vec![],
            auto_sort: true,
            restore_desktop_on_exit: false,
            rules: vec![],
            folders: vec![],
            bar: collection::default_bar(),
            bar_revision: 1,
        }
    }
}

pub fn path() -> io::Result<PathBuf> {
    env::var_os("LOCALAPPDATA")
        .map(|root| PathBuf::from(root).join("GuiyuX/settings.json"))
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "LOCALAPPDATA"))
}

pub fn load(path: &Path) -> io::Result<Settings> {
    match fs::read(path) {
        Ok(bytes) => {
            let mut data: Settings = serde_json::from_slice(&bytes)?;
            data.appearance.transparency = data.appearance.transparency.min(100);
            let migrate_bar = data.bar_revision == 0;
            if migrate_bar {
                let defaults = collection::default_bar();
                if data.bar.as_slice() == &defaults[..4] {
                    data.bar = defaults;
                }
                data.bar_revision = 1;
            }
            let migrate_items = !data.inbox.is_empty();
            if migrate_items {
                let mut paths = std::mem::take(&mut data.inbox);
                paths.reverse();
                data.place(&paths, &Placement::Collect);
            }
            if migrate_bar || migrate_items {
                save(path, &data)?;
            }
            Ok(data)
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Settings::default()),
        Err(e) => Err(e),
    }
}

pub fn save(path: &Path, settings: &Settings) -> io::Result<()> {
    let dir = path.parent().ok_or(io::ErrorKind::InvalidInput)?;
    fs::create_dir_all(dir)?;
    let mut temp = tempfile::NamedTempFile::new_in(dir)?;
    serde_json::to_writer_pretty(&mut temp, settings)?;
    temp.as_file().sync_all()?;
    temp.persist(path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn old_settings_get_glass_defaults_and_a_selected_style_survives_reload() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        fs::write(&path, "{}").unwrap();
        let mut settings = load(&path).unwrap();
        assert_eq!(settings.appearance, Appearance::default());
        fs::write(&path, r#"{"menu":{"frosted":false,"transparency":45}}"#).unwrap();
        settings = load(&path).unwrap();
        assert_eq!(
            settings.appearance,
            Appearance {
                frosted: false,
                transparency: 45,
            }
        );
        save(&path, &settings).unwrap();
        assert_eq!(load(&path).unwrap().appearance, settings.appearance);
        for percent in [0, 1, 37, 83, 99, 100, 255] {
            settings.appearance.transparency = percent;
            save(&path, &settings).unwrap();
            assert_eq!(
                load(&path).unwrap().appearance.transparency,
                percent.min(100)
            );
        }
    }

    #[test]
    fn upgrade_only_extends_the_old_default_and_respects_later_unpins() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let default = collection::default_bar();
        assert_eq!(default.len(), collection::BAR_CAPACITY);
        let old = &default[..4];
        let save_old = |bar: &[Shortcut]| {
            fs::write(
                &path,
                serde_json::to_vec(&serde_json::json!({"bar":bar})).unwrap(),
            )
            .unwrap()
        };
        save_old(old);
        let mut data = load(&path).unwrap();
        assert_eq!(data.bar, default);
        data.bar.truncate(4);
        save(&path, &data).unwrap();
        assert_eq!(load(&path).unwrap().bar, old);
        let mut custom = old.to_vec();
        custom.swap(0, 1);
        save_old(&custom);
        assert_eq!(load(&path).unwrap().bar, custom);
        save_old(&[]);
        assert!(load(&path).unwrap().bar.is_empty());
    }
}
