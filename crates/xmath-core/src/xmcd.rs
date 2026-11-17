//! Import of Mathcad 15 worksheets (`.xmcd`, XML) into xmath worksheets.
//!
//! The importer is deliberately forgiving: anything it cannot translate is
//! kept as a text region (with a best-effort textual rendering) and reported
//! in [`ImportResult::warnings`]. It never panics on malformed input.

use crate::ast::{BinOp, Expr, PStmt, Stmt, Target, UnOp};
use crate::parser::{parse_expr, parse_statement};
use crate::worksheet::{Region, RegionKind, TextStyle, Worksheet};
use roxmltree::{Document, Node};
use std::collections::HashSet;
use std::path::Path;

/// Result of an import: the worksheet and everything that was lost or approximated.
#[derive(Debug, Clone)]
pub struct ImportResult {
    pub worksheet: Worksheet,
    pub warnings: Vec<String>,
}

/// Mathcad coordinates are points; convert to 96 dpi pixels.
const SCALE: f32 = 96.0 / 72.0;

type R<T> = Result<T, String>;

/// Imports a worksheet from `.xmcd` XML text.
pub fn import_xmcd(xml: &str) -> Result<ImportResult, String> {
    let xml = xml.trim_start_matches('\u{feff}');
    let doc = Document::parse(xml).map_err(|e| format!("некорректный XML: {e}"))?;
    let root = doc.root_element();
    if lname(root) != "worksheet" {
        return Err(format!("корневой элемент <{}> не является <worksheet>", lname(root)));
    }
    let mut cx = Cx { warnings: Vec::new(), label: String::new() };
    let mut ws = Worksheet::default();
    let mut used: HashSet<u64> = HashSet::new();
    let mut origin: Option<i64> = None;

    if let Some(settings) = child(root, "settings") {
        if let Some(v) = find_setting(settings, &["arrayorigin", "origin"]) {
            origin = v.trim().parse::<f64>().ok().map(|x| x as i64);
        }
        if let Some(p) = find_setting(settings, &["displayedprecision", "decimalplaces", "precision"])
            .and_then(|v| v.trim().parse::<usize>().ok())
        {
            if p <= 20 {
                ws.format.precision = p;
            }
        }
        if let Some(t) = find_setting(settings, &["exponentialthreshold", "exponentthreshold"])
            .and_then(|v| v.trim().parse::<i32>().ok())
        {
            ws.format.exp_threshold = t;
        }
    }

    if let Some(regions) = child(root, "regions") {
        for reg in kids(regions) {
            if lname(reg) != "region" {
                continue;
            }
            let id = reg
                .attribute("region-id")
                .and_then(|s| s.trim().parse::<u64>().ok())
                .filter(|i| !used.contains(i))
                .unwrap_or_else(|| used.iter().max().map_or(1, |m| m + 1));
            used.insert(id);
            cx.label = format!("область {id}");
            let x = num_attr(reg, "left").unwrap_or(0.0) * SCALE;
            let y = num_attr(reg, "top").unwrap_or(0.0) * SCALE;
            let w = num_attr(reg, "width").map(|w| (w * SCALE).max(40.0));
            let h = num_attr(reg, "height").map(|h| (h * SCALE).max(40.0));
            let Some(body) = kids(reg).into_iter().next() else {
                cx.warn("пустая область пропущена");
                continue;
            };
            let kind = convert_region(&mut cx, body, w, h);
            ws.regions.push(Region { id, x, y, kind });
        }
    }

    if let Some(o) = origin.filter(|&o| o != 0) {
        let top = ws.regions.iter().map(|r| r.y).fold(0.0f32, f32::min);
        let id = used.iter().max().map_or(1, |m| m + 1);
        ws.regions.push(Region { id, x: 20.0, y: top - 30.0, kind: RegionKind::Math { source: format!("ORIGIN ≡ {o}") } });
    }
    Ok(ImportResult { worksheet: ws, warnings: cx.warnings })
}

/// Reads and imports a `.xmcd` file.
pub fn import_xmcd_file(path: impl AsRef<Path>) -> Result<ImportResult, String> {
    let path = path.as_ref();
    let bytes = std::fs::read(path).map_err(|e| format!("не удалось прочитать {}: {e}", path.display()))?;
    import_xmcd(&String::from_utf8_lossy(&bytes))
}

struct Cx {
    warnings: Vec<String>,
    label: String,
}

impl Cx {
    fn warn(&mut self, msg: impl AsRef<str>) {
        self.warnings.push(format!("{}: {}", self.label, msg.as_ref()));
    }
}

