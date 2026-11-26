// Run via build.ps1 -VerifyOnly. See .agents/notes/module.md#技术路线与验证.
#[path = "../src/test_desktop.rs"]
mod test_desktop;

use serde_json::json;
use std::{
    fs, io,
    os::windows::{
        ffi::OsStrExt,
        io::{AsRawHandle, FromRawHandle, OwnedHandle},
    },
    path::Path,
    ptr::{null, null_mut},
    thread,
    time::{Duration, Instant},
};
use test_desktop::TestDesktop;
use windows_sys::Win32::{
    Foundation::{HANDLE, HWND, RECT, WAIT_OBJECT_0},
    System::{JobObjects::*, Threading::*},
    UI::WindowsAndMessaging::*,
};

fn check(ok: i32) -> io::Result<()> {
    if ok == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

fn own(handle: HANDLE) -> io::Result<OwnedHandle> {
    if handle.is_null() {
        Err(io::Error::last_os_error())
    } else {
        Ok(unsafe { OwnedHandle::from_raw_handle(handle) })
    }
}

// Assign the suspended core before its first instruction. Closing the job also
// cleans up its settings host and WebView processes on timeout or failure.
fn launch(exe: &Path) -> io::Result<(OwnedHandle, OwnedHandle, u32)> {
    let path: Vec<_> = exe.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut desktop = TestDesktop::name();
    unsafe {
        let job = own(CreateJobObjectW(null(), null()))?;
        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        check(SetInformationJobObject(
            job.as_raw_handle(),
            JobObjectExtendedLimitInformation,
            (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
            size_of_val(&limits) as u32,
        ))?;
        let startup = STARTUPINFOW {
            cb: size_of::<STARTUPINFOW>() as u32,
            lpDesktop: desktop.as_mut_ptr(),
            ..Default::default()
        };
        let mut child = PROCESS_INFORMATION::default();
        check(CreateProcessW(
            path.as_ptr(),
            null_mut(),
            null(),
            null(),
            0,
            CREATE_SUSPENDED | CREATE_NO_WINDOW,
            null(),
            null(),
            &startup,
            &mut child,
        ))?;
        let process = own(child.hProcess)?;
        let main_thread = own(child.hThread)?;
        if let Err(e) = check(AssignProcessToJobObject(
            job.as_raw_handle(),
            process.as_raw_handle(),
        )) {
            TerminateProcess(process.as_raw_handle(), 1);
            return Err(e);
        }
        if ResumeThread(main_thread.as_raw_handle()) == u32::MAX {
            return Err(io::Error::last_os_error());
        }
        Ok((job, process, child.dwProcessId))
    }
}

fn find(class: &[u16], pid: Option<u32>) -> Option<HWND> {
    unsafe {
        let mut window = null_mut();
        loop {
            window = FindWindowExW(null_mut(), window, class.as_ptr(), null());
            if window.is_null() {
                return None;
            }
            let mut owner = 0;
            GetWindowThreadProcessId(window, &mut owner);
            if pid.is_none_or(|id| id == owner) {
                return Some(window);
            }
        }
    }
}

fn wait<T>(label: &str, mut probe: impl FnMut() -> Option<T>) -> io::Result<T> {
    let start = Instant::now();
    loop {
        if let Some(result) = probe() {
            return Ok(result);
        }
        if start.elapsed() >= Duration::from_secs(16) {
            return Err(io::Error::new(io::ErrorKind::TimedOut, label.to_owned()));
        }
        thread::sleep(Duration::from_millis(30));
    }
}

fn title(hwnd: HWND) -> String {
    let mut buf = [0; 128];
    let len = unsafe { GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32) };
    String::from_utf16_lossy(&buf[..len as usize])
}

fn command(hwnd: HWND, id: usize) -> io::Result<()> {
    check(unsafe { PostMessageW(hwnd, WM_COMMAND, id, 0) })
}

fn bottom_capsule(hwnd: HWND, work: &RECT, depth: i32) -> bool {
    let mut rect = RECT::default();
    (unsafe { GetWindowRect(hwnd, &mut rect) != 0 })
        && rect.bottom == work.bottom
        && rect.top == work.bottom - depth
        && rect.right - rect.left == 120
        && (rect.left + rect.right - work.left - work.right).abs() <= 1
}

fn main() -> io::Result<()> {
    let folder = std::env::args_os()
        .nth(1)
        .ok_or(io::ErrorKind::InvalidInput)?;
    let exe = fs::canonicalize(Path::new(&folder).join("guiyux.exe"))?;
    fs::metadata(exe.with_file_name("guiyux-settings.exe"))?;
    let _desktop = TestDesktop::enter()?;
    let temp = tempfile::tempdir()?;
    let profile = temp.path().join("GuiyuX");
    fs::create_dir(&profile)?;
    let group = json!({"builtin":"documents"});
    let mut items = Vec::new();
    for i in 0..13 {
        let file = temp.path().join(format!("sample-{i}.txt"));
        fs::write(&file, "GuiyuX test")?;
        items.push(json!({"path":file,"group":group,"manual":true}));
    }
    fs::write(
        profile.join("settings.json"),
        json!({
            "language":"en","bar_revision":1,"bar":[{"group":group}],"items":items,
            "dock":{"edge":"bottom","mode":"autoHide"}
        })
        .to_string(),
    )?;
    // This standalone test driver is single-threaded. Its children inherit
    // these process-local paths, including a separate WebView2 data directory.
    unsafe {
        std::env::set_var("LOCALAPPDATA", temp.path());
        std::env::set_var("WEBVIEW2_USER_DATA_FOLDER", temp.path().join("webview"));
    }
    let (job, process, pid) = launch(&exe)?;
    let wide = |s: &str| s.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
    let island = wait("island startup", || find(&wide("GuiyuX.Island"), Some(pid)))?;
    let button = |id| unsafe { GetDlgItem(island, id) };
    let mut work = RECT::default();
    check(unsafe {
        SystemParametersInfoW(SPI_GETWORKAREA, 0, (&mut work as *mut RECT).cast(), 0)
    })?;
    let capsule_at = |depth| {
        bottom_capsule(island, &work, depth) && unsafe { IsWindowVisible(button(200)) == 0 }
    };
    wait("tucked bottom capsule", || capsule_at(5).then_some(()))?;
    // A private desktop has no physical pointer. The native handler test drives
    // enter/leave and checks the actual window bounds without moving one here.
    command(island, 101)?;
    wait("expanded shortcuts", || {
        (title(button(200)) == "Documents").then_some(())
    })?;
    command(island, 200)?;
    wait("first grid page", || {
        (title(button(211)) == "sample-11.txt").then_some(())
    })?;
    command(island, 301)?;
    wait("second grid page", || {
        (title(button(200)) == "sample-12.txt" && unsafe { IsWindowVisible(button(201)) } == 0)
            .then_some(())
    })?;
    command(island, 300)?;
    wait("previous grid page", || {
        (title(button(200)) == "sample-0.txt").then_some(())
    })?;
    for _ in 0..2 {
        command(island, 4001)?;
        let settings = wait("settings page", || {
            find(&wide("Tauri Window"), None)
                .filter(|&w| unsafe { IsWindowVisible(w) != 0 } && title(w) == "GuiyuX Settings")
        })?;
        let mut host_pid = 0;
        unsafe { GetWindowThreadProcessId(settings, &mut host_pid) };
        let host = own(unsafe {
            OpenProcess(
                PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
                0,
                host_pid,
            )
        })?;
        let mut in_job = 0;
        check(unsafe { IsProcessInJob(host.as_raw_handle(), job.as_raw_handle(), &mut in_job) })?;
        if in_job == 0 {
            return Err(io::Error::other("Unexpected settings process"));
        }
        let helper = find(&wide("Tao Thread Event Target"), Some(host_pid))
            .ok_or_else(|| io::Error::other("Missing settings event target"))?;
        check(unsafe { PostMessageW(helper, WM_CLOSE, 0, 0) })?;
        wait("settings shutdown", || {
            (unsafe { WaitForSingleObject(host.as_raw_handle(), 0) } == WAIT_OBJECT_0).then_some(())
        })?;
    }
    check(unsafe { PostMessageW(island, WM_CLOSE, 0, 0) })?;
    wait("core shutdown", || {
        (unsafe { WaitForSingleObject(process.as_raw_handle(), 0) } == WAIT_OBJECT_0).then_some(())
    })?;
    println!(
        "Private desktop checks passed: tucked bottom capsule, click to open, grid paging, settings close and reopen."
    );
    Ok(())
}
