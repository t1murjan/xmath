//! Client of the live link to running xmath windows (see `xmath_core::live`): the tools take
//! `path: "window"` (the window used last) or `"window:<pid>"` instead of a file.

use serde_json::{json, Value};
use std::path::{Path, PathBuf};

/// Whether a tool `path` names an open window instead of a file.
pub fn is_window(path: &str) -> bool {
    let p = path.trim();
    p == "window" || p.starts_with("window:")
}

#[cfg(unix)]
fn request(sock: &Path, cmd: &Value) -> Result<Value, String> {
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::net::UnixStream;
    use std::time::Duration;
    let mut s = UnixStream::connect(sock).map_err(|e| format!("окно xmath недоступно ({e})"))?;
    let _ = s.set_read_timeout(Some(Duration::from_secs(25)));
    writeln!(s, "{cmd}").map_err(|e| e.to_string())?;
    let mut line = String::new();
    BufReader::new(&s).read_line(&mut line).map_err(|e| e.to_string())?;
    let v: Value = serde_json::from_str(&line).map_err(|_| "окно xmath не ответило (возможно, оно свёрнуто или занято)".to_string())?;
    match v.get("error").and_then(Value::as_str) {
        Some(e) => Err(e.to_string()),
        None => Ok(v),
    }
}

#[cfg(not(unix))]
fn request(_sock: &Path, _cmd: &Value) -> Result<Value, String> {
    Err("связь с открытым окном xmath пока есть только в Linux/macOS".into())
}

/// The open windows with their info (`pid`, `title`, `path`, `modified`, `regions`,
/// `focused_at`); sockets of windows that are gone are removed.
pub fn windows() -> Vec<(PathBuf, Value)> {
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir(xmath_core::live::socket_dir()) else { return out };
    for e in rd.flatten() {
        let p = e.path();
        if p.extension().is_none_or(|x| x != "sock") {
            continue;
        }
        match request(&p, &json!({"cmd": "info"})) {
            Ok(info) => out.push((p, info)),
            Err(_) => {
                // a window that crashed leaves its socket behind
                if std::os::unix::net::UnixStream::connect(&p).is_err() {
                    let _ = std::fs::remove_file(&p);
                }
            }
        }
    }
    out.sort_by(|a, b| b.1["focused_at"].as_f64().unwrap_or(0.0).total_cmp(&a.1["focused_at"].as_f64().unwrap_or(0.0)));
    out
}

/// The socket of the window `spec` names: "window" = the window used last.
fn pick(spec: &str) -> Result<(PathBuf, Value), String> {
    let ws = windows();
    if ws.is_empty() {
        return Err("нет открытых окон xmath: попросите пользователя запустить программу xmath (или работайте с файлом .xmcd/.xmath)".into());
    }
    match spec.trim().strip_prefix("window:") {
        Some(pid) => ws.into_iter().find(|(_, i)| i["pid"].to_string() == pid.trim()).ok_or_else(|| format!("нет окна xmath с pid {pid} (см. list_open_windows)")),
        None => Ok(ws.into_iter().next().unwrap()),
    }
}

/// The worksheet of an open window and the folder of its file (if saved).
pub fn load(spec: &str) -> Result<(xmath_core::Worksheet, Option<PathBuf>), String> {
    let (sock, _) = pick(spec)?;
    let v = request(&sock, &json!({"cmd": "get"}))?;
    let ws = xmath_core::Worksheet::from_json(&v["worksheet"].to_string())?;
    let dir = v["path"].as_str().and_then(|p| Path::new(p).parent().map(Path::to_path_buf));
    Ok((ws, dir))
}

/// Puts the changed worksheet into the window (one undo step there).
pub fn store(spec: &str, ws: &xmath_core::Worksheet) -> Result<(), String> {
    let (sock, _) = pick(spec)?;
    let w: Value = serde_json::from_str(&ws.to_json()).map_err(|e| e.to_string())?;
    request(&sock, &json!({"cmd": "set", "worksheet": w})).map(|_| ())
}
