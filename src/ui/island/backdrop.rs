// The composited material behind the island is described in .agents/notes/module.md.
use super::*;
use windows::{
    System::{DispatcherQueue, DispatcherQueueController},
    UI::Composition::{
        CompositionRoundedRectangleGeometry, Compositor, Desktop::DesktopWindowTarget,
    },
    Win32::System::WinRT::{
        Composition::ICompositorDesktopInterop, CreateDispatcherQueueController, DQTAT_COM_NONE,
        DQTYPE_THREAD_CURRENT, DispatcherQueueOptions,
    },
    core::Interface,
};
use windows_numerics::Vector2;
use windows_sys::Win32::Graphics::Dwm::{DWMWA_USE_HOSTBACKDROPBRUSH, DwmSetWindowAttribute};
use windows_sys::Win32::System::LibraryLoader::GetProcAddress;

// The layered input window paints the tint and controls. This input-transparent
// window immediately below it lets DWM blur the live desktop even while idle.
struct HostWindow(HWND);

impl Drop for HostWindow {
    fn drop(&mut self) {
        unsafe { DestroyWindow(self.0) };
    }
}

struct Backdrop {
    shape: CompositionRoundedRectangleGeometry,
    _target: DesktopWindowTarget,
    _compositor: Compositor,
    _queue: Option<DispatcherQueueController>,
    window: HostWindow,
    rect: Option<RECT>,
    outline: Option<capsule::Outline>,
}

fn enable_host_brush(hwnd: HWND) -> windows::core::Result<()> {
    let on = 1i32;
    let result = windows::core::HRESULT(unsafe {
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_HOSTBACKDROPBRUSH as u32,
            (&on as *const i32).cast(),
            4,
        )
    });
    if result.is_ok() {
        return Ok(());
    }
    // Windows 10 exposes host backdrop brushes via the User32 accent policy.
    // Resolve it only if DWM rejects the Windows 11 attribute.
    #[repr(C)]
    struct Attribute {
        id: u32,
        value: *mut [u32; 4],
        bytes: usize,
    }
    const WCA_ACCENT_POLICY: u32 = 19;
    const ACCENT_ENABLE_HOSTBACKDROP: u32 = 5;
    unsafe {
        let user32 = GetModuleHandleW(windows_sys::w!("user32.dll"));
        if let Some(proc) = GetProcAddress(user32, windows_sys::s!("SetWindowCompositionAttribute"))
        {
            let set: unsafe extern "system" fn(HWND, *mut Attribute) -> i32 =
                std::mem::transmute(proc);
            let mut policy = [ACCENT_ENABLE_HOSTBACKDROP, 0, 0, 0];
            let mut attr = Attribute {
                id: WCA_ACCENT_POLICY,
                value: &mut policy,
                bytes: size_of_val(&policy),
            };
            if set(hwnd, &mut attr) != 0 {
                return Ok(());
            }
        }
    }
    result.ok()
}

impl Backdrop {
    fn new() -> windows::core::Result<Self> {
        let class = "GuiyuX.IslandBackdrop";
        if let Err(error) = crate::ui::register(class, Some(procedure))
            && error.raw_os_error() != Some(ERROR_CLASS_ALREADY_EXISTS as i32)
        {
            return Err(error.into());
        }
        let window = HostWindow(window(
            class,
            "",
            WS_POPUP | WS_DISABLED,
            WS_EX_TOPMOST
                | WS_EX_TOOLWINDOW
                | WS_EX_NOACTIVATE
                | WS_EX_TRANSPARENT
                | WS_EX_NOREDIRECTIONBITMAP,
            null_mut(),
            0,
        )?);
        enable_host_brush(window.0)?;
        let queue = if DispatcherQueue::GetForCurrentThread().is_err() {
            Some(unsafe {
                CreateDispatcherQueueController(DispatcherQueueOptions {
                    dwSize: size_of::<DispatcherQueueOptions>() as u32,
                    threadType: DQTYPE_THREAD_CURRENT,
                    apartmentType: DQTAT_COM_NONE,
                })?
            })
        } else {
            None
        };
        let compositor = Compositor::new()?;
        let interop: ICompositorDesktopInterop = compositor.cast()?;
        let target = unsafe {
            interop.CreateDesktopWindowTarget(windows::Win32::Foundation::HWND(window.0), true)?
        };
        let shape = compositor.CreateRoundedRectangleGeometry()?;
        let visual = compositor.CreateSpriteVisual()?;
        visual.SetRelativeSizeAdjustment(Vector2 { X: 1., Y: 1. })?;
        visual.SetBrush(&compositor.CreateHostBackdropBrush()?)?;
        visual.SetClip(&compositor.CreateGeometricClipWithGeometry(&shape)?)?;
        target.SetRoot(&visual)?;
        Ok(Self {
            shape,
            _target: target,
            _compositor: compositor,
            _queue: queue,
            window,
            rect: None,
            outline: None,
        })
    }

