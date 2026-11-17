//! Expression evaluation in Mathcad order: every region sees the definitions
//! made above / to the left of it.

use crate::ast::{BinOp, Expr, Target, UnOp};
use crate::builtins;
use crate::units::{self, Dim};
use crate::value::*;
use num_complex::Complex64 as C64;
use std::cell::Cell;
use std::rc::Rc;

/// Maximum nesting of user-function calls (protects against runaway recursion).
pub const MAX_CALL_DEPTH: usize = 2000;

/// Lexical scope: how many worksheet definitions are visible, plus local bindings
/// (function parameters, iteration variables).
#[derive(Clone, Debug)]
pub struct Scope {
    pub limit: usize,
    pub locals: Rc<Vec<(String, Value)>>,
}

impl Scope {
    pub fn with(&self, name: &str, v: Value) -> Scope {
        let mut locals = (*self.locals).clone();
        locals.push((name.to_string(), v));
        Scope { limit: self.limit, locals: Rc::new(locals) }
    }
}

#[derive(Default)]
pub struct Evaluator {
    /// All worksheet definitions, in evaluation order (append-only).
    pub defs: Vec<(String, Value)>,
    depth: Cell<usize>,
}

pub const PI: f64 = std::f64::consts::PI;

/// Built-in constants (looked up after user definitions, before units).
pub fn constant(name: &str) -> Option<Num> {
    Some(match name {
        "π" | "pi" => Num::real(PI),
        "e" => Num::real(std::f64::consts::E),
        "∞" => Num::real(1e307),
        "g" => Num::with_dim(9.806_65, units::ACCELERATION),
        "c" => Num::with_dim(299_792_458.0, units::VELOCITY),
        "TOL" => Num::real(1e-3),
        "CTOL" => Num::real(1e-3),
        "ORIGIN" => Num::real(0.0),
        _ => return None,
    })
}

/// Names of special forms whose first argument is a bound variable.
const BINDING_FORMS: &[&str] = &["sum", "prod", "integrate", "diff", "root", "Σ", "Π", "∫"];

impl Evaluator {
    pub fn new() -> Evaluator {
        Evaluator::default()
    }

    pub fn top_scope(&self) -> Scope {
        Scope { limit: self.defs.len(), locals: Rc::new(Vec::new()) }
    }

    pub fn lookup(&self, name: &str, sc: &Scope) -> Option<Value> {
        if let Some((_, v)) = sc.locals.iter().rev().find(|(n, _)| n == name) {
            return Some(v.clone());
        }
        if let Some((_, v)) = self.defs[..sc.limit].iter().rev().find(|(n, _)| n == name) {
            return Some(v.clone());
        }
        if let Some(c) = constant(name) {
            return Some(Value::Num(c));
        }
        units::lookup(name).map(|u| Value::Num(Num::with_dim(u.factor, u.dim)))
    }

    fn origin(&self, sc: &Scope) -> i64 {
        self.lookup("ORIGIN", sc).and_then(|v| v.as_int().ok()).unwrap_or(0)
    }

