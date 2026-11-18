//! File input/output functions like in Mathcad 15: `READPRN`, `WRITEPRN`, `APPENDPRN`,
//! `READFILE`, `READEXCEL` (plus the extras `READCSV` / `WRITECSV`).
//!
//! Paths are relative to the directory of the worksheet ([`EvalEnv::dir`]).
//! Reading is always allowed. Writing happens only from an explicit definition such as
//! `WRITEPRN("out.prn") := M`; a relative path must stay inside the worksheet directory
//! (no `..`), an absolute path written by the user is accepted as is. Without a worksheet
//! directory (a worksheet that was never saved, the `evaluate` tool of the MCP server)
//! nothing can be written.

use crate::value::{EResult, Matrix, Num, Value};
use std::path::{Component, Path, PathBuf};

/// Where and how an evaluation may touch the file system.
#[derive(Clone, Debug, Default)]
pub struct EvalEnv {
    /// Directory of the worksheet (base of relative paths).
    pub dir: Option<PathBuf>,
    /// Files may be written (`WRITEPRN(...) := M`). Needs [`EvalEnv::dir`] for relative paths.
    pub allow_write: bool,
    /// Worksheets being evaluated through references (cycle protection).
    pub stack: Vec<PathBuf>,
}

impl EvalEnv {
    /// Environment of a worksheet stored in `dir`: files can be read and written there.
    pub fn in_dir(dir: impl Into<PathBuf>) -> EvalEnv {
        EvalEnv { dir: Some(dir.into()), allow_write: true, stack: Vec::new() }
    }
}

/// Names of the functions that write files (used as `NAME("file") := value`).
pub fn is_write_name(name: &str) -> bool {
    matches!(name, "WRITEPRN" | "APPENDPRN" | "WRITECSV")
}

/// Resolves `p` for reading.
pub fn resolve_read(env: &EvalEnv, p: &str) -> PathBuf {
    let path = Path::new(p);
    if path.is_absolute() {
        return path.to_path_buf();
    }
    match &env.dir {
        Some(d) => d.join(path),
        None => path.to_path_buf(),
    }
}

/// Resolves `p` for writing (see the module documentation for the rules).
pub fn resolve_write(env: &EvalEnv, p: &str) -> EResult<PathBuf> {
    if !env.allow_write {
        return Err("запись файлов отключена".into());
    }
    let path = Path::new(p);
    if path.is_absolute() {
        return Ok(path.to_path_buf());
    }
    let Some(dir) = &env.dir else {
        return Err("запись файла невозможна: рабочая тетрадь не сохранена в файл (сохраните её или укажите абсолютный путь)".into());
    };
    if path.components().any(|c| matches!(c, Component::ParentDir | Component::Prefix(_) | Component::RootDir)) {
        return Err(format!("запись вне папки рабочей тетради запрещена: {p} (используйте абсолютный путь)"));
    }
    Ok(dir.join(path))
}

fn path_arg(v: &Value, what: &str) -> EResult<String> {
    match v {
        Value::Str(s) => Ok(s.to_string()),
        other => Err(format!("{what}: ожидалась строка с именем файла, найдено: {}", other.type_name())),
    }
}

/// The path argument of a write target `NAME(arg) := ...`: a string literal (stored with its
/// quotes in the parameter list) or the name of a variable holding a string.
pub fn target_path(param: &str, lookup: &dyn Fn(&str) -> Option<Value>) -> EResult<String> {
    if let Some(inner) = param.strip_prefix('"') {
        return Ok(inner.strip_suffix('"').unwrap_or(inner).to_string());
    }
    match lookup(param) {
        Some(v) => path_arg(&v, "имя файла"),
        None => Err(format!("'{param}' не определена")),
    }
}

/// Calls a read function. `None` if `name` is not a file function.
pub fn call(env: &EvalEnv, name: &str, args: &[Value]) -> Option<EResult<Value>> {
    let r = match name {
        "READPRN" => (|| {
            check_args(name, args, 1, 1)?;
            read_prn(&resolve_read(env, &path_arg(&args[0], name)?))
        })(),
        "READCSV" => (|| {
            check_args(name, args, 1, 1)?;
            read_delimited(&resolve_read(env, &path_arg(&args[0], name)?))
        })(),
        "READFILE" => (|| {
            check_args(name, args, 1, 2)?;
            let p = resolve_read(env, &path_arg(&args[0], name)?);
            let kind = match args.get(1) {
                Some(k) => path_arg(k, name)?.to_lowercase(),
                None => "delimited".to_string(),
            };
            match kind.as_str() {
                "delimited" | "csv" => read_delimited(&p),
                "text" | "string" => std::fs::read_to_string(&p)
                    .map(|s| Value::Str(s.into()))
                    .map_err(|e| format!("не удалось прочитать {}: {e}", p.display())),
                other => Err(format!("{name}: неизвестный тип «{other}» (поддерживаются \"delimited\" и \"text\")")),
            }
        })(),
        "READEXCEL" => (|| {
            check_args(name, args, 1, 2)?;
            let p = resolve_read(env, &path_arg(&args[0], name)?);
            let range = match args.get(1) {
                Some(r) => Some(path_arg(r, name)?),
                None => None,
            };
            read_excel(&p, range.as_deref())
        })(),
        _ => return None,
    };
    Some(r.map_err(|e| if e.starts_with(name) { e } else { format!("{name}: {e}") }))
}

