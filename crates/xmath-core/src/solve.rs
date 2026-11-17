//! Mathcad solve blocks: `Given` / constraints / `Find`, `Minerr`, `Maximize`,
//! `Minimize`.
//!
//! The worksheet collects the constraint regions of a block and, when it meets
//! the closing call, rewrites it into `Find(__given(c1, c2, ...), x, y)`
//! (see [`inject`]). Because the constraints live inside the expression, a
//! function of a solve block (`f(a) := Find(x)`) re-solves them every time it
//! is called with the parameter bound.
//!
//! Numerics: Levenberg–Marquardt on scaled residuals (equalities `l - r`,
//! inequalities as one-sided violations) and Nelder–Mead with a quadratic
//! penalty for `Maximize` / `Minimize`. Everything runs in SI base units.

use crate::ast::{BinOp, Expr};
use crate::eval::{Evaluator, Scope};
use crate::units::Dim;
use crate::value::*;
use num_complex::Complex64 as C64;

pub const SOLVE_NAMES: &[&str] = &["Find", "Minerr", "Maximize", "Minimize"];
const GIVEN_CALL: &str = "__given";

pub fn is_solve_name(n: &str) -> bool {
    SOLVE_NAMES.contains(&n)
}

/// True if `e` contains a call to Find/Minerr/Maximize/Minimize.
pub fn contains_solve(e: &Expr) -> bool {
    let mut found = false;
    walk(e, &mut |x| {
        if let Expr::Call(n, _) = x {
            if is_solve_name(n) {
                found = true;
            }
        }
    });
    found
}

/// Names given as arguments of the first solve call in `e` (the unknowns).
pub fn solve_call_vars(e: &Expr) -> Vec<String> {
    let mut out = Vec::new();
    let mut done = false;
    walk(e, &mut |x| {
        if let (false, Expr::Call(n, args)) = (done, x) {
            if is_solve_name(n) {
                done = true;
                for a in args {
                    if let Expr::Var(v) = a {
                        out.push(v.clone());
                    }
                }
            }
        }
    });
    out
}

fn walk(e: &Expr, f: &mut dyn FnMut(&Expr)) {
    f(e);
    match e {
        Expr::Unary(_, a) | Expr::Abs(a) | Expr::Vectorize(a) => walk(a, f),
        Expr::MatCol(a, b) | Expr::OnError(a, b) => {
            walk(a, f);
            walk(b, f);
        }
        Expr::Binary(_, a, b) => {
            walk(a, f);
            walk(b, f);
        }
        Expr::Call(_, args) => args.iter().for_each(|a| walk(a, f)),
        Expr::Index(b, idx) => {
            walk(b, f);
            idx.iter().for_each(|a| walk(a, f));
        }
        Expr::Matrix(rows) => rows.iter().flatten().for_each(|a| walk(a, f)),
        Expr::Range(a, b, c) => {
            walk(a, f);
            if let Some(b) = b {
                walk(b, f);
            }
            walk(c, f);
        }
        _ => {}
    }
}

/// Rewrites every solve call in `e` so that it carries the block constraints.
pub fn inject(e: &Expr, cs: &[Expr]) -> Expr {
    let r = |x: &Expr| Box::new(inject(x, cs));
    match e {
        Expr::Call(n, args) if is_solve_name(n) => {
            let mut v = vec![Expr::Call(GIVEN_CALL.into(), cs.to_vec())];
            v.extend(args.iter().cloned());
            Expr::Call(n.clone(), v)
        }
        Expr::Call(n, args) => Expr::Call(n.clone(), args.iter().map(|a| inject(a, cs)).collect()),
        Expr::Unary(op, a) => Expr::Unary(*op, r(a)),
        Expr::Abs(a) => Expr::Abs(r(a)),
        Expr::Vectorize(a) => Expr::Vectorize(r(a)),
        Expr::MatCol(a, b) => Expr::MatCol(r(a), r(b)),
        Expr::OnError(a, b) => Expr::OnError(r(a), r(b)),
        Expr::Binary(op, a, b) => Expr::Binary(*op, r(a), r(b)),
        Expr::Index(b, idx) => Expr::Index(r(b), idx.iter().map(|a| inject(a, cs)).collect()),
        Expr::Matrix(rows) => Expr::Matrix(rows.iter().map(|row| row.iter().map(|a| inject(a, cs)).collect()).collect()),
        Expr::Range(a, b, c) => Expr::Range(r(a), b.as_ref().map(|b| r(b)), r(c)),
        other => other.clone(),
    }
}

