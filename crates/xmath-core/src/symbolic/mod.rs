//! Mathcad-style symbolic evaluation (`expr →`, `expr → simplify`, `expr → solve, x`, ...).
//!
//! The engine works on canonical symbolic values ([`sx::Sx`]) with exact rational
//! arithmetic; results are converted back to syntax trees ([`Expr`]) so that they can be
//! printed (and re-parsed) and drawn by the front ends.

pub mod algebra;
pub mod calculus;
pub mod convert;
pub mod matrix;
pub mod poly;
pub mod solve;
pub mod sx;

#[cfg(test)]
mod tests;

use crate::ast::{BinOp, Expr, SymOp};
use convert::{ConvErr, Env};
use std::collections::HashMap;
use sx::*;

/// Definitions visible to a symbolic evaluation (recorded by the worksheet in document order).
#[derive(Default, Clone, Debug)]
pub struct Defs {
    pub vars: HashMap<String, Expr>,
    pub funcs: HashMap<String, (Vec<String>, Expr)>,
}

impl Defs {
    /// Records `target := expr`.
    pub fn define(&mut self, target: &crate::ast::Target, expr: &Expr) {
        use crate::ast::Target;
        match target {
            Target::Var(n) => {
                self.funcs.remove(n);
                self.vars.insert(n.clone(), expr.clone());
            }
            Target::Func(n, params) => {
                self.vars.remove(n);
                self.funcs.insert(n.clone(), (params.clone(), expr.clone()));
            }
            Target::Indexed(n, _) => {
                // an indexed definition makes the whole variable numeric
                self.vars.remove(n);
            }
            Target::Vector(names) => {
                for n in names {
                    self.vars.remove(n);
                    self.funcs.remove(n);
                }
            }
        }
    }
}

pub struct Ctx<'a> {
    pub defs: &'a Defs,
    /// Numeric evaluation of closed sub-expressions (built-in functions, indexing, ...).
    pub numeric: Option<&'a dyn Fn(&Expr) -> Option<Sx>>,
    /// The numeric value of a defined name.
    pub value_of: Option<&'a dyn Fn(&str) -> Option<Sx>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SymOut {
    pub result: Expr,
    pub note: Option<String>,
}

pub enum Outcome {
    Done(SymOut),
    /// The expression has no symbolic form (programs, solve blocks, ...): evaluate numerically.
    Fallback(String),
}

/// Names that are variables of a keyword (never substituted from definitions).
fn protected_names(ops: &[SymOp]) -> Vec<String> {
    let mut out = Vec::new();
    fn walk(e: &Expr, out: &mut Vec<String>) {
        match e {
            Expr::Var(n) => out.push(n.clone()),
            Expr::Matrix(rows) => rows.iter().flatten().for_each(|x| walk(x, out)),
            Expr::Binary(BinOp::Eq, a, _) => walk(a, out),
            _ => {}
        }
    }
    for op in ops {
        match op.name.as_str() {
            "solve" | "series" | "collect" | "coeffs" | "parfrac" | "substitute" => {
                for a in &op.args {
                    walk(a, &mut out);
                }
            }
            _ => {}
        }
    }
    out
}

fn conv_all(expr: &Expr, ctx: &Ctx, protect: &[String]) -> Result<Sx, ConvErr> {
    let mut env = Env::new(ctx);
    for p in protect {
        env.locals.push((p.clone(), sym(p)));
    }
    env.conv(expr)
}

fn var_sym(e: &Expr) -> Result<Sx, String> {
    match e {
        Expr::Var(n) => Ok(sym(n)),
        other => Err(format!("ожидалась переменная, найдено «{other}»")),
    }
}

fn var_list(e: &Expr) -> Result<Vec<Sx>, String> {
    match e {
        Expr::Matrix(rows) => rows.iter().flatten().map(var_sym).collect(),
        other => Ok(vec![var_sym(other)?]),
    }
}