    fn place(&mut self, island: HWND, rect: RECT) -> windows::core::Result<()> {
        let (w, h) = (rect.right - rect.left, rect.bottom - rect.top);
        let outline = capsule::Outline::current(w, h);
        if self.outline != Some(outline) {
            let radius = outline.diameter as f32 / 2.;
            self.shape.SetOffset(Vector2 {
                X: outline.x as f32,
                Y: outline.y as f32,
            })?;
            self.shape.SetSize(Vector2 {
                X: outline.w as f32,
                Y: outline.h as f32,
            })?;
            self.shape.SetCornerRadius(Vector2 {
                X: radius,
                Y: radius,
            })?;
            // The compositor clip only affects pixels. Match the HWND's hit
            // region too, so the desktop still receives clicks in the corners.
            unsafe {
                let region = CreateRoundRectRgn(
                    outline.x,
                    outline.y,
                    outline.x + outline.w + 1,
                    outline.y + outline.h + 1,
                    outline.diameter,
                    outline.diameter,
                );
                if region.is_null() {
                    return Err(windows::core::Error::from_thread());
                }
                if SetWindowRgn(self.window.0, region, 0) == 0 {
                    let error = windows::core::Error::from_thread();
                    DeleteObject(region);
                    return Err(error);
                }
            }
            self.outline = Some(outline);
        } else if self.rect.is_some_and(|old| {
            old.left == rect.left
                && old.top == rect.top
                && old.right == rect.right
                && old.bottom == rect.bottom
        }) && unsafe { GetWindow(self.window.0, GW_HWNDPREV) } == island
        {
            return Ok(());
        }
        if unsafe {
            SetWindowPos(
                self.window.0,
                island,
                rect.left,
                rect.top,
                w,
                h,
                SWP_NOACTIVATE | SWP_SHOWWINDOW,
            )
        } == 0
        {
            return Err(windows::core::Error::from_thread());
        }
        self.rect = Some(rect);
        Ok(())
    }

    fn hide(&mut self) {
        if self.rect.take().is_some() {
            unsafe { ShowWindow(self.window.0, SW_HIDE) };
        }
    }
}

unsafe extern "system" fn procedure(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_NCHITTEST => HTTRANSPARENT as LRESULT,
        WM_MOUSEACTIVATE => MA_NOACTIVATE as LRESULT,
        WM_ERASEBKGND => 1,
        _ => unsafe { DefWindowProcW(hwnd, msg, wp, lp) },
    }
}

thread_local! {
    // Cache an unsupported compositor as well. Transparent mode still works.
    static HOST: RefCell<Option<windows::core::Result<Backdrop>>> = const { RefCell::new(None) };
}

pub(super) fn sync(island: HWND, rect: RECT, enabled: bool) {
    HOST.with(|slot| {
        // Showing or moving a window can synchronously dispatch position messages.
        let Ok(mut slot) = slot.try_borrow_mut() else {
            return;
        };
        if enabled
            && unsafe { IsWindowVisible(island) } != 0
            && rect.right > rect.left
            && rect.bottom > rect.top
        {
            let result = slot.get_or_insert_with(Backdrop::new);
            if let Ok(host) = result
                && let Err(error) = host.place(island, rect)
            {
                *result = Err(error);
            }
        } else if let Some(Ok(host)) = slot.as_mut() {
            host.hide();
        }
    });
}

pub(super) fn reset() {
    HOST.with(|slot| {
        slot.take();
    });
}
