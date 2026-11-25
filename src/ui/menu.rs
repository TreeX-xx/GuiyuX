use super::*;
use windows_sys::Win32::UI::Accessibility::{MSAA_MENU_SIG, MSAAMENUINFO};
use windows_sys::Win32::UI::Controls::{DRAWITEMSTRUCT, MEASUREITEMSTRUCT, ODS_SELECTED, ODT_MENU};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetKeyState, VK_APPS, VK_F10, VK_SHIFT};

mod frame;

const OPEN_SETTINGS: u32 = 4001;
const QUIT: u32 = 4002;
const ORGANIZE: u32 = 4003;
const COMMANDS: [u32; 3] = [OPEN_SETTINGS, ORGANIZE, QUIT];
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
    frame::select(lp as HMENU, (wp & 0xffff) as u32);
}

struct Popup {
    handle: HMENU,
    brush: HBRUSH,
    // User32 retains the addresses of both the MSAA records and their UTF-16 text.
    names: [Vec<u16>; 3],
    accessible: Box<[MSAAMENUINFO; 3]>,
}

impl Popup {
    fn new() -> io::Result<Self> {
        unsafe {
            let handle = CreatePopupMenu();
            if handle.is_null() {
                return Err(io::Error::last_os_error());
            }
            let custom = !high_contrast();
            let t = crate::language().text();
            let mut names = [
                wide(&format!("{}…", t.settings)),
                wide(crate::language().pick("立即整理", "Organize now")),
                wide(t.quit),
            ];
            let accessible = Box::new(names.each_mut().map(|name| MSAAMENUINFO {
                dwMSAASignature: MSAA_MENU_SIG as u32,
                cchWText: (name.len() - 1) as u32,
                pszWText: name.as_mut_ptr(),
            }));
            let popup = Self {
                handle,
                brush: if custom {
                    CreateSolidBrush(POPUP_BG)
                } else {
                    null_mut()
                },
                names,
                accessible,
            };
            if custom {
                if popup.brush.is_null() {
                    return Err(io::Error::last_os_error());
                }
                let info = MENUINFO {
                    cbSize: size_of::<MENUINFO>() as u32,
                    fMask: MIM_BACKGROUND | MIM_STYLE,
                    hbrBack: popup.brush,
                    dwStyle: MNS_NOCHECK,
                    ..Default::default()
                };
                if SetMenuInfo(handle, &info) == 0 {
                    return Err(io::Error::last_os_error());
                }
            }
            for (index, id) in COMMANDS.into_iter().enumerate() {
                let item = MENUITEMINFOW {
                    cbSize: size_of::<MENUITEMINFOW>() as u32,
                    fMask: MIIM_FTYPE | MIIM_ID | MIIM_STRING | MIIM_DATA,
                    wID: id,
                    fType: if custom { MFT_OWNERDRAW } else { MFT_STRING },
                    dwTypeData: popup.names[index].as_ptr().cast_mut(),
                    dwItemData: (&popup.accessible[index] as *const MSAAMENUINFO) as usize,
                    ..Default::default()
                };
                if InsertMenuItemW(handle, index as u32, 1, &item) == 0 {
                    return Err(io::Error::last_os_error());
                }
            }
            Ok(popup)
        }
    }
}

impl Drop for Popup {
    fn drop(&mut self) {
        unsafe {
            DestroyMenu(self.handle);
            if !self.brush.is_null() {
                DeleteObject(self.brush);
            }
        }
    }
}

pub(super) fn show(owner: HWND, lp: LPARAM) -> io::Result<()> {
    unsafe {
        let popup = Popup::new()?;
        let (x, y) = if lp == -1 {
            let mut r = RECT::default();
            GetWindowRect(owner, &mut r);
            ((r.left + r.right) / 2, r.bottom + 4)
        } else {
            (lp as i16 as i32, (lp >> 16) as i16 as i32)
        };
        SetForegroundWindow(owner);
        let custom = !popup.brush.is_null();
        let _frame = custom.then(|| frame::Watch::start(popup.handle));
        TrackPopupMenu(
            popup.handle,
            TPM_RIGHTBUTTON | if custom { TPM_NOANIMATION } else { 0 },
            x,
            y,
            0,
            owner,
            null_mut(),
        );
    }
    Ok(())
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
    if item.CtlType != ODT_MENU {
        return false;
    }
    item.itemWidth = 132;
    item.itemHeight = 32;
    true
}

pub(super) fn draw(d: &DRAWITEMSTRUCT) -> bool {
    if d.CtlType != ODT_MENU {
        return false;
    }
    let r = d.rcItem;
    // Also supplies the native fallback if the layered surface is unavailable.
    fill(d.hDC, &r, POPUP_BG);
    let face = area(r.left + 1, r.top + 1, r.right - 1, r.bottom - 1);
    draw_item(d.hDC, face, d.itemID, d.itemState & ODS_SELECTED != 0);
    true
}

fn draw_item(dc: HDC, r: RECT, id: u32, active: bool) {
    let t = crate::language().text();
    if active {
        rounded_fill(dc, r, ROW_RADIUS, HOVER_BG);
    }
    let (icon, name, tint, ink) = if id == OPEN_SETTINGS {
        ("\u{e713}", format!("{}…", t.settings), SILVER, INK)
    } else if id == ORGANIZE {
        (
            "\u{e8cb}",
            crate::language()
                .pick("立即整理", "Organize now")
                .to_owned(),
            SILVER,
            INK,
        )
    } else {
        ("\u{e7e8}", t.quit.to_owned(), CORAL, CORAL)
    };
    glyph(
        dc,
        area(r.left + 4, r.top, r.left + 26, r.bottom),
        icon,
        14,
        tint,
    );
    interface_text(
        dc,
        area(r.left + 31, r.top, r.right - 8, r.bottom),
        &name,
        POPUP_TEXT_SIZE,
        ink,
        DT_LEFT,
    );
}
