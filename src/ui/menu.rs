use super::*;
use std::rc::Rc;
use windows_sys::Win32::UI::Accessibility::{MSAA_MENU_SIG, MSAAMENUINFO};
use windows_sys::Win32::UI::Controls::{DRAWITEMSTRUCT, MEASUREITEMSTRUCT, ODS_SELECTED, ODT_MENU};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetKeyState, VK_APPS, VK_F10, VK_SHIFT};

mod frame;

const OPEN_SETTINGS: u32 = 4001;
const QUIT: u32 = 4002;
const ORGANIZE: u32 = 4003;

const BORDER: u32 = 0x504c4c;
const INSET: i32 = PANEL_RADIUS - ROW_RADIUS;

pub(super) fn context_key(owner: HWND, msg: &MSG) -> bool {
    match (msg.message, msg.wParam as u16) {
        (WM_KEYDOWN, VK_APPS) => return true,
        (WM_KEYUP, VK_APPS) => {}
        (WM_KEYDOWN | WM_SYSKEYDOWN, VK_F10) if unsafe { GetKeyState(VK_SHIFT as i32) } < 0 => {}
        _ => return false,
    }
    unsafe { SendMessageW(owner, WM_CONTEXTMENU, msg.hwnd as usize, -1) };
    true
}

pub(super) fn select(wp: WPARAM, lp: LPARAM) {
    frame::select(lp as HMENU, wp);
}

pub(super) struct Row {
    id: u32,
    name: String,
    icon: &'static str,
    children: Option<Vec<Row>>,
    accent: bool,
}

impl Row {
    pub(super) fn new(id: u32, name: impl Into<String>, icon: &'static str) -> Self {
        Self {
            id,
            name: name.into(),
            icon,
            children: None,
            accent: false,
        }
    }

    pub(super) fn separator() -> Self {
        Self::new(0, "", "")
    }

    pub(super) fn submenu(id: u32, name: &str, icon: &'static str, children: Vec<Row>) -> Self {
        Self {
            children: Some(children),
            ..Self::new(id, name, icon)
        }
    }

    fn height(&self) -> i32 {
        if self.id == 0 { 8 } else { 32 }
    }
}

fn main_rows() -> Vec<Row> {
    let t = crate::language().text();
    vec![
        Row::new(OPEN_SETTINGS, format!("{}…", t.settings), "\u{e713}"),
        Row::new(
            ORGANIZE,
            crate::language().pick("立即整理", "Organize now"),
            "\u{e8cb}",
        ),
        Row {
            accent: true,
            ..Row::new(QUIT, t.quit, "\u{e7e8}")
        },
    ]
}

// User32 reads the leading MSAA record to name an owner-drawn menu item.
#[repr(C)]
struct Item {
    accessible: MSAAMENUINFO,
    wide_name: Vec<u16>,
    row: Row,
    submenu: Option<Popup>,
}

struct Popup {
    handle: HMENU,
    owned: bool,
    brush: HBRUSH,
    items: Rc<[Item]>,
}

impl Popup {
    fn new(rows: Vec<Row>, custom: bool) -> io::Result<Self> {
        let mut items = Vec::new();
        for mut row in rows {
            let mut wide_name = wide(&row.name);
            let submenu = row
                .children
                .take()
                .map(|rows| Self::new(rows, custom))
                .transpose()?;
            items.push(Item {
                accessible: MSAAMENUINFO {
                    dwMSAASignature: MSAA_MENU_SIG as u32,
                    cchWText: (wide_name.len() - 1) as u32,
                    pszWText: wide_name.as_mut_ptr(),
                },
                wide_name,
                row,
                submenu,
            });
        }
        unsafe {
            let handle = CreatePopupMenu();
            if handle.is_null() {
                return Err(io::Error::last_os_error());
            }
            let mut popup = Self {
                handle,
                owned: true,
                brush: if custom {
                    CreateSolidBrush(POPUP_BG)
                } else {
                    null_mut()
                },
                items: items.into(),
            };
            if custom {
                let info = MENUINFO {
                    cbSize: size_of::<MENUINFO>() as u32,
                    fMask: MIM_BACKGROUND | MIM_STYLE,
                    hbrBack: popup.brush,
                    dwStyle: MNS_NOCHECK,
                    ..Default::default()
                };
                if popup.brush.is_null() || SetMenuInfo(handle, &info) == 0 {
                    return Err(io::Error::last_os_error());
                }
            }
            for (index, item) in Rc::get_mut(&mut popup.items)
                .unwrap()
                .iter_mut()
                .enumerate()
            {
                let info = MENUITEMINFOW {
                    cbSize: size_of::<MENUITEMINFOW>() as u32,
                    fMask: MIIM_FTYPE | MIIM_ID | MIIM_STRING | MIIM_DATA | MIIM_SUBMENU,
                    wID: item.row.id,
                    fType: (if custom { MFT_OWNERDRAW } else { MFT_STRING })
                        | if item.row.id == 0 { MFT_SEPARATOR } else { 0 },
                    dwTypeData: item.wide_name.as_mut_ptr(),
                    dwItemData: item as *const Item as usize,
                    hSubMenu: item
                        .submenu
                        .as_ref()
                        .map_or(null_mut(), |child| child.handle),
                    ..Default::default()
                };
                if InsertMenuItemW(handle, index as u32, 1, &info) == 0 {
                    return Err(io::Error::last_os_error());
                }
                if let Some(child) = &mut item.submenu {
                    child.owned = false; // DestroyMenu on the parent now owns this handle.
                }
            }
            Ok(popup)
        }
    }
}