    pub fn eval(&self, e: &Expr, sc: &Scope) -> EResult<Value> {
        match e {
            Expr::Num(x) => Ok(Value::real(*x)),
            Expr::Imag(x) => Ok(Value::Num(Num::new(C64::new(0.0, *x), Dim::NONE))),
            Expr::Str(s) => Ok(Value::Str(s.as_str().into())),
            Expr::Var(n) => self.lookup(n, sc).ok_or_else(|| format!("'{n}' is not defined")),
            Expr::Unary(op, a) => {
                let v = self.eval(a, sc)?;
                match op {
                    UnOp::Neg => neg(&v),
                    UnOp::Not => Ok(Value::bool(!v.truthy()?)),
                    UnOp::Factorial => builtins::call("factorial", &[v], 0).unwrap(),
                }
            }
            Expr::Binary(op, a, b) => {
                match op {
                    BinOp::And => {
                        return Ok(Value::bool(self.eval(a, sc)?.truthy()? && self.eval(b, sc)?.truthy()?));
                    }
                    BinOp::Or => {
                        return Ok(Value::bool(self.eval(a, sc)?.truthy()? || self.eval(b, sc)?.truthy()?));
                    }
                    _ => {}
                }
                let x = self.eval(a, sc)?;
                let y = self.eval(b, sc)?;
                match op {
                    BinOp::Add => add(&x, &y),
                    BinOp::Sub => sub(&x, &y),
                    BinOp::Mul => mul(&x, &y),
                    BinOp::Div => div(&x, &y),
                    BinOp::Pow => pow(&x, &y),
                    _ => cmp_op(&x, &y, *op),
                }
            }
            Expr::Call(name, args) => self.call(name, args, sc),
            Expr::Index(base, idx) => {
                let b = self.eval(base, sc)?;
                let m = match &b {
                    Value::Matrix(m) => m,
                    Value::Range(_) => return Err("a range variable cannot be indexed; use a vector".into()),
                    other => return Err(format!("only vectors and matrices can be indexed, found {}", other.type_name())),
                };
                let origin = self.origin(sc);
                let ix = idx
                    .iter()
                    .map(|i| self.eval(i, sc)?.as_int().map(|k| k - origin))
                    .collect::<EResult<Vec<i64>>>()?;
                let out_of_range = || "index out of range".to_string();
                match ix.as_slice() {
                    [k] => {
                        if !m.is_vector() {
                            return Err("a matrix needs two indices".into());
                        }
                        let k = usize::try_from(*k).map_err(|_| out_of_range())?;
                        m.data.get(k).map(|n| Value::Num(*n)).ok_or_else(out_of_range)
                    }
                    [r, c] => {
                        let r = usize::try_from(*r).map_err(|_| out_of_range())?;
                        let c = usize::try_from(*c).map_err(|_| out_of_range())?;
                        if r >= m.rows || c >= m.cols {
                            return Err(out_of_range());
                        }
                        Ok(Value::Num(m.get(r, c)))
                    }
                    _ => Err("too many indices".into()),
                }
            }
            Expr::Matrix(rows) => {
                let nrows = rows.len();
                let ncols = rows[0].len();
                // Allow block matrices made of scalars only; vectors as elements
                // of a single row/column are concatenated.
                let mut data = Vec::with_capacity(nrows * ncols);
                for row in rows {
                    for cell in row {
                        match self.eval(cell, sc)? {
                            Value::Num(n) => data.push(n),
                            other => {
                                return Err(format!("matrix elements must be scalars, found {}", other.type_name()))
                            }
                        }
                    }
                }
                Ok(Value::Matrix(Matrix::new(nrows, ncols, data)))
            }
            Expr::Range(first, second, last) => {
                let a = self.eval(first, sc)?.as_num()?;
                let c = self.eval(last, sc)?.as_num()?;
                if !a.is_real() || !c.is_real() {
                    return Err("range limits must be real".into());
                }
                if a.dim != c.dim {
                    return Err("range limits have different units".into());
                }
                let step = match second {
                    Some(b) => {
                        let b = self.eval(b, sc)?.as_num()?;
                        if b.dim != a.dim {
                            return Err("range limits have different units".into());
                        }
                        b.v.re - a.v.re
                    }
                    None => {
                        if c.v.re >= a.v.re {
                            1.0
                        } else {
                            -1.0
                        }
                    }
                };
                if step == 0.0 || !step.is_finite() {
                    return Err("the range step must not be zero".into());
                }
                let r = Range { start: a.v.re, step, end: c.v.re, dim: a.dim };
                if r.len() > MAX_RANGE_LEN {
                    return Err(format!("the range has too many values (more than {MAX_RANGE_LEN})"));
                }
                Ok(Value::Range(r))
            }
            Expr::Abs(a) => match self.eval(a, sc)? {
                Value::Num(n) => Ok(Value::Num(Num::new(C64::new(n.v.norm(), 0.0), n.dim))),
                Value::Matrix(m) if m.cols == 1 || m.rows == 1 => {
                    let d = m.uniform_dim()?;
                    let s: f64 = m.data.iter().map(|n| n.v.norm_sqr()).sum();
                    Ok(Value::Num(Num::with_dim(s.sqrt(), d)))
                }
                Value::Matrix(m) => Ok(Value::Num(det(&m)?)),
                Value::Str(s) => Ok(Value::real(s.chars().count() as f64)),
                other => Err(format!("cannot take the absolute value of {}", other.type_name())),
            },
        }
    }

