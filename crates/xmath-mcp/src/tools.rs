//! Tool definitions and implementations.

use crate::help;
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use xmath_core::{evaluate_worksheet, Output, Plot3dKind, PlotKind, Region, RegionKind, RegionOutput, Worksheet};

/// Successful tool result: human-readable text plus structured data.
pub struct ToolOutput {
    pub text: String,
    pub data: Value,
}

pub type ToolResult = Result<ToolOutput, String>;

const MATH_H: f32 = 30.0;
const GAP: f32 = 0.0;
const LEFT: f32 = 20.0;

pub fn tool_list() -> Value {
    let path = json!({"type":"string","description":"Path to a .xmath or Mathcad 15 .xmcd file (absolute, or relative to the server's working directory)."});
    json!([
        {
            "name": "evaluate",
            "description": "Evaluate Mathcad-style math lines (one region per line, top to bottom, with physical units) and return each line with its result or error. Use this first for quick calculations and to test snippets. Definitions use `:=` (e.g. `L := 6 m`, `f(x) := x^2`), a line ending in `=` shows a result (`M =`, or `M = kN*m` to choose the unit). Lines starting with `#` are comments. Nothing is written to disk. Call `syntax_help` for the full syntax.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "lines": {"type":"array","items":{"type":"string"},"description":"Lines to evaluate, e.g. [\"L := 6 m\", \"q := 12 kN/m\", \"M := q*L^2/8\", \"M = kN*m\"]."},
                    "text": {"type":"string","description":"Alternative to `lines`: the same content as one string with newline-separated lines."}
                }
            }
        },
        {
            "name": "open_worksheet",
            "description": "Load a .xmath or Mathcad .xmcd worksheet file, evaluate it and return every region (id, type, position, source/text, result, error, plot summary) in evaluation order.",
            "inputSchema": {"type":"object","properties":{"path":path.clone()},"required":["path"]}
        },
        {
            "name": "create_worksheet",
            "description": "Create a new .xmath or .xmcd (Mathcad 15) worksheet file, optionally filled with lines (one math region per line; `# text` lines become text regions; regions are stacked vertically). Returns the evaluated regions. Refuses to overwrite an existing file unless `overwrite` is true.",
            "inputSchema": {
                "type":"object",
                "properties":{
                    "path":path.clone(),
                    "lines":{"type":"array","items":{"type":"string"},"description":"Initial lines, as in `evaluate`."},
                    "overwrite":{"type":"boolean","description":"Replace an existing file (default false)."}
                },
                "required":["path"]
            }
        },
        {
            "name": "add_region",
            "description": "Insert a region into a .xmath or .xmcd worksheet and save it (an .xmcd file is rewritten as a Mathcad 15 file: plots as Mathcad plots with pictures, results stored). kind `math` needs `source` (one statement, e.g. `A := b*h`), kind `text` needs `text`, kind `plot` needs `y_exprs` (list of expressions) and usually `x_expr` (a range variable defined above, e.g. `x`). Default position: below the last region; with `after_id` the region is placed right after that one and the following regions are pushed down; explicit `x`/`y` override. Remember that evaluation order is top-to-bottom, so definitions must sit above their uses. Returns the new region id and the re-evaluated worksheet.",
            "inputSchema": {
                "type":"object",
                "properties":{
                    "path":path.clone(),
                    "kind":{"type":"string","enum":["math","text","plot","polar_plot","surface_plot","contour_plot","scatter_plot","bar_plot"],"description":"polar_plot takes x_expr (angle) and y_exprs (radii); surface_plot / contour_plot / bar_plot take `expr` (a matrix, a function of two variables, or an expression in x and y); scatter_plot takes `expr` = `X, Y, Z`."},
                    "expr":{"type":"string","description":"3D plots: what to plot (matrix name, function name, expression in x, y, or `X, Y, Z` vectors for scatter_plot)."},
                    "source":{"type":"string","description":"math: the single-line statement."},
                    "text":{"type":"string","description":"text: the comment text."},
                    "x_expr":{"type":"string","description":"plot: abscissa expression, normally a range variable such as `x`."},
                    "y_exprs":{"type":"array","items":{"type":"string"},"description":"plot: one or more ordinate expressions, e.g. [\"sin(x)\", \"cos(x)\"]."},
                    "after_id":{"type":"integer","description":"Insert directly below the region with this id."},
                    "x":{"type":"number","description":"Explicit x position (default 20)."},
                    "y":{"type":"number","description":"Explicit y position (overrides the automatic placement)."}
                },
                "required":["path","kind"]
            }
        },
        {
            "name": "update_region",
            "description": "Change an existing region of a .xmath or .xmcd worksheet (by id) and save. Pass `source` for math regions, `text` for text regions, `x_expr` / `y_exprs` for plots, and/or a new position `x` / `y`. Returns the re-evaluated worksheet.",
            "inputSchema": {
                "type":"object",
                "properties":{
                    "path":path.clone(),
                    "id":{"type":"integer","description":"Region id, as returned by open_worksheet / add_region."},
                    "source":{"type":"string"},
                    "text":{"type":"string"},
                    "x_expr":{"type":"string"},
                    "expr":{"type":"string","description":"3D plots: what to plot."},
                    "y_exprs":{"type":"array","items":{"type":"string"}},
                    "x":{"type":"number"},
                    "y":{"type":"number"}
                },
                "required":["path","id"]
            }
        },
        {
            "name": "delete_region",
            "description": "Delete a region (by id) from a .xmath or .xmcd worksheet, save, and return the re-evaluated worksheet. Other regions keep their positions and ids.",
            "inputSchema": {"type":"object","properties":{"path":path,"id":{"type":"integer"}},"required":["path","id"]}
        },
        {
            "name": "export_pdf",
            "description": "Render a worksheet (.xmath, or a Mathcad .xmcd) to a PDF file: formulas typeset in 2D with their results, text, plots and pictures, paginated by the page setup of the worksheet (A4 by default). Relative file names inside the worksheet (READPRN, READEXCEL, references) are resolved in the worksheet's folder; files are never written during the export. Only files with the .pdf extension may be written.",
            "inputSchema": {"type":"object","properties":{"path":{"type":"string","description":"Worksheet to render (.xmath or .xmcd)."},"out":{"type":"string","description":"Output PDF file (must end in .pdf)."}},"required":["path","out"]}
        },
        {
            "name": "export_xmcd",
            "description": "Save a worksheet as a Mathcad 15 .xmcd file (worksheet30 / math30 XML). X-Y plots are written as Mathcad plots; polar and 3D plots as pictures (xmath restores them on opening); problems are reported as warnings. Only files with the .xmcd extension may be written.",
            "inputSchema": {"type":"object","properties":{"path":{"type":"string","description":"Worksheet to convert (.xmath or .xmcd)."},"out":{"type":"string","description":"Output file (must end in .xmcd)."}},"required":["path","out"]}
        },
        {
            "name": "list_functions",
            "description": "List all built-in functions with one-line descriptions (trigonometry, statistics, matrices, integrate/diff/root, sum/prod, ...).",
            "inputSchema": {"type":"object","properties":{}}
        },
        {
            "name": "list_units",
            "description": "List all supported physical units grouped by dimension (length, force, pressure, ...), with the factor to SI base units.",
            "inputSchema": {"type":"object","properties":{}}
        },
        {
            "name": "syntax_help",
            "description": "Return the complete xmath syntax reference: statements, operators, units, functions, matrices, ranges, plots and common pitfalls. Read it before writing anything non-trivial.",
            "inputSchema": {"type":"object","properties":{}}
        }
    ])
}