fn check_args(name: &str, args: &[Value], min: usize, max: usize) -> EResult<()> {
    if args.len() < min || args.len() > max {
        return Err(format!("{name} принимает {} аргумент(а)", if min == max { min.to_string() } else { format!("{min}–{max}") }));
    }
    Ok(())
}

// ---------------------------------------------------------------- reading

fn parse_number(s: &str) -> Option<f64> {
    let t = s.trim().trim_matches('"').trim();
    if t.is_empty() {
        return None;
    }
    // decimal comma ("1,5") is accepted when the cell has no other punctuation
    let t = t.replace(',', ".");
    t.parse::<f64>().ok().filter(|x| x.is_finite())
}

fn matrix_from_rows(rows: Vec<Vec<f64>>) -> EResult<Value> {
    if rows.is_empty() {
        return Err("в файле нет числовых данных".into());
    }
    let cols = rows.iter().map(Vec::len).max().unwrap_or(0);
    if rows.iter().any(|r| r.len() != cols) {
        return Err("строки файла содержат разное число значений".into());
    }
    let data = rows.into_iter().flatten().map(Num::real).collect::<Vec<_>>();
    let n = data.len() / cols.max(1);
    Ok(Value::Matrix(Matrix::new(n, cols, data)))
}

fn read_text(p: &Path) -> EResult<String> {
    let bytes = std::fs::read(p).map_err(|e| format!("не удалось прочитать {}: {e}", p.display()))?;
    let s = String::from_utf8_lossy(&bytes).into_owned();
    Ok(s.trim_start_matches('\u{feff}').to_string())
}

/// `READPRN`: whitespace (or comma, semicolon) separated numbers, one matrix row per line.
/// Lines without any number (headers, blank lines) are skipped.
pub fn read_prn(p: &Path) -> EResult<Value> {
    let text = read_text(p)?;
    let mut rows: Vec<Vec<f64>> = Vec::new();
    for line in text.lines() {
        let toks: Vec<&str> = line.split(|c: char| c.is_whitespace() || c == ';').filter(|t| !t.is_empty()).collect();
        let nums: Vec<Option<f64>> = toks.iter().map(|t| parse_number(t)).collect();
        if nums.is_empty() || nums.iter().all(Option::is_none) {
            continue;
        }
        let mut row = Vec::with_capacity(nums.len());
        for (n, t) in nums.iter().zip(&toks) {
            row.push(n.ok_or_else(|| format!("не число: «{t}»"))?);
        }
        rows.push(row);
    }
    matrix_from_rows(rows)
}

fn detect_delimiter(text: &str) -> char {
    let sample: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).take(20).collect();
    for d in ['\t', ';', ','] {
        let counts: Vec<usize> = sample.iter().map(|l| l.matches(d).count()).collect();
        if !counts.is_empty() && counts[0] > 0 && counts.iter().all(|c| *c == counts[0]) {
            return d;
        }
    }
    for d in ['\t', ';', ','] {
        if sample.iter().any(|l| l.contains(d)) {
            return d;
        }
    }
    ' '
}

fn split_fields(line: &str, d: char) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    for ch in line.chars() {
        match ch {
            '"' => quoted = !quoted,
            c if c == d && !quoted => out.push(std::mem::take(&mut cur)),
            c => cur.push(c),
        }
    }
    out.push(cur);
    if d == ' ' {
        out.retain(|f| !f.is_empty());
    }
    out
}

