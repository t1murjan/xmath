//! xmath-mcp: a hand-written MCP (Model Context Protocol) server for xmath.
//!
//! The protocol layer is a pure function, [`handle`], from one JSON-RPC message to an optional reply.

mod help;
mod tools;

use serde_json::{json, Value};

pub const SUPPORTED_VERSIONS: [&str; 3] = ["2025-06-18", "2025-03-26", "2024-11-05"];

pub const INSTRUCTIONS: &str = "\
xmath is a free Mathcad-15-style calculator with physical units. Workflow: test snippets with `evaluate`, \
then build files with `create_worksheet` / `add_region` and inspect them with `open_worksheet`. \
Syntax (one statement per line/region): `x := 5 m` defines; `f(x) := x^2` defines a function; `g0 ≡ 9.81` is a global definition; \
`expr =` shows the result; `expr = kN*m` shows it in a unit; ranges `i := 0..10`, `x := 0, 0.1..1`; indexed `v[i] := i^2`; \
definition with shown value `d := a/b =` / `d := a/b = mm`; vector definition `[a; b] := M*v`; matrix column `M^<j>`; element-wise `vectorize(v*w)`; matrices `[1, 2; 3, 4]`; `|x|`, `n!`, implicit multiplication `5 kN`, complex `2i`; \
sum(k,a,b,f), prod, integrate(expr, x, a, b), diff(expr, x, x0), root(expr, x, a, b), if(c,a,b), lsolve, polyroots. \
Programs: `f(n) := { s ← 0; for i ∈ 1..n { s ← s + i^2 }; s }` (also `<-`, `in`; `while c { }`, `if c { } otherwise { }`, `return e if c`, `break`, `continue`, `e1 on error e2`). \
Use `^` for powers, never `**`. `g` is gravity (9.807 m/s^2), grams are `gm`; indices start at 0 (ORIGIN). \
Regions are evaluated top-to-bottom then left-to-right, so define before use. \
Adding incompatible units (`5 m + 3 s`) is an error reported per line. Call `syntax_help` for the full reference, \
`list_functions` and `list_units` for the catalogs.";

/// Server state (currently only tracks the negotiated protocol version).
#[derive(Default)]
pub struct State {
    pub protocol_version: Option<String>,
}

fn ok(id: Value, result: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "result": result})
}

fn err(id: Value, code: i64, message: impl Into<String>) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message.into()}})
}

fn tool_error(msg: String) -> Value {
    json!({"content": [{"type": "text", "text": msg}], "isError": true})
}