fn assumption_from(cond: &Expr, a: &mut Assumptions) -> Result<(), String> {
    match cond {
        Expr::Binary(BinOp::Gt | BinOp::Ge, l, r) => match (&**l, &**r) {
            (Expr::Var(n), Expr::Num(v)) if *v >= 0.0 => {
                a.positive.insert(n.clone());
                Ok(())
            }
            (Expr::Var(n), _) => {
                a.real.insert(n.clone());
                Ok(())
            }
            _ => Err("условие assume должно иметь вид «x > 0»".into()),
        },
        Expr::Binary(BinOp::Lt | BinOp::Le, l, r) => match (&**l, &**r) {
            (Expr::Var(n), Expr::Num(v)) if *v <= 0.0 => {
                a.negative.insert(n.clone());
                Ok(())
            }
            (Expr::Var(n), _) => {
                a.real.insert(n.clone());
                Ok(())
            }
            _ => Err("условие assume должно иметь вид «x < 0»".into()),
        },
        Expr::Call(n, args) if (n == "real" || n == "RealRange") && !args.is_empty() => {
            if let Expr::Var(v) = &args[0] {
                a.real.insert(v.clone());
            }
            Ok(())
        }
        Expr::Var(n) => {
            a.real.insert(n.clone());
            Ok(())
        }
        _ => Err("условие assume должно иметь вид «x > 0»".into()),
    }
}

fn check_err(r: Sx) -> Result<Sx, String> {
    if let Some(e) = take_error() {
        return Err(e);
    }
    if let Sx::Fun(n, _) = &r {
        if n == "undefined" {
            return Err("результат не определён".into());
        }
    }
    Ok(r)
}

/// Applies the keyword operations in order.
pub fn evaluate(expr: &Expr, ops: &[SymOp], ctx: &Ctx) -> Result<Outcome, String> {
    take_error();
    set_assumptions(Assumptions::default());
    let protect = protected_names(ops);
    let conv = |e: &Expr| -> Result<Sx, String> {
        match conv_all(e, ctx, &protect) {
            Ok(v) => check_err(v),
            Err(ConvErr::Error(m)) => Err(m),
            Err(ConvErr::NotSymbolic(m)) => Err(format!("\u{0}{m}")),
        }
    };
    let mut cur = match conv(expr) {
        Ok(v) => v,
        Err(m) if m.starts_with('\u{0}') => return Ok(Outcome::Fallback(m[1..].to_string())),
        Err(m) => return Err(m),
    };
    let mut note: Option<String> = None;
    for op in ops {
        cur = apply(op, cur, expr, ctx, &protect, &mut note)?;
        cur = check_err(cur)?;
    }
    if cur.free_symbols().iter().any(|s| s.starts_with('§')) {
        return Err("внутренняя ошибка символьного вычисления".into());
    }
    set_assumptions(Assumptions::default());
    Ok(Outcome::Done(SymOut { result: convert::to_expr(&cur), note }))
}

fn arg_conv(e: &Expr, ctx: &Ctx, protect: &[String]) -> Result<Sx, String> {
    match conv_all(e, ctx, protect) {
        Ok(v) => check_err(v),
        Err(ConvErr::Error(m)) | Err(ConvErr::NotSymbolic(m)) => Err(m),
    }
}

fn need_args(op: &SymOp, n: usize) -> Result<(), String> {
    if op.args.len() < n {
        Err(format!("ключевое слово «{}» требует аргумент (например «{}, x»)", op.name, op.name))
    } else {
        Ok(())
    }
}

