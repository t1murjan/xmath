//! `xmath-mcp`: MCP server over stdio (newline-delimited JSON-RPC 2.0).
//! Logs go to stderr only; stdout carries protocol messages.

use serde_json::{json, Value};
use std::io::{BufRead, Write};
use std::panic::{catch_unwind, AssertUnwindSafe};
use xmath_mcp::{handle, State};

fn main() {
    eprintln!("xmath-mcp {} ready (stdio)", env!("CARGO_PKG_VERSION"));
    let stdin = std::io::stdin();
    let mut out = std::io::stdout().lock();
    let mut state = State::default();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let reply = match serde_json::from_str::<Value>(line) {
            Ok(msg) => {
                let id = msg.get("id").cloned();
                match catch_unwind(AssertUnwindSafe(|| handle(&mut state, msg))) {
                    Ok(r) => r,
                    Err(_) => id.map(|id| {
                        json!({"jsonrpc":"2.0","id":id,"error":{"code":-32603,"message":"internal error"}})
                    }),
                }
            }
            Err(e) => Some(json!({"jsonrpc":"2.0","id":null,"error":{"code":-32700,"message":format!("parse error: {e}")}})),
        };
        if let Some(r) = reply {
            if writeln!(out, "{r}").and_then(|_| out.flush()).is_err() {
                break;
            }
        }
    }
}
