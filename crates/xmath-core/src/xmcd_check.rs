//! Compatibility checker: compares what xmath computes for an imported Mathcad
//! worksheet with the results Mathcad itself saved in the file.

use crate::ast::{Expr, PStmt, Stmt, Target};
use crate::value::{Num, Value};
use crate::worksheet::{RegionKind, Snapshot, Worksheet};
use crate::xmcd::{Expected, ExpectedValue, ImportResult};
use std::collections::{HashMap, HashSet};

/// Relative tolerance of the comparison.
pub const REL_TOL: f64 = 1e-6;

#[derive(Debug, Clone, PartialEq)]
pub enum Verdict {
    Ok,
    /// Computed, but differs from Mathcad's value.
    Mismatch(String),
    /// xmath produced no value (error, unsupported region, ...).
    NotEvaluated(String),
    /// Cannot be compared (unit unknown to xmath).
    Skipped(String),
}

impl Verdict {
    pub fn label(&self) -> &'static str {
        match self {
            Verdict::Ok => "OK",
            Verdict::Mismatch(_) => "MISMATCH",
            Verdict::NotEvaluated(_) => "NOT EVALUATED",
            Verdict::Skipped(_) => "SKIPPED",
        }
    }
}

#[derive(Debug, Clone)]
pub struct RegionCheck {
    pub id: u64,
    /// Source of the region (math) for display.
    pub source: String,
    pub verdict: Verdict,
}

#[derive(Debug, Clone, Default)]
pub struct CheckReport {
    /// In evaluation order.
    pub regions: Vec<RegionCheck>,
}

impl CheckReport {
    pub fn count(&self, f: impl Fn(&Verdict) -> bool) -> usize {
        self.regions.iter().filter(|r| f(&r.verdict)).count()
    }
    pub fn ok(&self) -> usize {
        self.count(|v| matches!(v, Verdict::Ok))
    }
    pub fn mismatch(&self) -> usize {
        self.count(|v| matches!(v, Verdict::Mismatch(_)))
    }
    pub fn not_evaluated(&self) -> usize {
        self.count(|v| matches!(v, Verdict::NotEvaluated(_)))
    }
    pub fn skipped(&self) -> usize {
        self.count(|v| matches!(v, Verdict::Skipped(_)))
    }
}

/// Evaluates the imported worksheet and compares every region that has a
/// Mathcad result.
pub fn check_import(r: &ImportResult) -> CheckReport {
    let ws = &r.worksheet;
    let outputs = crate::evaluate_worksheet_opts(ws, true);
    let random = random_dependent_regions(ws);
    let mut report = CheckReport::default();
    for i in ws.evaluation_order() {
        let region = &ws.regions[i];
        let Some(exp) = r.expected.get(&region.id) else { continue };
        let source = match &region.kind {
            RegionKind::Math { source } => source.clone(),
            RegionKind::Text { text, .. } => format!("[текст] {}", text.lines().next().unwrap_or("")),
            _ => String::new(),
        };
        let verdict = if random.contains(&region.id) {
            Verdict::Skipped("случайные числа: Mathcad сохранил свой розыгрыш".into())
        } else {
            verdict_of(r, region.id, &region.kind, &source, exp, outputs.get(&region.id))
        };
        report.regions.push(RegionCheck { id: region.id, source, verdict });
    }
    report
}

/// Random generators of Mathcad: a result that depends on one can never match the value Mathcad
/// saved, because the draw is not reproduced.
const RANDOM_FNS: &[&str] = &[
    "rnd", "runif", "rnorm", "rexp", "rpois", "rbinom", "rgamma", "rchisq", "rbeta", "rt", "rlnorm", "rweibull",
    "rcauchy", "rlogis", "rgeom", "whiten", "rnd", "noise",
];

