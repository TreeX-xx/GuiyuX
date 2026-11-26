// Native UI checks use a separate desktop in the same window station.
// SetThreadDesktop attaches the test thread without switching the user's display.
// See .agents/notes/module.md#技术路线与验证.
use std::{io, ptr::null};
use windows_sys::Win32::System::{
    StationsAndDesktops::{
        CloseDesktop, CreateDesktopW, DESKTOP_CREATEMENU, DESKTOP_CREATEWINDOW, DESKTOP_ENUMERATE,
        DESKTOP_READOBJECTS, DESKTOP_WRITEOBJECTS, GetThreadDesktop, HDESK, SetThreadDesktop,
    },
    Threading::GetCurrentThreadId,
};

pub(crate) struct TestDesktop {
    handle: HDESK,
    previous: HDESK,
}

impl TestDesktop {
    pub(crate) fn name() -> Vec<u16> {
        let tid = unsafe { GetCurrentThreadId() };
        format!("GuiyuX-test-{}-{tid}", std::process::id())
            .encode_utf16()
            .chain(Some(0))
            .collect()
    }

    pub(crate) fn enter() -> io::Result<Self> {
        unsafe {
            let previous = GetThreadDesktop(GetCurrentThreadId());
            let name = Self::name();
            let access = DESKTOP_CREATEWINDOW
                | DESKTOP_CREATEMENU
                | DESKTOP_ENUMERATE
                | DESKTOP_READOBJECTS
                | DESKTOP_WRITEOBJECTS;
            let handle = CreateDesktopW(name.as_ptr(), null(), null(), 0, access, null());
            if handle.is_null() {
                return Err(io::Error::last_os_error());
            }
            if SetThreadDesktop(handle) == 0 {
                let error = io::Error::last_os_error();
                CloseDesktop(handle);
                return Err(error);
            }
            Ok(Self { handle, previous })
        }
    }
}

impl Drop for TestDesktop {
    fn drop(&mut self) {
        unsafe {
            SetThreadDesktop(self.previous);
            CloseDesktop(self.handle);
        }
    }
}
