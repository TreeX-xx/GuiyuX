use crate::{
    collection::{self, Folder, Item, Placement, Rule, Shortcut},
    language::Language,
};
use serde::{Deserialize, Serialize};
use std::{
    env, fs, io,
    path::{Path, PathBuf},
};

const BAR_REVISION: u8 = 3;

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum DockEdge {
    Top,
    #[default]
    Bottom,
    Left,
    Right,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum DockMode {
    AlwaysVisible,
    #[default]
    AutoHide,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default = "Dock::legacy")]
pub struct Dock {
    pub edge: DockEdge,
    pub mode: DockMode,
}

impl Dock {
    fn legacy() -> Self {
        Self {
            edge: DockEdge::Top,
            mode: DockMode::AlwaysVisible,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default = "Appearance::legacy", rename_all = "camelCase")]
pub struct Appearance {
    pub frosted: bool,
    pub transparency: u8,
    pub edge_margin: Option<u8>,
    pub bar_padding: u8,
    pub item_gap: u8,
    pub corner_radius: u8,
}

impl Default for Appearance {
    fn default() -> Self {
        Self {
            frosted: true,
            transparency: 50,
            edge_margin: Some(4),
            bar_padding: 0,
            item_gap: 3,
            corner_radius: 19,
        }
    }
}

impl Appearance {
    fn legacy() -> Self {
        Self {
            frosted: true,
            transparency: 30,
            edge_margin: None,
            bar_padding: 8,
            item_gap: 0,
            corner_radius: 12,
        }
    }

    pub fn bounded(mut self) -> Self {
        self.transparency = self.transparency.min(100);
        self.edge_margin = self.edge_margin.map(|v| v.min(48));
        self.bar_padding = self.bar_padding.min(24);
        self.item_gap = self.item_gap.min(24);
        self.corner_radius = self.corner_radius.min(32);
        self
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default)]
pub struct Settings {
    pub language: Language,
    #[serde(default = "Dock::legacy")]
    pub dock: Dock,
    // The island and menus share the appearance saved by menu-only versions.
    #[serde(rename = "menu", default = "Appearance::legacy")]
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
            dock: Dock::default(),
            appearance: Appearance::default(),
            inbox: vec![],
            items: vec![],
            auto_sort: true,
            restore_desktop_on_exit: false,
            rules: vec![],
            folders: vec![],
            bar: collection::default_bar(),
            bar_revision: BAR_REVISION,
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
            data.appearance = data.appearance.bounded();
            let migrate_bar = data.bar_revision < BAR_REVISION;
            if migrate_bar {
                use collection::{Category::*, Group};
                let previous = [Office, Developer, Documents, Folders, Assistant, Design]
                    .map(|c| Shortcut::Group(Group::Builtin(c)));
                let preview = [Office, Assistant, Documents, Folders, Developer, Design]
                    .map(|c| Shortcut::Group(Group::Builtin(c)));
                let previous_default = match data.bar_revision {
                    0 => &previous[..4],
                    2 => &preview[..],
                    _ => &previous[..],
                };
                if data.bar == previous_default {
                    data.bar = collection::default_bar();
                }
                data.bar_revision = BAR_REVISION;
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
    fn first_launch_uses_the_saved_appearance_preset() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let fresh = load(&path).unwrap();
        assert_eq!(
            fresh.dock,
            Dock {
                edge: DockEdge::Bottom,
                mode: DockMode::AutoHide
            }
        );
        assert_eq!(
            fresh.appearance,
            Appearance {
                frosted: true,
                transparency: 50,
                edge_margin: Some(4),
                bar_padding: 0,
                item_gap: 3,
                corner_radius: 19,
            }
        );
        save(&path, &fresh).unwrap();
        assert_eq!(load(&path).unwrap(), fresh);
    }
    #[test]
    fn legacy_settings_use_a_resident_top_dock_and_save_a_new_choice() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        fs::write(&path, r#"{"language":"en","menu":{"transparency":57}}"#).unwrap();
        let mut settings = load(&path).unwrap();
        assert_eq!(
            settings.dock,
            Dock {
                edge: DockEdge::Top,
                mode: DockMode::AlwaysVisible
            }
        );
        settings.dock = Dock {
            edge: DockEdge::Right,
            mode: DockMode::AutoHide,
        };
        save(&path, &settings).unwrap();
        let saved = load(&path).unwrap();
        assert_eq!(saved.dock, settings.dock);
        assert_eq!(saved.appearance.transparency, 57);
        assert_eq!(saved.language, Language::English);
    }

    #[test]
    fn old_settings_get_glass_defaults_and_a_selected_style_survives_reload() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        fs::write(&path, "{}").unwrap();
        let mut settings = load(&path).unwrap();
        assert_eq!(settings.appearance, Appearance::legacy());
        fs::write(&path, r#"{"menu":{"frosted":false,"transparency":45}}"#).unwrap();
        settings = load(&path).unwrap();
        assert_eq!(
            settings.appearance,
            Appearance {
                frosted: false,
                transparency: 45,
                ..Appearance::legacy()
            }
        );
        settings.appearance.edge_margin = Some(36);
        settings.appearance.bar_padding = 17;
        settings.appearance.item_gap = 5;
        settings.appearance.corner_radius = 0;
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
    fn upgrade_reorders_legacy_defaults_and_preserves_custom_bars() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let default = collection::default_bar();
        assert_eq!(default.len(), collection::BAR_CAPACITY);
        use collection::{Category::*, Group};
        let group = |c| Shortcut::Group(Group::Builtin(c));
        assert_eq!(default[1], group(Developer));
        assert_eq!(default[4], group(Assistant));
        let old = [Office, Developer, Documents, Folders, Assistant, Design].map(group);
        let save_old = |bar: &[Shortcut], revision: u8| {
            fs::write(
                &path,
                serde_json::to_vec(&serde_json::json!({"bar":bar,"bar_revision":revision}))
                    .unwrap(),
            )
            .unwrap()
        };
        save_old(&old[..4], 0);
        assert_eq!(load(&path).unwrap().bar, default);
        let mut preview = old.clone();
        preview.swap(1, 4);
        save_old(&preview, 2);
        assert_eq!(load(&path).unwrap().bar, default);
        save_old(&old, 1);
        let mut data = load(&path).unwrap();
        assert_eq!(data.bar, default);
        data.bar.truncate(4);
        save(&path, &data).unwrap();
        assert_eq!(load(&path).unwrap().bar, default[..4]);
        save_old(&old[..4], 1);
        assert_eq!(load(&path).unwrap().bar, old[..4]);
        let mut custom = old.to_vec();
        custom.swap(0, 1);
        save_old(&custom, 1);
        assert_eq!(load(&path).unwrap().bar, custom);
        save_old(&[], 0);
        assert!(load(&path).unwrap().bar.is_empty());
    }
}