// ---------------------------------------------------------------- XML helpers

fn lname<'a>(n: Node<'a, '_>) -> &'a str {
    n.tag_name().name()
}

fn kids<'a, 'i>(n: Node<'a, 'i>) -> Vec<Node<'a, 'i>> {
    n.children().filter(|c| c.is_element()).collect()
}

fn child<'a, 'i>(n: Node<'a, 'i>, name: &str) -> Option<Node<'a, 'i>> {
    n.children().find(|c| c.is_element() && lname(*c) == name)
}

fn num_attr(n: Node, name: &str) -> Option<f32> {
    n.attribute(name).and_then(|s| s.trim().parse::<f32>().ok()).filter(|v| v.is_finite())
}

fn norm(s: &str) -> String {
    s.chars().filter(|c| c.is_alphanumeric()).flat_map(|c| c.to_lowercase()).collect()
}

fn find_setting(settings: Node, names: &[&str]) -> Option<String> {
    settings
        .descendants()
        .filter(|n| n.is_element())
        .find(|n| names.contains(&norm(lname(*n)).as_str()) && n.text().map_or(false, |t| !t.trim().is_empty()))
        .and_then(|n| n.text().map(|t| t.to_string()))
}

fn is_math_ns(n: Node) -> bool {
    n.tag_name().namespace().map_or(false, |u| u.contains("/math"))
}

fn is_true(v: Option<&str>) -> bool {
    matches!(v.map(|s| s.trim().to_lowercase()).as_deref(), Some("true") | Some("1") | Some("yes"))
}

// ------------------------------------------------------------------- regions

fn text_kind(text: String, w: Option<f32>, style: TextStyle) -> RegionKind {
    let mut k = RegionKind::new_text(text);
    if let RegionKind::Text { width, style: s, .. } = &mut k {
        if let Some(w) = w {
            *width = w;
        }
        *s = style;
    }
    k
}

fn note(text: &str, w: Option<f32>) -> RegionKind {
    text_kind(text.to_string(), w, TextStyle::default())
}

fn convert_region(cx: &mut Cx, body: Node, w: Option<f32>, h: Option<f32>) -> RegionKind {
    match lname(body) {
        "math" => convert_math(cx, body, w),
        "text" => convert_text(cx, body, w),
        "plot" => match convert_plot(cx, body, w, h) {
            Ok(k) => k,
            Err(e) => {
                cx.warn(format!("график не импортирован ({e})"));
                note("[График не импортирован]", w)
            }
        },
        "picture" | "image" => {
            cx.warn("рисунок не импортирован");
            note("[Рисунок не импортирован]", w)
        }
        other => {
            cx.warn(format!("область типа <{other}> не поддерживается"));
            note(&format!("[Область «{other}» не импортирована]"), w)
        }
    }
}

// ---------------------------------------------------------------------- math

fn convert_math(cx: &mut Cx, math: Node, w: Option<f32>) -> RegionKind {
    let Some(top) = kids(math).into_iter().next() else {
        cx.warn("пустая математическая область");
        return note("[Пустая область]", w);
    };
    // a bare comma list (axis labels next to plots, `a, b, c`) is shown as text
    if lname(top) == "sequence" {
        let mut items = Vec::new();
        for k in kids(top) {
            if lname(k) == "placeholder" {
                continue;
            }
            match conv(cx, k) {
                Ok(e) => items.push(e.to_string()),
                Err(e) => cx.warn(format!("элемент списка отброшен ({e})")),
            }
        }
        return note(&items.join(", "), w);
    }
    let has_program = math.descendants().any(|n| n.is_element() && lname(n) == "program");
    let fallback = |cx: &mut Cx, why: String| {
        if has_program {
            cx.warn(format!("программа Mathcad не импортирована ({why}), область преобразована в текст"));
            note(&format!("[Программа Mathcad не поддерживается]\n{}", dump(top)), w)
        } else {
            cx.warn(format!("{why}; область преобразована в текст"));
            note(&dump(top), w)
        }
    };
    match conv_stmt(cx, top) {
        Ok(stmt) => {
            let src = stmt.to_string();
            match parse_statement(&src) {
                Ok(_) => RegionKind::Math { source: src },
                Err(e) => fallback(cx, format!("сгенерированное выражение «{src}» не разбирается: {}", e.msg)),
            }
        }
        Err(e) => fallback(cx, e),
    }
}