pub fn call_tool(name: &str, args: &Value) -> Option<ToolResult> {
    let empty = Map::new();
    let a = args.as_object().unwrap_or(&empty);
    Some(match name {
        "evaluate" => evaluate(a),
        "open_worksheet" => open_worksheet(a),
        "create_worksheet" => create_worksheet(a),
        "add_region" => add_region(a),
        "update_region" => update_region(a),
        "delete_region" => delete_region(a),
        "export_pdf" => export_pdf(a),
        "export_xmcd" => export_xmcd(a),
        "list_functions" => list_functions(),
        "list_units" => list_units(),
        "syntax_help" => Ok(ToolOutput { text: help::SYNTAX_HELP.to_string(), data: Value::Null }),
        _ => return None,
    })
}

// ---------------------------------------------------------------- arguments

fn get_str<'a>(a: &'a Map<String, Value>, key: &str) -> Result<Option<&'a str>, String> {
    match a.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => Ok(Some(s)),
        Some(_) => Err(format!("`{key}` must be a string")),
    }
}

fn req_str<'a>(a: &'a Map<String, Value>, key: &str) -> Result<&'a str, String> {
    get_str(a, key)?.ok_or_else(|| format!("missing required argument `{key}`"))
}

fn get_f32(a: &Map<String, Value>, key: &str) -> Result<Option<f32>, String> {
    match a.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => match v.as_f64() {
            Some(f) if f.is_finite() => Ok(Some(f as f32)),
            _ => Err(format!("`{key}` must be a finite number")),
        },
    }
}