/// Regions whose value comes from a random generator, directly or through a variable defined
/// from one (`ξ := rnorm(N, a, σ)`, then `Xmean := mean(ξ)`).
fn random_dependent_regions(ws: &Worksheet) -> HashSet<u64> {
    let mut tainted: HashSet<String> = HashSet::new();
    let mut regions = HashSet::new();
    for i in ws.evaluation_order() {
        let region = &ws.regions[i];
        let RegionKind::Math { source } = &region.kind else { continue };
        let Ok(stmt) = crate::parse_statement(source) else { continue };
        let mut names = Names::default();
        names.stmt(&stmt);
        let uses_random = names.calls.iter().any(|n| RANDOM_FNS.contains(&n.as_str()))
            || names.vars.iter().any(|n| tainted.contains(n));
        if uses_random {
            regions.insert(region.id);
            if let Stmt::Define { target, .. } = &stmt {
                tainted.insert(target_name(target));
            }
        }
    }
    regions
}

fn target_name(t: &Target) -> String {
    match t {
        Target::Var(n) | Target::Func(n, _) | Target::Indexed(n, _) => n.clone(),
        Target::Vector(ns) => ns.first().cloned().unwrap_or_default(),
    }
}

/// Identifiers of a statement: which functions it calls and which variables it mentions.
#[derive(Default)]
struct Names {
    calls: HashSet<String>,
    vars: HashSet<String>,
}

impl Names {
    fn stmt(&mut self, s: &Stmt) {
        match s {
            Stmt::Define { target, expr, show, .. } => {
                self.expr(expr);
                if let Some(Some(u)) = show {
                    self.expr(u);
                }
                if let Target::Indexed(_, idx) = target {
                    for e in idx {
                        self.expr(e);
                    }
                }
            }
            Stmt::Eval { expr, unit } => {
                self.expr(expr);
                if let Some(u) = unit {
                    self.expr(u);
                }
            }
            Stmt::Show(e) => self.expr(e),
            Stmt::Symbolic { expr, ops, .. } => {
                self.expr(expr);
                for op in ops {
                    for a in &op.args {
                        self.expr(a);
                    }
                }
            }
        }
    }

    fn expr(&mut self, e: &Expr) {
        match e {
            Expr::Var(n) => {
                self.vars.insert(n.clone());
            }
            Expr::Call(n, args) => {
                self.calls.insert(n.clone());
                for a in args {
                    self.expr(a);
                }
            }
            Expr::Unary(_, a) | Expr::Abs(a) | Expr::Vectorize(a) | Expr::MatCol(a, _) => self.expr(a),
            Expr::Binary(_, a, b) | Expr::OnError(a, b) => {
                self.expr(a);
                self.expr(b);
            }
            Expr::Index(a, idx) => {
                self.expr(a);
                for i in idx {
                    self.expr(i);
                }
            }
            Expr::Range(a, b, c) => {
                self.expr(a);
                if let Some(b) = b {
                    self.expr(b);
                }
                self.expr(c);
            }
            Expr::Matrix(rows) => {
                for r in rows {
                    for e in r {
                        self.expr(e);
                    }
                }
            }
            Expr::Program(stmts) => {
                for s in stmts {
                    self.pstmt(s);
                }
            }
            Expr::Num(_) | Expr::Imag(_) | Expr::Str(_) => {}
        }
    }

    fn pstmt(&mut self, s: &PStmt) {
        match s {
            PStmt::Assign(_, e) | PStmt::Expr(e) | PStmt::Return(e) => self.expr(e),
            PStmt::If { cond, then, else_ } => {
                self.expr(cond);
                for s in then {
                    self.pstmt(s);
                }
                if let Some(else_) = else_ {
                    for s in else_ {
                        self.pstmt(s);
                    }
                }
            }
            PStmt::For { iter, body, .. } => {
                self.expr(iter);
                for s in body {
                    self.pstmt(s);
                }
            }
            PStmt::While { cond, body } => {
                self.expr(cond);
                for s in body {
                    self.pstmt(s);
                }
            }
            PStmt::Break | PStmt::Continue => {}
        }
    }
}

fn import_warning(r: &ImportResult, id: u64) -> Option<String> {
    let prefix = format!("область {id}: ");
    r.warnings.iter().find_map(|w| w.strip_prefix(&prefix).map(|s| s.to_string()))
}