impl Drop for Popup {
    fn drop(&mut self) {
        unsafe {
            if self.owned {
                DestroyMenu(self.handle);
            }
            if !self.brush.is_null() {
                DeleteObject(self.brush);
            }
        }
    }
}

pub(super) fn track(owner: HWND, pt: POINT, rows: Vec<Row>, flags: u32) -> io::Result<i32> {
    let custom = !high_contrast();
    let popup = Popup::new(rows, custom)?;
    unsafe {
        SetForegroundWindow(owner);
        let _frame = custom.then(|| frame::Watch::start(&popup));
        Ok(TrackPopupMenu(
            popup.handle,
            flags | TPM_RIGHTBUTTON | if custom { TPM_NOANIMATION } else { 0 },
            pt.x,
            pt.y,
            0,
            owner,
            null_mut(),
        ))
    }
}

pub(super) fn show(owner: HWND, lp: LPARAM) -> io::Result<()> {
    let pt = if lp == -1 {
        let mut r = RECT::default();
        unsafe {
            GetWindowRect(owner, &mut r);
        }
        POINT {
            x: (r.left + r.right) / 2,
            y: r.bottom + 4,
        }
    } else {
        POINT {
            x: lp as i16 as i32,
            y: (lp >> 16) as i16 as i32,
        }
    };
    track(owner, pt, main_rows(), 0).map(|_| ())
}

pub(super) fn command(owner: HWND, id: u32) -> Option<io::Result<()>> {
    Some(match id {
        OPEN_SETTINGS => settings::open(),
        ORGANIZE => settings::organize(),
        QUIT => {
            unsafe { PostMessageW(owner, WM_CLOSE, 0, 0) };
            Ok(())
        }
        _ => return None,
    })
}

pub(super) fn measure(lp: LPARAM) -> bool {
    let item = unsafe { &mut *(lp as *mut MEASUREITEMSTRUCT) };
    if item.CtlType != ODT_MENU || item.itemData == 0 {
        return false;
    }
    let entry = unsafe { &*(item.itemData as *const Item) };
    let row = &entry.row;
    let mut width = SIZE::default();
    unsafe {
        let dc = GetDC(null_mut());
        let font = interface_font(&row.name, POPUP_TEXT_SIZE, 1);
        let old = SelectObject(dc, font);
        let name = &entry.wide_name;
        GetTextExtentPoint32W(dc, name.as_ptr(), (name.len() - 1) as i32, &mut width);
        SelectObject(dc, old);
        DeleteObject(font);
        ReleaseDC(null_mut(), dc);
    }
    item.itemWidth = 132.max(width.cx + 58) as u32;
    item.itemHeight = row.height() as u32;
    true
}

pub(super) fn draw(d: &DRAWITEMSTRUCT) -> bool {
    if d.CtlType != ODT_MENU || d.itemData == 0 {
        return false;
    }
    let r = d.rcItem;
    // Also supplies the native fallback if the layered surface is unavailable.
    fill(d.hDC, &r, POPUP_BG);
    let face = area(r.left + 1, r.top + 1, r.right - 1, r.bottom - 1);
    draw_item(
        d.hDC,
        face,
        unsafe { &*(d.itemData as *const Item) },
        d.itemState & ODS_SELECTED != 0,
    );
    true
}

fn draw_item(dc: HDC, r: RECT, item: &Item, active: bool) {
    let row = &item.row;
    if row.id == 0 {
        let y = (r.top + r.bottom) / 2;
        fill(dc, &area(r.left + 8, y, r.right - 8, y + 1), EDGE);
        return;
    }
    if active {
        rounded_fill(dc, r, ROW_RADIUS, HOVER_BG);
    }
    let (tint, ink) = if row.accent {
        (CORAL, CORAL)
    } else {
        (SILVER, INK)
    };
    let right = if item.submenu.is_some() {
        glyph(
            dc,
            area(r.right - 20, r.top, r.right - 4, r.bottom),
            "\u{e76c}",
            10,
            SILVER,
        );
        r.right - 22
    } else {
        r.right - 8
    };
    glyph(
        dc,
        area(r.left + 4, r.top, r.left + 26, r.bottom),
        row.icon,
        14,
        tint,
    );
    interface_text(
        dc,
        area(r.left + 31, r.top, right, r.bottom),
        &row.name,
        POPUP_TEXT_SIZE,
        ink,
        DT_LEFT,
    );
}