fn get_id(a: &Map<String, Value>, key: &str) -> Result<Option<u64>, String> {
    match a.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v
            .as_u64()
            .or_else(|| v.as_f64().filter(|f| *f >= 0.0 && f.fract() == 0.0).map(|f| f as u64))
            .map(Some)
            .ok_or_else(|| format!("`{key}` must be a non-negative integer")),
    }
}

fn get_str_list(a: &Map<String, Value>, key: &str) -> Result<Option<Vec<String>>, String> {
    match a.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Array(items)) => items
            .iter()
            .map(|v| v.as_str().map(str::to_string).ok_or_else(|| format!("`{key}` must be an array of strings")))
            .collect::<Result<Vec<_>, _>>()
            .map(Some),
        Some(_) => Err(format!("`{key}` must be an array of strings")),
    }
}

// -------------------------------------------------------------------- files

fn resolve(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")) {
            return Path::new(&home).join(rest);
        }
    }
    PathBuf::from(path)
}

fn check_write_path(path: &str) -> Result<PathBuf, String> {
    if path.trim().is_empty() {
        return Err("`path` is empty".into());
    }
    let p = resolve(path);
    let ok = p.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("xmath") || e.eq_ignore_ascii_case("xmcd"));
    if !ok {
        return Err(format!("refusing to write '{path}': only files with the .xmath or .xmcd extension may be written"));
    }
    Ok(p)
}

/// Like [`load`], and also reads Mathcad `.xmcd` files.
fn load_any(p: &Path) -> Result<Worksheet, String> {
    let src = std::fs::read(p).map_err(|e| format!("cannot read {}: {e}", p.display()))?;
    let src = String::from_utf8_lossy(&src);
    if p.extension().is_some_and(|e| e.eq_ignore_ascii_case("xmcd")) || src.trim_start().starts_with("<?xml") {
        xmath_core::xmcd::import_xmcd(&src).map(|r| r.worksheet).map_err(|e| format!("{}: {e}", p.display()))
    } else {
        load_xmath(p)
    }
}

fn check_out_path(path: &str, ext: &str) -> Result<PathBuf, String> {
    if path.trim().is_empty() {
        return Err("`out` is empty".into());
    }
    let p = resolve(path);
    if !p.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case(ext)) {
        return Err(format!("refusing to write '{path}': only files with the .{ext} extension may be written by this tool"));
    }
    if let Some(dir) = p.parent() {
        if !dir.as_os_str().is_empty() && !dir.exists() {
            return Err(format!("directory {} does not exist", dir.display()));
        }
    }
    Ok(p)
}

fn export_pdf(a: &Map<String, Value>) -> ToolResult {
    let src = resolve(req_str(a, "path")?);
    let out = check_out_path(req_str(a, "out")?, "pdf")?;
    let ws = load_any(&src)?;
    let dir = src.parent().filter(|d| !d.as_os_str().is_empty()).map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from("."));
    // reading is allowed (READPRN, references); nothing is written while evaluating
    let env = xmath_core::EvalEnv { dir: Some(dir), allow_write: false, stack: Vec::new() };
    let outputs = xmath_core::evaluate_worksheet_with(&ws, false, &env);
    let title = src.file_stem().map_or_else(|| "xmath".to_string(), |s| s.to_string_lossy().into_owned());
    let bytes = xmath_gui::print::export_pdf(&ws, &outputs, &xmath_gui::print::PdfOptions { title });
    let pages = String::from_utf8_lossy(&bytes).matches("/Type /Page").count() - String::from_utf8_lossy(&bytes).matches("/Type /Pages").count();
    std::fs::write(&out, &bytes).map_err(|e| format!("cannot write {}: {e}", out.display()))?;
    Ok(ToolOutput {
        text: format!("wrote {} ({pages} page(s), {} bytes)", out.display(), bytes.len()),
        data: json!({"out": out.display().to_string(), "pages": pages, "bytes": bytes.len()}),
    })
}

