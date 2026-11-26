use super::*;
use crate::{
    collection::{self, Category, Group, OTHER, Placement, Rule, RuleKind, Shortcut},
    config::Settings,
    language::Language,
};
use std::cell::RefCell;
use windows_sys::Win32::UI::{
    Controls::{DRAWITEMSTRUCT, MEASUREITEMSTRUCT},
    Input::KeyboardAndMouse::{EnableWindow, SetFocus},
    Shell::SetWindowSubclass,
};

mod appearance;
mod editor;

const WIDTH: i32 = 704;
const HEIGHT: i32 = 470;
const SIDE: i32 = 156;
const X: i32 = 180;
const W: i32 = 500;
const STYLE: u32 = WS_CAPTION | WS_SYSMENU | WS_CLIPCHILDREN;
const EX: u32 = WS_EX_DLGMODALFRAME | WS_EX_COMPOSITED;
const GENERAL: i32 = 410;
const BAR: i32 = 411;
const RULES: i32 = 412;
const LANG: i32 = 413;
const APPEARANCE: i32 = 414;
const PREVIEW: i32 = 415;
const MATERIAL: i32 = 483;
const TRANSPARENCY: i32 = 484;
const TOGGLE: i32 = 420;
const SORT: i32 = 421;
const VIEW_OTHER: i32 = 422;
const TILE: i32 = 430;
const BAR_LIST: i32 = 450;
const BAR_UP: i32 = 451;
const BAR_DOWN: i32 = 452;
const BAR_PIN: i32 = 453;
const FOLDER_DELETE: i32 = 454;
const NAME: i32 = 455;
const CREATE: i32 = 456;
const RENAME: i32 = 457;
const DIR: i32 = 458;
const RETARGET: i32 = 459;
const FILE: i32 = 460;
const RULE_LIST: i32 = 470;
const NEW_RULE: i32 = 471;
const RULE_UP: i32 = 472;
const RULE_DOWN: i32 = 473;
const DEL_RULE: i32 = 474;
const KIND: i32 = 475;
const PATTERN: i32 = 476;
const DEST: i32 = 477;
const TRY: i32 = 478;
const SAVE: i32 = 479;
const APPLY: i32 = 480;
const PREVIEW_LIST: i32 = 481;
const BACK: i32 = 482;
const CHOICES: [(i32, Language); 3] = [
    (201, Language::System),
    (202, Language::Chinese),
    (203, Language::English),
];
const SYNC: u32 = WM_APP + 9;