fn apply(op: &SymOp, cur: Sx, orig: &Expr, ctx: &Ctx, protect: &[String], note: &mut Option<String>) -> Result<Sx, String> {
    match op.name.as_str() {
        "simplify" => Ok(algebra::simplify(&cur)),
        "expand" => Ok(algebra::expand(&cur)),
        "factor" => Ok(algebra::factor(&cur)),
        "complex" | "rectangular" => Ok(algebra::expand(&cur)),
        "collect" => {
            need_args(op, 1)?;
            let x = var_sym(&op.args[0])?;
            Ok(algebra::collect(&cur, &x))
        }
        "coeffs" => {
            need_args(op, 1)?;
            let x = var_sym(&op.args[0])?;
            algebra::coeffs(&cur, &x).ok_or_else(|| "выражение не является многочленом по этой переменной".to_string())
        }
        "parfrac" => {
            need_args(op, 1)?;
            let x = var_sym(&op.args[0])?;
            algebra::parfrac(&cur, &x).ok_or_else(|| "разложение на простейшие дроби возможно только для рациональной функции с числовыми коэффициентами".to_string())
        }
        "float" => {
            let digits = match op.args.first() {
                Some(Expr::Num(n)) if *n >= 1.0 => *n as usize,
                None => 20,
                _ => return Err("float: число цифр должно быть натуральным числом".into()),
            };
            if digits > 17 {
                *note = Some("точность ограничена 17 значащими цифрами".into());
            }
            Ok(floatify(&cur, digits))
        }
        "substitute" => {
            need_args(op, 1)?;
            let mut pairs = Vec::new();
            for a in &op.args {
                match a {
                    Expr::Binary(BinOp::Eq, l, r) => {
                        let v = arg_conv(r, ctx, &[])?;
                        pairs.push((var_sym(l)?, v));
                    }
                    other => return Err(format!("substitute: ожидалось «x = значение», найдено «{other}»")),
                }
            }
            // simultaneous substitution
            Ok(subs_many(&cur, &pairs))
        }
        "series" => {
            need_args(op, 1)?;
            let (x, at) = match &op.args[0] {
                Expr::Binary(BinOp::Eq, l, r) => (var_sym(l)?, arg_conv(r, ctx, &[])?),
                other => (var_sym(other)?, zero()),
            };
            let n = match op.args.get(1) {
                Some(e) => match arg_conv(e, ctx, &[])?.as_int() {
                    Some(k) if (1..=40).contains(&k) => k as usize,
                    _ => return Err("series: порядок должен быть натуральным числом".into()),
                },
                None => 6,
            };
            calculus::series(&cur, &x, &at, n).ok_or_else(|| "не удалось разложить выражение в ряд".to_string())
        }
        "solve" => solve_op(op, cur, ctx, protect, note),
        "assume" => {
            need_args(op, 1)?;
            let mut a = assumptions();
            for c in &op.args {
                assumption_from(c, &mut a)?;
            }
            set_assumptions(a);
            // re-evaluate the original expression under the assumptions
            let mut p = protect.to_vec();
            p.dedup();
            arg_conv(orig, ctx, &p)
        }
        other => Err(format!("ключевое слово «{other}» не поддерживается")),
    }
}

fn solve_op(op: &SymOp, cur: Sx, ctx: &Ctx, protect: &[String], note: &mut Option<String>) -> Result<Sx, String> {
    let _ = (ctx, protect);
    let vars: Vec<Sx> = match op.args.first() {
        Some(e) => var_list(e)?,
        None => {
            let mut s = std::collections::BTreeSet::new();
            cur.symbols(&mut s);
            if s.len() != 1 {
                return Err("solve: укажите переменную, например «solve, x»".into());
            }
            vec![Sx::Sym(s.into_iter().next().unwrap())]
        }
    };
    let eqs: Vec<Sx> = match &cur {
        Sx::Mat(m) => m.iter().flatten().cloned().collect(),
        other => vec![other.clone()],
    };
    let solved = if vars.len() == 1 && eqs.len() == 1 {
        solve::solve_one(&eqs[0], &vars[0])?
    } else {
        solve::solve_system(&eqs, &vars)?
    };
    if solved.note.is_some() {
        *note = solved.note.clone();
    }
    let t = solved.tuples;
    if t.is_empty() {
        return Err("решений нет".into());
    }
    Ok(if vars.len() == 1 {
        if t.len() == 1 {
            t[0][0].clone()
        } else {
            Sx::Mat(t.into_iter().map(|r| vec![r[0].clone()]).collect())
        }
    } else if t.len() == 1 {
        Sx::Mat(t[0].iter().map(|v| vec![v.clone()]).collect())
    } else {
        Sx::Mat(t)
    })
}

// ---------------------------------------------------------------- text helpers

/// Parses `src` (an expression, optionally with `->` keyword operations) and evaluates
/// it symbolically with no definitions; the result is printed back as source text.
pub fn eval_text(src: &str) -> Result<String, String> {
    let stmt = crate::parser::parse_statement(src).map_err(|e| e.msg)?;
    let (expr, ops) = match stmt {
        crate::ast::Stmt::Symbolic { expr, ops, .. } => (expr, ops),
        crate::ast::Stmt::Show(e) => (e, Vec::new()),
        other => return Err(format!("не символьное выражение: {other}")),
    };
    let defs = Defs::default();
    let ctx = Ctx { defs: &defs, numeric: None, value_of: None };
    match evaluate(&expr, &ops, &ctx)? {
        Outcome::Done(o) => Ok(o.result.to_string()),
        Outcome::Fallback(m) => Err(m),
    }
}