    /// Calls a function value with already evaluated arguments.
    pub fn call_value(&self, f: &UserFunc, args: Vec<Value>) -> EResult<Value> {
        if args.len() != f.params.len() {
            return Err(format!(
                "'{}' takes {} argument{}, but {} were given",
                f.name,
                f.params.len(),
                if f.params.len() == 1 { "" } else { "s" },
                args.len()
            ));
        }
        let d = self.depth.get();
        if d >= MAX_CALL_DEPTH {
            return Err("recursion is too deep".into());
        }
        let mut locals = (*f.locals).clone();
        for (p, a) in f.params.iter().zip(args) {
            locals.push((p.clone(), a));
        }
        let sc = Scope { limit: f.limit, locals: Rc::new(locals) };
        self.depth.set(d + 1);
        let r = self.eval(&f.body, &sc);
        self.depth.set(d);
        r
    }

    fn eval_args(&self, args: &[Expr], sc: &Scope) -> EResult<Vec<Value>> {
        args.iter().map(|a| self.eval(a, sc)).collect()
    }

    fn call(&self, name: &str, args: &[Expr], sc: &Scope) -> EResult<Value> {
        // user-defined functions (and function-valued parameters) come first
        let user = self.lookup(name, sc);
        if let Some(Value::Func(f)) = &user {
            let vals = self.eval_args(args, sc)?;
            return self.call_value(f, vals);
        }
        if let Some(r) = self.special_form(name, args, sc) {
            return r;
        }
        let vals = self.eval_args(args, sc)?;
        if name == "matrix" {
            return self.builtin_matrix(&vals);
        }
        if let Some(r) = builtins::call(name, &vals, self.origin(sc)) {
            return r.map_err(|e| format!("{name}: {e}"));
        }
        // `x(a + b)` with a plain variable `x` means multiplication
        if let Some(v) = user {
            if vals.len() == 1 {
                return mul(&v, &vals[0]);
            }
        }
        Err(format!("'{name}' is not a defined function"))
    }

    fn builtin_matrix(&self, vals: &[Value]) -> EResult<Value> {
        if vals.len() != 3 {
            return Err("matrix(m, n, f) takes 3 arguments".into());
        }
        let m = vals[0].as_int()?;
        let n = vals[1].as_int()?;
        let Value::Func(f) = &vals[2] else {
            return Err("matrix: the third argument must be a function f(i, j)".into());
        };
        if m <= 0 || n <= 0 || m * n > MAX_RANGE_LEN as i64 {
            return Err("matrix: invalid size".into());
        }
        let mut out = Matrix::zeros(m as usize, n as usize);
        for i in 0..m as usize {
            for j in 0..n as usize {
                let v = self.call_value(f, vec![Value::real(i as f64), Value::real(j as f64)])?;
                out.set(i, j, v.as_num()?);
            }
        }
        Ok(Value::Matrix(out))
    }