thread_local! {
    static PAGE:Cell<i32>=const {Cell::new(GENERAL)};
    static DATA:RefCell<Settings>=RefCell::default();
    static STATUS:RefCell<String>=const{RefCell::new(String::new())};
    static FONT:Cell<HFONT>=const{Cell::new(null_mut())};
    static BRUSH:Cell<HBRUSH>=const{Cell::new(null_mut())};
}
fn tr(zh: &'static str, en: &'static str) -> &'static str {
    crate::language().pick(zh, en)
}
fn child(hwnd: HWND, id: i32) -> HWND {
    unsafe { GetDlgItem(hwnd, id) }
}
fn send(hwnd: HWND, id: i32, msg: u32, wp: usize, lp: isize) -> isize {
    unsafe { SendMessageW(child(hwnd, id), msg, wp, lp) }
}
fn field(hwnd: HWND, id: i32) -> String {
    window_text(child(hwnd, id))
}
fn window_text(hwnd: HWND) -> String {
    unsafe {
        let mut buf = vec![0; GetWindowTextLengthW(hwnd) as usize + 1];
        let n = GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32);
        String::from_utf16_lossy(&buf[..n as usize])
    }
}
fn choice(hwnd: HWND, id: i32, msg: u32) -> Option<usize> {
    usize::try_from(send(hwnd, id, msg, 0, 0)).ok()
}
fn enable(hwnd: HWND, id: i32, on: bool) {
    unsafe {
        EnableWindow(child(hwnd, id), i32::from(on));
    }
}
fn say(hwnd: HWND, text: String) {
    STATUS.replace(text);
    unsafe {
        InvalidateRect(hwnd, null_mut(), 0);
    }
}
fn page_of(id: i32) -> i32 {
    match id {
        201..=203 => LANG,
        MATERIAL | TRANSPARENCY => APPEARANCE,
        420..=449 => GENERAL,
        450..=460 => BAR,
        470..=480 => RULES,
        481..=482 => PREVIEW,
        _ => 0,
    }
}
fn title(id: i32) -> &'static str {
    match id {
        APPEARANCE => tr("外观", "Appearance"),
        GENERAL => tr("收纳", "Collection"),
        BAR => tr("快捷栏", "Shortcuts"),
        RULES => tr("整理规则", "Sorting rules"),
        LANG => crate::language().text().language,
        PREVIEW => tr("整理预览", "Sort preview"),
        TOGGLE => tr("自动整理新收藏", "Sort new items automatically"),
        SORT => tr("立即整理", "Organize now"),
        VIEW_OTHER => tr("查看其他", "View Other"),
        BAR_UP | RULE_UP => tr("上移", "Move up"),
        BAR_DOWN | RULE_DOWN => tr("下移", "Move down"),
        FOLDER_DELETE => tr("删除文件夹", "Delete folder"),
        CREATE => tr("新建文件夹", "New folder"),
        RENAME => tr("重命名", "Rename"),
        DIR => tr("添加本地目录…", "Add directory…"),
        RETARGET => tr("更换目录…", "Change path…"),
        FILE => tr("固定文件…", "Pin file…"),
        NEW_RULE => tr("新规则", "New rule"),
        DEL_RULE => tr("删除", "Delete"),
        TRY => tr("预览", "Preview"),
        SAVE => tr("保存规则", "Save rule"),
        APPLY => tr("保存并整理", "Save and sort"),
        BACK => tr("返回规则", "Back to rules"),
        201 => crate::language().text().system,
        202 => "简体中文",
        203 => "English",
        _ => "",
    }
}