/// `READFILE(file, "delimited")` / `READCSV`: delimiter-separated values (tab, semicolon,
/// comma or blanks, detected). Leading rows without any number are treated as a header and
/// skipped; empty cells are zero.
pub fn read_delimited(p: &Path) -> EResult<Value> {
    let text = read_text(p)?;
    let d = detect_delimiter(&text);
    let mut cells: Vec<Vec<String>> = Vec::new();
    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        cells.push(split_fields(line, d));
    }
    // a decimal comma only makes sense when the delimiter is not a comma
    let to_num = |s: &str| if d == ',' { s.trim().trim_matches('"').trim().parse::<f64>().ok().filter(|x| x.is_finite()) } else { parse_number(s) };
    let first_data = cells.iter().position(|r| r.iter().any(|c| to_num(c).is_some())).unwrap_or(cells.len());
    let mut rows = Vec::new();
    for (i, r) in cells.iter().enumerate().skip(first_data) {
        let mut row = Vec::with_capacity(r.len());
        for (j, c) in r.iter().enumerate() {
            if c.trim().is_empty() {
                row.push(0.0);
            } else {
                row.push(to_num(c).ok_or_else(|| format!("строка {}, столбец {}: не число «{}»", i + 1, j + 1, c.trim()))?);
            }
        }
        rows.push(row);
    }
    matrix_from_rows(rows)
}

/// `A`, `B`, ..., `AA` to a zero based column index.
fn col_index(s: &str) -> Option<u32> {
    if s.is_empty() {
        return None;
    }
    let mut n: u32 = 0;
    for c in s.chars() {
        if !c.is_ascii_alphabetic() {
            return None;
        }
        n = n.checked_mul(26)?.checked_add(c.to_ascii_uppercase() as u32 - 'A' as u32 + 1)?;
    }
    Some(n - 1)
}

/// `B3` to zero based (row, column).
fn parse_cell(s: &str) -> Option<(u32, u32)> {
    let s = s.trim().replace('$', "");
    let split = s.find(|c: char| c.is_ascii_digit())?;
    let col = col_index(&s[..split])?;
    let row: u32 = s[split..].parse().ok()?;
    (row >= 1).then(|| (row - 1, col))
}

/// `READEXCEL(file[, range])`: `range` is `"Sheet1!A1:C10"`, `"A1:C10"` (first sheet),
/// `"Sheet1"` (the used part of the sheet) or omitted (first sheet). Supports xlsx, xls, ods.
/// Empty cells are zero; leading text-only rows of an automatically determined range are
/// skipped as a header; any other text is an error.
pub fn read_excel(p: &Path, range: Option<&str>) -> EResult<Value> {
    use calamine::{open_workbook_auto, Data, Reader};
    let mut wb = open_workbook_auto(p).map_err(|e| format!("не удалось открыть {}: {e}", p.display()))?;
    let names = wb.sheet_names();
    let (sheet, cells) = match range.map(str::trim).filter(|r| !r.is_empty()) {
        None => (names.first().cloned().ok_or("в книге нет листов")?, None),
        Some(r) => match r.rsplit_once('!') {
            Some((s, c)) => (s.trim().trim_matches('\'').to_string(), Some(c.to_string())),
            None if parse_cell(r.split(':').next().unwrap_or("")).is_some() && !names.iter().any(|n| n == r) => {
                (names.first().cloned().ok_or("в книге нет листов")?, Some(r.to_string()))
            }
            None => (r.to_string(), None),
        },
    };
    if !names.iter().any(|n| *n == sheet) {
        return Err(format!("нет листа «{sheet}» (есть: {})", names.join(", ")));
    }
    let sheet_range = wb.worksheet_range(&sheet).map_err(|e| format!("лист «{sheet}»: {e}"))?;
    let (r0, c0, r1, c1, explicit) = match &cells {
        Some(c) => {
            let (a, b) = c.split_once(':').unwrap_or((c.as_str(), c.as_str()));
            let a = parse_cell(a).ok_or_else(|| format!("недопустимый диапазон «{c}»"))?;
            let b = parse_cell(b).ok_or_else(|| format!("недопустимый диапазон «{c}»"))?;
            (a.0.min(b.0), a.1.min(b.1), a.0.max(b.0), a.1.max(b.1), true)
        }
        None => match (sheet_range.start(), sheet_range.end()) {
            (Some(s), Some(e)) => (s.0, s.1, e.0, e.1, false),
            _ => return Err(format!("лист «{sheet}» пуст")),
        },
    };
    if (r1 - r0 + 1) as usize * (c1 - c0 + 1) as usize > crate::value::MAX_RANGE_LEN {
        return Err("диапазон слишком велик".into());
    }
    let mut grid: Vec<Vec<Option<f64>>> = Vec::new(); // None: text
    let mut texts: Vec<Vec<String>> = Vec::new();
    for r in r0..=r1 {
        let mut row = Vec::new();
        let mut trow = Vec::new();
        for c in c0..=c1 {
            let (v, t) = match sheet_range.get_value((r, c)) {
                None | Some(Data::Empty) => (Some(0.0), String::new()),
                Some(Data::Int(i)) => (Some(*i as f64), String::new()),
                Some(Data::Float(f)) => (Some(*f), String::new()),
                Some(Data::Bool(b)) => (Some(if *b { 1.0 } else { 0.0 }), String::new()),
                Some(Data::DateTime(d)) => (Some(d.as_f64()), String::new()),
                Some(Data::String(s)) => match parse_number(s) {
                    Some(x) if !s.trim().is_empty() => (Some(x), String::new()),
                    _ if s.trim().is_empty() => (Some(0.0), String::new()),
                    _ => (None, s.clone()),
                },
                Some(other) => (None, format!("{other:?}")),
            };
            row.push(v);
            trow.push(t);
        }
        grid.push(row);
        texts.push(trow);
    }
    let mut start = 0;
    if !explicit {
        // header rows: every cell is text
        while start < grid.len() && grid[start].iter().all(Option::is_none) {
            start += 1;
        }
    }
    let mut rows = Vec::new();
    for (i, row) in grid.iter().enumerate().skip(start) {
        let mut out = Vec::new();
        for (j, v) in row.iter().enumerate() {
            match v {
                Some(x) => out.push(*x),
                None => {
                    return Err(format!(
                        "ячейка {}{} содержит текст «{}»",
                        col_name(c0 + j as u32),
                        r0 + i as u32 + 1,
                        texts[i][j]
                    ))
                }
            }
        }
        rows.push(out);
    }
    matrix_from_rows(rows)
}

