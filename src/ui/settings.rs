// The resident core owns settings and launches the WebView only on demand.
// See .agents/notes/module.md「设置窗口与常驻核心」.
use super::*;
use crate::collection::{self, OTHER};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    io::{BufRead, BufReader, Write},
    process::{Child, Command, Stdio},
    sync::mpsc::{self, Receiver, Sender},
    thread,
};

mod actions;

pub(super) const DISPATCH: u32 = WM_APP + 24;

#[derive(Deserialize)]
struct Request {
    id: u64,
    action: Value,
}

enum Incoming {
    Request(Request),
    Closed,
}

struct Session {
    generation: usize,
    child: Child,
    output: Sender<Value>,
    input: Receiver<Incoming>,
    ready: bool,
    notice: Option<String>,
}

thread_local! {
    static SESSION: RefCell<Option<Session>> = const { RefCell::new(None) };
    static GENERATION: Cell<usize> = const { Cell::new(0) };
}

fn restore_appearance() {
    if let Ok(saved) = collection::current()
        && APPEARANCE_STYLE.replace(saved.appearance) != saved.appearance
    {
        island::appearance_changed();
    }
}

pub(super) fn open() -> io::Result<()> {
    let active = SESSION.with_borrow_mut(|slot| -> io::Result<bool> {
        let Some(s) = slot else { return Ok(false) };
        if s.child.try_wait()?.is_some() {
            return Ok(false);
        }
        unsafe { AllowSetForegroundWindow(s.child.id()) };
        Ok(s.output.send(json!({"event": "focus"})).is_ok())
    })?;
    if active {
        return Ok(());
    }
    if SESSION.take().is_some() {
        restore_appearance();
    }

    let program = std::env::current_exe()?.with_file_name("guiyux-settings.exe");
    let mut child = Command::new(&program)
        .arg("--core-stdio")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|e| io::Error::new(e.kind(), format!("{}\n{e}", program.display())))?;
    unsafe { AllowSetForegroundWindow(child.id()) };
    let generation = GENERATION.get().wrapping_add(1);
    GENERATION.set(generation);
    let mut writer = child.stdin.take().unwrap();
    let reader = child.stdout.take().unwrap();
    let (output, outgoing) = mpsc::channel::<Value>();
    let (incoming, input) = mpsc::channel();
    let island = ISLAND.get() as usize;
    thread::spawn(move || {
        for msg in outgoing {
            if writeln!(writer, "{msg}")
                .and_then(|_| writer.flush())
                .is_err()
            {
                break;
            }
        }
    });
    thread::spawn(move || {
        let wake = |event| {
            incoming.send(event).is_ok()
                && unsafe { PostMessageW(island as HWND, DISPATCH, generation, 0) != 0 }
        };
        for line in BufReader::new(reader).lines().map_while(Result::ok) {
            if let Ok(request) = serde_json::from_str(&line)
                && !wake(Incoming::Request(request))
            {
                return;
            }
        }
        wake(Incoming::Closed);
    });
    SESSION.replace(Some(Session {
        generation,
        child,
        output,
        input,
        ready: false,
        notice: None,
    }));
    Ok(())
}

pub(super) fn notify() {
    SESSION.with_borrow(|slot| {
        if let Some(s) = slot {
            let _ = s.output.send(json!({"event": "refresh"}));
        }
    });
}

pub(super) fn organize() -> io::Result<()> {
    let (changed, other) = collection::update(|s| (s.organize(), s.count(OTHER)))?;
    island::collection_changed()?;
    open()?;
    let message = actions::summary(changed, other);
    SESSION.with_borrow_mut(|slot| {
        if let Some(s) = slot {
            if s.ready {
                let _ = s
                    .output
                    .send(json!({"event":"organized", "message":message}));
            } else {
                s.notice = Some(message);
            }
        }
    });
    Ok(())
}

pub(super) fn dispatch(generation: usize) {
    let event = SESSION.with_borrow(|slot| {
        slot.as_ref()
            .filter(|s| s.generation == generation)?
            .input
            .try_recv()
            .ok()
    });
    match event {
        Some(Incoming::Request(request)) => {
            let initial = request.action["type"] == "initial";
            let result = actions::execute(request.action);
            SESSION.with_borrow_mut(|slot| {
                if let Some(s) = slot.as_mut().filter(|s| s.generation == generation) {
                    let response = match result {
                        Ok(mut value) => {
                            if initial {
                                s.ready = true;
                                value["notice"] = json!(s.notice.take());
                            }
                            json!({"id": request.id, "value": value})
                        }
                        Err(e) => json!({"id": request.id, "error": e.to_string()}),
                    };
                    let _ = s.output.send(response);
                }
            });
        }
        Some(Incoming::Closed) => {
            let session = SESSION.take().unwrap();
            let started = session.ready;
            drop(session);
            restore_appearance();
            if !started {
                report(&io::Error::other(crate::language().pick(
                    "设置窗口未能启动。请检查同目录的设置程序和 Microsoft Edge WebView2 运行时。",
                    "Settings could not start. Check the companion executable and the Microsoft Edge WebView2 Runtime.",
                )));
            }
        }
        None => {}
    }
}

pub(super) fn shutdown() {
    // Closing the inherited stdin also closes the companion if the core exits unexpectedly.
    SESSION.take();
}