    fn bound_var<'a>(&self, e: &'a Expr, form: &str) -> EResult<&'a str> {
        match e {
            Expr::Var(n) => Ok(n),
            _ => Err(format!("{form}: the variable argument must be a name")),
        }
    }

    /// Operators that need unevaluated arguments.
    fn special_form(&self, name: &str, args: &[Expr], sc: &Scope) -> Option<EResult<Value>> {
        let r = match (name, args.len()) {
            ("if", 3) => (|| {
                if self.eval(&args[0], sc)?.truthy()? {
                    self.eval(&args[1], sc)
                } else {
                    self.eval(&args[2], sc)
                }
            })(),
            ("sum" | "Σ" | "prod" | "Π", 4) | ("sum" | "Σ" | "prod" | "Π", 2) => self.iterated(name, args, sc),
            ("integrate" | "∫", 4) => self.integrate(args, sc),
            ("diff", 2) | ("diff", 3) => self.diff(args, sc),
            ("root", 2) | ("root", 4) => self.root(args, sc),
            _ => return None,
        };
        Some(r.map_err(|e| if e.starts_with(name) { e } else { format!("{name}: {e}") }))
    }

    fn iterated(&self, name: &str, args: &[Expr], sc: &Scope) -> EResult<Value> {
        let is_sum = name == "sum" || name == "Σ";
        let var = self.bound_var(&args[0], name)?;
        let body = args.last().unwrap();
        let values: Vec<Value> = if args.len() == 4 {
            let a = self.eval(&args[1], sc)?.as_int()?;
            let b = self.eval(&args[2], sc)?.as_int()?;
            if b - a > MAX_RANGE_LEN as i64 {
                return Err("too many terms".into());
            }
            (a..=b).map(|k| Value::real(k as f64)).collect()
        } else {
            match self.lookup(var, sc) {
                Some(Value::Range(r)) => r.values().map(Value::Num).collect(),
                Some(Value::Matrix(m)) if m.is_vector() => m.data.iter().map(|n| Value::Num(*n)).collect(),
                _ => return Err(format!("'{var}' must be a range variable")),
            }
        };
        let mut acc: Option<Value> = None;
        for v in values {
            let t = self.eval(body, &sc.with(var, v))?;
            acc = Some(match acc {
                None => t,
                Some(a) if is_sum => add(&a, &t)?,
                Some(a) => mul(&a, &t)?,
            });
        }
        Ok(acc.unwrap_or_else(|| Value::real(if is_sum { 0.0 } else { 1.0 })))
    }

    fn scalar_fn<'a>(&'a self, body: &'a Expr, var: &'a str, dim: Dim, sc: &'a Scope) -> impl Fn(C64) -> EResult<Num> + 'a {
        move |x| self.eval(body, &sc.with(var, Value::Num(Num::new(x, dim))))?.as_num()
    }

    fn integrate(&self, args: &[Expr], sc: &Scope) -> EResult<Value> {
        let var = self.bound_var(&args[1], "integrate")?;
        let a = self.eval(&args[2], sc)?.as_num()?;
        let b = self.eval(&args[3], sc)?.as_num()?;
        if a.dim != b.dim {
            return Err("the limits have different units".into());
        }
        if !a.is_real() || !b.is_real() {
            return Err("the limits must be real".into());
        }
        let f = self.scalar_fn(&args[0], var, a.dim, sc);
        let out_dim = Cell::new(None::<Dim>);
        let g = |x: f64| -> EResult<C64> {
            let n = f(C64::new(x, 0.0))?;
            match out_dim.get() {
                None => out_dim.set(Some(n.dim)),
                Some(d) if d != n.dim => return Err("the integrand has inconsistent units".into()),
                _ => {}
            }
            Ok(n.v)
        };
        let v = crate::numeric::integrate(&g, a.v.re, b.v.re)?;
        let dim = out_dim.get().unwrap_or(Dim::NONE).mul(a.dim);
        Ok(Value::Num(Num::new(v, dim)))
    }

    fn diff(&self, args: &[Expr], sc: &Scope) -> EResult<Value> {
        let var = self.bound_var(&args[1], "diff")?;
        let at = match args.get(2) {
            Some(e) => self.eval(e, sc)?.as_num()?,
            None => self
                .lookup(var, sc)
                .ok_or_else(|| format!("'{var}' must be defined (it is the point where the derivative is taken)"))?
                .as_num()?,
        };
        let f = self.scalar_fn(&args[0], var, at.dim, sc);
        let dim = Cell::new(Dim::NONE);
        let g = |x: f64| -> EResult<C64> {
            let n = f(C64::new(x, at.v.im))?;
            dim.set(n.dim);
            Ok(n.v)
        };
        let v = crate::numeric::derivative(&g, at.v.re)?;
        Ok(Value::Num(Num::new(v, dim.get().div(at.dim))))
    }

    fn root(&self, args: &[Expr], sc: &Scope) -> EResult<Value> {
        let var = self.bound_var(&args[1], "root")?;
        if args.len() == 4 {
            let a = self.eval(&args[2], sc)?.as_num()?;
            let b = self.eval(&args[3], sc)?.as_num()?;
            if a.dim != b.dim {
                return Err("the bracket limits have different units".into());
            }
            let f = self.scalar_fn(&args[0], var, a.dim, sc);
            let g = |x: f64| -> EResult<f64> { Ok(f(C64::new(x, 0.0))?.v.re) };
            let x = crate::numeric::brent(&g, a.v.re, b.v.re)?;
            return Ok(Value::Num(Num::with_dim(x, a.dim)));
        }
        let guess = self
            .lookup(var, sc)
            .ok_or_else(|| format!("'{var}' needs a guess value defined above"))?
            .as_num()?;
        let f = self.scalar_fn(&args[0], var, guess.dim, sc);
        let g = |x: C64| -> EResult<C64> { Ok(f(x)?.v) };
        let x = crate::numeric::secant(&g, guess.v)?;
        Ok(Value::Num(Num::new(x, guess.dim)))
    }

    // -----------------------------------------------------------------------
    // Range-variable iteration

    /// Names in `e` that are bound to range variables in scope.
    pub fn range_vars(&self, exprs: &[&Expr], sc: &Scope) -> Vec<(String, Range)> {
        let mut names = Vec::new();
        for e in exprs {
            free_vars(e, &mut Vec::new(), &mut names);
        }
        let mut out: Vec<(String, Range)> = Vec::new();
        for n in names {
            if out.iter().any(|(m, _)| *m == n) {
                continue;
            }
            if let Some(Value::Range(r)) = self.lookup(&n, sc) {
                out.push((n, r));
            }
        }
        out
    }

    /// Calls `f` once for every combination of the range variables.
    pub fn for_each_binding(
        &self,
        ranges: &[(String, Range)],
        sc: &Scope,
        f: &mut dyn FnMut(&Scope) -> EResult<()>,
    ) -> EResult<()> {
        match ranges.split_first() {
            None => f(sc),
            Some(((name, r), rest)) => {
                for v in r.values() {
                    self.for_each_binding(rest, &sc.with(name, Value::Num(v)), f)?;
                }
                Ok(())
            }
        }
    }

    /// Evaluates an expression, iterating over any range variables it uses.
    /// With range variables the result is a column vector of all values.
    pub fn eval_iterated(&self, e: &Expr, sc: &Scope) -> EResult<Value> {
        let ranges = self.range_vars(&[e], sc);
        if ranges.is_empty() {
            return self.eval(e, sc);
        }
        let mut out = Vec::new();
        self.for_each_binding(&ranges, sc, &mut |s| {
            out.push(self.eval(e, s)?.as_num()?);
            if out.len() > MAX_RANGE_LEN {
                return Err("too many values".into());
            }
            Ok(())
        })?;
        Ok(Value::Matrix(Matrix::column(out)))
    }

    // -----------------------------------------------------------------------
    // Definitions

    pub fn define(&mut self, target: &Target, expr: &Expr) -> EResult<()> {
        let sc = self.top_scope();
        let (name, value) = match target {
            Target::Var(name) => {
                let ranges = self.range_vars(&[expr], &sc);
                if let Some((r, _)) = ranges.first() {
                    return Err(format!(
                        "the range variable '{r}' can only be used in an indexed definition such as {name}[{r}] := …"
                    ));
                }
                (name.clone(), self.eval(expr, &sc)?)
            }
            Target::Func(name, params) => {
                let f = UserFunc {
                    name: name.clone(),
                    params: params.clone(),
                    body: Rc::new(expr.clone()),
                    limit: self.defs.len() + 1, // the function can see itself (recursion)
                    locals: Rc::new(Vec::new()),
                };
                (name.clone(), Value::Func(Rc::new(f)))
            }
            Target::Indexed(name, idx) => {
                let mut m = match self.lookup(name, &sc) {
                    Some(Value::Matrix(m)) => m,
                    _ => Matrix::zeros(0, 0),
                };
                let origin = self.origin(&sc);
                let mut all: Vec<&Expr> = idx.iter().collect();
                all.push(expr);
                let ranges = self.range_vars(&all, &sc);
                self.for_each_binding(&ranges, &sc, &mut |s| {
                    let ix = idx
                        .iter()
                        .map(|i| self.eval(i, s)?.as_int().map(|k| k - origin))
                        .collect::<EResult<Vec<i64>>>()?;
                    let v = self.eval(expr, s)?.as_num()?;
                    let (r, c) = match ix.as_slice() {
                        [r] => (*r, 0),
                        [r, c] => (*r, *c),
                        _ => return Err("too many indices".into()),
                    };
                    if r < 0 || c < 0 {
                        return Err("index out of range".into());
                    }
                    if (r as usize + 1) * (c as usize + 1) > MAX_RANGE_LEN {
                        return Err("the matrix would be too large".into());
                    }
                    if m.rows == 0 && c == 0 {
                        m = Matrix::zeros(0, 1);
                    }
                    m.ensure(r as usize, c as usize);
                    m.set(r as usize, c as usize, v);
                    Ok(())
                })?;
                (name.clone(), Value::Matrix(m))
            }
        };
        self.defs.push((name, value));
        Ok(())
    }
}