fn col_name(mut c: u32) -> String {
    let mut s = String::new();
    loop {
        s.insert(0, (b'A' + (c % 26) as u8) as char);
        if c < 26 {
            break;
        }
        c = c / 26 - 1;
    }
    s
}

// ---------------------------------------------------------------- writing

fn real_grid(v: &Value) -> EResult<(usize, usize, Vec<f64>)> {
    let m = match v {
        Value::Matrix(m) => m.clone(),
        Value::Num(n) => Matrix::new(1, 1, vec![*n]),
        Value::Range(r) => Matrix::column(r.values().collect()),
        other => return Err(format!("записать можно число или матрицу, найдено: {}", other.type_name())),
    };
    let mut out = Vec::with_capacity(m.data.len());
    for n in &m.data {
        if !n.is_real() {
            return Err("комплексные числа записать нельзя".into());
        }
        out.push(n.v.re);
    }
    Ok((m.rows, m.cols, out))
}

fn fmt_f(x: f64) -> String {
    if x == x.trunc() && x.abs() < 1e15 {
        format!("{}", x as i64)
    } else {
        format!("{x}")
    }
}

/// Writes (or appends) `v` to `path` in a text format: values separated by `sep`, one matrix
/// row per line. Values are written in SI base units, without units.
pub fn write_matrix(path: &Path, v: &Value, sep: &str, append: bool) -> EResult<()> {
    use std::io::Write;
    let (rows, cols, data) = real_grid(v)?;
    let mut text = String::new();
    for i in 0..rows {
        let line: Vec<String> = (0..cols).map(|j| fmt_f(data[i * cols + j])).collect();
        text.push_str(&line.join(sep));
        text.push('\n');
    }
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .append(append)
        .truncate(!append)
        .open(path)
        .map_err(|e| format!("не удалось открыть {} для записи: {e}", path.display()))?;
    f.write_all(text.as_bytes()).map_err(|e| format!("ошибка записи {}: {e}", path.display()))
}

