//! `xmath-cli file.xmath` (JSON worksheet), `file.xmcd` (Mathcad 15) or
//! `file.txt` (one math region per line) prints every region with its result.
//! `--convert out.xmath` additionally saves the loaded worksheet as JSON.

use std::process::ExitCode;
use xmath_core::{evaluate_worksheet, Output, RegionKind, Worksheet};

fn main() -> ExitCode {
    let mut path: Option<String> = None;
    let mut convert: Option<String> = None;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        if a == "--convert" {
            convert = args.next();
            if convert.is_none() {
                eprintln!("--convert needs an output file");
                return ExitCode::from(2);
            }
        } else if path.is_none() {
            path = Some(a);
        }
    }
    let Some(path) = path else {
        eprintln!("usage: xmath-cli <worksheet.xmath | worksheet.xmcd | lines.txt> [--convert out.xmath]");
        return ExitCode::from(2);
    };
    let bytes = match std::fs::read(&path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("cannot read {path}: {e}");
            return ExitCode::from(2);
        }
    };
    let src = String::from_utf8_lossy(&bytes).into_owned();
    let head = src.trim_start_matches('\u{feff}').trim_start();
    let is_xmcd = path.to_lowercase().ends_with(".xmcd") || (head.starts_with("<?xml") && head.contains("worksheet"));
    let ws = if is_xmcd {
        match xmath_core::xmcd::import_xmcd(&src) {
            Ok(r) => {
                for w in &r.warnings {
                    eprintln!("warning: {w}");
                }
                r.worksheet
            }
            Err(e) => {
                eprintln!("{path}: {e}");
                return ExitCode::from(2);
            }
        }
    } else if head.starts_with('{') {
        match Worksheet::from_json(&src) {
            Ok(ws) => ws,
            Err(e) => {
                eprintln!("{path}: {e}");
                return ExitCode::from(2);
            }
        }
    } else {
        Worksheet::from_lines(&src)
    };
    if let Some(out) = &convert {
        if let Err(e) = std::fs::write(out, ws.to_json()) {
            eprintln!("cannot write {out}: {e}");
            return ExitCode::from(2);
        }
    }
    let outputs = evaluate_worksheet(&ws);
    let mut errors = 0;
    for i in ws.evaluation_order() {
        let r = &ws.regions[i];
        let out = &outputs[&r.id];
        match &r.kind {
            RegionKind::Text { text, .. } => println!("# {text}"),
            RegionKind::Math { source } => {
                let shown = match &out.stmt {
                    Some(Ok(s)) => s.to_string(),
                    _ => source.clone(),
                };
                match &out.output {
                    Output::Value { view, unit } => {
                        let lhs = shown.trim_end_matches(|c: char| c != '=');
                        match unit {
                            Some(u) => println!("{lhs} {view} {u}"),
                            None => println!("{lhs} {view}"),
                        }
                    }
                    _ => println!("{shown}"),
                }
            }
            RegionKind::Plot { x, y, .. } => {
                let n = match &out.output {
                    Output::Plot(p) => p.series.len(),
                    _ => 0,
                };
                println!("[plot {} vs {x}: {n} series]", y.join(", "));
            }
        }
        if let Some(e) = out.error() {
            errors += 1;
            println!("    ^ error: {e}");
        }
    }
    if errors > 0 {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}
