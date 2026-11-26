use super::*;

thread_local! {static PREVIEW_RULE:RefCell<Option<(Option<usize>,Rule)>>=const{RefCell::new(None)};}
fn entries(s: &Settings) -> Vec<Shortcut> {
    let mut all = s.bar.clone();
    for group in s.groups() {
        let key = Shortcut::Group(group);
        if !all.contains(&key) {
            all.push(key);
        }
    }
    all
}
pub(super) fn populate(
    hwnd: HWND,
    id: i32,
    values: &[String],
    selected: Option<usize>,
    combo: bool,
) {
    let (reset, add, select) = if combo {
        (CB_RESETCONTENT, CB_ADDSTRING, CB_SETCURSEL)
    } else {
        (LB_RESETCONTENT, LB_ADDSTRING, LB_SETCURSEL)
    };
    send(hwnd, id, WM_SETREDRAW, 0, 0);
    send(hwnd, id, reset, 0, 0);
    for value in values {
        send(hwnd, id, add, 0, wide(value).as_ptr() as isize);
    }
    send(
        hwnd,
        id,
        select,
        selected.filter(|i| *i < values.len()).unwrap_or(usize::MAX),
        0,
    );
    send(hwnd, id, WM_SETREDRAW, 1, 0);
    unsafe {
        InvalidateRect(child(hwnd, id), null_mut(), 0);
    }
}
fn bar_index(hwnd: HWND) -> Option<usize> {
    choice(hwnd, BAR_LIST, LB_GETCURSEL)
}
fn selected_bar(hwnd: HWND) -> Option<Shortcut> {
    let n = bar_index(hwnd)?;
    DATA.with_borrow(|s| entries(s).get(n).cloned())
}
pub(super) fn bar_list(hwnd: HWND) {
    let prev = bar_index(hwnd);
    let rows: Vec<_> = DATA.with_borrow(|s| {
        entries(s)
            .iter()
            .enumerate()
            .map(|(n, key)| {
                let mut title = s.title(key, crate::language());
                if let Shortcut::Item(path) = key {
                    title = format!("{title}    {}", path.display());
                }
                if n >= s.bar.len() {
                    title.push_str(tr("  ·  未固定", "  ·  Unpinned"));
                }
                title
            })
            .collect()
    });
    populate(
        hwnd,
        BAR_LIST,
        &rows,
        prev.or(Some(0))
            .map(|i| i.min(rows.len().saturating_sub(1))),
        false,
    );
    select_bar(hwnd);
}
pub(super) fn select_bar(hwnd: HWND) {
    let sel = selected_bar(hwnd);
    let index = bar_index(hwnd);
    DATA.with_borrow(|s|{
        let pinned=index.is_some_and(|i|i<s.bar.len());
        let folder=matches!(sel,Some(Shortcut::Group(Group::Folder(_))));
        let local=matches!(sel,Some(Shortcut::Item(ref p)) if p.is_dir() || s.items.iter().any(|i|&i.path==p && i.directory));
        enable(hwnd,BAR_UP,pinned && index!=Some(0));
        enable(hwnd,BAR_DOWN,pinned && index.is_some_and(|i|i+1<s.bar.len()));
        enable(hwnd,BAR_PIN,sel.is_some());enable(hwnd,FOLDER_DELETE,folder);enable(hwnd,RENAME,folder);enable(hwnd,RETARGET,local);
        caption(child(hwnd,BAR_PIN),if pinned{tr("取消固定","Unpin")}else{tr("固定到快捷栏","Pin to bar")});
        let name=sel.as_ref().filter(|_|folder).map(|skey|s.title(skey,crate::language())).unwrap_or_default();
        caption(child(hwnd,NAME),&name);
    });
}
fn pick_bar(hwnd: HWND, key: &Shortcut) {
    if let Some(i) = DATA.with_borrow(|s| entries(s).iter().position(|k| k == key)) {
        send(hwnd, BAR_LIST, LB_SETCURSEL, i, 0);
        select_bar(hwnd);
    }
}
fn invalid(hwnd: HWND, id: i32, message: &str) {
    say(hwnd, message.to_owned());
    unsafe {
        SetFocus(child(hwnd, id));
    }
}
pub(super) fn bar_action(hwnd: HWND, id: i32) -> io::Result<()> {
    let selected = selected_bar(hwnd);
    let mut choose = selected.clone();
    match id {
        CREATE | RENAME => {
            let raw = field(hwnd, NAME);
            let name = raw.trim();
            if name.is_empty() {
                invalid(hwnd, NAME, tr("请输入文件夹名称。", "Enter a folder name."));
                return Ok(());
            }
            if id == CREATE {
                choose = Some(Shortcut::Group(collection::update(|s| s.add_folder(name))?));
            } else if let Some(Shortcut::Group(Group::Folder(id))) = selected {
                collection::update(|s| {
                    if let Some(f) = s.folders.iter_mut().find(|f| f.id == id) {
                        f.name = name.into();
                    }
                })?;
            }
        }
        BAR_UP | BAR_DOWN => {
            if let Some(key) = selected {
                collection::update(|s| {
                    if let Some(n) = s.bar.iter().position(|k| k == &key) {
                        let to = if id == BAR_UP {
                            n.saturating_sub(1)
                        } else {
                            (n + 1).min(s.bar.len() - 1)
                        };
                        s.bar.swap(n, to);
                    }
                })?;
            }
        }
        BAR_PIN => {
            if let Some(key) = selected {
                collection::update(|s| {
                    if s.bar.contains(&key) {
                        s.bar.retain(|k| k != &key);
                    } else {
                        s.pin(key, 0)
                    }
                })?;
            }
        }
        FOLDER_DELETE => {
            if let Some(Shortcut::Group(Group::Folder(id))) = selected {
                let answer = unsafe {
                    MessageBoxW(
                        hwnd,
                        wide(tr(
                            "删除这个收纳文件夹？里面的图标将移到「其他」。",
                            "Delete this folder? Its items will move to Other.",
                        ))
                        .as_ptr(),
                        wide(title(FOLDER_DELETE)).as_ptr(),
                        MB_OKCANCEL | MB_DEFBUTTON2,
                    )
                };
                if answer != IDOK {
                    return Ok(());
                }
                collection::update(|s| s.delete_folder(id))?;
                choose = None;
            }
        }
        DIR | RETARGET | FILE => {
            let Some(path) = crate::shell::choose(hwnd, id != FILE)? else {
                return Ok(());
            };
            choose = Some(Shortcut::Item(path.clone()));
            collection::update(|s| {
                let mut at = 0;
                if id == RETARGET
                    && let Some(old) = &selected
                    && let Some(i) = s.bar.iter().position(|k| k == old)
                {
                    at = i;
                    s.bar.remove(i);
                }
                s.place(&[path], &Placement::Pin(at));
            })?;
        }
        _ => return Ok(()),
    }
    saved(hwnd)?;
    if let Some(key) = choose {
        pick_bar(hwnd, &key);
    }
    say(hwnd, tr("快捷栏已保存", "Shortcuts saved").into());
    Ok(())
}

