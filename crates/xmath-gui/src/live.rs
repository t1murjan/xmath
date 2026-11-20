//! The window's end of the live link (see `xmath_core::live`): a Unix socket served by a
//! background thread; requests are handed to the UI thread, which owns the worksheet.

use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixListener;
use std::hash::{BuildHasher, RandomState};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

/// Computed on the UI thread so drags, undo, Save As and in-place edits are all visible.
pub fn revision(document: &RandomState, path: Option<&Path>, snapshot: &str) -> String {
    format!("{:016x}", document.hash_one((path, snapshot)))
}

/// One request from a client; the UI thread answers through `reply`.
pub struct Request {
    pub cmd: Value,
    pub reply: Sender<Value>,
}

pub struct Server {
    pub rx: Receiver<Request>,
    path: PathBuf,
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Starts listening (`None` if the socket cannot be created; the window works without it).
pub fn start(ctx: egui::Context) -> Option<Server> {
    let dir = xmath_core::live::socket_dir();
    std::fs::create_dir_all(&dir).ok()?;
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700));
    }
    let path = dir.join(format!("{}.sock", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let listener = UnixListener::bind(&path).ok()?;
    let (tx, rx) = mpsc::channel::<Request>();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let tx = tx.clone();
            let ctx = ctx.clone();
            std::thread::spawn(move || {
                let mut line = String::new();
                if BufReader::new(&stream).read_line(&mut line).is_err() {
                    return;
                }
                let cmd: Value = serde_json::from_str(&line).unwrap_or_else(|_| json!({"cmd": ""}));
                let (rtx, rrx) = mpsc::channel();
                if tx.send(Request { cmd, reply: rtx }).is_err() {
                    return;
                }
                ctx.request_repaint();
                let reply = rrx.recv_timeout(Duration::from_secs(20)).unwrap_or_else(|_| json!({"error": "окно xmath не ответило"}));
                let mut w = &stream;
                let _ = writeln!(w, "{reply}");
            });
        }
    });
    Some(Server { rx, path })
}
