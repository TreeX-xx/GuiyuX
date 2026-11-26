use super::*;
use windows_sys::Win32::UI::{
    Controls::{ODS_FOCUS, ODS_NOFOCUSRECT, ODS_SELECTED},
    Input::KeyboardAndMouse::{GetFocus, IsWindowEnabled, VK_DOWN, VK_LEFT, VK_RIGHT, VK_UP},
    Shell::{DefSubclassProc, RemoveWindowSubclass},
};
const SIDEBAR: u32 = 0x262424;
const SURFACE: u32 = 0x292727;
const SELECT: u32 = 0x3c3939;

fn text(dc: HDC, rect: RECT, value: &str, size: i32, color: u32) {
    interface_text(dc, rect, value, size, color, DT_LEFT);
}
fn palette() -> (u32, u32, u32) {
    if high_contrast() {
        unsafe {
            (
                GetSysColor(COLOR_WINDOW),
                GetSysColor(COLOR_WINDOWTEXT),
                GetSysColor(COLOR_WINDOWTEXT),
            )
        }
    } else {
        (BG, INK, MUTED)
    }
}
pub(super) fn panel(hwnd: HWND, dc: HDC) {
    let (bg, ink, muted) = palette();
    let mut r = RECT::default();
    unsafe {
        GetClientRect(hwnd, &mut r);
    }
    fill(dc, &r, bg);
    fill(
        dc,
        &area(0, 0, SIDE, r.bottom),
        if high_contrast() { bg } else { SIDEBAR },
    );
    fill(
        dc,
        &area(SIDE - 1, 0, SIDE, r.bottom),
        if high_contrast() { ink } else { EDGE },
    );
    text(
        dc,
        area(20, 17, SIDE - 12, 54),
        crate::language().text().settings,
        18,
        ink,
    );
    let page = PAGE.get();
    text(dc, area(X, 13, WIDTH - 22, 55), title(page), 20, ink);
    let hint = match page {
        APPEARANCE => tr(
            "调整右键菜单背景，下次打开菜单时生效。",
            "Changes apply the next time you open a context menu.",
        ),
        BAR => tr(
            "前六个入口显示在胶囊上，点击「更多」可展开其余入口。",
            "The first six shortcuts appear on the bar. Select More to expand it.",
        ),
        RULES => tr(
            "从上到下匹配，手动放置的图标保留原位。",
            "Rules match from top to bottom. Manually placed items stay put.",
        ),
        PREVIEW => tr(
            "现有收藏应用当前规则后的归属变化。",
            "How the current rules would change your saved items.",
        ),
        LANG => tr(
            "选择界面语言，更改立即生效。",
            "Choose the language used throughout GuiyuX.",
        ),
        _ => "",
    };
    if !hint.is_empty() {
        text(dc, area(X, 51, WIDTH - 22, 79), hint, 13, muted);
    }
    match page {
        APPEARANCE => {
            text(
                dc,
                area(X, 82, X + 230, 110),
                tr("背景效果", "Background effect"),
                13,
                ink,
            );
            text(
                dc,
                area(X + 252, 82, X + W, 110),
                tr("背景透明度", "Background transparency"),
                13,
                ink,
            );
        }
        GENERAL => {
            let total = DATA.with_borrow(|s| s.items.len());
            let label = if crate::language().is_chinese() {
                format!("收藏分类 · {total} 项")
            } else {
                format!("Categories · {total} items")
            };
            text(dc, area(X, 201, X + W, 235), &label, 15, ink);
        }
        BAR => {
            text(
                dc,
                area(X, 282, X + W, 309),
                tr("收纳文件夹名称", "Collection folder name"),
                12,
                muted,
            );
            text(
                dc,
                area(X, 347, X + W, 374),
                tr("直接打开", "Open directly"),
                12,
                muted,
            );
        }
        RULES => {
            for (x, w, s) in [
                (0, 146, tr("匹配条件", "Match by")),
                (154, 183, tr("匹配值", "Value")),
                (345, 155, tr("归入分类", "Category")),
            ] {
                text(dc, area(X + x, 249, X + x + w, 277), s, 12, muted);
            }
        }
        _ => {}
    }
    STATUS.with_borrow(|s| text(dc, area(X, 428, X + W, 459), s, 13, muted));
}
const TILE_TIMER: usize = 1;
thread_local! {static SWAY:Cell<Option<(HWND,std::time::Instant)>>=const{Cell::new(None)};}
fn tile_motion(hwnd: HWND, msg: u32, wp: WPARAM, entering: bool) {
    unsafe {
        if entering
            && island::motion::animation_enabled()
            && SetTimer(hwnd, TILE_TIMER, 16, None) != 0
        {
            if let Some((old, _)) = SWAY.replace(Some((hwnd, std::time::Instant::now())))
                && old != hwnd
            {
                KillTimer(old, TILE_TIMER);
                InvalidateRect(old, null_mut(), 0);
            }
        }
        if let Some((owner, start)) = SWAY.get()
            && owner == hwnd
        {
            let tick = msg == WM_TIMER && wp == TILE_TIMER;
            if msg == windows_sys::Win32::UI::Controls::WM_MOUSELEAVE
                || msg == WM_NCDESTROY
                || (msg == WM_SHOWWINDOW && wp == 0)
                || (tick && start.elapsed().as_secs_f32() >= island::motion::SWAY_SECONDS)
            {
                SWAY.set(None);
                KillTimer(hwnd, TILE_TIMER);
            }
            if tick {
                InvalidateRect(hwnd, null_mut(), 0);
            }
        }
    }
}

