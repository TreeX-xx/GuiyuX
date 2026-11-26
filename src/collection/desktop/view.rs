use std::io;
use windows::{
    Win32::{
        System::{
            Com::{CLSCTX_LOCAL_SERVER, CoCreateInstance, IServiceProvider},
            Variant::VARIANT,
        },
        UI::Shell::{
            FWF_NOICONS, IFolderView2, IShellBrowser, IShellWindows, SID_STopLevelBrowser,
            SWC_DESKTOP, SWFO_NEEDDISPATCH, ShellWindows,
        },
    },
    core::Interface,
};

pub(super) struct DesktopView(IFolderView2);

impl DesktopView {
    pub(super) fn connect() -> io::Result<Self> {
        fn find() -> windows::core::Result<IFolderView2> {
            unsafe {
                let windows: IShellWindows =
                    CoCreateInstance(&ShellWindows, None, CLSCTX_LOCAL_SERVER)?;
                let empty = VARIANT::default();
                let mut hwnd = 0;
                let dispatch = windows.FindWindowSW(
                    &empty,
                    &empty,
                    SWC_DESKTOP,
                    &mut hwnd,
                    SWFO_NEEDDISPATCH,
                )?;
                let browser: IShellBrowser = dispatch
                    .cast::<IServiceProvider>()?
                    .QueryService(&SID_STopLevelBrowser)?;
                browser.QueryActiveShellView()?.cast()
            }
        }
        find().map(Self).map_err(io::Error::other)
    }

    pub(super) fn hidden(&self) -> io::Result<bool> {
        let flags = unsafe { self.0.GetCurrentFolderFlags() }.map_err(io::Error::other)?;
        Ok(flags & FWF_NOICONS.0 as u32 != 0)
    }

    pub(super) fn set_hidden(&self, hidden: bool) -> io::Result<()> {
        if self.hidden()? != hidden {
            let mask = FWF_NOICONS.0 as u32;
            unsafe {
                self.0
                    .SetCurrentFolderFlags(mask, if hidden { mask } else { 0 })
            }
            .map_err(io::Error::other)?;
            if self.hidden()? != hidden {
                return Err(io::Error::other(crate::language().pick(
                    "Windows 未能切换桌面图标显示。",
                    "Windows could not change desktop icon visibility.",
                )));
            }
        }
        Ok(())
    }
}