/// A `Show` region that is a Boolean comparison is a constraint.
pub fn is_constraint(e: &Expr) -> bool {
    match e {
        Expr::Binary(BinOp::Eq | BinOp::Lt | BinOp::Gt | BinOp::Le | BinOp::Ge, _, _) => true,
        Expr::Binary(BinOp::And, a, b) => is_constraint(a) && is_constraint(b),
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// Constraint model

#[derive(Clone, Copy, PartialEq)]
enum Rel {
    Eq,
    Le,
    Ge,
}

struct Constraint<'a> {
    rel: Rel,
    l: &'a Expr,
    r: &'a Expr,
}

fn flatten_constraints<'a>(e: &'a Expr, out: &mut Vec<Constraint<'a>>) -> EResult<()> {
    match e {
        Expr::Binary(BinOp::And, a, b) => {
            flatten_constraints(a, out)?;
            flatten_constraints(b, out)
        }
        Expr::Binary(op, l, r) => {
            let rel = match op {
                BinOp::Eq => Rel::Eq,
                BinOp::Le | BinOp::Lt => Rel::Le,
                BinOp::Ge | BinOp::Gt => Rel::Ge,
                _ => return Err(format!("недопустимое ограничение: {e}")),
            };
            out.push(Constraint { rel, l, r });
            Ok(())
        }
        _ => Err(format!("недопустимое ограничение: {e}")),
    }
}

fn components(v: &Value) -> EResult<Vec<Num>> {
    match v {
        Value::Num(n) => Ok(vec![*n]),
        Value::Matrix(m) => Ok(m.data.clone()),
        other => Err(format!("в ограничении недопустимое значение: {}", other.type_name())),
    }
}

struct Unknown {
    name: String,
    dim: Dim,
    complex: bool,
    scale: f64,
}

struct Problem<'a> {
    ev: &'a Evaluator,
    sc: &'a Scope,
    unknowns: Vec<Unknown>,
    cons: Vec<Constraint<'a>>,
    /// Scales of the residual components (fixed at the initial point).
    res_scale: std::cell::RefCell<Option<Vec<f64>>>,
    /// One common scale for all residuals (least squares must not reweight the equations).
    uniform: bool,
}

impl<'a> Problem<'a> {
    fn values(&self, z: &[f64]) -> Vec<Num> {
        let mut k = 0;
        self.unknowns
            .iter()
            .map(|u| {
                let re = z[k] * u.scale;
                let im = if u.complex {
                    k += 1;
                    z[k] * u.scale
                } else {
                    0.0
                };
                k += 1;
                Num::new(C64::new(re, im), u.dim)
            })
            .collect()
    }

    fn scope(&self, z: &[f64]) -> Scope {
        let mut sc = self.sc.clone();
        for (u, v) in self.unknowns.iter().zip(self.values(z)) {
            sc = sc.with(&u.name, Value::Num(v));
        }
        sc
    }