fn conv_stmt(cx: &mut Cx, n: Node) -> R<Stmt> {
    let ks = kids(n);
    match lname(n) {
        "define" | "globalDefine" => {
            if ks.len() != 2 {
                return Err("определение должно содержать две части".into());
            }
            let target = conv_target(cx, ks[0])?;
            // `d := expr = unit`: the right side is an `<eval>` holding the expression,
            // the display unit and the cached result
            let (expr, show) = if lname(ks[1]) == "eval" {
                let (expr, unit) = conv_eval(cx, ks[1])?;
                (expr, Some(unit))
            } else {
                (conv(cx, ks[1])?, None)
            };
            Ok(Stmt::Define { target, expr, global: lname(n) == "globalDefine", show })
        }
        "eval" => {
            let (expr, unit) = conv_eval(cx, n)?;
            Ok(Stmt::Eval { expr, unit })
        }
        "symEval" => {
            cx.warn("символьное вычисление (→) заменено численным (=)");
            let first = ks.first().ok_or("пустое символьное вычисление")?;
            Ok(Stmt::Eval { expr: conv(cx, *first)?, unit: None })
        }
        _ => Ok(Stmt::Show(conv(cx, n)?)),
    }
}

/// The content of an `<eval>`: the expression and its optional display unit
/// (the cached `<result>` is recomputed).
fn conv_eval(cx: &mut Cx, n: Node) -> R<(Expr, Option<Expr>)> {
    let mut expr = None;
    let mut unit = None;
    for k in kids(n) {
        match lname(k) {
            "result" => {}
            "unitOverride" => {
                if let Some(u) = kids(k).into_iter().next() {
                    if lname(u) != "placeholder" {
                        match conv(cx, u) {
                            Ok(e) => unit = Some(e),
                            Err(e) => cx.warn(format!("единица вывода отброшена ({e})")),
                        }
                    }
                }
            }
            _ if expr.is_none() => expr = Some(conv(cx, k)?),
            _ => {}
        }
    }
    Ok((expr.ok_or("пустое вычисление")?, unit))
}

fn conv_target(cx: &mut Cx, n: Node) -> R<Target> {
    match lname(n) {
        "matrix" => {
            let rows: usize = n.attribute("rows").and_then(|s| s.trim().parse().ok()).unwrap_or(0);
            let cols: usize = n.attribute("cols").and_then(|s| s.trim().parse().ok()).unwrap_or(0);
            if rows != 1 && cols != 1 {
                return Err("недопустимая левая часть определения (матрица имён)".into());
            }
            let mut names = Vec::new();
            for k in kids(n) {
                names.push(id_name(k).map_err(|_| "недопустимая левая часть определения: в векторе должны быть имена".to_string())?);
            }
            if names.is_empty() {
                return Err("пустой вектор имён".into());
            }
            Ok(Target::Vector(names))
        }
        "function" => {
            let ks = kids(n);
            let name = id_name(*ks.first().ok_or("функция без имени")?)?;
            let mut params = Vec::new();
            for bv in ks.iter().filter(|k| lname(**k) == "boundVars") {
                for p in kids(*bv) {
                    params.push(id_name(p)?);
                }
            }
            Ok(Target::Func(name, params))
        }
        _ => match conv(cx, n)? {
            Expr::Var(v) => Ok(Target::Var(v)),
            Expr::Index(base, idx) => match *base {
                Expr::Var(v) => Ok(Target::Indexed(v, idx)),
                _ => Err("недопустимая левая часть определения".into()),
            },
            Expr::Call(name, args) => {
                let mut ps = Vec::new();
                for a in args {
                    match a {
                        Expr::Var(v) => ps.push(v),
                        _ => return Err("параметры функции должны быть именами".into()),
                    }
                }
                Ok(Target::Func(name, ps))
            }
            _ => Err("недопустимая левая часть определения".into()),
        },
    }
}

fn id_name(n: Node) -> R<String> {
    if lname(n) != "id" {
        return Err(format!("ожидался идентификатор, найден <{}>", lname(n)));
    }
    let mut s: String = n.text().unwrap_or("").trim().to_string();
    if s.is_empty() {
        return Err("пустой идентификатор".into());
    }
    if let Some(sub) = n.attribute("subscript").map(str::trim).filter(|x| !x.is_empty()) {
        s.push('.');
        s.push_str(sub);
    }
    Ok(s)
}

fn bin(op: BinOp, a: Expr, b: Expr) -> Expr {
    Expr::Binary(op, Box::new(a), Box::new(b))
}

