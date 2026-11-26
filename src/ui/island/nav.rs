use super::*;
use crate::collection::{self, Category, Group, OTHER, Shortcut};
use crate::config::Settings;

#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub(super) enum View {
    #[default]
    Home,
    More,
    Group(Group),
}
#[derive(Clone, PartialEq, Eq)]
pub(super) enum Action {
    Open(Shortcut),
    More,
}
#[derive(Clone)]
pub(super) struct Row {
    pub title: String,
    pub detail: String,
    pub symbol: art::Symbol,
    pub action: Action,
}

impl Row {
    pub(super) fn full_name(&self) -> std::borrow::Cow<'_, str> {
        if let Action::Open(Shortcut::Item(p)) = &self.action {
            return p.file_name().unwrap_or(p.as_os_str()).to_string_lossy();
        }
        (&self.title).into()
    }
}

fn shortcut_title(name: &str) -> &str {
    match name.rsplit_once('.') {
        Some((stem, ext))
            if !stem.is_empty()
                && (ext.eq_ignore_ascii_case("lnk") || ext.eq_ignore_ascii_case("url")) =>
        {
            stem
        }
        _ => name,
    }
}

thread_local! {
    static DATA: RefCell<Settings> = RefCell::default();
    pub(super) static VIEW: Cell<View> = const { Cell::new(View::Home) };
    static PARENT: Cell<View> = const { Cell::new(View::Home) };
    static BAR_DATA:RefCell<Vec<Row>>=const{RefCell::new(Vec::new())};
    static ROW_DATA: RefCell<Vec<Row>> = const { RefCell::new(Vec::new()) };
}

pub(super) fn len() -> usize {
    ROW_DATA.with_borrow(Vec::len)
}
pub(super) fn row(index: usize) -> Option<Row> {
    ROW_DATA.with_borrow(|r| r.get(index).cloned())
}
pub(super) fn toolbar() -> bool {
    !matches!(VIEW.get(), View::Group(_))
}
pub(super) fn bar_len() -> usize {
    BAR_DATA.with_borrow(Vec::len)
}
pub(super) fn bar_slot(id: i32) -> bool {
    toolbar() && (FIRST..FIRST + BAR_SLOTS as i32).contains(&id)
}
pub(super) fn content_first() -> i32 {
    if VIEW.get() == View::More {
        FIRST + BAR_SLOTS as i32
    } else {
        FIRST
    }
}
pub(super) fn button_row(id: i32) -> Option<Row> {
    if bar_slot(id) {
        return BAR_DATA.with_borrow(|r| r.get((id - FIRST) as usize).cloned());
    }
    let first = content_first();
    if VIEW.get() == View::Home || !(first..first + GRID_PAGE as i32).contains(&id) {
        return None;
    }
    row(PAGE.get() * GRID_PAGE + (id - first) as usize)
}
pub(super) fn auto_sort() -> bool {
    DATA.with_borrow(|s| s.auto_sort)
}
pub(super) fn dock() -> Dock {
    DATA.with_borrow(|s| s.dock)
}
pub(super) fn reload() -> io::Result<()> {
    set_data(collection::current()?);
    Ok(())
}
pub(super) fn set_data(settings: Settings) {
    DATA.replace(settings);
    rebuild();
}
pub(super) fn rebuild() {
    let lang = crate::language();
    DATA.with_borrow(|data| {
        let make_row = |entry: Shortcut, item: Option<&collection::Item>| {
            let mut title = data.title(&entry, lang);
            let (category, detail) = match &entry {
                Shortcut::Group(g) => {
                    let category = match g {
                        Group::Builtin(cat) => *cat,
                        Group::Folder(_) => Category::Folders,
                    };
                    (
                        category,
                        format!("{} {}", data.count(*g), lang.pick("项收藏", "items")),
                    )
                }
                Shortcut::Item(p) => {
                    let item = item.or_else(|| data.items.iter().find(|i| &i.path == p));
                    let cat = if item.is_some_and(|i| i.directory) {
                        Category::Folders
                    } else {
                        title.truncate(shortcut_title(&title).len());
                        match item.map(|i| i.group) {
                            Some(Group::Builtin(c)) => c,
                            _ => Category::Other,
                        }
                    };
                    (cat, p.parent().unwrap_or(p).to_string_lossy().into_owned())
                }
            };
            Row {
                title,
                detail,
                symbol: art::Symbol::Category(category),
                action: Action::Open(entry),
            }
        };
        if toolbar() {
            let mut bar: Vec<_> = data
                .bar
                .iter()
                .take(BAR_CAPACITY)
                .cloned()
                .map(|entry| make_row(entry, None))
                .collect();
            bar.push(Row {
                title: if VIEW.get() == View::More {
                    lang.pick("折叠更多", "Show less")
                } else {
                    lang.pick("更多", "More")
                }
                .into(),
                detail: String::new(),
                symbol: if VIEW.get() == View::More {
                    art::Symbol::Less
                } else {
                    art::Symbol::More
                },
                action: Action::More,
            });
            BAR_DATA.replace(bar);
        }
        let rows: Vec<_> = match VIEW.get() {
            View::Home => Vec::new(),
            View::More => {
                let mut entries = Vec::new();
                let other = Shortcut::Group(OTHER);
                entries.extend(
                    data.bar
                        .iter()
                        .skip(BAR_CAPACITY)
                        .filter(|s| **s != other)
                        .cloned(),
                );
                for g in data.groups() {
                    let key = Shortcut::Group(g);
                    if g != OTHER && !data.bar.contains(&key) {
                        entries.push(key);
                    }
                }
                if !data.bar.iter().take(BAR_CAPACITY).any(|key| key == &other) {
                    entries.push(other);
                }
                entries
                    .into_iter()
                    .map(|entry| make_row(entry, None))
                    .collect()
            }
            View::Group(g) => data
                .items
                .iter()
                .filter(|it| it.group == g)
                .map(|it| make_row(Shortcut::Item(it.path.clone()), Some(it)))
                .collect(),
        };
        PAGE.set(PAGE.get().min(rows.len().saturating_sub(1) / GRID_PAGE));
        ROW_DATA.replace(rows);
    });
}