fn export_xmcd(a: &Map<String, Value>) -> ToolResult {
    let src = resolve(req_str(a, "path")?);
    let out = check_out_path(req_str(a, "out")?, "xmcd")?;
    let ws = load_any(&src)?;
    let r = xmath_core::xmcd_export::export_xmcd(&ws);
    std::fs::write(&out, &r.xml).map_err(|e| format!("cannot write {}: {e}", out.display()))?;
    let mut text = format!("wrote {} ({} regions)", out.display(), ws.regions.len());
    for w in &r.warnings {
        text.push_str(&format!("
warning: {w}"));
    }
    Ok(ToolOutput { text, data: json!({"out": out.display().to_string(), "warnings": r.warnings}) })
}

fn load(p: &Path) -> Result<Worksheet, String> {
    load_any(p)
}

/// The worksheet of an `.xmath` (JSON or one line per region) file.
fn load_xmath(p: &Path) -> Result<Worksheet, String> {
    let src = std::fs::read_to_string(p).map_err(|e| format!("cannot read {}: {e}", p.display()))?;
    if src.trim_start().starts_with('{') {
        Worksheet::from_json(&src).map_err(|e| format!("{}: invalid worksheet JSON: {e}", p.display()))
    } else {
        Ok(Worksheet::from_lines(&src))
    }
}

fn save(p: &Path, ws: &Worksheet) -> Result<(), String> {
    if let Some(dir) = p.parent() {
        if !dir.as_os_str().is_empty() && !dir.exists() {
            return Err(format!("directory {} does not exist", dir.display()));
        }
    }
    if p.extension().is_some_and(|e| e.eq_ignore_ascii_case("xmcd")) {
        // a Mathcad 15 file: plots as Mathcad plot items plus the pictures, results of the evaluation
        let dir = p.parent().filter(|d| !d.as_os_str().is_empty()).map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from("."));
        let env = xmath_core::EvalEnv { dir: Some(dir), allow_write: false, stack: Vec::new() };
        let outputs = ws.evaluate_with(true, &env);
        let has_plots = ws.regions.iter().any(|r| matches!(r.kind, RegionKind::Plot { .. } | RegionKind::Plot3d { .. }));
        let renderings = if has_plots { xmath_gui::print::render_plot_pngs(ws, &outputs) } else { Default::default() };
        let opts = xmath_core::xmcd_export::ExportOptions { renderings, rendering_scale: xmath_gui::print::PNG_SCALE, env: Some(env), outputs: Some(outputs) };
        let r = xmath_core::xmcd_export::export_xmcd_with(ws, &opts);
        return std::fs::write(p, r.xml).map_err(|e| format!("cannot write {}: {e}", p.display()));
    }
    std::fs::write(p, ws.to_json()).map_err(|e| format!("cannot write {}: {e}", p.display()))
}

// ------------------------------------------------------------ result output

struct Row {
    json: Value,
    text: String,
}

fn fmt_range(it: impl Iterator<Item = f64>) -> Value {
    let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
    for v in it.filter(|v| v.is_finite()) {
        lo = lo.min(v);
        hi = hi.max(v);
    }
    if lo <= hi {
        json!([lo, hi])
    } else {
        Value::Null
    }
}

/// Returns `(display, result)` for a math region.
fn math_text(source: &str, out: &RegionOutput) -> (String, Option<String>) {
    let shown = match &out.stmt {
        Some(Ok(s)) => s.to_string(),
        _ => source.to_string(),
    };
    match &out.output {
        Output::Value { view, unit } => {
            let lhs = shown.trim_end_matches(|c: char| c != '=');
            let res = match unit {
                Some(u) => format!("{view} {u}"),
                None => view.to_string(),
            };
            (format!("{lhs} {res}"), Some(res))
        }
        _ => (shown, None),
    }
}

fn row(r: &Region, out: Option<&RegionOutput>) -> Row {
    let mut j = json!({"id": r.id, "x": r.x, "y": r.y});
    let obj = j.as_object_mut().expect("object");
    let mut text;
    match &r.kind {
        RegionKind::Math { source } => {
            obj.insert("type".into(), "math".into());
            obj.insert("source".into(), source.clone().into());
            let (display, result) = match out {
                Some(o) => math_text(source, o),
                None => (source.clone(), None),
            };
            text = format!("[{}] {display}", r.id);
            if let Some(res) = result {
                obj.insert("result".into(), res.into());
            }
        }
        RegionKind::Text { text: t, .. } => {
            obj.insert("type".into(), "text".into());
            obj.insert("text".into(), t.clone().into());
            text = format!("[{}] # {t}", r.id);
        }
        RegionKind::Image { width, height, .. } => {
            obj.insert("type".into(), "image".into());
            obj.insert("width".into(), (*width).into());
            obj.insert("height".into(), (*height).into());
            text = format!("[{}] [image {}x{}]", r.id, width.round(), height.round());
        }
        RegionKind::Reference { href } => {
            obj.insert("type".into(), "reference".into());
            obj.insert("href".into(), href.clone().into());
            text = format!("[{}] [reference: {href}]", r.id);
        }
        RegionKind::Plot3d { kind, expr, .. } => {
            let tname = match kind {
                Plot3dKind::Surface => "surface_plot",
                Plot3dKind::Contour => "contour_plot",
                Plot3dKind::Scatter => "scatter_plot",
                Plot3dKind::Bar => "bar_plot",
            };
            obj.insert("type".into(), tname.into());
            obj.insert("expr".into(), expr.clone().into());
            text = format!("[{}] {tname} {expr}", r.id);
            if let Some(Output::Plot3d(d)) = out.map(|o| &o.output) {
                let mut summary = json!({"grid": [d.nx(), d.ny()], "points": d.pts.len(), "x_range": fmt_range(d.xs.iter().copied()), "y_range": fmt_range(d.ys.iter().copied())});
                summary["z_range"] = d.z_range().map_or(Value::Null, |(a, b)| json!([a, b]));
                if *kind == Plot3dKind::Scatter {
                    summary["x_range"] = fmt_range(d.pts.iter().map(|p| p[0]));
                    summary["y_range"] = fmt_range(d.pts.iter().map(|p| p[1]));
                }
                text.push_str(&format!("\n    grid {}x{}, {} points, z in {}", d.nx(), d.ny(), d.pts.len(), summary["z_range"]));
                if let Some(e) = &d.error {
                    text.push_str(&format!("\n    PLOT ERROR: {e}"));
                    summary["error"] = e.clone().into();
                }
                obj.insert("plot".into(), summary);
            }
        }
        RegionKind::Plot { x, y, kind, .. } => {
            let polar = *kind == PlotKind::Polar;
            obj.insert("type".into(), if polar { "polar_plot" } else { "plot" }.into());
            obj.insert("x_expr".into(), x.clone().into());
            obj.insert("y_exprs".into(), json!(y));
            let mut summary = json!({"series": [], "errors": []});
            if let Some(Output::Plot(p)) = out.map(|o| &o.output) {
                let series: Vec<Value> = p
                    .series
                    .iter()
                    .map(|s| {
                        json!({
                            "label": s.label,
                            "points": s.points.len(),
                            "x_range": fmt_range(s.points.iter().map(|p| p[0])),
                            "y_range": fmt_range(s.points.iter().map(|p| p[1])),
                        })
                    })
                    .collect();
                summary = json!({"series": series, "errors": p.errors});
            }
            text = format!("[{}] {} {} {} {}", r.id, if polar { "polar_plot" } else { "plot" }, y.join(", "), if polar { "over angle" } else { "vs" }, if x.is_empty() { "(auto)" } else { x });
            if let Some(arr) = summary["series"].as_array() {
                for s in arr {
                    text.push_str(&format!(
                        "\n    series {}: {} points, x in {}, y in {}",
                        s["label"].as_str().unwrap_or(""),
                        s["points"],
                        s["x_range"],
                        s["y_range"]
                    ));
                }
            }
            obj.insert("plot".into(), summary);
        }
    }
    if let Some(e) = out.and_then(|o| o.error()) {
        text.push_str(&format!("\n    ERROR: {e}"));
        obj.insert("error".into(), e.into());
    }
    if let Some(Output::Plot(p)) = out.map(|o| &o.output) {
        for e in &p.errors {
            text.push_str(&format!("\n    PLOT ERROR: {e}"));
        }
    }
    Row { json: j, text }
}

fn worksheet_report(ws: &Worksheet) -> (String, Value, usize) {
    let outputs: HashMap<u64, RegionOutput> = evaluate_worksheet(ws);
    let mut texts = Vec::new();
    let mut rows = Vec::new();
    let mut errors = 0;
    for i in ws.evaluation_order() {
        let r = &ws.regions[i];
        let rw = row(r, outputs.get(&r.id));
        if rw.json.get("error").is_some() {
            errors += 1;
        }
        texts.push(rw.text);
        rows.push(rw.json);
    }
    (texts.join("\n"), json!(rows), errors)
}

fn report(header: String, ws: &Worksheet, extra: Option<(&str, Value)>) -> ToolOutput {
    let (body, rows, errors) = worksheet_report(ws);
    let mut data = json!({"regions": rows, "errors": errors});
    if let Some((k, v)) = extra {
        data[k] = v;
    }
    let text = if body.is_empty() { format!("{header}\n(empty worksheet)") } else { format!("{header}\n{body}") };
    ToolOutput { text, data }
}

// -------------------------------------------------------------------- tools

fn split_lines(a: &Map<String, Value>) -> Result<Vec<String>, String> {
    let mut lines = Vec::new();
    if let Some(l) = get_str_list(a, "lines")? {
        for item in l {
            lines.extend(item.lines().map(str::to_string));
        }
    }
    if let Some(t) = get_str(a, "text")? {
        lines.extend(t.lines().map(str::to_string));
    }
    Ok(lines)
}

fn evaluate(a: &Map<String, Value>) -> ToolResult {
    if !a.contains_key("lines") && !a.contains_key("text") {
        return Err("provide `lines` (array of strings) or `text`".into());
    }
    let lines = split_lines(a)?;
    let nonblank: Vec<&String> = lines.iter().filter(|l| !l.trim().is_empty()).collect();
    let ws = Worksheet::from_lines(&lines.join("\n"));
    let outputs = evaluate_worksheet(&ws);
    // from_lines creates one region per non-blank line, in order.
    let mut text = Vec::new();
    let mut items = Vec::new();
    let mut errors = 0;
    for (line, r) in nonblank.iter().zip(&ws.regions) {
        let rw = row(r, outputs.get(&r.id));
        let mut item = json!({"line": line.trim()});
        if let Some(res) = rw.json.get("result") {
            item["result"] = res.clone();
        }
        if let Some(e) = rw.json.get("error") {
            item["error"] = e.clone();
            errors += 1;
        }
        let rendered = match (item.get("result"), item.get("error")) {
            (Some(res), _) => format!("{}  ->  {}", line.trim(), res.as_str().unwrap_or("")),
            (_, Some(e)) => format!("{}  ->  ERROR: {}", line.trim(), e.as_str().unwrap_or("")),
            _ => line.trim().to_string(),
        };
        text.push(rendered);
        items.push(item);
    }
    let mut out = text.join("\n");
    if errors > 0 {
        out.push_str(&format!("\n({errors} line(s) with errors)"));
    }
    Ok(ToolOutput { text: out, data: json!({"results": items, "errors": errors}) })
}

fn open_worksheet(a: &Map<String, Value>) -> ToolResult {
    let path = req_str(a, "path")?;
    let p = resolve(path);
    let ws = load(&p)?;
    Ok(report(format!("{} ({} regions)", p.display(), ws.regions.len()), &ws, None))
}

fn create_worksheet(a: &Map<String, Value>) -> ToolResult {
    let path = req_str(a, "path")?;
    let p = check_write_path(path)?;
    let overwrite = a.get("overwrite").and_then(Value::as_bool).unwrap_or(false);
    if p.exists() && !overwrite {
        return Err(format!("{} already exists; pass overwrite=true to replace it", p.display()));
    }
    let ws = Worksheet::from_lines(&split_lines(a)?.join("\n"));
    save(&p, &ws)?;
    Ok(report(format!("created {} ({} regions)", p.display(), ws.regions.len()), &ws, None))
}

fn region_height(r: &Region) -> f32 {
    match &r.kind {
        RegionKind::Math { .. } => MATH_H,
        RegionKind::Text { text, .. } => MATH_H + 14.0 * (text.lines().count().max(1) - 1) as f32,
        RegionKind::Plot { height, .. } | RegionKind::Plot3d { height, .. } | RegionKind::Image { height, .. } => height + 10.0,
        RegionKind::Reference { .. } => MATH_H,
    }
}

fn no_extra(a: &Map<String, Value>, kind: &str, forbidden: &[&str]) -> Result<(), String> {
    for k in forbidden {
        if a.get(*k).is_some_and(|v| !v.is_null()) {
            return Err(format!("`{k}` is not valid for a {kind} region"));
        }
    }
    Ok(())
}

fn single_line(s: &str, what: &str) -> Result<String, String> {
    let t = s.trim();
    if t.is_empty() {
        return Err(format!("`{what}` is empty"));
    }
    if t.contains('\n') || t.contains('\r') {
        return Err(format!("`{what}` must be a single line: one statement per region"));
    }
    Ok(t.to_string())
}

fn add_region(a: &Map<String, Value>) -> ToolResult {
    let path = req_str(a, "path")?;
    let p = check_write_path(path)?;
    let kind_name = req_str(a, "kind")?;
    let kind = match kind_name {
        "math" => {
            no_extra(a, "math", &["text", "x_expr", "y_exprs"])?;
            RegionKind::Math { source: single_line(req_str(a, "source")?, "source")? }
        }
        "text" => {
            no_extra(a, "text", &["source", "x_expr", "y_exprs"])?;
            let t = req_str(a, "text")?;
            if t.trim().is_empty() {
                return Err("`text` is empty".into());
            }
            RegionKind::new_text(t.trim())
        }
        "plot" => {
            no_extra(a, "plot", &["source", "text"])?;
            let y = get_str_list(a, "y_exprs")?.unwrap_or_default();
            if y.iter().all(|s| s.trim().is_empty()) {
                return Err("a plot needs `y_exprs` (at least one expression)".into());
            }
            let mut k = RegionKind::new_plot();
            if let RegionKind::Plot { x, y: ys, .. } = &mut k {
                *x = get_str(a, "x_expr")?.unwrap_or("").trim().to_string();
                *ys = y.iter().map(|s| s.trim().to_string()).collect();
            }
            k
        }
        "polar_plot" => {
            no_extra(a, "polar_plot", &["source", "text", "expr"])?;
            let y = get_str_list(a, "y_exprs")?.unwrap_or_default();
            if y.iter().all(|s| s.trim().is_empty()) {
                return Err("a polar plot needs `y_exprs` (radius expressions)".into());
            }
            let mut k = RegionKind::new_polar_plot();
            if let RegionKind::Plot { x, y: ys, .. } = &mut k {
                *x = get_str(a, "x_expr")?.unwrap_or("").trim().to_string();
                *ys = y.iter().map(|s| s.trim().to_string()).collect();
            }
            k
        }
        "surface_plot" | "contour_plot" | "scatter_plot" | "bar_plot" => {
            no_extra(a, kind_name, &["source", "text", "x_expr", "y_exprs"])?;
            let e = single_line(req_str(a, "expr")?, "expr")?;
            if e.trim().is_empty() {
                return Err("`expr` is empty".into());
            }
            let pk = match kind_name {
                "surface_plot" => Plot3dKind::Surface,
                "contour_plot" => Plot3dKind::Contour,
                "scatter_plot" => Plot3dKind::Scatter,
                _ => Plot3dKind::Bar,
            };
            let mut k = RegionKind::new_plot3d(pk);
            if let RegionKind::Plot3d { expr, .. } = &mut k {
                *expr = e.trim().to_string();
            }
            k
        }
        other => return Err(format!("unknown kind '{other}': use math, text, plot, polar_plot, surface_plot, contour_plot, scatter_plot or bar_plot")),
    };
    let mut ws = load(&p)?;
    let after = get_id(a, "after_id")?;
    let mut region = Region::new(ws.next_id(), get_f32(a, "x")?.unwrap_or(LEFT), 0.0, kind);
    let h = region_height(&region);
    let explicit_y = get_f32(a, "y")?;
    let auto_y = match after {
        Some(id) => {
            let r = ws.regions.iter().find(|r| r.id == id).ok_or_else(|| format!("no region with id {id}"))?;
            let ny = r.y + region_height(r) + GAP;
            if explicit_y.is_none() {
                for o in ws.regions.iter_mut().filter(|o| o.id != id && o.y >= ny - 0.01) {
                    o.y += h;
                }
            }
            ny
        }
        None => ws.regions.iter().map(|r| r.y + region_height(r)).fold(20.0, f32::max) + GAP,
    };
    region.y = explicit_y.unwrap_or(auto_y);
    let id = region.id;
    ws.regions.push(region);
    save(&p, &ws)?;
    Ok(report(format!("added region {id} to {}", p.display()), &ws, Some(("new_id", json!(id)))))
}

fn update_region(a: &Map<String, Value>) -> ToolResult {
    let path = req_str(a, "path")?;
    let p = check_write_path(path)?;
    let id = get_id(a, "id")?.ok_or("missing required argument `id`")?;
    let mut ws = load(&p)?;
    let r = ws.regions.iter_mut().find(|r| r.id == id).ok_or_else(|| format!("no region with id {id}"))?;
    match &mut r.kind {
        RegionKind::Math { source } => {
            no_extra(a, "math", &["text", "x_expr", "y_exprs"])?;
            if let Some(s) = get_str(a, "source")? {
                *source = single_line(s, "source")?;
            }
        }
        RegionKind::Text { text, .. } => {
            no_extra(a, "text", &["source", "x_expr", "y_exprs"])?;
            if let Some(t) = get_str(a, "text")? {
                if t.trim().is_empty() {
                    return Err("`text` is empty".into());
                }
                *text = t.trim().to_string();
            }
        }
        RegionKind::Image { .. } => return Err("image regions cannot be edited; delete or move them".into()),
        RegionKind::Plot3d { expr, .. } => {
            no_extra(a, "3d plot", &["source", "text", "x_expr", "y_exprs"])?;
            if let Some(s) = get_str(a, "expr")? {
                *expr = single_line(s, "expr")?.trim().to_string();
            }
        }
        RegionKind::Reference { .. } => return Err("reference regions cannot be edited; delete or move them".into()),
        RegionKind::Plot { x, y, .. } => {
            no_extra(a, "plot", &["source", "text", "expr"])?;
            if let Some(s) = get_str(a, "x_expr")? {
                *x = s.trim().to_string();
            }
            if let Some(l) = get_str_list(a, "y_exprs")? {
                if l.iter().all(|s| s.trim().is_empty()) {
                    return Err("`y_exprs` needs at least one expression".into());
                }
                *y = l.iter().map(|s| s.trim().to_string()).collect();
            }
        }
    }
    if let Some(v) = get_f32(a, "x")? {
        r.x = v;
    }
    if let Some(v) = get_f32(a, "y")? {
        r.y = v;
    }
    save(&p, &ws)?;
    Ok(report(format!("updated region {id} in {}", p.display()), &ws, None))
}

fn delete_region(a: &Map<String, Value>) -> ToolResult {
    let path = req_str(a, "path")?;
    let p = check_write_path(path)?;
    let id = get_id(a, "id")?.ok_or("missing required argument `id`")?;
    let mut ws = load(&p)?;
    let before = ws.regions.len();
    ws.regions.retain(|r| r.id != id);
    if ws.regions.len() == before {
        return Err(format!("no region with id {id}"));
    }
    save(&p, &ws)?;
    Ok(report(format!("deleted region {id} from {}", p.display()), &ws, None))
}

fn list_functions() -> ToolResult {
    let mut text = String::new();
    let mut items = Vec::new();
    for name in xmath_core::builtins::NAMES {
        let doc = help::FUNCTION_DOCS.iter().find(|(n, _)| n == name).map_or("", |(_, d)| *d);
        text.push_str(&format!("{doc}\n"));
        items.push(json!({"name": name, "description": doc}));
    }
    Ok(ToolOutput { text, data: json!({"functions": items}) })
}

fn list_units() -> ToolResult {
    let mut text = String::new();
    let mut groups = Vec::new();
    for (title, units) in help::units_by_dimension() {
        let names: Vec<&str> = units.iter().map(|u| u.0).collect();
        text.push_str(&format!("{title}: {}\n", names.join(" ")));
        groups.push(json!({
            "dimension": title,
            "units": units.iter().map(|(n, f)| json!({"name": n, "si_factor": f})).collect::<Vec<_>>(),
        }));
    }
    text.push_str("\nUse `expr = unit` to display a result in a unit; prefixed forms not listed here are not supported.\n");
    Ok(ToolOutput { text, data: json!({"groups": groups}) })
}
