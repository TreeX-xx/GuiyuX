#![windows_subsystem = "windows"]

use serde_json::{Value, json};
use std::{
    collections::HashMap,
    io::{self, BufRead, Write},
    sync::{
        Mutex,
        atomic::{AtomicU64, Ordering},
    },
};
use tauri::{Emitter, Manager};
use tokio::sync::oneshot;

#[derive(Default)]
struct Bridge {
    next: AtomicU64,
    waiting: Mutex<HashMap<u64, oneshot::Sender<Result<Value, String>>>>,
}

#[tauri::command]
async fn request(action: Value, bridge: tauri::State<'_, Bridge>) -> Result<Value, String> {
    let id = bridge.next.fetch_add(1, Ordering::Relaxed);
    let (tx, rx) = oneshot::channel();
    bridge.waiting.lock().unwrap().insert(id, tx);
    let sent = {
        let mut writer = io::stdout().lock();
        writeln!(writer, "{}", json!({ "id": id, "action": action })).and_then(|_| writer.flush())
    };
    if let Err(error) = sent {
        bridge.waiting.lock().unwrap().remove(&id);
        return Err(error.to_string());
    }
    rx.await.map_err(|_| "GuiyuX disconnected".to_owned())?
}

#[tauri::command]
fn present(window: tauri::WebviewWindow, title: String) -> Result<(), String> {
    window
        .set_title(&title)
        .and_then(|_| window.show())
        .and_then(|_| window.set_focus())
        .map_err(|e| e.to_string())
}

fn read_core(app: tauri::AppHandle) {
    for line in io::stdin().lock().lines().map_while(Result::ok) {
        let Ok(mut msg) = serde_json::from_str::<Value>(&line) else {
            break;
        };
        if let Some(id) = msg["id"].as_u64() {
            if let Some(tx) = app.state::<Bridge>().waiting.lock().unwrap().remove(&id) {
                let result = match msg["error"].as_str() {
                    Some(error) => Err(error.to_owned()),
                    None => Ok(msg["value"].take()),
                };
                let _ = tx.send(result);
            }
        } else if msg["event"] == "focus" {
            if let Some(w) = app.get_webview_window("settings") {
                let _ = w.unminimize();
                if w.is_visible().unwrap_or(false) {
                    let _ = w.set_focus();
                }
            }
        } else {
            let _ = app.emit("core-change", msg);
        }
    }
    app.state::<Bridge>().waiting.lock().unwrap().clear();
    app.exit(0);
}

fn main() {
    if std::env::args().nth(1).as_deref() != Some("--core-stdio") {
        return;
    }
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(Bridge::default())
        .setup(|app| {
            let handle = app.handle().clone();
            std::thread::spawn(move || read_core(handle));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![request, present])
        .run(tauri::generate_context!())
        .expect("GuiyuX settings host failed");
}