    /// Raw residual components with their magnitudes: (violation, magnitude).
    fn raw(&self, z: &[f64]) -> EResult<Vec<(f64, f64)>> {
        let sc = self.scope(z);
        let mut out = Vec::new();
        for c in &self.cons {
            let l = components(&self.ev.eval(c.l, &sc)?)?;
            let r = components(&self.ev.eval(c.r, &sc)?)?;
            let n = l.len().max(r.len());
            if (l.len() != n && l.len() != 1) || (r.len() != n && r.len() != 1) {
                return Err("в ограничении слева и справа разное число элементов".into());
            }
            for k in 0..n {
                let a = l[if l.len() == 1 { 0 } else { k }];
                let b = r[if r.len() == 1 { 0 } else { k }];
                if a.dim != b.dim {
                    return Err("в ограничении левая и правая части имеют разные единицы измерения".into());
                }
                let mag = a.v.norm().max(b.v.norm());
                match c.rel {
                    Rel::Eq => {
                        let d = a.v - b.v;
                        out.push((d.re, mag));
                        // keep the component count stable: complex unknowns always add the imaginary part
                        out.push((d.im, mag));
                    }
                    Rel::Le | Rel::Ge => {
                        if a.v.im != 0.0 || b.v.im != 0.0 {
                            return Err("неравенства определены только для вещественных значений".into());
                        }
                        let d = if c.rel == Rel::Le { a.v.re - b.v.re } else { b.v.re - a.v.re };
                        out.push((d.max(0.0), mag));
                    }
                }
            }
        }
        Ok(out)
    }

    /// Scaled residuals; the scales are fixed on the first call.
    fn residuals(&self, z: &[f64]) -> EResult<Vec<f64>> {
        let raw = self.raw(z)?;
        let mut sc = self.res_scale.borrow_mut();
        if sc.is_none() {
            let per: Vec<f64> = raw.iter().map(|(_, m)| if *m > 1e-300 { *m } else { 1.0 }).collect();
            *sc = Some(if self.uniform {
                let mx = raw.iter().fold(0.0_f64, |a, (_, m)| a.max(*m));
                vec![if mx > 1e-300 { mx } else { 1.0 }; per.len()]
            } else {
                per
            });
        }
        let s = sc.as_ref().unwrap();
        if s.len() != raw.len() {
            return Err("число условий изменилось в процессе решения".into());
        }
        let r: Vec<f64> = raw.iter().zip(s).map(|((v, _), s)| v / s).collect();
        if r.iter().any(|x| !x.is_finite()) {
            return Err("ограничение не определено в этой точке".into());
        }
        Ok(r)
    }

    fn z0(&self) -> Vec<f64> {
        let mut z = Vec::new();
        for u in &self.unknowns {
            let g = self.ev.lookup_def(&u.name, self.sc).and_then(|v| v.as_num().ok()).unwrap_or_else(Num::zero);
            z.push(g.v.re / u.scale);
            if u.complex {
                z.push(g.v.im / u.scale);
            }
        }
        z
    }

    fn result(&self, z: &[f64]) -> Value {
        let vals = self.values(z);
        if vals.len() == 1 {
            Value::Num(vals[0])
        } else {
            Value::Matrix(Matrix::column(vals))
        }
    }
}

// ---------------------------------------------------------------------------
// Numerics

fn max_abs(v: &[f64]) -> f64 {
    v.iter().fold(0.0, |a, x| a.max(x.abs()))
}

fn cost(r: &[f64]) -> f64 {
    0.5 * r.iter().map(|x| x * x).sum::<f64>()
}

fn solve_linear(mut a: Vec<Vec<f64>>, mut b: Vec<f64>) -> Option<Vec<f64>> {
    let n = b.len();
    for i in 0..n {
        let p = (i..n).max_by(|&x, &y| a[x][i].abs().total_cmp(&a[y][i].abs()))?;
        if a[p][i].abs() < 1e-300 {
            return None;
        }
        a.swap(i, p);
        b.swap(i, p);
        for j in i + 1..n {
            let f = a[j][i] / a[i][i];
            for k in i..n {
                a[j][k] -= f * a[i][k];
            }
            b[j] -= f * b[i];
        }
    }
    let mut x = vec![0.0; n];
    for i in (0..n).rev() {
        let s: f64 = (i + 1..n).map(|k| a[i][k] * x[k]).sum();
        x[i] = (b[i] - s) / a[i][i];
    }
    x.iter().all(|v| v.is_finite()).then_some(x)
}