/// Collects free variable names (excluding variables bound by sum/integral/...).
pub fn free_vars(e: &Expr, bound: &mut Vec<String>, out: &mut Vec<String>) {
    match e {
        Expr::Var(n) => {
            if !bound.contains(n) {
                out.push(n.clone());
            }
        }
        Expr::Num(_) | Expr::Imag(_) | Expr::Str(_) => {}
        Expr::Unary(_, a) | Expr::Abs(a) => free_vars(a, bound, out),
        Expr::Binary(_, a, b) => {
            free_vars(a, bound, out);
            free_vars(b, bound, out);
        }
        Expr::Call(name, args) => {
            // `diff(f(x), x)` uses the current value of x, so x stays free there.
            let binding = BINDING_FORMS.contains(&name.as_str())
                && args.len() >= 2
                && !(name == "diff" && args.len() == 2);
            let var_pos = if name == "sum" || name == "prod" || name == "Σ" || name == "Π" { 0 } else { 1 };
            if binding {
                if let Some(Expr::Var(v)) = args.get(var_pos) {
                    bound.push(v.clone());
                    for (i, a) in args.iter().enumerate() {
                        if i != var_pos {
                            free_vars(a, bound, out);
                        }
                    }
                    bound.pop();
                    return;
                }
            }
            for a in args {
                free_vars(a, bound, out);
            }
        }
        Expr::Index(b, idx) => {
            free_vars(b, bound, out);
            for i in idx {
                free_vars(i, bound, out);
            }
        }
        Expr::Matrix(rows) => {
            for r in rows {
                for c in r {
                    free_vars(c, bound, out);
                }
            }
        }
        Expr::Range(a, b, c) => {
            free_vars(a, bound, out);
            if let Some(b) = b {
                free_vars(b, bound, out);
            }
            free_vars(c, bound, out);
        }
    }
}
