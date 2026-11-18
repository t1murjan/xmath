//! `xmath-cli file.xmath` (JSON worksheet), `file.xmcd` (Mathcad 15) or
//! `file.txt` (one math region per line) prints every region with its result.
//! `--convert out.xmath` additionally saves the loaded worksheet as JSON.
//! `--check a.xmcd [b.xmcd ...] [-v]` compares the results with the ones Mathcad
//! saved in the files and prints a summary (a per-region list with `-v`, or for a
//! single file).

use std::process::ExitCode;
use xmath_core::{evaluate_worksheet, Output, RegionKind, Worksheet};

fn check_main(files: Vec<String>, verbose: bool, no_anchors: bool) -> ExitCode {
    use xmath_core::xmcd_check::{check_import, top_causes, CheckReport, Verdict};
    let single = files.len() == 1;
    let mut reports: Vec<CheckReport> = Vec::new();
    let mut bad_files = 0;
    println!("{:<28} {:>5} {:>9} {:>14} {:>8}", "file", "OK", "MISMATCH", "NOT EVALUATED", "SKIPPED");
    for f in &files {
        let name = std::path::Path::new(f).file_name().map_or(f.clone(), |n| n.to_string_lossy().into_owned());
        let imported = std::fs::read(f).map_err(|e| e.to_string()).and_then(|b| xmath_core::xmcd::import_xmcd(&String::from_utf8_lossy(&b)));
        let r = match imported {
            Ok(mut r) => {
                if no_anchors {
                    // evaluate in top-left order (the behaviour before baseline ordering)
                    r.worksheet.regions.iter_mut().for_each(|g| g.anchor = [0.0, 0.0]);
                }
                r
            }
            Err(e) => {
                bad_files += 1;
                println!("{name:<28} not a worksheet: {}", e.chars().take(60).collect::<String>());
                continue;
            }
        };
        let rep = check_import(&r);
        println!("{name:<28} {:>5} {:>9} {:>14} {:>8}", rep.ok(), rep.mismatch(), rep.not_evaluated(), rep.skipped());
        if verbose || single {
            for c in &rep.regions {
                let detail = match &c.verdict {
                    Verdict::Ok => String::new(),
                    Verdict::Mismatch(s) | Verdict::NotEvaluated(s) | Verdict::Skipped(s) => format!("  -- {s}"),
                };
                let src: String = c.source.chars().take(70).collect();
                println!("  {:<14} #{:<6} {src}{detail}", c.verdict.label(), c.id);
            }
        }
        reports.push(rep);
    }
    let refs: Vec<&CheckReport> = reports.iter().collect();
    let sum = |f: &dyn Fn(&CheckReport) -> usize| reports.iter().map(f).sum::<usize>();
    let (ok, mm, ne, sk) = (sum(&|r| r.ok()), sum(&|r| r.mismatch()), sum(&|r| r.not_evaluated()), sum(&|r| r.skipped()));
    println!("{:<28} {ok:>5} {mm:>9} {ne:>14} {sk:>8}", "TOTAL");
    println!("{} files checked, {bad_files} not worksheets; {} results compared", reports.len(), ok + mm + ne + sk);
    let causes = top_causes(&refs);
    if !causes.is_empty() {
        println!("top causes:");
        for (c, n) in causes.iter().take(15) {
            println!("  {n:>5}  {c}");
        }
    }
    if mm + ne > 0 {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

fn main() -> ExitCode {
    let mut path: Option<String> = None;
    let mut convert: Option<String> = None;
    let mut args = std::env::args().skip(1);
    if std::env::args().nth(1).as_deref() == Some("--check") {
        let rest: Vec<String> = std::env::args().skip(2).collect();
        let verbose = rest.iter().any(|a| a == "-v" || a == "--verbose");
        let no_anchors = rest.iter().any(|a| a == "--no-anchors");
        let files: Vec<String> = rest.into_iter().filter(|a| !a.starts_with('-')).collect();
        if files.is_empty() {
            eprintln!("usage: xmath-cli --check <file.xmcd>... [-v]");
            return ExitCode::from(2);
        }
        return check_main(files, verbose, no_anchors);
    }
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
        eprintln!("usage: xmath-cli <worksheet.xmath | worksheet.xmcd | lines.txt> [--convert out.xmath] | --check <file.xmcd>... [-v]");
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
                    Output::Symbolic { result, note } => {
                        let sep = if shown.ends_with('→') { "" } else { " →" };
                        println!("{shown}{sep} {result}");
                        if let Some(n) = note {
                            println!("    note: {n}");
                        }
                    }
                    _ => println!("{shown}"),
                }
            }
            RegionKind::Image { width, height, .. } => println!("[изображение {}×{}]", width.round(), height.round()),
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