/// Levenberg–Marquardt. `f` returns `None` where the residuals cannot be computed.
fn levenberg_marquardt(f: &dyn Fn(&[f64]) -> Option<Vec<f64>>, z0: &[f64]) -> Option<(Vec<f64>, Vec<f64>)> {
    let n = z0.len();
    let mut z = z0.to_vec();
    let mut r = f(&z)?;
    let mut c = cost(&r);
    let mut lambda = 1e-3;
    for _ in 0..400 {
        if max_abs(&r) < 1e-14 {
            break;
        }
        let m = r.len();
        let mut jac = vec![vec![0.0; n]; m];
        for j in 0..n {
            let h = 1e-6 * z[j].abs().max(1.0);
            let (mut zp, mut zm) = (z.clone(), z.clone());
            zp[j] += h;
            zm[j] -= h;
            match (f(&zp), f(&zm)) {
                (Some(rp), Some(rm)) => (0..m).for_each(|i| jac[i][j] = (rp[i] - rm[i]) / (2.0 * h)),
                (Some(rp), None) => (0..m).for_each(|i| jac[i][j] = (rp[i] - r[i]) / h),
                (None, Some(rm)) => (0..m).for_each(|i| jac[i][j] = (r[i] - rm[i]) / h),
                _ => {}
            }
        }
        let mut a = vec![vec![0.0; n]; n];
        let mut g = vec![0.0; n];
        for i in 0..m {
            for p in 0..n {
                g[p] += jac[i][p] * r[i];
                for q in 0..n {
                    a[p][q] += jac[i][p] * jac[i][q];
                }
            }
        }
        if max_abs(&g) < 1e-16 {
            break;
        }
        let mut accepted = false;
        for _ in 0..40 {
            let mut damped = a.clone();
            for p in 0..n {
                damped[p][p] += lambda * a[p][p].max(1e-12);
            }
            if let Some(d) = solve_linear(damped, g.iter().map(|x| -x).collect()) {
                let zt: Vec<f64> = z.iter().zip(&d).map(|(a, b)| a + b).collect();
                if let Some(rt) = f(&zt) {
                    let ct = cost(&rt);
                    if ct < c {
                        let step = max_abs(&d);
                        let small = step < 1e-13 * (1.0 + max_abs(&z)) || (c - ct) < 1e-18 * c.max(1e-300);
                        z = zt;
                        r = rt;
                        c = ct;
                        lambda = (lambda / 5.0).max(1e-14);
                        accepted = true;
                        if small {
                            return Some((z, r));
                        }
                        break;
                    }
                }
            }
            lambda *= 5.0;
            if lambda > 1e14 {
                break;
            }
        }
        if !accepted {
            break;
        }
    }
    Some((z, r))
}

