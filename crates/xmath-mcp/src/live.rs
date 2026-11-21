//! Client of the live link to running xmath windows (see `xmath_core::live`): the tools take
//! `path: "window"` (the document used last), `"window:<pid>"` (active in that process),
//! or `"window:<pid>:<document_id>"` (a specific open document).

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
#[cfg(not(unix))]
pub fn windows() -> Vec<(PathBuf, Value)> {
    Vec::new()
}

#[cfg(unix)]
pub fn windows() -> Vec<(PathBuf, Value)> {
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir(xmath_core::live::socket_dir()) else { return out };
    for e in rd.flatten() {
        let p = e.path();
        if p.extension().is_none_or(|x| x != "sock") {
            continue;
        }
        match request(&p, &json!({"cmd": "info"})) {
            Ok(info) => {
                let documents = info["documents"].as_array().cloned().unwrap_or_else(|| vec![info]);
                out.extend(documents.into_iter().map(|doc| (p.clone(), with_target(doc))));
            }
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

#[cfg(unix)]
fn with_target(mut info: Value) -> Value {
    let pid = &info["pid"];
    let target = match info["document_id"].as_u64() {
        Some(id) => format!("window:{pid}:{id}"),
        None => format!("window:{pid}"),
    };
    info["target"] = json!(target);
    info
}

/// Resolves a worksheet to one socket and, when supported, one document in that process.
fn pick(spec: &str) -> Result<(PathBuf, Value), String> {
    let ws = windows();
    if ws.is_empty() {
        return Err("нет открытых листов xmath: откройте документ или укажите путь к файлу".into());
    }
    let spec = spec.trim();
    select_target(ws, spec)
        .ok_or_else(|| format!("нет открытого листа xmath «{spec}» (см. list_open_windows)"))
}

fn select_target(windows: Vec<(PathBuf, Value)>, spec: &str) -> Option<(PathBuf, Value)> {
    if spec == "window" {
        return windows.into_iter().next();
    }
    let pid = spec.strip_prefix("window:")?;
    windows.into_iter().find(|(_, info)| {
        info["target"] == spec
            || (info["active"].as_bool().unwrap_or(true)
                && info["pid"].to_string() == pid)
    })
}

/// The exact socket and document revision returned by a single read.
pub struct WindowEdit {
    socket: PathBuf,
    document_id: Option<u64>,
    pub revision: Option<String>,
}

pub fn load(spec: &str) -> Result<(xmath_core::Worksheet, Option<PathBuf>, WindowEdit), String> {
    let (socket, info) = pick(spec)?;
    let document_id = info["document_id"].as_u64();
    let mut cmd = json!({"cmd": "get"});
    if let Some(id) = document_id {
        cmd["document_id"] = json!(id);
    }
    let v = request(&socket, &cmd)?;
    let ws = xmath_core::Worksheet::from_json(&v["worksheet"].to_string())?;
    let dir = v["path"].as_str().and_then(|p| Path::new(p).parent().map(Path::to_path_buf));
    let revision = v["revision"].as_str().map(str::to_owned);
    Ok((ws, dir, WindowEdit { socket, document_id, revision }))
}

impl WindowEdit {
    pub fn store(&mut self, ws: &xmath_core::Worksheet) -> Result<(), String> {
        let revision = self.revision.as_deref().ok_or("Обновите и перезапустите xmath: открытое окно не поддерживает проверку версии листа.")?;
        let mut cmd = json!({"cmd": "set_if_current", "revision": revision, "worksheet": ws});
        if let Some(id) = self.document_id {
            cmd["document_id"] = json!(id);
        }
        let reply = request(&self.socket, &cmd)?;
        if reply["ok"] != true {
            return Err("Окно xmath не подтвердило изменение листа.".into());
        }
        self.revision = reply["revision"].as_str().map(str::to_owned);
        Ok(())
    }
}


#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn targets_distinguish_documents_sharing_one_socket() {
        let documents = || vec![
            (PathBuf::from("12.sock"), with_target(json!({"pid": 12, "document_id": 8, "active": true}))),
            (PathBuf::from("12.sock"), with_target(json!({"pid": 12, "document_id": 7, "active": false}))),
            (PathBuf::from("13.sock"), with_target(json!({"pid": 13}))),
        ];
        assert_eq!(select_target(documents(), "window").unwrap().1["target"], "window:12:8");
        assert_eq!(select_target(documents(), "window:12").unwrap().1["document_id"], 8);
        assert_eq!(select_target(documents(), "window:12:7").unwrap().1["document_id"], 7);
        assert_eq!(select_target(documents(), "window:13").unwrap().1["target"], "window:13");
        assert!(select_target(documents(), "window:12:6").is_none());
    }
}