fn need(args: &[Node], n: usize, what: &str) -> R<()> {
    if args.len() == n {
        Ok(())
    } else {
        Err(format!("операция {what} ожидает {n} аргумент(ов), найдено {}", args.len()))
    }
}

fn conv_all(cx: &mut Cx, args: &[Node]) -> R<Vec<Expr>> {
    let mut out = Vec::new();
    for a in args {
        if lname(*a) == "sequence" {
            for k in kids(*a) {
                out.push(conv(cx, k)?);
            }
        } else {
            out.push(conv(cx, *a)?);
        }
    }
    Ok(out)
}

fn fold(op: BinOp, mut v: Vec<Expr>) -> R<Expr> {
    if v.is_empty() {
        return Err("операция без аргументов".into());
    }
    let first = v.remove(0);
    Ok(v.into_iter().fold(first, |acc, e| bin(op, acc, e)))
}

fn conv(cx: &mut Cx, n: Node) -> R<Expr> {
    match lname(n) {
        "id" => Ok(Expr::Var(id_name(n)?)),
        "real" => {
            let t = n.text().unwrap_or("").trim();
            t.parse::<f64>().map(Expr::Num).map_err(|_| format!("некорректное число «{t}»"))
        }
        "imag" => {
            let t = n.text().unwrap_or("").trim();
            let v = if t.is_empty() { 1.0 } else { t.parse::<f64>().map_err(|_| format!("некорректное число «{t}»"))? };
            Ok(Expr::Imag(v))
        }
        "str" => Ok(Expr::Str(n.text().unwrap_or("").to_string())),
        "placeholder" => {
            cx.warn("незаполненный плейсхолдер ■");
            Ok(Expr::Var("■".into()))
        }
        "parens" => {
            let ks = kids(n);
            need(&ks, 1, "parens")?;
            conv(cx, ks[0])
        }
        "apply" => conv_apply(cx, n),
        "program" => conv_program(cx, n),
        "onError" => {
            let ks = kids(n);
            need(&ks, 2, "onError")?;
            Ok(Expr::OnError(Box::new(conv(cx, ks[0])?), Box::new(conv(cx, ks[1])?)))
        }
        "matrix" => conv_matrix(cx, n),
        "range" => range_from(cx, &kids(n)),
        "vectorize" => {
            let ks = kids(n);
            Ok(Expr::Vectorize(Box::new(conv(cx, *ks.first().ok_or("пустой оператор vectorize")?)?)))
        }
        "symEval" => {
            cx.warn("оператор <symEval> отброшен");
            let ks = kids(n);
            conv(cx, *ks.first().ok_or("пустой оператор")?)
        }
        "integral" | "derivative" | "summation" | "product" | "rangeSum" | "rangeProduct" => {
            binding(cx, n, &kids(n))
        }
        other => Err(format!("неподдерживаемый элемент <{other}>")),
    }
}

fn conv_matrix(cx: &mut Cx, n: Node) -> R<Expr> {
    let rows: usize = n.attribute("rows").and_then(|s| s.trim().parse().ok()).ok_or("матрица без rows")?;
    let cols: usize = n.attribute("cols").and_then(|s| s.trim().parse().ok()).ok_or("матрица без cols")?;
    let ks = kids(n);
    if rows == 0 || cols == 0 || rows.checked_mul(cols) != Some(ks.len()) {
        return Err("число элементов матрицы не совпадает с rows*cols".into());
    }
    let mut items = Vec::new();
    for k in ks {
        items.push(conv(cx, k)?);
    }
    // Mathcad stores matrices column-major.
    let grid = (0..rows).map(|r| (0..cols).map(|c| items[c * rows + r].clone()).collect()).collect();
    Ok(Expr::Matrix(grid))
}

fn range_from(cx: &mut Cx, args: &[Node]) -> R<Expr> {
    let (firsts, last): (Vec<Expr>, Expr) = match args.len() {
        2 => {
            let f = if lname(args[0]) == "sequence" { conv_all(cx, &args[..1])? } else { vec![conv(cx, args[0])?] };
            (f, conv(cx, args[1])?)
        }
        3 => (vec![conv(cx, args[0])?, conv(cx, args[1])?], conv(cx, args[2])?),
        _ => return Err("некорректный диапазон".into()),
    };
    let mut it = firsts.into_iter();
    match (it.next(), it.next(), it.next()) {
        (Some(a), None, _) => Ok(Expr::Range(Box::new(a), None, Box::new(last))),
        (Some(a), Some(b), None) => Ok(Expr::Range(Box::new(a), Some(Box::new(b)), Box::new(last))),
        _ => Err("некорректный диапазон".into()),
    }
}

