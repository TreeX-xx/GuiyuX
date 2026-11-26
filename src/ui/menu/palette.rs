use super::*;
use windows_sys::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW};

pub(super) struct Palette {
    pub face: u32,
    pub border: u32,
    pub hover: u32,
    pub divider: u32,
    pub text: u32,
    pub icon: u32,
    pub danger: u32,
}

// Design colours are written as sRGB, while GDI expects a COLORREF.
const fn rgb(value: u32) -> u32 {
    (value & 0xff) << 16 | (value & 0xff00) | (value >> 16)
}

pub(super) const LIGHT: Palette = Palette {
    face: rgb(0xf4f4f7),
    border: rgb(0xceced2),
    hover: rgb(0xe4e4e8),
    divider: rgb(0xdadade),
    text: rgb(0x202024),
    icon: rgb(0x53535b),
    danger: rgb(0xa52c24),
};

pub(super) const DARK: Palette = Palette {
    face: rgb(0x37373d),
    border: rgb(0x5a5a62),
    hover: rgb(0x494950),
    divider: rgb(0x515158),
    text: rgb(0xf7f7fa),
    icon: rgb(0xd2d2da),
    danger: rgb(0xffadb5),
};

pub(super) fn system() -> &'static Palette {
    let mut light = 1u32;
    let mut bytes = size_of::<u32>() as u32;
    let result = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            windows_sys::w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize"),
            windows_sys::w!("AppsUseLightTheme"),
            RRF_RT_REG_DWORD,
            null_mut(),
            (&mut light as *mut u32).cast(),
            &mut bytes,
        )
    };
    if result == ERROR_SUCCESS && light == 0 {
        &DARK
    } else {
        &LIGHT
    }
}