fn button(hwnd: HWND, id: i32, dc: HDC) {
    let mut r = RECT::default();
    unsafe {
        GetClientRect(hwnd, &mut r);
    }
    let w = r.right;
    let h = r.bottom;
    let Some(bmp) = Bitmap::new(w, h).ok() else {
        return;
    };
    let target = dc;
    let dc = bmp.dc;
    let nav = (GENERAL..=APPEARANCE).contains(&id);
    let checked = unsafe { SendMessageW(hwnd, BM_GETCHECK, 0, 0) != 0 };
    let page = if PAGE.get() == PREVIEW {
        RULES
    } else {
        PAGE.get()
    };
    let selected = if nav { page == id } else { checked };
    let disabled = unsafe { IsWindowEnabled(hwnd) == 0 };
    let hot = dialog::HOT.get() == hwnd && !disabled;
    let focus = unsafe {
        GetFocus() == hwnd
            && SendMessageW(hwnd, WM_QUERYUISTATE, 0, 0) & UISF_HIDEFOCUS as isize == 0
    };
    let down = unsafe { SendMessageW(hwnd, BM_GETSTATE, 0, 0) & BST_PUSHED as isize != 0 };
    let tile = (TILE..TILE + 11).contains(&id);
    let primary = matches!(id, SORT | APPLY);
    let base = if nav { SIDEBAR } else { BG };
    let face = if disabled {
        SURFACE
    } else if down || (tile && focus) {
        FOCUS_BG
    } else if primary {
        if hot { INK } else { SILVER }
    } else if hot {
        HOVER_BG
    } else if nav && selected {
        SELECT
    } else if nav || tile {
        base
    } else {
        SURFACE
    };
    let ink = if disabled {
        0x888484
    } else if primary {
        BG
    } else if nav && !selected {
        MUTED
    } else {
        INK
    };
    let radius = if id == TOGGLE {
        PANEL_RADIUS
    } else {
        ROW_RADIUS
    };
    fill(dc, &r, base);
    if !tile && (focus || (!nav && !primary)) {
        rounded_fill(dc, r, radius, if focus { SILVER } else { EDGE });
        rounded_fill(dc, area(1, 1, w - 1, h - 1), radius - 1, face);
    } else if face != base {
        rounded_fill(dc, r, radius, face);
    }
    let name = window_text(hwnd);
    if nav {
        let category = match id {
            GENERAL => Some(Category::Other),
            BAR => Some(Category::Folders),
            RULES => Some(Category::Utilities),
            _ => None,
        };
        if let Some(c) = category {
            island::art::emblem(
                dc,
                area(10, 6, 32, h - 6),
                island::art::Symbol::Category(c),
                face,
                ink,
            );
        } else {
            glyph(
                dc,
                area(9, 0, 34, h),
                if id == APPEARANCE {
                    "\u{e790}"
                } else {
                    "\u{e774}"
                },
                14,
                ink,
            );
        }
        text(dc, area(41, 0, w - 5, h), &name, 13, ink);
    } else if id == TOGGLE {
        text(dc, area(16, 8, w - 76, 39), &name, 15, ink);
        text(
            dc,
            area(16, 36, w - 76, 65),
            tr(
                "关闭后，拖入的图标仍会收入「其他」。",
                "When off, dropped items are still saved in Other.",
            ),
            13,
            MUTED,
        );
        let x = w - 61;
        let y = (h - 23) / 2;
        rounded_fill(
            dc,
            area(x, y, x + 43, y + 23),
            12,
            if checked { SILVER } else { FOCUS_BG },
        );
        let dot = if checked { x + 22 } else { x + 3 };
        rounded_fill(
            dc,
            area(dot, y + 3, dot + 17, y + 20),
            9,
            if checked { BG } else { INK },
        );
    } else if tile {
        let cat = Category::ALL[(id - TILE) as usize];
        let dx = SWAY
            .get()
            .filter(|(owner, _)| *owner == hwnd)
            .map_or(0, |(_, t)| {
                island::motion::icon_sway(t.elapsed().as_secs_f32()).round() as i32
            });
        island::art::emblem(
            dc,
            area(7 + dx, 5, 36 + dx, h - 5),
            island::art::Symbol::Category(cat),
            face,
            SILVER,
        );
        text(dc, area(40, 0, w - 25, h), &name, 13, ink);
        let count = DATA.with_borrow(|s| s.count(Group::Builtin(cat)));
        interface_text(
            dc,
            area(w - 36, 0, w - 8, h),
            &count.to_string(),
            12,
            MUTED,
            DT_RIGHT,
        );
    } else if (201..=203).contains(&id) {
        text(dc, area(16, 0, w - 146, h), &name, 14, ink);
        if id == 201 {
            interface_text(
                dc,
                area(w - 164, 0, w - 55, h),
                Language::System.native_name(),
                13,
                MUTED,
                DT_RIGHT,
            );
        }
        let x = w - 34;
        let y = (h - 17) / 2;
        rounded_fill(
            dc,
            area(x, y, x + 17, y + 17),
            9,
            if checked { SILVER } else { MUTED },
        );
        rounded_fill(dc, area(x + 2, y + 2, x + 15, y + 15), 7, face);
        if checked {
            rounded_fill(dc, area(x + 4, y + 4, x + 13, y + 13), 5, SILVER);
        }
    } else {
        interface_text(dc, area(3, 0, w - 3, h), &name, 13, ink, DT_CENTER);
    }
    unsafe {
        BitBlt(target, 0, 0, w, h, dc, 0, 0, SRCCOPY);
    }
}