fn conv_apply(cx: &mut Cx, n: Node) -> R<Expr> {
    let ks = kids(n);
    let Some((&head, args)) = ks.split_first() else {
        return Err("пустое применение".into());
    };
    let hn = lname(head);
    let un = |cx: &mut Cx, op: UnOp, what: &str| -> R<Expr> {
        need(args, 1, what)?;
        Ok(Expr::Unary(op, Box::new(conv(cx, args[0])?)))
    };
    let two = |cx: &mut Cx, op: BinOp, what: &str| -> R<Expr> {
        need(args, 2, what)?;
        Ok(bin(op, conv(cx, args[0])?, conv(cx, args[1])?))
    };
    match hn {
        "plus" => {
            let v = conv_all(cx, args)?;
            fold(BinOp::Add, v)
        }
        "minus" => {
            if args.len() == 1 {
                un(cx, UnOp::Neg, "minus")
            } else {
                two(cx, BinOp::Sub, "minus")
            }
        }
        "mult" => {
            let v = conv_all(cx, args)?;
            fold(BinOp::Mul, v)
        }
        "div" => two(cx, BinOp::Div, "div"),
        "pow" => two(cx, BinOp::Pow, "pow"),
        "neg" => un(cx, UnOp::Neg, "neg"),
        "factorial" => un(cx, UnOp::Factorial, "factorial"),
        "not" => un(cx, UnOp::Not, "not"),
        "sqrt" => {
            need(args, 1, "sqrt")?;
            Ok(Expr::Call("sqrt".into(), vec![conv(cx, args[0])?]))
        }
        "nthRoot" => {
            need(args, 2, "nthRoot")?;
            Ok(Expr::Call("nthroot".into(), vec![conv(cx, args[0])?, conv(cx, args[1])?]))
        }
        "absval" => {
            need(args, 1, "absval")?;
            Ok(Expr::Abs(Box::new(conv(cx, args[0])?)))
        }
        "equal" => two(cx, BinOp::Eq, "equal"),
        "notEqual" => two(cx, BinOp::Ne, "notEqual"),
        "lessThan" => two(cx, BinOp::Lt, "lessThan"),
        "greaterThan" => two(cx, BinOp::Gt, "greaterThan"),
        "lessOrEqual" => two(cx, BinOp::Le, "lessOrEqual"),
        "greaterOrEqual" => two(cx, BinOp::Ge, "greaterOrEqual"),
        "and" => {
            let v = conv_all(cx, args)?;
            fold(BinOp::And, v)
        }
        "or" => {
            let v = conv_all(cx, args)?;
            fold(BinOp::Or, v)
        }
        "parens" => {
            need(args, 1, "parens")?;
            conv(cx, args[0])
        }
        "indexer" => {
            let (b, idx) = args.split_first().ok_or("индексация без аргументов")?;
            let base = conv(cx, *b)?;
            let idx = conv_all(cx, idx)?;
            if idx.is_empty() {
                return Err("индексация без индекса".into());
            }
            Ok(Expr::Index(Box::new(base), idx))
        }
        "vectorize" => {
            need(args, 1, "vectorize")?;
            Ok(Expr::Vectorize(Box::new(conv(cx, args[0])?)))
        }
        "matcol" => {
            need(args, 2, "matcol")?;
            Ok(Expr::MatCol(Box::new(conv(cx, args[0])?), Box::new(conv(cx, args[1])?)))
        }
        "transpose" => {
            need(args, 1, "transpose")?;
            Ok(Expr::Call("transpose".into(), vec![conv(cx, args[0])?]))
        }
        "range" => range_from(cx, args),
        "integral" | "derivative" | "summation" | "product" | "rangeSum" | "rangeProduct" => {
            binding(cx, head, args)
        }
        "id" => {
            let name = id_name(head)?;
            Ok(Expr::Call(name, conv_all(cx, args)?))
        }
        other => Err(format!("неподдерживаемая операция <{other}>")),
    }
}

// ------------------------------------------------------------------ programs

fn conv_program(cx: &mut Cx, n: Node) -> R<Expr> {
    let stmts = conv_pstmts(cx, &kids(n))?;
    if stmts.is_empty() {
        return Err("пустая программа".into());
    }
    Ok(Expr::Program(stmts))
}

