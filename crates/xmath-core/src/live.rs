//! The live link between running xmath windows and other programs (the MCP server, so that
//! an AI assistant works in the window the user has open instead of starting a new one).
//!
//! Every window listens on a Unix socket `<socket_dir()>/<pid>.sock`. A request is one line
//! of JSON, the reply one line of JSON:
//! * `{"cmd":"info"}` → `{"pid", "title", "path", "modified", "regions", "focused_at"}`
//! * `{"cmd":"get"}` → `{"path", "worksheet": <worksheet JSON>, "revision": <opaque string>}`
//! * `{"cmd":"set_if_current","revision": <from get>, "worksheet": <worksheet JSON>}` → `{"ok": true, "revision": <new revision>}` if the document still matches
//! * Legacy clients: `{"cmd":"set","worksheet": <worksheet JSON>}` → `{"ok": true}` (one undo step in the window)
//!
//! Errors are replied as `{"error": "..."}`.

use std::path::PathBuf;

/// The directory of the window sockets (per user, not readable by others).
pub fn socket_dir() -> PathBuf {
    if let Some(d) = std::env::var_os("XDG_RUNTIME_DIR").filter(|d| !d.is_empty()) {
        return PathBuf::from(d).join("xmath");
    }
    let user = std::env::var("USER").or_else(|_| std::env::var("LOGNAME")).unwrap_or_else(|_| "user".into());
    std::env::temp_dir().join(format!("xmath-{user}"))
}
