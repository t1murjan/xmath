//! Compatibility checker: compares what xmath computes for an imported Mathcad
//! worksheet with the results Mathcad itself saved in the file.

use crate::value::Num;
use crate::worksheet::{RegionKind, Snapshot};
use crate::xmcd::{Expected, ExpectedValue, ImportResult};
use std::collections::HashMap;

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
    let mut report = CheckReport::default();
    for i in ws.evaluation_order() {
        let region = &ws.regions[i];
        let Some(exp) = r.expected.get(&region.id) else { continue };
        let source = match &region.kind {
            RegionKind::Math { source } => source.clone(),
            RegionKind::Text { text, .. } => format!("[текст] {}", text.lines().next().unwrap_or("")),
            _ => String::new(),
        };
        let verdict = verdict_of(r, region.id, &region.kind, exp, outputs.get(&region.id));
        report.regions.push(RegionCheck { id: region.id, source, verdict });
    }
    report
}

fn import_warning(r: &ImportResult, id: u64) -> Option<String> {
    let prefix = format!("область {id}: ");
    r.warnings.iter().find_map(|w| w.strip_prefix(&prefix).map(|s| s.to_string()))
}

fn verdict_of(
    r: &ImportResult,
    id: u64,
    kind: &RegionKind,
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
        (Expected::Error(e), None) => Verdict::Mismatch(format!("Mathcad показал ошибку «{e}», а xmath вычислил значение")),
        (Expected::Value(_), Some(e)) => Verdict::NotEvaluated(e),
        (Expected::Value(ev), None) => match &out.value {
            None => Verdict::NotEvaluated("нет значения".into()),
            Some(s) => compare(s, ev),
        },
    }
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
}
