use std::{
    ffi::OsString,
    io,
    os::windows::ffi::{OsStrExt, OsStringExt},
    path::{Path, PathBuf},
};
use windows::{
    Win32::{
        Foundation::PROPERTYKEY,
        Storage::EnhancedStorage::{
            PKEY_AppUserModel_ID, PKEY_Link_TargetParsingPath, PKEY_Software_ProductName,
        },
        System::{
            Com::CoTaskMemFree,
            Ole::{OleInitialize, OleUninitialize},
        },
        UI::Shell::{IShellItem2, SHCreateItemFromParsingName},
    },
    core::{PCWSTR, PWSTR},
};

pub struct Apartment;
impl Apartment {
    pub fn open() -> io::Result<Self> {
        unsafe { OleInitialize(None) }.map_err(io::Error::other)?;
        Ok(Self)
    }
}
impl Drop for Apartment {
    fn drop(&mut self) {
        unsafe { OleUninitialize() };
    }
}

pub fn owned_string(value: PWSTR) -> OsString {
    if value.is_null() {
        return OsString::new();
    }
    unsafe {
        let text = OsString::from_wide(value.as_wide());
        CoTaskMemFree(Some(value.0.cast()));
        text
    }
}

pub struct Properties(IShellItem2);
impl Properties {
    pub fn from_path(path: &Path) -> Option<Self> {
        let wide: Vec<_> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        unsafe { SHCreateItemFromParsingName(PCWSTR(wide.as_ptr()), None) }
            .ok()
            .map(Self)
    }
    fn value(&self, key: &PROPERTYKEY) -> Option<OsString> {
        unsafe { self.0.GetString(key) }
            .ok()
            .map(owned_string)
            .filter(|s| !s.is_empty())
    }
    pub fn target(&self) -> Option<PathBuf> {
        self.value(&PKEY_Link_TargetParsingPath).map(Into::into)
    }
    pub fn app_id(&self) -> Option<String> {
        self.value(&PKEY_AppUserModel_ID)?.into_string().ok()
    }
    pub fn product(&self) -> Option<String> {
        self.value(&PKEY_Software_ProductName)?.into_string().ok()
    }
}