pub(super) fn show(view: View) {
    motion::clear_hover();
    let old = VIEW.replace(view);
    if matches!(view, View::Group(_)) && !matches!(old, View::Group(_)) {
        PARENT.set(old);
    }
    PAGE.set(0);
    FEEDBACK.set(None);
    rebuild();
    start_transition(
        ISLAND.get(),
        false,
        matches!(
            (old, view),
            (View::Home, View::More) | (View::More, View::Home)
        ),
    );
    refresh();
}
pub(super) fn back() {
    match VIEW.get() {
        View::Home => collapse(ISLAND.get()),
        View::More => {
            show(View::Home);
            unsafe {
                windows_sys::Win32::UI::Input::KeyboardAndMouse::SetFocus(GetDlgItem(
                    ISLAND.get(),
                    FIRST + bar_len() as i32 - 1,
                ));
            }
        }
        View::Group(g) => {
            show(PARENT.get());
            let key = Action::Open(Shortcut::Group(g));
            let bar = BAR_DATA.with_borrow(|r| r.iter().position(|r| r.action == key));
            let id = if let Some(i) = bar {
                Some(FIRST + i as i32)
            } else {
                ROW_DATA
                    .with_borrow(|r| r.iter().position(|r| r.action == key))
                    .map(|index| {
                        PAGE.set(index / GRID_PAGE);
                        refresh();
                        content_first() + (index % GRID_PAGE) as i32
                    })
            };
            if let Some(id) = id {
                unsafe {
                    windows_sys::Win32::UI::Input::KeyboardAndMouse::SetFocus(GetDlgItem(
                        ISLAND.get(),
                        id,
                    ));
                }
            }
        }
    }
}
pub(super) fn activate(hwnd: HWND, id: i32) -> io::Result<()> {
    let Some(row) = button_row(id) else {
        return Ok(());
    };
    match row.action {
        Action::More => {
            show(if VIEW.get() == View::More {
                View::Home
            } else {
                View::More
            });
            return Ok(());
        }
        Action::Open(Shortcut::Group(g)) => show(View::Group(g)),
        Action::Open(Shortcut::Item(p)) => {
            open_item(hwnd, &p)?;
            collapse(hwnd);
            return Ok(());
        }
    }
    unsafe {
        windows_sys::Win32::UI::Input::KeyboardAndMouse::SetFocus(GetDlgItem(hwnd, HEADER));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shorter_launch_labels_preserve_names_and_paths_for_hover_and_open() {
        let cases = [
            ("团队工具.LNK", false, "团队工具"),
            ("帮助中心.url", false, "帮助中心"),
            ("记录 01.docx", false, "记录 01.docx"),
            ("资料.lnk", true, "资料.lnk"),
        ];
        let mut data = Settings::default();
        let parent = std::path::Path::new("原始位置");
        for (name, directory, _) in cases {
            data.items.push(collection::Item {
                path: parent.join(name),
                group: OTHER,
                manual: true,
                directory,
            });
        }
        VIEW.set(View::Group(OTHER));
        DATA.replace(data);
        rebuild();
        for (i, (name, _, label)) in cases.into_iter().enumerate() {
            let row = button_row(FIRST + i as i32).unwrap();
            assert_eq!(row.title, label);
            assert_eq!(row.full_name(), name);
            assert_eq!(row.detail, parent.to_string_lossy());
            assert!(row.action == Action::Open(Shortcut::Item(parent.join(name))));
        }
    }
}