/// Statements of a program; `otherwise` is attached to the preceding `if`.
fn conv_pstmts(cx: &mut Cx, nodes: &[Node]) -> R<Vec<PStmt>> {
    let mut out: Vec<PStmt> = Vec::new();
    for n in nodes {
        if lname(*n) == "otherwise" {
            let ks = kids(*n);
            need(&ks, 1, "otherwise")?;
            let body = conv_body(cx, ks[0])?;
            match out.last_mut() {
                Some(PStmt::If { else_: e @ None, .. }) => *e = Some(body),
                _ => return Err("otherwise без предшествующего if".into()),
            }
        } else {
            out.push(conv_pstmt(cx, *n)?);
        }
    }
    Ok(out)
}

/// A loop or branch body: a nested program is flattened, a single statement is wrapped.
fn conv_body(cx: &mut Cx, n: Node) -> R<Vec<PStmt>> {
    match lname(n) {
        "program" => conv_pstmts(cx, &kids(n)),
        _ => Ok(vec![conv_pstmt(cx, n)?]),
    }
}

fn conv_pstmt(cx: &mut Cx, n: Node) -> R<PStmt> {
    let ks = kids(n);
    match lname(n) {
        "localDefine" => {
            need(&ks, 2, "localDefine")?;
            let target = match conv_target(cx, ks[0])? {
                t @ (Target::Var(_) | Target::Indexed(..)) => t,
                Target::Func(..) => return Err("в программе нельзя определять функции".into()),
                Target::Vector(_) => return Err("в программе нельзя использовать векторное определение".into()),
            };
            Ok(PStmt::Assign(target, conv(cx, ks[1])?))
        }
        "ifThen" => {
            need(&ks, 2, "ifThen")?;
            Ok(PStmt::If { cond: conv(cx, ks[0])?, then: conv_body(cx, ks[1])?, else_: None })
        }
        "for" => {
            need(&ks, 3, "for")?;
            let var = id_name(ks[0])?;
            let iter = conv(cx, ks[1])?;
            Ok(PStmt::For { var, iter, body: conv_body(cx, ks[2])? })
        }
        "while" => {
            need(&ks, 2, "while")?;
            Ok(PStmt::While { cond: conv(cx, ks[0])?, body: conv_body(cx, ks[1])? })
        }
        "break" => Ok(PStmt::Break),
        "continue" => Ok(PStmt::Continue),
        "return" => {
            need(&ks, 1, "return")?;
            Ok(PStmt::Return(conv(cx, ks[0])?))
        }
        "parens" if ks.len() == 1 && matches!(lname(ks[0]), "break" | "continue" | "return" | "localDefine") => {
            conv_pstmt(cx, ks[0])
        }
        _ => Ok(PStmt::Expr(conv(cx, n)?)),
    }
}

/// Integral, derivative, sums and products: a lambda plus bounds.
fn binding(cx: &mut Cx, op: Node, args: &[Node]) -> R<Expr> {
    let name = lname(op);
    let lam = args.iter().find(|a| lname(**a) == "lambda").ok_or_else(|| format!("<{name}> без lambda"))?;
    let lk = kids(*lam);
    let mut vars = Vec::new();
    for bv in lk.iter().filter(|k| lname(**k) == "boundVars") {
        for p in kids(*bv) {
            vars.push(id_name(p)?);
        }
    }
    let var = vars.into_iter().next().ok_or("lambda без связанной переменной")?;
    let body_node = lk.iter().rev().find(|k| lname(**k) != "boundVars").ok_or("lambda без тела")?;
    let body = conv(cx, *body_node)?;
    let mut rest: Vec<Node> = Vec::new();
    for a in args.iter().filter(|a| lname(**a) != "lambda") {
        if lname(*a) == "bounds" {
            rest.extend(kids(*a));
        } else {
            rest.push(*a);
        }
    }
    let mut bounds = conv_all(cx, &rest)?;
    if bounds.len() == 1 {
        if let Expr::Range(a, None, b) = bounds[0].clone() {
            bounds = vec![*a, *b];
        }
    }
    let v = Expr::Var(var);
    match name {
        "integral" => {
            if bounds.len() != 2 {
                return Err("неопределённые интегралы не поддерживаются".into());
            }
            let mut it = bounds.into_iter();
            let (a, b) = (it.next().unwrap(), it.next().unwrap());
            Ok(Expr::Call("integrate".into(), vec![body, v, a, b]))
        }
        "derivative" => {
            if let Some(p) = bounds.first() {
                if *p != v {
                    cx.warn("значение производной в точке заменено производной по переменной");
                }
            }
            let order = op.attribute("order").and_then(|s| s.trim().parse::<f64>().ok()).unwrap_or(1.0);
            let mut a = vec![body, v];
            if order > 1.0 {
                a.push(Expr::Num(order));
            }
            Ok(Expr::Call("diff".into(), a))
        }
        "summation" | "product" if bounds.is_empty() => {
            // `Σ_i f(i)` over a range variable `i`
            let f = if name == "summation" { "sum" } else { "prod" };
            Ok(Expr::Call(f.into(), vec![v, body]))
        }
        "summation" | "product" => {
            if bounds.len() != 2 {
                return Err(format!("<{name}> требует две границы"));
            }
            let f = if name == "summation" { "sum" } else { "prod" };
            let mut it = bounds.into_iter();
            let (a, b) = (it.next().unwrap(), it.next().unwrap());
            Ok(Expr::Call(f.into(), vec![v, a, b, body]))
        }
        _ => {
            let f = if name == "rangeSum" { "sum" } else { "prod" };
            Ok(Expr::Call(f.into(), vec![v, body]))
        }
    }
}