pub(super) fn open() -> io::Result<()> {
    let mut hwnd = SETTINGS.get();
    if hwnd.is_null() {
        hwnd = window("GuiyuX.Settings", "", STYLE, EX, ISLAND.get(), 0)?;
        if let Err(e) =
            create(hwnd).and_then(|_| dialog::position(hwnd, ISLAND.get(), WIDTH, HEIGHT))
        {
            unsafe {
                DestroyWindow(hwnd);
            }
            return Err(e);
        }
        SETTINGS.set(hwnd);
        dialog::style_caption(hwnd);
        PAGE.set(GENERAL);
    }
    load(hwnd)?;
    unsafe {
        ShowWindow(hwnd, SW_SHOW);
        SetForegroundWindow(hwnd);
        SetFocus(child(
            hwnd,
            if PAGE.get() == PREVIEW {
                BACK
            } else {
                PAGE.get()
            },
        ));
    }
    Ok(())
}
fn create(hwnd: HWND) -> io::Result<()> {
    FONT.set(label_font(13));
    BRUSH.set(unsafe { CreateSolidBrush(ROW_BG) });
    let add = |class: &str, style: u32, id, rect| -> io::Result<()> {
        control(hwnd, class, style | WS_TABSTOP, id, rect)?;
        let h = child(hwnd, id);
        unsafe {
            SendMessageW(h, WM_SETFONT, FONT.get() as usize, 0);
            if class == "BUTTON"
                && SetWindowSubclass(h, Some(appearance::button_proc), id as usize, 0) == 0
            {
                return Err(io::Error::last_os_error());
            }
            if class == "COMBOBOX"
                && SetWindowSubclass(h, Some(appearance::combo_proc), id as usize, 0) == 0
            {
                return Err(io::Error::last_os_error());
            }
            if class != "BUTTON" && !high_contrast() {
                windows_sys::Win32::UI::Controls::SetWindowTheme(
                    h,
                    wide("DarkMode_Explorer").as_ptr(),
                    null_mut(),
                );
            }
        }
        Ok(())
    };
    let button = |id, rect| add("BUTTON", BS_PUSHBUTTON as u32, id, rect);
    for (i, id) in [GENERAL, BAR, RULES, APPEARANCE, LANG]
        .into_iter()
        .enumerate()
    {
        button(id, [10, 76 + i as i32 * 43, SIDE - 20, 38])?;
    }
    add("BUTTON", BS_CHECKBOX as u32, TOGGLE, [X, 66, W, 74])?;
    button(SORT, [X, 154, 126, 35])?;
    button(VIEW_OTHER, [X + 136, 154, 118, 35])?;
    for (i, _) in Category::ALL.iter().enumerate() {
        button(
            TILE + i as i32,
            [X + (i as i32 % 3) * 168, 241 + (i as i32 / 3) * 46, 160, 40],
        )?;
    }
    let list = |id, rect| {
        add(
            "LISTBOX",
            (LBS_NOTIFY | LBS_HASSTRINGS | LBS_OWNERDRAWFIXED | LBS_NOINTEGRALHEIGHT) as u32
                | WS_VSCROLL,
            id,
            rect,
        )
    };
    list(BAR_LIST, [X, 92, W, 144])?;
    for (id, x, w) in [
        (BAR_UP, 0, 96),
        (BAR_DOWN, 102, 96),
        (BAR_PIN, 204, 126),
        (FOLDER_DELETE, 336, 164),
    ] {
        button(id, [X + x, 245, w, 32])?;
    }
    add(
        "EDIT",
        WS_BORDER | ES_AUTOHSCROLL as u32,
        NAME,
        [X, 313, 240, 32],
    )?;
    button(CREATE, [X + 248, 312, 130, 34])?;
    button(RENAME, [X + 386, 312, 114, 34])?;
    for (id, x, w) in [(DIR, 0, 170), (RETARGET, 178, 164), (FILE, 350, 150)] {
        button(id, [X + x, 375, w, 34])?;
    }

    list(RULE_LIST, [X, 92, W, 112])?;
    for (id, x, w) in [
        (NEW_RULE, 0, 119),
        (RULE_UP, 127, 119),
        (RULE_DOWN, 254, 119),
        (DEL_RULE, 381, 119),
    ] {
        button(id, [X + x, 212, w, 32])?;
    }
    let combo = |id, rect| add("COMBOBOX", CBS_DROPDOWNLIST as u32 | WS_VSCROLL, id, rect);
    combo(MATERIAL, [X, 115, 230, 136])?;
    combo(TRANSPARENCY, [X + 252, 115, 200, 176])?;
    combo(KIND, [X, 280, 146, 175])?;
    add(
        "EDIT",
        WS_BORDER | ES_AUTOHSCROLL as u32,
        PATTERN,
        [X + 154, 280, 183, 30],
    )?;
    combo(DEST, [X + 345, 280, 155, 350])?;
    for (id, x, w) in [(TRY, 0, 110), (SAVE, 241, 116), (APPLY, 365, 135)] {
        button(id, [X + x, 332, w, 36])?;
    }
    list(PREVIEW_LIST, [X, 96, W, 277])?;
    button(BACK, [X, 387, 150, 36])?;
    for (i, (id, _)) in CHOICES.iter().enumerate() {
        add(
            "BUTTON",
            BS_RADIOBUTTON as u32,
            *id,
            [X, 86 + i as i32 * 60, W, 50],
        )?;
    }
    Ok(())
}