/// Nelder–Mead minimisation with restarts.
fn nelder_mead(f: &dyn Fn(&[f64]) -> f64, start: &[f64], rounds: usize) -> Vec<f64> {
    let n = start.len();
    let mut best = start.to_vec();
    let mut fbest = f(&best);
    for _ in 0..rounds {
        let mut simplex: Vec<(Vec<f64>, f64)> = vec![(best.clone(), fbest)];
        for i in 0..n {
            let mut p = best.clone();
            p[i] += 0.1 * best[i].abs().max(1.0);
            let fp = f(&p);
            simplex.push((p, fp));
        }
        for _ in 0..(1500 * n).max(1500) {
            simplex.sort_by(|a, b| a.1.total_cmp(&b.1));
            let spread = (simplex[n].1 - simplex[0].1).abs();
            let size = simplex
                .iter()
                .map(|s| max_abs(&s.0.iter().zip(&simplex[0].0).map(|(a, b)| a - b).collect::<Vec<_>>()))
                .fold(0.0, f64::max);
            if spread <= 1e-16 * (1.0 + simplex[0].1.abs()) && size < 1e-10 {
                break;
            }
            let cen: Vec<f64> = (0..n).map(|j| simplex[..n].iter().map(|s| s.0[j]).sum::<f64>() / n as f64).collect();
            let at = |t: f64| -> Vec<f64> { (0..n).map(|j| cen[j] + t * (simplex[n].0[j] - cen[j])).collect() };
            let xr = at(-1.0);
            let fr = f(&xr);
            if fr < simplex[0].1 {
                let xe = at(-2.0);
                let fe = f(&xe);
                simplex[n] = if fe < fr { (xe, fe) } else { (xr, fr) };
            } else if fr < simplex[n - 1].1 {
                simplex[n] = (xr, fr);
            } else {
                let x = at(if fr < simplex[n].1 { -0.5 } else { 0.5 });
                let fc = f(&x);
                if fc < fr.min(simplex[n].1) {
                    simplex[n] = (x, fc);
                } else {
                    let b = simplex[0].0.clone();
                    for s in simplex.iter_mut().skip(1) {
                        for j in 0..n {
                            s.0[j] = b[j] + 0.5 * (s.0[j] - b[j]);
                        }
                        s.1 = f(&s.0);
                    }
                }
            }
        }
        simplex.sort_by(|a, b| a.1.total_cmp(&b.1));
        let improved = fbest - simplex[0].1;
        let old = fbest;
        best = simplex[0].0.clone();
        fbest = simplex[0].1;
        if improved <= 1e-15 * (1.0 + old.abs()) {
            break;
        }
    }
    best
}

// ---------------------------------------------------------------------------
// Evaluator entry point

const OUTSIDE: &str = "можно использовать только в блоке решения: добавьте выше область Given и ограничения";

impl Evaluator {
    /// A name bound by a worksheet definition or a local binding (not a unit/constant).
    pub(crate) fn lookup_def(&self, name: &str, sc: &Scope) -> Option<Value> {
        if let Some((_, v)) = sc.locals.iter().rev().find(|(n, _)| n == name) {
            return Some(v.clone());
        }
        self.defs[..sc.limit].iter().rev().find(|(n, _)| n == name).map(|(_, v)| v.clone())
    }

    fn tolerance(&self, name: &str, sc: &Scope) -> f64 {
        self.lookup(name, sc).and_then(|v| v.as_dimless_real().ok()).filter(|t| *t > 0.0).unwrap_or(1e-3)
    }

