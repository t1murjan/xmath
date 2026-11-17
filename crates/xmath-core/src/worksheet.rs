//! The worksheet document: freely placed regions evaluated top-to-bottom,
//! left-to-right, like in Mathcad.

use crate::ast::{Expr, Stmt};
use crate::eval::{Evaluator, Scope};
use crate::format::{FormatOptions, ResultView};
use crate::solve;
use crate::parser::{parse_expr, parse_statement, ParseError};
use crate::value::{EResult, Num, Value};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub const FILE_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Worksheet {
    #[serde(default = "file_version")]
    pub version: u32,
    #[serde(default)]
    pub format: FormatOptions,
    pub regions: Vec<Region>,
}

fn file_version() -> u32 {
    FILE_VERSION
}

impl Default for Worksheet {
    fn default() -> Self {
        Worksheet { version: FILE_VERSION, format: FormatOptions::default(), regions: Vec::new() }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Region {
    pub id: u64,
    pub x: f32,
    pub y: f32,
    #[serde(flatten)]
    pub kind: RegionKind,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum RegionKind {
    Math {
        source: String,
    },
    Text {
        text: String,
        #[serde(default = "default_text_width")]
        width: f32,
        #[serde(default)]
        style: TextStyle,
    },
    Plot {
        /// Abscissa expression (usually a range variable).
        #[serde(rename = "x_expr")]
        x: String,
        /// Ordinate expressions.
        #[serde(rename = "y_exprs")]
        y: Vec<String>,
        #[serde(default = "default_plot_w")]
        width: f32,
        #[serde(default = "default_plot_h")]
        height: f32,
    },
}

/// Character formatting of a text region.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TextStyle {
    /// Font size in points.
    pub size: f32,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
}

impl Default for TextStyle {
    fn default() -> Self {
        TextStyle { size: 10.0, bold: false, italic: false, underline: false }
    }
}

fn default_text_width() -> f32 {
    320.0
}
fn default_plot_w() -> f32 {
    360.0
}
fn default_plot_h() -> f32 {
    240.0
}

impl RegionKind {
    pub fn new_plot() -> RegionKind {
        RegionKind::Plot { x: String::new(), y: vec![String::new()], width: default_plot_w(), height: default_plot_h() }
    }

    pub fn new_text(text: impl Into<String>) -> RegionKind {
        RegionKind::Text { text: text.into(), width: default_text_width(), style: TextStyle::default() }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Series {
    pub label: String,
    pub points: Vec<[f64; 2]>,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct PlotData {
    pub series: Vec<Series>,
    /// Errors per y-expression (or for the x-expression).
    pub errors: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Output {
    /// Nothing to show (definitions, text regions).
    None,
    Value { view: ResultView, unit: Option<Expr> },
    Error(String),
    Plot(PlotData),
}

#[derive(Clone, Debug, PartialEq)]
pub struct RegionOutput {
    pub stmt: Option<Result<Stmt, ParseError>>,
    pub output: Output,
}

impl RegionOutput {
    pub fn error(&self) -> Option<String> {
        match (&self.stmt, &self.output) {
            (Some(Err(e)), _) => Some(e.msg.clone()),
            (_, Output::Error(e)) => Some(e.clone()),
            _ => None,
        }
    }
}

/// Default plot domain when the abscissa is an undefined variable (QuickPlot).
const QUICKPLOT_RANGE: (f64, f64, usize) = (-10.0, 10.0, 400);

impl Worksheet {
    /// Region ids in evaluation order (top-to-bottom, then left-to-right).
    pub fn evaluation_order(&self) -> Vec<usize> {
        let mut idx: Vec<usize> = (0..self.regions.len()).collect();
        idx.sort_by(|&a, &b| {
            let (ra, rb) = (&self.regions[a], &self.regions[b]);
            ra.y.total_cmp(&rb.y).then(ra.x.total_cmp(&rb.x))
        });
        idx
    }

    pub fn next_id(&self) -> u64 {
        self.regions.iter().map(|r| r.id).max().map_or(1, |m| m + 1)
    }

    pub fn to_json(&self) -> String {
        // (plot expressions are stored as `x_expr`/`y_exprs`, so they don't clash
        // with the region position `x`/`y`)
        serde_json::to_string_pretty(self).expect("worksheet serialization")
    }

    pub fn from_json(s: &str) -> Result<Worksheet, String> {
        serde_json::from_str(s).map_err(|e| e.to_string())
    }

    /// Builds a worksheet from plain text: one math region per line,
    /// lines starting with `#` become text regions.
    pub fn from_lines(src: &str) -> Worksheet {
        let mut ws = Worksheet::default();
        let mut y = 20.0;
        for line in src.lines() {
            let t = line.trim();
            if t.is_empty() {
                y += 12.0;
                continue;
            }
            let kind = if let Some(text) = t.strip_prefix('#') {
                RegionKind::new_text(text.trim())
            } else {
                RegionKind::Math { source: t.to_string() }
            };
            let id = ws.next_id();
            ws.regions.push(Region { id, x: 20.0, y, kind });
            y += 30.0;
        }
        ws
    }

    /// Evaluates the whole worksheet.
    pub fn evaluate(&self) -> HashMap<u64, RegionOutput> {
        let mut out = HashMap::new();
        let order = self.evaluation_order();
        let parsed: Vec<Option<Result<Stmt, ParseError>>> = self
            .regions
            .iter()
            .map(|r| match &r.kind {
                RegionKind::Math { source } => Some(parse_statement(source)),
                _ => None,
            })
            .collect();

        let mut ev = Evaluator::new();

        // Pass 1: global definitions (≡) are visible everywhere.
        for &i in &order {
            if let Some(Ok(Stmt::Define { target, expr, global: true })) = &parsed[i] {
                let output = match ev.define(target, expr) {
                    Ok(()) => Output::None,
                    Err(e) => Output::Error(e),
                };
                out.insert(self.regions[i].id, RegionOutput { stmt: parsed[i].clone(), output });
            }
        }

        // Pass 2: everything else in document order.
        let mut block: Option<SolveBlock> = None;
        for &i in &order {
            let region = &self.regions[i];
            let output = match (&region.kind, &parsed[i]) {
                (_, Some(Ok(Stmt::Define { global: true, .. }))) => continue,
                (RegionKind::Math { .. }, Some(Err(_))) => Output::None,
                (RegionKind::Math { .. }, Some(Ok(stmt))) => {
                    self.eval_in_block(&mut ev, stmt, region.id, &mut block, &mut out)
                }
                (RegionKind::Plot { x, y, .. }, _) => Output::Plot(plot(&ev, x, y)),
                _ => Output::None,
            };
            out.insert(region.id, RegionOutput { stmt: parsed[i].clone(), output });
        }
        if let Some(b) = block {
            flag_unclosed(&mut out, &b);
        }
        out
    }

    /// Handles solve blocks: `Given`, constraint regions and the closing call.
    fn eval_in_block(
        &self,
        ev: &mut Evaluator,
        stmt: &Stmt,
        id: u64,
        block: &mut Option<SolveBlock>,
        out: &mut HashMap<u64, RegionOutput>,
    ) -> Output {
        if matches!(stmt, Stmt::Show(Expr::Var(n)) if n.eq_ignore_ascii_case("given")) {
            if let Some(old) = block.take() {
                flag_unclosed(out, &old);
            }
            *block = Some(SolveBlock { given: id, constraints: Vec::new() });
            return Output::None;
        }
        let Some(b) = block.as_mut() else {
            return self.eval_stmt(ev, stmt);
        };
        let expr = match stmt {
            Stmt::Define { expr, .. } | Stmt::Eval { expr, .. } | Stmt::Show(expr) => expr,
        };
        if solve::contains_solve(expr) {
            let b = block.take().unwrap();
            // constraint regions may only use defined names, the unknowns (and function parameters)
            let mut known = solve::solve_call_vars(expr);
            if let Stmt::Define { target: crate::ast::Target::Func(_, params), .. } = stmt {
                known.extend(params.iter().cloned());
            }
            let sc = ev.top_scope();
            for (cid, ce) in &b.constraints {
                let mut names = Vec::new();
                crate::eval::free_vars(ce, &mut Vec::new(), &mut names);
                if let Some(n) = names.iter().find(|n| !known.contains(n) && ev.lookup(n, &sc).is_none()) {
                    if let Some(o) = out.get_mut(cid) {
                        o.output = Output::Error(format!("'{n}' не определена"));
                    }
                }
            }
            let cs: Vec<Expr> = b.constraints.iter().map(|(_, e)| e.clone()).collect();
            let new_stmt = match stmt {
                Stmt::Define { target, expr, global } => {
                    Stmt::Define { target: target.clone(), expr: solve::inject(expr, &cs), global: *global }
                }
                Stmt::Eval { expr, unit } => Stmt::Eval { expr: solve::inject(expr, &cs), unit: unit.clone() },
                Stmt::Show(expr) => Stmt::Show(solve::inject(expr, &cs)),
            };
            return self.eval_stmt(ev, &new_stmt);
        }
        match stmt {
            Stmt::Define { .. } => Output::Error(
                "Определения внутри блока решения недопустимы: поместите их выше Given или ниже Find".into(),
            ),
            Stmt::Show(e) if solve::is_constraint(e) => {
                b.constraints.push((id, e.clone()));
                Output::None
            }
            _ => self.eval_stmt(ev, stmt),
        }
    }

    fn eval_stmt(&self, ev: &mut Evaluator, stmt: &Stmt) -> Output {
        let result: EResult<Output> = (|| match stmt {
            Stmt::Define { target, expr, .. } => {
                ev.define(target, expr)?;
                Ok(Output::None)
            }
            Stmt::Eval { expr, unit } => {
                let sc = ev.top_scope();
                let v = ev.eval_iterated(expr, &sc)?;
                let u = match unit {
                    None => None,
                    Some(u) => Some(ev.eval(u, &sc)?.as_num().map_err(|_| "единица отображения должна быть скаляром".to_string())?),
                };
                let view = ResultView::from_value(&v, u, &self.format)?;
                Ok(Output::Value { view, unit: unit.clone() })
            }
            Stmt::Show(_) => Ok(Output::None),
        })();
        result.unwrap_or_else(Output::Error)
    }
}

/// An open solve block: the `Given` region and the constraints collected so far.
struct SolveBlock {
    given: u64,
    constraints: Vec<(u64, Expr)>,
}

fn flag_unclosed(out: &mut HashMap<u64, RegionOutput>, b: &SolveBlock) {
    if let Some(o) = out.get_mut(&b.given) {
        o.output = Output::Error("Given без Find, Minerr, Maximize или Minimize: блок решения не закрыт".into());
    }
}

fn plot(ev: &Evaluator, x_src: &str, y_srcs: &[String]) -> PlotData {
    let mut data = PlotData::default();
    let ys: Vec<(String, Expr)> = y_srcs
        .iter()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .filter_map(|s| match parse_expr(s) {
            Ok(e) => Some((s.to_string(), e)),
            Err(e) => {
                data.errors.push(format!("{s}: {}", e.msg));
                None
            }
        })
        .collect();
    if ys.is_empty() {
        return data;
    }
    let sc = ev.top_scope();
    let x_src = x_src.trim();

    // The variable swept along the x-axis, if any.
    let sweep_var: Option<String> = if x_src.is_empty() {
        let mut names = Vec::new();
        crate::eval::free_vars(&ys[0].1, &mut Vec::new(), &mut names);
        names
            .into_iter()
            .find(|n| matches!(ev.lookup(n, &sc), None | Some(Value::Range(_))))
    } else {
        match parse_expr(x_src) {
            Ok(Expr::Var(n)) if matches!(ev.lookup(&n, &sc), None | Some(Value::Range(_))) => Some(n),
            Ok(_) => None,
            Err(e) => {
                data.errors.push(format!("ось X: {}", e.msg));
                return data;
            }
        }
    };

    match sweep_var {
        Some(name) => {
            let values: Vec<Num> = match ev.lookup(&name, &sc) {
                Some(Value::Range(r)) => r.values().collect(),
                _ => {
                    let (a, b, n) = QUICKPLOT_RANGE;
                    (0..=n).map(|k| Num::real(a + (b - a) * k as f64 / n as f64)).collect()
                }
            };
            for (label, e) in &ys {
                let mut points = Vec::with_capacity(values.len());
                let mut first_err = None;
                for t in &values {
                    match ev.eval(e, &sc.with(&name, Value::Num(*t))).and_then(|v| v.as_num()) {
                        Ok(y) if y.is_real() && y.v.re.is_finite() => points.push([t.v.re, y.v.re]),
                        Ok(_) => points.push([t.v.re, f64::NAN]),
                        Err(err) => {
                            first_err.get_or_insert(err);
                            points.push([t.v.re, f64::NAN]);
                        }
                    }
                }
                if points.iter().all(|p| p[1].is_nan()) {
                    if let Some(err) = first_err {
                        data.errors.push(format!("{label}: {err}"));
                        continue;
                    }
                }
                data.series.push(Series { label: label.clone(), points });
            }
        }
        None => {
            let xs = match parse_expr(x_src).map_err(|e| e.msg).and_then(|e| vector_of(ev, &e, &sc)) {
                Ok(v) => v,
                Err(e) => {
                    data.errors.push(format!("ось X: {e}"));
                    return data;
                }
            };
            for (label, e) in &ys {
                match vector_of(ev, e, &sc) {
                    Ok(yv) if yv.len() == xs.len() => data.series.push(Series {
                        label: label.clone(),
                        points: xs.iter().zip(&yv).map(|(x, y)| [x.v.re, y.v.re]).collect(),
                    }),
                    Ok(yv) => data.errors.push(format!(
                        "{label}: {} значений, а по оси X — {}",
                        yv.len(),
                        xs.len()
                    )),
                    Err(err) => data.errors.push(format!("{label}: {err}")),
                }
            }
        }
    }
    data
}

fn vector_of(ev: &Evaluator, e: &Expr, sc: &Scope) -> EResult<Vec<Num>> {
    match ev.eval_iterated(e, sc)? {
        Value::Matrix(m) if m.is_vector() => Ok(m.data),
        Value::Num(n) => Ok(vec![n]),
        Value::Range(r) => Ok(r.values().collect()),
        other => Err(format!("ожидался вектор, найдено: {}", other.type_name())),
    }
}