fn load(hwnd: HWND) -> io::Result<()> {
    DATA.replace(collection::current()?);
    layout(hwnd);
    Ok(())
}
fn layout(hwnd: HWND) {
    let page = PAGE.get();
    let lang = crate::language();
    caption(hwnd, &format!("GuiyuX · {}", lang.text().settings));
    for id in (GENERAL..=APPEARANCE)
        .chain(TOGGLE..=TILE + 10)
        .chain(BAR_LIST..=FILE)
        .chain(RULE_LIST..=TRANSPARENCY)
        .chain(201..=203)
    {
        let h = child(hwnd, id);
        if h.is_null() {
            continue;
        }
        let visible = page_of(id) == 0 || page_of(id) == page;
        let name = if (TILE..TILE + 11).contains(&id) {
            Category::ALL[(id - TILE) as usize].name(lang)
        } else {
            title(id)
        };
        if !name.is_empty() {
            caption(h, name);
        }
        unsafe {
            ShowWindow(h, if visible { SW_SHOWNA } else { SW_HIDE });
        }
    }
    match page {
        GENERAL => {
            send(
                hwnd,
                TOGGLE,
                BM_SETCHECK,
                usize::from(DATA.with_borrow(|s| s.auto_sort)),
                0,
            );
        }
        APPEARANCE => {
            let style = DATA.with_borrow(|s| s.menu);
            editor::populate(
                hwnd,
                MATERIAL,
                &[
                    tr("毛玻璃", "Frosted glass").into(),
                    tr("透明", "Clear").into(),
                ],
                Some(usize::from(!style.frosted)),
                true,
            );
            let levels = [0, 15, 30, 45, 60].map(|n| format!("{n}%"));
            editor::populate(
                hwnd,
                TRANSPARENCY,
                &levels,
                Some(usize::from(style.transparency / 15)),
                true,
            );
        }
        BAR => editor::bar_list(hwnd),
        RULES => editor::rule_list(hwnd),
        LANG => {
            for (id, choice) in CHOICES {
                send(hwnd, id, BM_SETCHECK, usize::from(choice == lang), 0);
            }
        }
        _ => {}
    }
    unsafe {
        RedrawWindow(
            hwnd,
            null_mut(),
            null_mut(),
            RDW_INVALIDATE | RDW_ALLCHILDREN,
        );
    }
}
fn switch(hwnd: HWND, page: i32) {
    PAGE.set(page);
    STATUS.with_borrow_mut(String::clear);
    layout(hwnd);
}
pub(super) fn notify() {
    if !SETTINGS.get().is_null() {
        unsafe {
            PostMessageW(SETTINGS.get(), SYNC, 0, 0);
        }
    }
}
fn saved(hwnd: HWND) -> io::Result<()> {
    load(hwnd)?;
    island::collection_changed()
}
pub(super) fn organize() -> io::Result<()> {
    let (changed, other) = collection::update(|s| (s.organize(), s.count(OTHER)))?;
    let h = SETTINGS.get();
    if h.is_null() {
        open()?;
    } else {
        load(h)?;
        unsafe {
            SetForegroundWindow(h);
        }
    }
    let h = SETTINGS.get();
    if PAGE.get() != GENERAL {
        switch(h, GENERAL);
    }
    island::collection_changed()?;
    say(h, summary(changed, other));
    Ok(())
}
fn summary(changed: usize, other: usize) -> String {
    if crate::language().is_chinese() {
        format!("已调整 {changed} 项 · 其他 {other} 项")
    } else {
        format!("{changed} moved · {other} in Other")
    }
}
fn command(hwnd: HWND, id: i32, event: u32) -> io::Result<()> {
    match (id, event) {
        (IDCANCEL, _) => {
            if PAGE.get() == PREVIEW {
                switch(hwnd, RULES);
                editor::restore_draft(hwnd);
            } else {
                unsafe {
                    DestroyWindow(hwnd);
                }
            }
        }
        (GENERAL..=APPEARANCE, 0) => switch(hwnd, id),
        (MATERIAL | TRANSPARENCY, e) if e == CBN_SELCHANGE => {
            if let Some(index) = choice(hwnd, id, CB_GETCURSEL) {
                let style = collection::update(|s| {
                    if id == MATERIAL {
                        s.menu.frosted = index == 0;
                    } else {
                        s.menu.transparency = (index.min(4) * 15) as u8;
                    }
                    s.menu
                })?;
                MENU_APPEARANCE.set(style);
                DATA.with_borrow_mut(|s| s.menu = style);
            }
        }
        (TOGGLE, 0) => {
            collection::update(|s| s.auto_sort = !s.auto_sort)?;
            saved(hwnd)?;
        }
        (SORT, 0) => return organize(),
        (VIEW_OTHER, 0) => return island::show_group(OTHER),
        (id, 0) if (TILE..TILE + 11).contains(&id) => {
            return island::show_group(Group::Builtin(Category::ALL[(id - TILE) as usize]));
        }
        (201..=203, 0) => {
            let choice = CHOICES[(id - 201) as usize].1;
            crate::change_language(choice)?;
            saved(hwnd)?;
        }
        (BACK, 0) => {
            PAGE.set(RULES);
            layout(hwnd);
            editor::restore_draft(hwnd);
        }
        (BAR_LIST, e) if e == LBN_SELCHANGE as u32 => editor::select_bar(hwnd),
        (RULE_LIST, e) if e == LBN_SELCHANGE as u32 => editor::select_rule(hwnd),
        (BAR_UP..=FILE, 0) if id != NAME => return editor::bar_action(hwnd, id),
        (NEW_RULE..=APPLY, 0) if !matches!(id, KIND | PATTERN | DEST) => {
            return editor::rule_action(hwnd, id);
        }
        _ => {}
    }
    Ok(())
}