    pub(crate) fn solve_block(&self, name: &str, args: &[Expr], sc: &Scope) -> EResult<Value> {
        let optimize = matches!(name, "Maximize" | "Minimize");
        let (cs, rest): (&[Expr], &[Expr]) = match args.split_first() {
            Some((Expr::Call(g, cs), rest)) if g == GIVEN_CALL => (cs, rest),
            // Maximize / Minimize also work without constraints
            _ if optimize => (&[], args),
            _ => return Err(format!("{name} {OUTSIDE}")),
        };
        let (objective, var_args) = if optimize {
            match rest.split_first() {
                Some((o, v)) if !v.is_empty() => (Some(o), v),
                _ => return Err(format!("{name}(f, x, …) принимает функцию и хотя бы одну переменную")),
            }
        } else {
            (None, rest)
        };
        if var_args.is_empty() {
            return Err(format!("{name}: укажите неизвестные, например {name}(x, y)"));
        }
        let mut unknowns = Vec::new();
        let mut missing: Option<String> = None;
        let guess_error = |n: &str| format!("для '{n}' нужно начальное приближение, заданное выше блока решения (Given)");
        for a in var_args {
            let Expr::Var(n) = a else {
                return Err(format!("{name}: неизвестная должна быть именем переменной"));
            };
            let g = match self.lookup_def(n, sc) {
                Some(Value::Num(g)) => g,
                Some(other) => {
                    return Err(format!("начальное приближение '{n}' должно быть числом, найдено: {}", other.type_name()))
                }
                // Mathcad solves linear systems without guesses: try 0 and report the
                // missing guess only if that does not work
                None if !optimize => {
                    missing.get_or_insert_with(|| n.clone());
                    Num::zero()
                }
                None => return Err(guess_error(n)),
            };
            let complex = g.v.im != 0.0;
            let mag = if complex { g.v.re.abs().max(g.v.im.abs()) } else { g.v.re.abs() };
            unknowns.push(Unknown { name: n.clone(), dim: g.dim, complex, scale: if mag > 0.0 { mag } else { 1.0 } });
        }
        let mut cons = Vec::new();
        for c in cs {
            flatten_constraints(c, &mut cons)?;
        }
        let p = Problem { ev: self, sc, unknowns, cons, res_scale: Default::default(), uniform: name != "Find" };
        let tol = self.tolerance("CTOL", sc);
        let z0 = p.z0();
        // validate at the initial point so that errors are reported verbatim
        let r0 = p.residuals(&z0).map_err(|e| missing.as_deref().map_or(e, guess_error))?;
        if !optimize && r0.is_empty() {
            return Err("блок решения не содержит ограничений".into());
        }
        if let Some(obj) = objective {
            return self.optimize(name, obj, &p, &z0, tol);
        }
        let f = |z: &[f64]| p.residuals(z).ok();
        let (z, r) = levenberg_marquardt(&f, &z0).ok_or_else(|| "не удалось вычислить ограничения".to_string())?;
        if name == "Find" && max_abs(&r) > tol {
            if let Some(n) = &missing {
                return Err(guess_error(n));
            }
            return Err("Решение не найдено. Попробуйте другие начальные приближения или используйте Minerr".into());
        }
        Ok(p.result(&z))
    }

    fn optimize(&self, name: &str, obj: &Expr, p: &Problem, z0: &[f64], tol: f64) -> EResult<Value> {
        let sign = if name == "Maximize" { -1.0 } else { 1.0 };
        let func = match obj {
            Expr::Var(n) => match self.lookup(n, p.sc) {
                Some(Value::Func(f)) => Some(f),
                _ => None,
            },
            _ => None,
        };
        let f_at = |z: &[f64]| -> EResult<f64> {
            let v = match &func {
                Some(f) => self.call_value(f, p.values(z).into_iter().map(Value::Num).collect())?,
                None => self.eval(obj, &p.scope(z))?,
            };
            let n = v.as_num()?;
            if !n.is_real() {
                return Err("целевая функция должна быть вещественной".into());
            }
            Ok(n.v.re)
        };
        let f0 = f_at(z0)?;
        let fs = if f0.abs() > 1e-300 { f0.abs() } else { 1.0 };
        let viol = |z: &[f64]| -> Option<f64> { p.residuals(z).ok().map(|r| r.iter().map(|x| x * x).sum::<f64>()) };
        let mut z = z0.to_vec();
        for mu in [10.0, 1e3, 1e5, 1e7, 1e9] {
            let phi = |z: &[f64]| -> f64 {
                match (f_at(z), viol(z)) {
                    (Ok(f), Some(v)) if f.is_finite() => sign * (f - f0) / fs + mu * v,
                    _ => 1e300,
                }
            };
            z = nelder_mead(&phi, &z, 6);
            if p.cons.is_empty() {
                break;
            }
        }
        if !p.cons.is_empty() {
            let r = p.residuals(&z)?;
            if max_abs(&r) > tol {
                return Err("Не удалось удовлетворить ограничения. Попробуйте другие начальные приближения".into());
            }
        }
        Ok(p.result(&z))
    }
}