pub(super) unsafe extern "system" fn button_proc(
    hwnd: HWND,
    msg: u32,
    wp: WPARAM,
    lp: LPARAM,
    ident: usize,
    _: usize,
) -> LRESULT {
    let entering = msg == WM_MOUSEMOVE && dialog::HOT.get() != hwnd;
    dialog::track_hover(hwnd, msg);
    let id = ident as i32;
    if (TILE..TILE + 11).contains(&id) {
        tile_motion(hwnd, msg, wp, entering);
    }
    unsafe {
        match msg {
            WM_PAINT | WM_PRINTCLIENT if !high_contrast() => {
                if msg == WM_PRINTCLIENT {
                    button(hwnd, id, wp as HDC);
                } else {
                    let mut ps = PAINTSTRUCT::default();
                    let dc = BeginPaint(hwnd, &mut ps);
                    button(hwnd, id, dc);
                    EndPaint(hwnd, &ps);
                }
                return 0;
            }
            WM_ERASEBKGND if !high_contrast() => return 1,
            BM_SETSTATE | BM_SETCHECK | WM_SETFOCUS | WM_KILLFOCUS | WM_UPDATEUISTATE
            | WM_ENABLE => {
                let result = DefSubclassProc(hwnd, msg, wp, lp);
                InvalidateRect(hwnd, null_mut(), 0);
                return result;
            }
            WM_GETDLGCODE if (201..=203).contains(&id) => {
                return DefSubclassProc(hwnd, msg, wp, lp) | DLGC_WANTARROWS as isize;
            }
            WM_KEYDOWN if (201..=203).contains(&id) => {
                let next = match wp as u16 {
                    VK_UP | VK_LEFT => (id - 201 + 2) % 3,
                    VK_DOWN | VK_RIGHT => (id - 201 + 1) % 3,
                    _ => return DefSubclassProc(hwnd, msg, wp, lp),
                };
                let h = child(GetParent(hwnd), 201 + next);
                SetFocus(h);
                SendMessageW(h, BM_CLICK, 0, 0);
                return 0;
            }
            WM_NCDESTROY => {
                RemoveWindowSubclass(hwnd, Some(button_proc), ident);
            }
            _ => {}
        }
        DefSubclassProc(hwnd, msg, wp, lp)
    }
}
pub(super) fn list_row(d: &DRAWITEMSTRUCT) {
    if d.itemID == u32::MAX {
        return;
    }
    let mut r = d.rcItem;
    let selected = d.itemState & ODS_SELECTED != 0;
    let hc = high_contrast();
    let (bg, ink) = if hc {
        unsafe {
            (
                GetSysColor(if selected {
                    COLOR_HIGHLIGHT
                } else {
                    COLOR_WINDOW
                }),
                GetSysColor(if selected {
                    COLOR_HIGHLIGHTTEXT
                } else {
                    COLOR_WINDOWTEXT
                }),
            )
        }
    } else {
        (if selected { SELECT } else { ROW_BG }, INK)
    };
    fill(d.hDC, &r, bg);
    let len = unsafe { SendMessageW(d.hwndItem, LB_GETTEXTLEN, d.itemID as usize, 0) };
    if len < 0 {
        return;
    }
    let mut buf = vec![0; len as usize + 1];
    unsafe {
        SendMessageW(
            d.hwndItem,
            LB_GETTEXT,
            d.itemID as usize,
            buf.as_mut_ptr() as isize,
        );
    }
    r.left += 10;
    r.right -= 8;
    text(
        d.hDC,
        r,
        &String::from_utf16_lossy(&buf[..len as usize]),
        13,
        ink,
    );
    if d.itemState & ODS_FOCUS != 0 && d.itemState & ODS_NOFOCUSRECT == 0 {
        unsafe {
            DrawFocusRect(d.hDC, &d.rcItem);
        }
    }
}