fn verdict_of(
    r: &ImportResult,
    id: u64,
    kind: &RegionKind,
    source: &str,
    exp: &Expected,
    out: Option<&crate::worksheet::RegionOutput>,
) -> Verdict {
    if !matches!(kind, RegionKind::Math { .. }) {
        let why = import_warning(r, id).unwrap_or_else(|| "область не импортирована как формула".into());
        return Verdict::NotEvaluated(format!("импорт: {why}"));
    }
    let Some(out) = out else { return Verdict::NotEvaluated("нет результата".into()) };
    let ours_err = out.error();
    match (exp, ours_err) {
        (Expected::Error(_), Some(_)) => Verdict::Ok,
        // Mathcad's error may be the overflow that xmath expresses as ∞: both agree the value
        // cannot be represented.
        (Expected::Error(_), None) if out.value.as_ref().is_some_and(overflowed) => Verdict::Ok,
        (Expected::Error(e), None) => Verdict::Mismatch(format!("Mathcad показал ошибку «{e}», а xmath вычислил значение")),
        (Expected::Value(_), Some(e)) => Verdict::NotEvaluated(e),
        (Expected::Value(ev), None) => match &out.value {
            None => Verdict::NotEvaluated("нет значения".into()),
            Some(s) => match in_display_unit(&r.worksheet, source, ev) {
                Some(scaled) => compare(s, &scaled),
                None => compare(s, ev),
            },
        },
    }
}

/// Mathcad stores a result shown in a display unit (`x = kB`, `<ml:unitOverride>`) as the bare
/// number of those units, while xmath works in SI. Resolve the unit in the document's own scope
/// (`kB := 1023*Byte` there) and convert the stored value to SI.
fn in_display_unit(ws: &Worksheet, source: &str, exp: &ExpectedValue) -> Option<ExpectedValue> {
    if !exp.display_unit {
        return None;
    }
    let Ok(crate::Stmt::Eval { unit: Some(u), .. }) = crate::parse_statement(source) else { return None };
    let Some(Value::Num(n)) = ws.eval_in_scope(&u) else { return None };
    let factor = n.v.re;
    if n.v.im != 0.0 || !factor.is_finite() || factor == 0.0 {
        return None;
    }
    let mut out = exp.clone();
    out.data = exp.data.iter().map(|(re, im)| (re * factor, im * factor)).collect();
    out.dim = n.dim;
    out.display_unit = false;
    Some(out)
}

/// Every element of the result is infinite or NaN (an overflow Mathcad could not show either).
fn overflowed(s: &Snapshot) -> bool {
    !s.data.is_empty() && s.data.iter().all(|n| !n.v.re.is_finite() || !n.v.im.is_finite())
}

fn close(a: f64, b: f64, scale: f64) -> bool {
    if a == b || (a.is_nan() && b.is_nan()) {
        return true;
    }
    let d = (a - b).abs();
    d <= REL_TOL * a.abs().max(b.abs()) || d <= 1e-12 * scale
}

fn show(z: (f64, f64)) -> String {
    if z.1 == 0.0 {
        format!("{:e}", z.0)
    } else {
        format!("{:e}{:+e}i", z.0, z.1)
    }
}

fn compare(ours: &Snapshot, exp: &ExpectedValue) -> Verdict {
    if let Some(u) = &exp.unknown_unit {
        return Verdict::Skipped(format!("неизвестная единица «{u}»"));
    }
    if let Some(t) = &exp.text {
        return match &ours.text {
            Some(o) if exp.data.is_empty() && (t.is_empty() || o == t) => Verdict::Ok,
            Some(o) => Verdict::Mismatch(format!("строка «{o}», у Mathcad «{t}»")),
            None => Verdict::Mismatch("у Mathcad строка".into()),
        };
    }
    if ours.text.is_some() {
        return Verdict::Mismatch("xmath вернул строку".into());
    }
    if (ours.rows, ours.cols) != (exp.rows, exp.cols) {
        return Verdict::Mismatch(format!("размер {}×{}, у Mathcad {}×{}", ours.rows, ours.cols, exp.rows, exp.cols));
    }
    let scale = exp.data.iter().flat_map(|z| [z.0.abs(), z.1.abs()]).fold(0.0f64, f64::max);
    for (k, (n, e)) in ours.data.iter().zip(&exp.data).enumerate() {
        let Num { v, dim } = *n;
        let zero = e.0 == 0.0 && e.1 == 0.0 && v.re == 0.0 && v.im == 0.0;
        if dim != exp.dim && !zero {
            return Verdict::Mismatch(format!("размерность {dim:?}, у Mathcad {:?}", exp.dim));
        }
        if !close(v.re, e.0, scale) || !close(v.im, e.1, scale) {
            let pos = if ours.data.len() > 1 { format!(" [элемент {}]", k + 1) } else { String::new() };
            return Verdict::Mismatch(format!("{}{pos}, у Mathcad {}", show((v.re, v.im)), show(*e)));
        }
    }
    Verdict::Ok
}