// ---------------------------------------------------------------------- text

#[derive(Default)]
struct Stats {
    total: usize,
    bold: usize,
    italic: usize,
    underline: usize,
    size: Option<f32>,
}

#[derive(Clone, Copy, Default)]
struct Fmt {
    bold: bool,
    italic: bool,
    underline: bool,
}

fn convert_text(cx: &mut Cx, node: Node, w: Option<f32>) -> RegionKind {
    let mut st = Stats::default();
    let mut paras: Vec<String> = Vec::new();
    let ps: Vec<Node> = kids(node).into_iter().filter(|k| lname(*k) == "p").collect();
    if ps.is_empty() {
        let mut s = String::new();
        inline(cx, node, &mut s, Fmt::default(), &mut st);
        paras.push(s);
    } else {
        for p in ps {
            let mut s = String::new();
            inline(cx, p, &mut s, Fmt::default(), &mut st);
            paras.push(s);
        }
    }
    let text = paras.iter().map(|p| tidy(p)).collect::<Vec<_>>().join("\n");
    let all = st.total > 0;
    let style = TextStyle {
        size: st.size.map_or(10.0, |s| s.clamp(4.0, 72.0)),
        bold: all && st.bold == st.total,
        italic: all && st.italic == st.total,
        underline: all && st.underline == st.total,
    };
    text_kind(text, w, style)
}