pub(super) unsafe extern "system" fn procedure(
    hwnd: HWND,
    msg: u32,
    wp: WPARAM,
    lp: LPARAM,
) -> LRESULT {
    unsafe {
        match msg {
            WM_ERASEBKGND => 1,
            WM_PAINT => {
                let mut ps = PAINTSTRUCT::default();
                let dc = BeginPaint(hwnd, &mut ps);
                appearance::panel(hwnd, dc);
                EndPaint(hwnd, &ps);
                0
            }
            WM_PRINTCLIENT => {
                appearance::panel(hwnd, wp as HDC);
                0
            }
            WM_MEASUREITEM => {
                let d = &mut *(lp as *mut MEASUREITEMSTRUCT);
                d.itemHeight = 28;
                1
            }
            WM_DRAWITEM => {
                appearance::list_row(&*(lp as *const DRAWITEMSTRUCT));
                1
            }
            WM_CTLCOLORLISTBOX | WM_CTLCOLOREDIT | WM_CTLCOLORSTATIC if !high_contrast() => {
                SetTextColor(wp as HDC, INK);
                SetBkColor(wp as HDC, ROW_BG);
                BRUSH.get() as isize
            }
            WM_COMMAND => {
                if let Err(e) = command(hwnd, (wp & 0xffff) as i32, (wp >> 16) as u32) {
                    report(&e);
                }
                0
            }
            SYNC => {
                if let Ok(s) = collection::current() {
                    DATA.replace(s);
                    if PAGE.get() == GENERAL || PAGE.get() == BAR {
                        layout(hwnd);
                    }
                }
                0
            }
            WM_THEMECHANGED | WM_SETTINGCHANGE => {
                dialog::style_caption(hwnd);
                layout(hwnd);
                0
            }
            WM_DESTROY => {
                SETTINGS.set(null_mut());
                STATUS.with_borrow_mut(String::clear);
                0
            }
            WM_NCDESTROY => {
                DeleteObject(FONT.replace(null_mut()));
                DeleteObject(BRUSH.replace(null_mut()));
                DefWindowProcW(hwnd, msg, wp, lp)
            }
            _ => DefWindowProcW(hwnd, msg, wp, lp),
        }
    }
}