/// Short, grouped description of why regions were not evaluated or differ
/// (for the "top causes" table).
pub fn cause(v: &Verdict) -> Option<String> {
    let text = match v {
        Verdict::NotEvaluated(s) => s,
        Verdict::Mismatch(_) => return Some("значение отличается".into()),
        Verdict::Skipped(s) => return Some(format!("пропущено: {s}")),
        Verdict::Ok => return None,
    };
    // generalise: keep function names, cut long messages
    let t: String = text.chars().take(80).collect();
    Some(t)
}

/// Counts of causes, most frequent first.
pub fn top_causes(reports: &[&CheckReport]) -> Vec<(String, usize)> {
    let mut m: HashMap<String, usize> = HashMap::new();
    for r in reports {
        for c in &r.regions {
            if let Some(k) = cause(&c.verdict) {
                *m.entry(k).or_default() += 1;
            }
        }
    }
    let mut v: Vec<(String, usize)> = m.into_iter().collect();
    v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xmcd::import_xmcd;

    fn sheet(body: &str) -> String {
        format!(
            r#"<worksheet xmlns="http://schemas.mathsoft.com/worksheet30" xmlns:ml="http://schemas.mathsoft.com/math30" xmlns:u="http://schemas.mathsoft.com/units10"><regions>{body}</regions></worksheet>"#
        )
    }

    fn region(id: u32, top: f32, math: &str) -> String {
        format!(r#"<region region-id="{id}" left="10" top="{top}" width="100" height="20" align-x="20" align-y="{}"><math>{math}</math></region>"#, top + 9.0)
    }

    #[test]
    fn compares_numbers_units_and_matrices() {
        let xml = sheet(&[
            region(1, 10.0, "<ml:define><ml:id>a</ml:id><ml:real>5</ml:real></ml:define>"),
            region(2, 40.0, "<ml:eval><ml:apply><ml:mult/><ml:id>a</ml:id><ml:real>2</ml:real></ml:apply><result xmlns=\"http://schemas.mathsoft.com/math30\"><ml:real>10</ml:real></result></ml:eval>"),
            // 5 cm = 0.05 m stored in the displayed unit
            region(3, 70.0, "<ml:eval><ml:apply><ml:mult/><ml:real>5</ml:real><ml:id>cm</ml:id></ml:apply><ml:unitOverride><ml:id>cm</ml:id></ml:unitOverride><result xmlns=\"http://schemas.mathsoft.com/math30\"><unitedValue><ml:real>5</ml:real><unitMonomial xmlns=\"http://schemas.mathsoft.com/units10\"><unitReference unit=\"centimeter\"/></unitMonomial></unitedValue></result></ml:eval>"),
            // wrong value
            region(4, 100.0, "<ml:eval><ml:apply><ml:plus/><ml:real>1</ml:real><ml:real>1</ml:real></ml:apply><result xmlns=\"http://schemas.mathsoft.com/math30\"><ml:real>3</ml:real></result></ml:eval>"),
            // matrix, column-major in the file
            region(5, 130.0, "<ml:eval><ml:matrix rows=\"2\" cols=\"2\"><ml:real>1</ml:real><ml:real>3</ml:real><ml:real>2</ml:real><ml:real>4</ml:real></ml:matrix><result xmlns=\"http://schemas.mathsoft.com/math30\"><ml:matrix rows=\"2\" cols=\"2\"><ml:real>1</ml:real><ml:real>3</ml:real><ml:real>2</ml:real><ml:real>4</ml:real></ml:matrix></result></ml:eval>"),
            // unknown function
            region(6, 160.0, "<ml:eval><ml:apply><ml:id>nosuchfn</ml:id><ml:real>1</ml:real></ml:apply><result xmlns=\"http://schemas.mathsoft.com/math30\"><ml:real>3</ml:real></result></ml:eval>"),
            // complex
            region(7, 190.0, "<ml:eval><ml:apply><ml:sqrt/><ml:real>-4</ml:real></ml:apply><result xmlns=\"http://schemas.mathsoft.com/math30\"><ml:complex><ml:real>0</ml:real><ml:imag>2</ml:imag></ml:complex></result></ml:eval>"),
        ].join(""));
        let r = import_xmcd(&xml).unwrap();
        assert_eq!(r.expected.len(), 6);
        let rep = check_import(&r);
        let v: HashMap<u64, &Verdict> = rep.regions.iter().map(|c| (c.id, &c.verdict)).collect();
        assert_eq!(v[&2], &Verdict::Ok, "{rep:?}");
        assert_eq!(v[&3], &Verdict::Ok, "{rep:?}");
        assert!(matches!(v[&4], Verdict::Mismatch(_)));
        assert_eq!(v[&5], &Verdict::Ok, "{rep:?}");
        assert!(matches!(v[&6], Verdict::NotEvaluated(_)));
        assert_eq!(v[&7], &Verdict::Ok, "{rep:?}");
        assert_eq!((rep.ok(), rep.mismatch(), rep.not_evaluated()), (4, 1, 1));
        assert!(top_causes(&[&rep]).iter().any(|(c, _)| c.contains("nosuchfn")));
    }

    #[test]
    fn display_units_of_stored_results() {
        // Mathcad writes the number as shown, with the display unit in <ml:unitOverride>: the
        // result of `F = kB` is 7.91 (kB is defined by the document itself), and `2 cm` is 2.
        let xml = sheet(&[
            region(1, 10.0, "<ml:define><ml:id>Byte</ml:id><ml:real>1</ml:real></ml:define>"),
            region(2, 40.0, "<ml:define><ml:id>kB</ml:id><ml:apply><ml:mult/><ml:real>1023</ml:real><ml:id>Byte</ml:id></ml:apply></ml:define>"),
            region(3, 70.0, "<ml:define><ml:id>F</ml:id><ml:apply><ml:mult/><ml:real>48</ml:real><ml:id>kB</ml:id></ml:apply></ml:define>"),
            region(4, 100.0, "<ml:eval><ml:id>F</ml:id><ml:unitOverride><ml:id>kB</ml:id></ml:unitOverride><result xmlns=\"http://schemas.mathsoft.com/math30\"><ml:real>48</ml:real></result></ml:eval>"),
            region(5, 130.0, "<ml:eval><ml:apply><ml:mult/><ml:real>2</ml:real><ml:id>cm</ml:id></ml:apply><ml:unitOverride><ml:id>cm</ml:id></ml:unitOverride><result xmlns=\"http://schemas.mathsoft.com/math30\"><ml:real>2</ml:real></result></ml:eval>"),
            // wrong number in the display unit: 3 cm where xmath computes 2 cm
            region(6, 160.0, "<ml:eval><ml:apply><ml:mult/><ml:real>2</ml:real><ml:id>cm</ml:id></ml:apply><ml:unitOverride><ml:id>cm</ml:id></ml:unitOverride><result xmlns=\"http://schemas.mathsoft.com/math30\"><ml:real>3</ml:real></result></ml:eval>"),
        ].join(""));
        let r = import_xmcd(&xml).unwrap();
        let rep = check_import(&r);
        let v: HashMap<u64, &Verdict> = rep.regions.iter().map(|c| (c.id, &c.verdict)).collect();
        assert_eq!(v[&4], &Verdict::Ok, "{rep:?}");
        assert_eq!(v[&5], &Verdict::Ok, "{rep:?}");
        assert!(matches!(v[&6], Verdict::Mismatch(_)), "{rep:?}");
    }

    #[test]
    fn evaluation_order_follows_anchors() {
        // region 1 starts higher, but both have the same baseline and region 2 is further left
        let r1 = r#"<region region-id="1" left="200" top="4" width="100" height="20" align-x="210" align-y="19"><math><ml:define><ml:id>f</ml:id><ml:id>K</ml:id></ml:define></math></region>"#;
        let r2 = r#"<region region-id="2" left="10" top="10" width="100" height="20" align-x="20" align-y="19"><math><ml:define><ml:id>K</ml:id><ml:real>3</ml:real></ml:define></math></region>"#;
        let r = import_xmcd(&sheet(&format!("{r1}{r2}"))).unwrap();
        let ws = &r.worksheet;
        assert!(ws.regions[0].anchor[1] > 0.0);
        let order: Vec<u64> = ws.evaluation_order().iter().map(|&i| ws.regions[i].id).collect();
        assert_eq!(order, vec![2, 1]);
        // without anchors the top-left corner decides
        let mut ws2 = ws.clone();
        for reg in &mut ws2.regions {
            reg.anchor = [0.0, 0.0];
        }
        let order: Vec<u64> = ws2.evaluation_order().iter().map(|&i| ws2.regions[i].id).collect();
        assert_eq!(order, vec![1, 2]);
    }

    fn eval_result(expr: &str, result: &str) -> String {
        format!("<ml:eval>{expr}<result xmlns=\"http://schemas.mathsoft.com/math30\">{result}</result></ml:eval>")
    }

    fn verdicts(body: &str) -> (HashMap<u64, Verdict>, CheckReport) {
        let r = import_xmcd(&sheet(body)).unwrap();
        let rep = check_import(&r);
        (rep.regions.iter().map(|c| (c.id, c.verdict.clone())).collect(), rep)
    }

    #[test]
    fn indexed_definition_with_shown_value() {
        // `v[1] := 2*a =` displays the assigned element, not the whole vector
        let body = [
            region(1, 10.0, "<ml:define><ml:id>a</ml:id><ml:real>5</ml:real></ml:define>"),
            region(2, 40.0, &format!(
                "<ml:define><ml:apply><ml:indexer/><ml:id>v</ml:id><ml:real>2</ml:real></ml:apply>{}</ml:define>",
                eval_result("<ml:apply><ml:mult/><ml:real>2</ml:real><ml:id>a</ml:id></ml:apply>", "<ml:real>10</ml:real>")
            )),
        ]
        .join("");
        let (v, rep) = verdicts(&body);
        assert_eq!(v[&2], Verdict::Ok, "{rep:?}");
    }

    #[test]
    fn conjugate_vector_sum_xor_polyroots() {
        let m = |vals: &[&str]| format!("<ml:matrix rows=\"{}\" cols=\"1\">{}</ml:matrix>", vals.len(), vals.iter().map(|v| format!("<ml:real>{v}</ml:real>")).collect::<String>());
        let body = [
            region(1, 10.0, &eval_result(&format!("<ml:apply><ml:vectorSum/>{}</ml:apply>", m(&["1", "2", "3"])), "<ml:real>6</ml:real>")),
            region(2, 40.0, &eval_result("<ml:apply><ml:xor/><ml:real>1</ml:real><ml:real>0</ml:real></ml:apply>", "<ml:real>1</ml:real>")),
            region(3, 70.0, &eval_result(
                "<ml:apply><ml:conjugate/><ml:apply><ml:sqrt/><ml:real>-4</ml:real></ml:apply></ml:apply>",
                "<ml:complex><ml:real>0</ml:real><ml:imag>-2</ml:imag></ml:complex>",
            )),
            // x^2 + 2x - 3 with coefficients in ascending order: roots -3 and 1
            region(4, 100.0, &eval_result(&format!("<ml:apply><ml:polyroots method=\"laguerre\"/>{}</ml:apply>", m(&["-3", "2", "1"])), &m(&["-3", "1"]))),
        ]
        .join("");
        let (v, rep) = verdicts(&body);
        for id in 1..=4 {
            assert_eq!(v[&id], Verdict::Ok, "{id}: {rep:?}");
        }
    }
}
