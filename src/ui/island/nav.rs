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
pub(super) fn page_size() -> usize {
    if VIEW.get() == View::More {
        GRID_PAGE
    } else {
        FILE_ROWS
    }
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
    if VIEW.get() == View::Home || !(first..first + page_size() as i32).contains(&id) {
        return None;
    }
    row(PAGE.get() * page_size() + (id - first) as usize)
}
pub(super) fn auto_sort() -> bool {
    DATA.with_borrow(|s| s.auto_sort)
}
pub(super) fn reload() -> io::Result<()> {
    DATA.replace(collection::current()?);
    rebuild();
    Ok(())
}
pub(super) fn rebuild() {
    let lang = crate::language();
    DATA.with_borrow(|data| {
        let make_row = |entry: Shortcut| {
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
                    let group = match VIEW.get() {
                        View::Group(g) => Some(g),
                        _ => data.items.iter().find(|i| &i.path == p).map(|i| i.group),
                    };
                    let cat = match group {
                        Some(Group::Builtin(c)) => c,
                        _ => Category::Other,
                    };
                    (cat, p.parent().unwrap_or(p).to_string_lossy().into_owned())
                }
            };
            Row {
                title: data.title(&entry, lang),
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
                .map(&make_row)
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
        let mut entries = Vec::new();
        match VIEW.get() {
            View::Home => {}
            View::More => {
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
            }
            View::Group(g) => entries.extend(
                data.items
                    .iter()
                    .filter(|it| it.group == g)
                    .map(|it| Shortcut::Item(it.path.clone())),
            ),
        }
        PAGE.set(
            PAGE.get()
                .min(entries.len().saturating_sub(1) / page_size()),
        );
        ROW_DATA.replace(entries.into_iter().map(make_row).collect());
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
                        PAGE.set(index / page_size());
                        refresh();
                        content_first() + (index % page_size()) as i32
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