/// Collapses XML pretty-printing whitespace but keeps explicit line breaks.
fn tidy(s: &str) -> String {
    s.split('\n')
        .map(|l| l.split_whitespace().collect::<Vec<_>>().join(" "))
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

fn inline(cx: &mut Cx, n: Node, out: &mut String, fmt: Fmt, st: &mut Stats) {
    for c in n.children() {
        if c.is_text() {
            let t = c.text().unwrap_or("");
            if !t.trim().is_empty() {
                st.total += 1;
                st.bold += fmt.bold as usize;
                st.italic += fmt.italic as usize;
                st.underline += fmt.underline as usize;
            }
            // Pretty-printing newlines inside a paragraph are not line breaks.
            out.push_str(&t.replace('\n', " "));
            continue;
        }
        if !c.is_element() {
            continue;
        }
        let mut f = fmt;
        let name = lname(c);
        match name {
            "b" => f.bold = true,
            "i" => f.italic = true,
            "u" => f.underline = true,
            _ => {}
        }
        if is_true(c.attribute("bold"))
            || matches!(c.attribute("font-weight").or(c.attribute("weight")), Some("bold") | Some("700"))
        {
            f.bold = true;
        }
        if is_true(c.attribute("italic")) || c.attribute("font-style") == Some("italic") {
            f.italic = true;
        }
        if is_true(c.attribute("underline")) {
            f.underline = true;
        }
        if st.size.is_none() {
            st.size = num_attr(c, "font-size").or_else(|| num_attr(c, "size"));
        }
        match name {
            "br" => out.push('\n'),
            "sub" => {
                out.push('_');
                inline(cx, c, out, f, st);
            }
            "sup" => {
                out.push('^');
                inline(cx, c, out, f, st);
            }
            "math" => {
                if let Some(top) = kids(c).into_iter().next() {
                    let s = match conv_stmt(cx, top) {
                        Ok(stmt) => stmt.to_string(),
                        Err(_) => dump(top),
                    };
                    out.push_str(&s);
                }
            }
            _ => inline(cx, c, out, f, st),
        }
    }
}

// ---------------------------------------------------------------------- plot

fn plot_roots<'a, 'i>(n: Node<'a, 'i>) -> Vec<Node<'a, 'i>> {
    n.descendants()
        .filter(|d| d.is_element() && is_math_ns(*d) && d.parent().map_or(true, |p| !is_math_ns(p)))
        .collect()
}

fn plot_expand<'a, 'i>(rs: &[Node<'a, 'i>]) -> Vec<Node<'a, 'i>> {
    let mut out = Vec::new();
    for r in rs {
        if lname(*r) == "sequence" {
            out.extend(kids(*r));
        } else {
            out.push(*r);
        }
    }
    out.into_iter().filter(|n| lname(*n) != "placeholder").collect()
}

fn convert_plot(cx: &mut Cx, plot: Node, w: Option<f32>, h: Option<f32>) -> R<RegionKind> {
    let axis = |names: &[&str]| {
        plot.descendants().find(|d| d.is_element() && names.contains(&norm(lname(*d)).as_str()))
    };
    let (xs, ys) = match (axis(&["xaxis", "xaxes"]), axis(&["yaxis", "yaxes"])) {
        (Some(xa), Some(ya)) => (plot_expand(&plot_roots(xa)), plot_expand(&plot_roots(ya))),
        _ => {
            let rs = plot_roots(plot);
            match rs.len() {
                0 => return Err("не найдены выражения трасс".into()),
                1 => (Vec::new(), plot_expand(&rs)),
                _ => (plot_expand(&rs[..1]), plot_expand(&rs[1..2])),
            }
        }
    };
    if ys.is_empty() {
        return Err("не найдены выражения по оси Y".into());
    }
    let to_src = |cx: &mut Cx, n: Node| -> R<String> {
        let s = conv(cx, n)?.to_string();
        parse_expr(&s).map_err(|e| format!("«{s}»: {}", e.msg))?;
        Ok(s)
    };
    if xs.len() > 1 {
        cx.warn("несколько выражений по оси X, используется первое");
    }
    let x = match xs.first() {
        Some(n) => to_src(cx, *n)?,
        None => String::new(),
    };
    let mut y = Vec::new();
    for n in ys {
        y.push(to_src(cx, n)?);
    }
    let mut k = RegionKind::new_plot();
    if let RegionKind::Plot { x: kx, y: ky, width, height, .. } = &mut k {
        *kx = x;
        *ky = y;
        if let Some(w) = w {
            *width = w;
        }
        if let Some(h) = h {
            *height = h;
        }
    }
    Ok(k)
}

// ---------------------------------------------------------------------- dump

/// Best-effort textual rendering of an arbitrary math node.
fn dump(n: Node) -> String {
    let ks = kids(n);
    match lname(n) {
        "id" => id_name(n).unwrap_or_default(),
        "str" => format!("\"{}\"", n.text().unwrap_or("")),
        "placeholder" => "■".into(),
        "apply" if !ks.is_empty() => {
            let args: Vec<String> = ks[1..].iter().map(|k| dump(*k)).collect();
            let op = match lname(ks[0]) {
                "plus" => Some("+"),
                "minus" => Some("-"),
                "mult" => Some("*"),
                "div" => Some("/"),
                "pow" => Some("^"),
                "equal" => Some("="),
                "notEqual" => Some("≠"),
                "lessThan" => Some("<"),
                "greaterThan" => Some(">"),
                "lessOrEqual" => Some("≤"),
                "greaterOrEqual" => Some("≥"),
                "and" => Some("and"),
                "or" => Some("or"),
                _ => None,
            };
            match op {
                Some("-") if args.len() == 1 => format!("-{}", args[0]),
                Some(op) => format!("({})", args.join(&format!(" {op} "))),
                None => format!("{}({})", dump(ks[0]), args.join(", ")),
            }
        }
        name => {
            if ks.is_empty() {
                let t = n.text().unwrap_or("").trim();
                if t.is_empty() {
                    name.to_string()
                } else {
                    t.to_string()
                }
            } else {
                let inner: Vec<String> = ks.iter().map(|k| dump(*k)).collect();
                match name {
                    "define" if inner.len() == 2 => format!("{} := {}", inner[0], inner[1]),
                    "globalDefine" if inner.len() == 2 => format!("{} ≡ {}", inner[0], inner[1]),
                    "sequence" | "boundVars" => inner.join(", "),
                    _ => format!("{name}({})", inner.join(", ")),
                }
            }
        }
    }
}