/// Handles one incoming JSON-RPC message. Returns `None` for notifications.
pub fn handle(state: &mut State, msg: Value) -> Option<Value> {
    let Some(obj) = msg.as_object() else {
        return Some(err(Value::Null, -32600, "invalid request: expected a JSON object"));
    };
    let method = obj.get("method").and_then(Value::as_str);
    let id = obj.get("id").cloned();
    let Some(method) = method else {
        // A response or a malformed message: ignore responses, reject the rest.
        return match id {
            Some(_) if obj.contains_key("result") || obj.contains_key("error") => None,
            Some(id) => Some(err(id, -32600, "invalid request: missing method")),
            None => None,
        };
    };
    // Notifications (no id) never get a reply.
    let id = id?;
    let params = obj.get("params").cloned().unwrap_or(Value::Null);
    Some(match method {
        "initialize" => {
            let requested = params.get("protocolVersion").and_then(Value::as_str).unwrap_or("");
            let version = if SUPPORTED_VERSIONS.contains(&requested) { requested } else { SUPPORTED_VERSIONS[0] };
            state.protocol_version = Some(version.to_string());
            ok(
                id,
                json!({
                    "protocolVersion": version,
                    "capabilities": {"tools": {}},
                    "serverInfo": {"name": "xmath", "version": env!("CARGO_PKG_VERSION")},
                    "instructions": INSTRUCTIONS,
                }),
            )
        }
        "ping" => ok(id, json!({})),
        "tools/list" => ok(id, json!({"tools": tools::tool_list()})),
        "tools/call" => {
            let Some(name) = params.get("name").and_then(Value::as_str) else {
                return Some(err(id, -32602, "tools/call: missing `name`"));
            };
            let args = params.get("arguments").cloned().unwrap_or_else(|| json!({}));
            if !args.is_object() && !args.is_null() {
                return Some(err(id, -32602, "tools/call: `arguments` must be an object"));
            }
            match tools::call_tool(name, &args) {
                None => err(id, -32602, format!("unknown tool: {name}")),
                Some(Ok(out)) => {
                    let mut content = vec![json!({"type": "text", "text": out.text})];
                    let mut res = json!({"isError": false});
                    if !out.data.is_null() {
                        content.push(json!({"type": "text", "text": out.data.to_string()}));
                        res["structuredContent"] = out.data;
                    }
                    res["content"] = Value::Array(content);
                    ok(id, res)
                }
                Some(Err(e)) => ok(id, tool_error(e)),
            }
        }
        "resources/list" => ok(id, json!({"resources": []})),
        "prompts/list" => ok(id, json!({"prompts": []})),
        other => err(id, -32601, format!("method not found: {other}")),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(state: &mut State, name: &str, args: Value) -> Value {
        let req = json!({"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":name,"arguments":args}});
        handle(state, req).expect("reply")
    }

    fn text(reply: &Value) -> String {
        reply["result"]["content"][0]["text"].as_str().unwrap_or_default().to_string()
    }

    fn temp_dir(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("xmath-mcp-test-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn initialize_negotiates_version() {
        let mut st = State::default();
        let r = handle(&mut st, json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05"}})).unwrap();
        assert_eq!(r["result"]["protocolVersion"], "2024-11-05");
        assert_eq!(r["result"]["serverInfo"]["name"], "xmath");
        assert!(r["result"]["capabilities"]["tools"].is_object());
        assert!(r["result"]["instructions"].as_str().unwrap().contains(":="));
        let r = handle(&mut st, json!({"jsonrpc":"2.0","id":2,"method":"initialize","params":{"protocolVersion":"1999"}})).unwrap();
        assert_eq!(r["result"]["protocolVersion"], "2025-06-18");
    }

    #[test]
    fn notifications_and_ping() {
        let mut st = State::default();
        assert!(handle(&mut st, json!({"jsonrpc":"2.0","method":"notifications/initialized"})).is_none());
        let r = handle(&mut st, json!({"jsonrpc":"2.0","id":"a","method":"ping"})).unwrap();
        assert_eq!(r["id"], "a");
        assert_eq!(r["result"], json!({}));
        let r = handle(&mut st, json!({"jsonrpc":"2.0","id":3,"method":"nope"})).unwrap();
        assert_eq!(r["error"]["code"], -32601);
        let r = handle(&mut st, json!([1, 2])).unwrap();
        assert_eq!(r["error"]["code"], -32600);
    }

    #[test]
    fn tools_list_has_all_tools() {
        let mut st = State::default();
        let r = handle(&mut st, json!({"jsonrpc":"2.0","id":1,"method":"tools/list"})).unwrap();
        let names: Vec<&str> = r["result"]["tools"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap()).collect();
        for n in [
            "evaluate", "open_worksheet", "create_worksheet", "add_region", "update_region", "delete_region",
            "list_functions", "list_units", "syntax_help", "export_pdf", "export_xmcd",
        ] {
            assert!(names.contains(&n), "missing {n}");
        }
        for t in r["result"]["tools"].as_array().unwrap() {
            assert_eq!(t["inputSchema"]["type"], "object");
            assert!(!t["description"].as_str().unwrap().is_empty());
        }
    }

    #[test]
    fn evaluate_units() {
        let mut st = State::default();
        let r = call(&mut st, "evaluate", json!({"lines": ["L := 6 m", "q := 12 kN/m", "M := q*L^2/8", "M = kN*m", "", "5 m + 3 s =", "1/0 ="]}));
        assert_eq!(r["result"]["isError"], false);
        let t = text(&r);
        assert!(t.contains("54 kN*m"), "{t}");
        let res = &r["result"]["structuredContent"]["results"];
        assert_eq!(res[3]["result"], "54 kN*m");
        assert!(res[4]["error"].as_str().unwrap().contains("Единицы"), "{}", res[4]["error"]);
        assert!(res[5]["error"].is_string());
        assert_eq!(r["result"]["structuredContent"]["errors"], 2);
    }

    #[test]
    fn evaluate_text_and_bad_args() {
        let mut st = State::default();
        let r = call(&mut st, "evaluate", json!({"text": "x := 3\nx^2 ="}));
        assert!(text(&r).contains("9"));
        let r = call(&mut st, "evaluate", json!({}));
        assert_eq!(r["result"]["isError"], true);
        let r = call(&mut st, "evaluate", json!({"lines": "nope"}));
        assert_eq!(r["result"]["isError"], true);
        let r = call(&mut st, "no_such_tool", json!({}));
        assert_eq!(r["error"]["code"], -32602);
        let r = handle(&mut st, json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{}})).unwrap();
        assert_eq!(r["error"]["code"], -32602);
        // garbage must not panic
        let r = call(&mut st, "evaluate", json!({"lines": ["((((", "]]][[", "∞∞ =", "f(x) := f(x)", "f(1) ="]}));
        assert_eq!(r["result"]["isError"], false);
    }

    #[test]
    fn file_roundtrip() {
        let dir = temp_dir("rt");
        let path = dir.join("beam.xmath");
        let p = path.to_str().unwrap();
        let mut st = State::default();

        let r = call(&mut st, "create_worksheet", json!({"path": p, "lines": ["# Beam", "L := 6 m", "q := 12 kN/m"]}));
        assert_eq!(r["result"]["isError"], false, "{r}");
        assert!(path.exists());
        // refuses to overwrite
        let r = call(&mut st, "create_worksheet", json!({"path": p}));
        assert_eq!(r["result"]["isError"], true);

        let r = call(&mut st, "add_region", json!({"path": p, "kind": "math", "source": "M := q*L^2/8"}));
        assert_eq!(r["result"]["isError"], false, "{r}");
        let r = call(&mut st, "add_region", json!({"path": p, "kind": "math", "source": "M = kN*m"}));
        assert!(text(&r).contains("54 kN*m"), "{}", text(&r));
        let new_id = r["result"]["structuredContent"]["new_id"].as_u64().unwrap();

        // insert in the middle pushes the following regions down
        let r = call(&mut st, "add_region", json!({"path": p, "kind": "math", "source": "k := 2", "after_id": 1}));
        assert_eq!(r["result"]["isError"], false, "{r}");
        let r = call(&mut st, "open_worksheet", json!({"path": p}));
        let regs = r["result"]["structuredContent"]["regions"].as_array().unwrap().clone();
        assert_eq!(regs.len(), 6);
        assert_eq!(regs[1]["source"], "k := 2");
        let ys: Vec<f64> = regs.iter().map(|r| r["y"].as_f64().unwrap()).collect();
        assert!(ys.windows(2).all(|w| w[0] < w[1]), "{ys:?}");

        let r = call(&mut st, "update_region", json!({"path": p, "id": new_id, "source": "M = N*m"}));
        assert!(text(&r).contains("54000 N*m") || text(&r).contains("5.4"), "{}", text(&r));

        let r = call(&mut st, "delete_region", json!({"path": p, "id": new_id}));
        assert_eq!(r["result"]["isError"], false);
        let r = call(&mut st, "delete_region", json!({"path": p, "id": 999}));
        assert_eq!(r["result"]["isError"], true);

        // plot region
        let r = call(&mut st, "add_region", json!({"path": p, "kind": "math", "source": "x := 0, 0.5..2"}));
        assert_eq!(r["result"]["isError"], false);
        let r = call(&mut st, "add_region", json!({"path": p, "kind": "plot", "x_expr": "x", "y_exprs": ["x^2", "x"]}));
        assert_eq!(r["result"]["isError"], false, "{r}");
        let regs = r["result"]["structuredContent"]["regions"].as_array().unwrap().clone();
        let plot = regs.iter().find(|r| r["type"] == "plot").unwrap();
        assert_eq!(plot["plot"]["series"].as_array().unwrap().len(), 2);
        assert_eq!(plot["plot"]["series"][0]["points"], 5);
        assert_eq!(plot["plot"]["series"][0]["y_range"], json!([0.0, 4.0]));

        // polar and 3D plots
        let r = call(&mut st, "add_region", json!({"path": p, "kind": "polar_plot", "x_expr": "x", "y_exprs": ["1 + cos(x)"]}));
        assert_eq!(r["result"]["isError"], false, "{r}");
        let r = call(&mut st, "add_region", json!({"path": p, "kind": "surface_plot", "expr": "sin(a)*cos(b)"}));
        assert_eq!(r["result"]["isError"], false, "{r}");
        let regs = r["result"]["structuredContent"]["regions"].as_array().unwrap().clone();
        assert!(regs.iter().any(|r| r["type"] == "polar_plot" && r["plot"]["series"][0]["points"] == 5));
        let s = regs.iter().find(|r| r["type"] == "surface_plot").unwrap();
        assert_eq!(s["plot"]["grid"], json!([25, 25]));
        assert!(s["plot"]["z_range"][0].as_f64().unwrap() < -0.9);
        let r = call(&mut st, "add_region", json!({"path": p, "kind": "surface_plot"}));
        assert_eq!(r["result"]["isError"], true);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn write_guards() {
        let dir = temp_dir("guard");
        let mut st = State::default();
        let bad = dir.join("evil.txt");
        let r = call(&mut st, "create_worksheet", json!({"path": bad.to_str().unwrap()}));
        assert_eq!(r["result"]["isError"], true);
        assert!(!bad.exists());
        let r = call(&mut st, "add_region", json!({"path": bad.to_str().unwrap(), "kind": "math", "source": "x := 1"}));
        assert_eq!(r["result"]["isError"], true);
        let r = call(&mut st, "open_worksheet", json!({"path": dir.join("missing.xmath").to_str().unwrap()}));
        assert_eq!(r["result"]["isError"], true);
        let good = dir.join("a.xmath");
        call(&mut st, "create_worksheet", json!({"path": good.to_str().unwrap()}));
        for args in [
            json!({"kind": "math"}),
            json!({"kind": "weird", "source": "x"}),
            json!({"kind": "math", "source": "a := 1\nb := 2"}),
            json!({"kind": "plot"}),
            json!({"kind": "math", "source": "x", "after_id": 42}),
            json!({"kind": "math", "source": 5}),
        ] {
            let mut a = args.clone();
            a["path"] = json!(good.to_str().unwrap());
            let r = call(&mut st, "add_region", a);
            assert_eq!(r["result"]["isError"], true, "{args}");
        }
        // corrupt file
        std::fs::write(&good, "{ not json").unwrap();
        let r = call(&mut st, "open_worksheet", json!({"path": good.to_str().unwrap()}));
        assert_eq!(r["result"]["isError"], true);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn catalogs_and_help() {
        let mut st = State::default();
        let r = call(&mut st, "list_functions", json!({}));
        let fs = r["result"]["structuredContent"]["functions"].as_array().unwrap().clone();
        assert_eq!(fs.len(), xmath_core::builtins::NAMES.len());
        assert!(fs.iter().all(|f| !f["description"].as_str().unwrap().is_empty()), "every builtin needs a description");
        let r = call(&mut st, "list_units", json!({}));
        assert!(text(&r).contains("kN"));
        let r = call(&mut st, "syntax_help", json!({}));
        assert!(text(&r).contains("Statements"));
    }
}
