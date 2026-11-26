use super::*;
use crate::{
    collection::{Group, Placement, Rule, RuleKind, Shortcut, desktop},
    config::{Appearance, Settings},
    language::Language,
};
use std::path::PathBuf;

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
enum Action {
    Initial,
    Snapshot,
    Appearance {
        style: Appearance,
        commit: bool,
    },
    AutoSort {
        enabled: bool,
    },
    Organize,
    RestoreDesktop,
    ShowGroup {
        group: Group,
    },
    Language {
        language: Language,
    },
    CreateFolder {
        name: String,
    },
    RenameFolder {
        id: u64,
        name: String,
    },
    DeleteFolder {
        id: u64,
    },
    Pin {
        key: Shortcut,
        pinned: bool,
    },
    MoveShortcut {
        key: Shortcut,
        down: bool,
    },
    PinPath {
        path: PathBuf,
        replace: Option<Shortcut>,
    },
    PreviewRule {
        index: Option<usize>,
        rule: Rule,
    },
    SaveRule {
        index: Option<usize>,
        rule: Rule,
        organize: bool,
    },
    MoveRule {
        index: usize,
        down: bool,
    },
    DeleteRule {
        index: usize,
    },
}

fn invalid(zh: &'static str, en: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, crate::language().pick(zh, en))
}

fn snapshot(s: &Settings) -> Value {
    let lang = s.language;
    let mut entries = s.bar.clone();
    let groups: Vec<_> = s
        .groups()
        .map(|group| {
            let key = Shortcut::Group(group);
            if !entries.contains(&key) {
                entries.push(key);
            }
            json!({"key":group, "name":s.group_name(group, lang), "count":s.count(group)})
        })
        .collect();
    let bar: Vec<_> = entries.iter().enumerate().map(|(i, key)| {
        let directory = match key {
            Shortcut::Item(path) => s.items.iter().find(|item| &item.path == path)
                .map_or_else(|| path.is_dir(), |item| item.directory),
            _ => false,
        };
        json!({"key": key, "title": s.title(key, lang), "pinned": i < s.bar.len(), "directory": directory})
    }).collect();
    json!({
        "language":lang, "locale": if lang.is_chinese() {"zh-CN"} else {"en"},
        "systemLanguage": Language::System.native_name(), "appearance":s.appearance,
        "autoSort":s.auto_sort, "groups":groups, "bar":bar, "rules":s.rules
    })
}

pub(super) fn summary(report: desktop::Organization) -> String {
    let desktop::Organization {
        added,
        moved,
        other,
        view_error,
    } = report;
    if crate::language().is_chinese() {
        let status = view_error.map_or_else(
            || "桌面图标已收起".to_owned(),
            |e| format!("桌面图标收起失败，请重试。{e}"),
        );
        format!("新收纳 {added} 项 · 已调整 {moved} 项 · 其他 {other} 项。{status}")
    } else {
        let status = view_error.map_or_else(
            || "Desktop icons are hidden.".to_owned(),
            |e| format!("Could not hide desktop icons. Try again. {e}"),
        );
        format!("{added} added · {moved} moved · {other} in Other. {status}")
    }
}

fn apply_style(style: Appearance, commit: bool) -> io::Result<()> {
    if style.transparency > 100 {
        return Err(invalid(
            "透明度应为 0–100。",
            "Transparency must be between 0 and 100.",
        ));
    }
    if commit && let Err(err) = collection::update(|s| s.appearance = style) {
        restore_appearance();
        return Err(err);
    }
    if APPEARANCE_STYLE.replace(style) != style {
        island::appearance_changed();
    }
    Ok(())
}

pub(super) fn restore_desktop() -> io::Result<&'static str> {
    desktop::restore()?;
    Ok(crate::language().pick(
        "桌面图标已恢复，分类收藏已保留。",
        "Desktop icons restored. Your grouped items are still in the island.",
    ))
}

fn folder_name(name: &str) -> io::Result<&str> {
    let name = name.trim();
    if name.is_empty() {
        return Err(invalid("请输入文件夹名称。", "Enter a folder name."));
    }
    Ok(name)
}

fn adjacent(index: usize, len: usize, down: bool) -> usize {
    if down {
        (index + 1).min(len.saturating_sub(1))
    } else {
        index.saturating_sub(1)
    }
}

fn rule_index(index: usize, len: usize) -> io::Result<usize> {
    if index < len {
        Ok(index)
    } else {
        Err(invalid(
            "规则已更改，请重新选择。",
            "The rule changed. Select it again.",
        ))
    }
}

fn put_rule(rules: &mut Vec<Rule>, index: Option<usize>, mut rule: Rule) -> io::Result<usize> {
    rule.pattern = rule.pattern.trim().to_owned();
    if rule.pattern.is_empty()
        || (rule.kind == RuleKind::Extension
            && rule
                .pattern
                .trim_start_matches("*.")
                .trim_start_matches('.')
                .is_empty())
    {
        return Err(invalid(
            "请输入匹配值，例如 .drawio。",
            "Enter a match value, such as .drawio.",
        ));
    }
    if let Some(i) = index {
        let i = rule_index(i, rules.len())?;
        rules[i] = rule;
        Ok(i)
    } else {
        rules.push(rule);
        Ok(rules.len() - 1)
    }
}