/// Performs `NAME(path) := v`.
pub fn write(env: &EvalEnv, name: &str, rel: &str, v: &Value) -> EResult<()> {
    let p = resolve_write(env, rel).map_err(|e| format!("{name}: {e}"))?;
    let (sep, append) = match name {
        "WRITEPRN" => ("  ", false),
        "APPENDPRN" => ("  ", true),
        _ => (",", false),
    };
    write_matrix(&p, v, sep, append).map_err(|e| format!("{name}: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{parse_statement, Stmt};

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("xmath-fileio-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn run(dir: &Path, lines: &[&str]) -> Vec<String> {
        let ws = crate::Worksheet::from_lines(&lines.join("\n"));
        let env = EvalEnv::in_dir(dir);
        let out = ws.evaluate_with(false, &env);
        ws.evaluation_order()
            .into_iter()
            .map(|i| match &out[&ws.regions[i].id].output {
                crate::Output::Value { view, .. } => view.to_string(),
                crate::Output::Error(e) => format!("ERR {e}"),
                _ => String::new(),
            })
            .collect()
    }

    #[test]
    fn parse_write_target() {
        let s = parse_statement("WRITEPRN(\"out.prn\") := M").unwrap();
        assert!(matches!(s, Stmt::Define { .. }));
        assert_eq!(s.to_string(), "WRITEPRN(\"out.prn\") := M");
    }

    #[test]
    fn prn_roundtrip() {
        let d = tmp("prn");
        let r = run(&d, &["M := [1, 2; 3, 4.5]", "WRITEPRN(\"out.prn\") := M", "N := READPRN(\"out.prn\")", "N ="]);
        assert!(r.last().unwrap().contains("4.5"), "{r:?}");
        let txt = std::fs::read_to_string(d.join("out.prn")).unwrap();
        assert_eq!(txt, "1  2\n3  4.5\n");
        let r = run(&d, &["APPENDPRN(\"out.prn\") := [5, 6]", "rows(READPRN(\"out.prn\")) ="]);
        assert_eq!(r.last().unwrap(), "3", "{r:?}");
    }

    #[test]
    fn csv_and_readfile() {
        let d = tmp("csv");
        std::fs::write(d.join("a.csv"), "x;y\n1;2,5\n3;4\n").unwrap();
        let r = run(&d, &["READFILE(\"a.csv\", \"delimited\")[1, 1] =", "rows(READCSV(\"a.csv\")) ="]);
        assert_eq!(r[0], "4", "{r:?}");
        assert_eq!(r[1], "2");
        run(&d, &["WRITECSV(\"b.csv\") := [1, 2; 3, 4]"]);
        assert_eq!(std::fs::read_to_string(d.join("b.csv")).unwrap(), "1,2\n3,4\n");
    }

    #[test]
    fn write_is_confined() {
        let d = tmp("conf");
        let r = run(&d, &["WRITEPRN(\"../evil.prn\") := [1]"]);
        assert!(r[0].starts_with("ERR"), "{r:?}");
        assert!(!d.parent().unwrap().join("evil.prn").exists());
        // no directory: no relative writes
        let ws = crate::Worksheet::from_lines("WRITEPRN(\"x.prn\") := [1]");
        let out = ws.evaluate_with(false, &EvalEnv::default());
        assert!(matches!(out[&1].output, crate::Output::Error(_)));
        // absolute path written by the user is accepted
        let abs = d.join("abs.prn");
        run(&d, &[&format!("WRITEPRN(\"{}\") := [7]", abs.display())]);
        assert!(abs.exists());
    }

    #[test]
    fn excel() {
        let d = tmp("xlsx");
        let p = d.join("t.xlsx");
        write_test_xlsx(&p);
        let r = run(&d, &["A := READEXCEL(\"t.xlsx\", \"Data!A2:B3\")", "A[1, 1] =", "rows(READEXCEL(\"t.xlsx\")) =", "READEXCEL(\"t.xlsx\", \"B3:B4\")[1, 0] ="]);
        assert_eq!(r[1], "4", "{r:?}");
        assert_eq!(r[2], "3"); // header row skipped
        assert_eq!(r[3], "6");
    }

    /// Writes a minimal xlsx (sheet "Data": header row, then 1 2 / 3 4 / 5 6 in A..B).
    fn write_test_xlsx(p: &Path) {
        use std::io::Write;
        let mut z = zip::ZipWriter::new(std::fs::File::create(p).unwrap());
        let o = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        let mut add = |name: &str, body: &str| {
            z.start_file(name, o).unwrap();
            z.write_all(body.as_bytes()).unwrap();
        };
        add("[Content_Types].xml", r#"<?xml version="1.0" encoding="UTF-8"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/></Types>"#);
        add("_rels/.rels", r#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#);
        add("xl/workbook.xml", r#"<?xml version="1.0" encoding="UTF-8"?><workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="Data" sheetId="1" r:id="rId1"/></sheets></workbook>"#);
        add("xl/_rels/workbook.xml.rels", r#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/></Relationships>"#);
        add("xl/worksheets/sheet1.xml", r#"<?xml version="1.0" encoding="UTF-8"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData><row r="1"><c r="A1" t="inlineStr"><is><t>x</t></is></c><c r="B1" t="inlineStr"><is><t>y</t></is></c></row><row r="2"><c r="A2"><v>1</v></c><c r="B2"><v>2</v></c></row><row r="3"><c r="A3"><v>3</v></c><c r="B3"><v>4</v></c></row><row r="4"><c r="A4"><v>5</v></c><c r="B4"><v>6</v></c></row></sheetData></worksheet>"#);
        z.finish().unwrap();
    }
}
