use std::{
    ffi::OsString,
    io,
    os::windows::ffi::{OsStrExt, OsStringExt},
    path::{Path, PathBuf},
};
use windows::{
    Win32::{
        Foundation::{ERROR_CANCELLED, HWND, PROPERTYKEY},
        Storage::EnhancedStorage::{
            PKEY_AppUserModel_ID, PKEY_Link_TargetParsingPath, PKEY_Software_ProductName,
        },
        System::{
            Com::{CLSCTX_INPROC_SERVER, CoCreateInstance, CoTaskMemFree},
            Ole::{OleInitialize, OleUninitialize},
        },
        UI::Shell::{
            FOS_FORCEFILESYSTEM, FOS_PICKFOLDERS, FileOpenDialog, IFileOpenDialog, IShellItem2,
            SHCreateItemFromParsingName, SIGDN_FILESYSPATH,
        },
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

pub fn choose(
    owner: windows_sys::Win32::Foundation::HWND,
    folder: bool,
) -> io::Result<Option<PathBuf>> {
    let result = unsafe {
        (|| -> windows::core::Result<PathBuf> {
            let picker: IFileOpenDialog =
                CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER)?;
            let mut flags = picker.GetOptions()? | FOS_FORCEFILESYSTEM;
            if folder {
                flags |= FOS_PICKFOLDERS;
            }
            picker.SetOptions(flags)?;
            picker.Show(Some(HWND(owner)))?;
            Ok(owned_string(picker.GetResult()?.GetDisplayName(SIGDN_FILESYSPATH)?).into())
        })()
    };
    match result {
        Ok(path) => Ok(Some(path)),
        Err(e) if e.code() == ERROR_CANCELLED.to_hresult() => Ok(None),
        Err(e) => Err(io::Error::other(e)),
    }
}