// All mutations run on the same STA as island drops and Windows shell classification.
pub(super) fn execute(value: Value) -> io::Result<Value> {
    let action: Action = serde_json::from_value(value)?;
    let mut result = json!({});
    match action {
        Action::Initial | Action::Snapshot => {
            return Ok(json!({"state":snapshot(&collection::current()?)}));
        }
        Action::Appearance { style, commit } => {
            apply_style(style, commit)?;
            return Ok(Value::Null);
        }
        Action::ShowGroup { group } => {
            island::show_group(group)?;
            return Ok(Value::Null);
        }
        Action::PreviewRule { index, rule } => {
            let s = collection::current()?;
            let mut rules = s.rules.clone();
            put_rule(&mut rules, index, rule)?;
            let changes: Vec<_> = desktop::preview(&s, &desktop::files()?, &rules)
                .into_iter()
                .map(|(path, from, to)| {
                    let from = from.map_or_else(
                        || s.language.pick("桌面", "Desktop").to_owned(),
                        |g| s.group_name(g, s.language),
                    );
                    json!({"path":path,"from":from,"to":s.group_name(to,s.language)})
                })
                .collect();
            return Ok(json!({"changes":changes}));
        }
        Action::Language { language } => crate::change_language(language)?,
        Action::AutoSort { enabled } => collection::update(|s| s.auto_sort = enabled)?,
        Action::Organize => {
            result["message"] = json!(summary(desktop::organize(|_| Ok(()))?.1));
        }
        Action::RestoreDesktop => {
            result["message"] = json!(restore_desktop()?);
        }
        Action::CreateFolder { name } => {
            let name = folder_name(&name)?;
            let group = collection::update(|s| s.add_folder(name))?;
            result["selection"] = json!(Shortcut::Group(group));
        }
        Action::RenameFolder { id, name } => {
            let name = folder_name(&name)?;
            collection::update(|s| -> io::Result<()> {
                let f = s
                    .folders
                    .iter_mut()
                    .find(|f| f.id == id)
                    .ok_or_else(|| invalid("文件夹已移除。", "Folder no longer exists."))?;
                f.name = name.to_owned();
                Ok(())
            })??;
        }
        Action::DeleteFolder { id } => collection::update(|s| s.delete_folder(id))?,
        Action::Pin { key, pinned } => collection::update(|s| {
            if pinned {
                if !s.bar.contains(&key) {
                    s.pin(key, 0);
                }
            } else {
                s.bar.retain(|k| k != &key);
            }
        })?,
        Action::MoveShortcut { key, down } => collection::update(|s| {
            if let Some(i) = s.bar.iter().position(|k| k == &key) {
                let to = adjacent(i, s.bar.len(), down);
                s.bar.swap(i, to);
            }
        })?,
        Action::PinPath { path, replace } => {
            if !path.is_absolute() {
                return Err(invalid("请选择绝对路径。", "Choose an absolute path."));
            }
            result["selection"] = json!(Shortcut::Item(path.clone()));
            collection::update(|s| {
                let mut at = 0;
                if let Some(old) = replace
                    && let Some(i) = s.bar.iter().position(|k| k == &old)
                {
                    at = i;
                    s.bar.remove(i);
                }
                s.place(&[path], &Placement::Pin(at));
            })?;
        }
        Action::SaveRule {
            index,
            rule,
            organize,
        } => {
            let edit = |s: &mut Settings| put_rule(&mut s.rules, index, rule);
            let i = if organize {
                let (i, report) = desktop::organize(edit)?;
                result["message"] = json!(summary(report));
                i
            } else {
                collection::update(edit)??
            };
            result["index"] = json!(i);
        }
        Action::MoveRule { index, down } => {
            let to = collection::update(|s| -> io::Result<_> {
                let i = rule_index(index, s.rules.len())?;
                let to = adjacent(i, s.rules.len(), down);
                s.rules.swap(i, to);
                Ok(to)
            })??;
            result["index"] = json!(to);
        }
        Action::DeleteRule { index } => collection::update(|s| -> io::Result<()> {
            let i = rule_index(index, s.rules.len())?;
            s.rules.remove(i);
            Ok(())
        })??,
    }
    island::collection_changed()?;
    result["state"] = snapshot(&collection::current()?);
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collection::Category;

    #[test]
    fn snapshot_exposes_stable_keys_for_pinned_and_unpinned_groups() {
        let mut settings = Settings::default();
        settings.language = Language::English;
        let folder = settings.add_folder("资料");
        settings.bar = vec![Shortcut::Group(folder)];
        let state = snapshot(&settings);
        assert_eq!(state["locale"], "en");
        assert_eq!(state["bar"][0]["key"], json!({"group": folder}));
        assert_eq!(state["bar"][0]["title"], "资料");
        assert_eq!(state["bar"][0]["pinned"], true);
        let rows = state["bar"].as_array().unwrap();
        assert_eq!(rows.len(), state["groups"].as_array().unwrap().len());
        assert!(rows[1..].iter().all(|row| row["pinned"] == false));
        assert_eq!(state["groups"][0]["key"], json!({"builtin": "office"}));
    }

    #[test]
    fn a_rule_draft_validates_before_replacing_a_saved_rule() {
        let rule = Rule {
            kind: RuleKind::Extension,
            pattern: " .drawio ".into(),
            category: Category::Design,
        };
        let mut rules = Vec::new();
        assert_eq!(put_rule(&mut rules, None, rule.clone()).unwrap(), 0);
        assert_eq!(rules[0].pattern, ".drawio");
        let saved = rules.clone();
        assert!(put_rule(&mut rules, Some(1), rule.clone()).is_err());
        assert!(
            put_rule(
                &mut rules,
                Some(0),
                Rule {
                    pattern: "*.".into(),
                    ..rule
                }
            )
            .is_err()
        );
        assert_eq!(rules, saved);
    }
}
