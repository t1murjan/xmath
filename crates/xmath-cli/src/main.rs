//! `xmath-cli file.xmath` (JSON worksheet) or `xmath-cli file.txt`
//! (one math region per line) prints every region with its result.

use std::process::ExitCode;
use xmath_core::{evaluate_worksheet, Output, RegionKind, Worksheet};

fn main() -> ExitCode {
    let Some(path) = std::env::args().nth(1) else {
        eprintln!("usage: xmath-cli <worksheet.xmath | lines.txt>");
        return ExitCode::from(2);
    };
    let src = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("cannot read {path}: {e}");
            return ExitCode::from(2);
        }
    };
    let ws = if src.trim_start().starts_with('{') {
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