fn kind_name(kind: RuleKind) -> &'static str {
    match kind {
        RuleKind::Extension => tr("扩展名", "Extension"),
        RuleKind::Name => tr("完整文件名", "File name"),
        RuleKind::Application => tr("应用标识", "Application ID"),
    }
}
fn rule_index(hwnd: HWND) -> Option<usize> {
    choice(hwnd, RULE_LIST, LB_GETCURSEL)
}
pub(super) fn rule_list(hwnd: HWND) {
    let selected = rule_index(hwnd);
    let lang = crate::language();
    let rows: Vec<_> = DATA.with_borrow(|s| {
        s.rules
            .iter()
            .map(|r| {
                format!(
                    "{}  ·  {}   →   {}",
                    kind_name(r.kind),
                    r.pattern,
                    r.category.name(lang)
                )
            })
            .collect()
    });
    populate(hwnd, RULE_LIST, &rows, selected, false);
    populate(
        hwnd,
        KIND,
        &RuleKind::ALL.map(|k| kind_name(k).to_owned()),
        Some(0),
        true,
    );
    populate(
        hwnd,
        DEST,
        &Category::ALL.map(|c| c.name(lang).to_owned()),
        Some(0),
        true,
    );
    select_rule(hwnd);
}
fn form(hwnd: HWND, rule: &Rule) {
    let kind = RuleKind::ALL.iter().position(|k| *k == rule.kind).unwrap();
    send(hwnd, KIND, CB_SETCURSEL, kind, 0);
    send(hwnd, DEST, CB_SETCURSEL, rule.category as usize, 0);
    caption(child(hwnd, PATTERN), &rule.pattern);
}
pub(super) fn select_rule(hwnd: HWND) {
    let index = rule_index(hwnd);
    DATA.with_borrow(|s| {
        let rule = index.and_then(|i| s.rules.get(i)).cloned().unwrap_or(Rule {
            kind: RuleKind::Extension,
            pattern: String::new(),
            category: Category::Documents,
        });
        form(hwnd, &rule);
        enable(hwnd, RULE_UP, index.is_some_and(|i| i > 0));
        enable(
            hwnd,
            RULE_DOWN,
            index.is_some_and(|i| i + 1 < s.rules.len()),
        );
        enable(hwnd, DEL_RULE, index.is_some());
    });
    if DATA.with_borrow(|s| s.rules.is_empty()) {
        say(
            hwnd,
            tr(
                "当前使用内置规则。填写一条规则即可添加。",
                "Built-in rules are active. Enter a rule to add your own.",
            )
            .into(),
        );
    }
}
pub(super) fn restore_draft(hwnd: HWND) {
    if let Some((selected, rule)) = PREVIEW_RULE.take() {
        send(
            hwnd,
            RULE_LIST,
            LB_SETCURSEL,
            selected.unwrap_or(usize::MAX),
            0,
        );
        select_rule(hwnd);
        form(hwnd, &rule);
    }
}
fn input_rule(hwnd: HWND) -> Option<Rule> {
    let pattern = field(hwnd, PATTERN).trim().to_owned();
    let kind = *RuleKind::ALL.get(choice(hwnd, KIND, CB_GETCURSEL)?)?;
    if pattern.is_empty()
        || (kind == RuleKind::Extension
            && pattern
                .trim_start_matches("*.")
                .trim_start_matches('.')
                .is_empty())
    {
        invalid(
            hwnd,
            PATTERN,
            tr(
                "请输入匹配值，例如 .drawio。",
                "Enter a match value, such as .drawio.",
            ),
        );
        return None;
    }
    let category = *Category::ALL.get(choice(hwnd, DEST, CB_GETCURSEL)?)?;
    Some(Rule {
        kind,
        pattern,
        category,
    })
}
fn set_rule(rules: &mut Vec<Rule>, index: Option<usize>, rule: Rule) -> usize {
    if let Some(i) = index
        && i < rules.len()
    {
        rules[i] = rule;
        i
    } else {
        rules.push(rule);
        rules.len() - 1
    }
}
fn preview(hwnd: HWND, index: Option<usize>, rule: Rule) {
    let lang = crate::language();
    let rows: Vec<String> = DATA.with_borrow(|s| {
        let mut rules = s.rules.clone();
        set_rule(&mut rules, index, rule.clone());
        s.changes(&rules)
            .iter()
            .map(|(i, to)| {
                let item = &s.items[*i];
                format!(
                    "{}  →  {}    ·    {}",
                    s.group_name(item.group, lang),
                    s.group_name(*to, lang),
                    item.path.display()
                )
            })
            .collect()
    });
    PREVIEW_RULE.replace(Some((index, rule)));
    switch(hwnd, PREVIEW);
    populate(hwnd, PREVIEW_LIST, &rows, None, false);
    say(
        hwnd,
        format!("{} {}", rows.len(), tr("项归属将改变", "items would move")),
    );
    unsafe {
        SetFocus(child(hwnd, BACK));
    }
}
pub(super) fn rule_action(hwnd: HWND, id: i32) -> io::Result<()> {
    let index = rule_index(hwnd);
    let selected;
    let mut message = tr("规则已保存", "Rules saved").to_owned();
    match id {
        NEW_RULE => {
            send(hwnd, RULE_LIST, LB_SETCURSEL, usize::MAX, 0);
            select_rule(hwnd);
            unsafe {
                SetFocus(child(hwnd, PATTERN));
            }
            return Ok(());
        }
        RULE_UP | RULE_DOWN | DEL_RULE => {
            let Some(i) = index else { return Ok(()) };
            selected = collection::update(|s| {
                if i >= s.rules.len() {
                    return None;
                }
                if id == DEL_RULE {
                    s.rules.remove(i);
                    return None;
                }
                let to = if id == RULE_UP {
                    i.saturating_sub(1)
                } else {
                    (i + 1).min(s.rules.len() - 1)
                };
                s.rules.swap(i, to);
                Some(to)
            })?;
        }
        TRY | SAVE | APPLY => {
            let Some(rule) = input_rule(hwnd) else {
                return Ok(());
            };
            if id == TRY {
                preview(hwnd, index, rule);
                return Ok(());
            }
            let (at, changed, other) = collection::update(|s| {
                let at = set_rule(&mut s.rules, index, rule);
                let count = if id == APPLY { s.organize() } else { 0 };
                (at, count, s.count(OTHER))
            })?;
            selected = Some(at);
            if id == APPLY {
                message = summary(changed, other);
            }
        }
        _ => return Ok(()),
    }
    saved(hwnd)?;
    send(
        hwnd,
        RULE_LIST,
        LB_SETCURSEL,
        selected.unwrap_or(usize::MAX),
        0,
    );
    select_rule(hwnd);
    say(hwnd, message);
    Ok(())
}