pub(super) unsafe extern "system" fn combo_proc(
    hwnd: HWND,
    msg: u32,
    wp: WPARAM,
    lp: LPARAM,
    id: usize,
    _: usize,
) -> LRESULT {
    dialog::track_hover(hwnd, msg);
    unsafe {
        match msg {
            WM_PAINT | WM_PRINTCLIENT if !high_contrast() => {
                let mut ps = PAINTSTRUCT::default();
                let dc = if msg == WM_PAINT {
                    BeginPaint(hwnd, &mut ps)
                } else {
                    wp as HDC
                };
                let mut r = RECT::default();
                GetClientRect(hwnd, &mut r);
                let w = r.right;
                let h = r.bottom;
                fill(dc, &r, BG);
                rounded_fill(
                    dc,
                    r,
                    ROW_RADIUS,
                    if GetFocus() == hwnd { SILVER } else { EDGE },
                );
                let face = if dialog::HOT.get() == hwnd {
                    HOVER_BG
                } else {
                    ROW_BG
                };
                rounded_fill(dc, area(1, 1, w - 1, h - 1), ROW_RADIUS - 1, face);
                text(dc, area(8, 0, w - 23, h), &window_text(hwnd), 13, INK);
                glyph(dc, area(w - 24, 0, w - 3, h), "\u{e70d}", 10, MUTED);
                if msg == WM_PAINT {
                    EndPaint(hwnd, &ps);
                }
                return 0;
            }
            WM_ERASEBKGND if !high_contrast() => return 1,
            CB_SETCURSEL | WM_SETFOCUS | WM_KILLFOCUS | CB_SHOWDROPDOWN => {
                let result = DefSubclassProc(hwnd, msg, wp, lp);
                InvalidateRect(hwnd, null_mut(), 0);
                return result;
            }
            WM_NCDESTROY => {
                RemoveWindowSubclass(hwnd, Some(combo_proc), id);
            }
            _ => {}
        }
        DefSubclassProc(hwnd, msg, wp, lp)
    }
}
