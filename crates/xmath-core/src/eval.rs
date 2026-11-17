//! Expression evaluation in Mathcad order: every region sees the definitions
//! made above / to the left of it.

use crate::ast::{BinOp, Expr, PStmt, Target, UnOp};
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

/// Maximum number of loop iterations in one program run.
pub const MAX_LOOP_ITERATIONS: u64 = 10_000_000;

pub struct Evaluator {
    /// All worksheet definitions, in evaluation order (append-only).
    pub defs: Vec<(String, Value)>,
    depth: Cell<usize>,
    /// Loop iteration limit of a single program run.
    pub max_iterations: u64,
}

impl Default for Evaluator {
    fn default() -> Self {
        Evaluator { defs: Vec::new(), depth: Cell::new(0), max_iterations: MAX_LOOP_ITERATIONS }
    }
}

/// How a statement block finished.
enum Flow {
    /// Normal end; the value of the last evaluated statement (if any).
    Normal(Option<Value>),
    Break,
    Continue,
    Return(Value),
}

/// Mutable state of one running program.
struct Frame {
    sc: Scope,
    /// Locals below this index belong to the enclosing scope and are never modified.
    base: usize,
    iters: u64,
}

impl Frame {
    fn local_pos(&self, name: &str) -> Option<usize> {
        self.sc.locals.iter().rposition(|(n, _)| n == name).filter(|i| *i >= self.base)
    }

    fn set(&mut self, name: &str, v: Value) {
        let pos = self.local_pos(name);
        let locals = Rc::make_mut(&mut self.sc.locals);
        match pos {
            Some(i) => locals[i].1 = v,
            None => locals.push((name.to_string(), v)),
        }
    }
}

/// Stores `v` at the (zero based) index `ix` of `m`, growing the matrix when needed.
fn set_element(m: &mut Matrix, ix: &[i64], v: Num) -> EResult<()> {
    let (r, c) = match ix {
        [r] => (*r, 0),
        [r, c] => (*r, *c),
        _ => return Err("слишком много индексов".into()),
    };
    if r < 0 || c < 0 {
        return Err("индекс вне допустимых границ".into());
    }
    if (r as usize + 1) * (c as usize + 1) > MAX_RANGE_LEN {
        return Err("матрица получилась бы слишком большой".into());
    }
    if m.rows == 0 && c == 0 {
        *m = Matrix::zeros(0, 1);
    }
    m.ensure(r as usize, c as usize);
    m.set(r as usize, c as usize, v);
    Ok(())
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

    /// The built-in offset unit (`°C`, `°F`) named by `e`, unless the user
    /// shadowed the name with a definition.
    fn offset_unit_of(&self, e: &Expr, sc: &Scope) -> Option<&'static units::UnitDef> {
        let Expr::Var(n) = e else { return None };
        let ud = units::offset_unit(n)?;
        match self.lookup(n, sc) {
            Some(Value::Num(v)) if v.dim == ud.dim && v.v.re == ud.factor && v.v.im == 0.0 => Some(ud),
            _ => None,
        }
    }

    /// `e` has the shape `<expr> °C` (implicit or explicit multiplication).
    fn is_offset_suffix(&self, e: &Expr, sc: &Scope) -> bool {
        matches!(e, Expr::Binary(BinOp::Mul, _, t) if self.offset_unit_of(t, sc).is_some())
    }

    /// `x * °C` with a dimensionless real scalar `x` -> `(x + offset) K`.
    fn offset_temperature(&self, x: &Value, t: &Expr, sc: &Scope) -> Option<Value> {
        let ud = self.offset_unit_of(t, sc)?;
        match x {
            Value::Num(n) if n.dim.is_none() && n.v.im == 0.0 => {
                Some(Value::Num(Num::with_dim((n.v.re + ud.offset) * ud.factor, ud.dim)))
            }
            _ => None,
        }
    }

    fn origin(&self, sc: &Scope) -> i64 {
        self.lookup("ORIGIN", sc).and_then(|v| v.as_int().ok()).unwrap_or(0)
    }

    pub fn eval(&self, e: &Expr, sc: &Scope) -> EResult<Value> {
        match e {
            Expr::Num(x) => Ok(Value::real(*x)),
            Expr::Imag(x) => Ok(Value::Num(Num::new(C64::new(0.0, *x), Dim::NONE))),
            Expr::Str(s) => Ok(Value::Str(s.as_str().into())),
            Expr::Var(n) => self.lookup(n, sc).ok_or_else(|| format!("'{n}' не определена")),
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
                // `x °C` / `x °F` (units.rs: offset units) is an absolute temperature;
                // as the left operand of * / ^ the offset unit behaves like K.
                let x = if matches!(op, BinOp::Mul | BinOp::Div | BinOp::Pow) && self.is_offset_suffix(a, sc) {
                    let Expr::Binary(_, v, t) = &**a else { unreachable!() };
                    mul(&self.eval(v, sc)?, &self.eval(t, sc)?)?
                } else {
                    self.eval(a, sc)?
                };
                let y = self.eval(b, sc)?;
                if matches!(op, BinOp::Mul) {
                    if let Some(t) = self.offset_temperature(&x, b, sc) {
                        return Ok(t);
                    }
                }
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
                    Value::Range(_) => return Err("дискретную переменную нельзя индексировать; используйте вектор".into()),
                    other => return Err(format!("индексировать можно только векторы и матрицы, найдено: {}", other.type_name())),
                };
                let origin = self.origin(sc);
                let ix = idx
                    .iter()
                    .map(|i| self.eval(i, sc)?.as_int().map(|k| k - origin))
                    .collect::<EResult<Vec<i64>>>()?;
                let out_of_range = || "индекс вне допустимых границ".to_string();
                match ix.as_slice() {
                    [k] => {
                        if !m.is_vector() {
                            return Err("для матрицы нужны два индекса".into());
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
                    _ => Err("слишком много индексов".into()),
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
                                return Err(format!("элементы матрицы должны быть скалярами, найдено: {}", other.type_name()))
                            }
                        }
                    }
                }
                Ok(Value::Matrix(Matrix::new(nrows, ncols, data)))
            }
            Expr::Range(first, second, last) => {
                let mut a = self.eval(first, sc)?.as_num()?;
                let mut c = self.eval(last, sc)?.as_num()?;
                unify_zero_dims(&mut a, &mut c);
                if !a.is_real() || !c.is_real() {
                    return Err("границы диапазона должны быть вещественными".into());
                }
                if a.dim != c.dim {
                    return Err("границы диапазона имеют разные единицы измерения".into());
                }
                let step = match second {
                    Some(b) => {
                        let mut b = self.eval(b, sc)?.as_num()?;
                        unify_zero_dims(&mut b, &mut a);
                        if b.dim != a.dim {
                            return Err("границы диапазона имеют разные единицы измерения".into());
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
                    return Err("шаг диапазона не может быть нулевым".into());
                }
                let r = Range { start: a.v.re, step, end: c.v.re, dim: a.dim };
                if r.len() > MAX_RANGE_LEN {
                    return Err(format!("в диапазоне слишком много значений (больше {MAX_RANGE_LEN})"));
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
                other => Err(format!("нельзя взять модуль: {}", other.type_name())),
            },
            Expr::OnError(a, b) => match self.eval(a, sc) {
                Ok(v) => Ok(v),
                Err(_) => self.eval(b, sc),
            },
            Expr::Program(stmts) => self.run_program(stmts, sc),
        }
    }

    // -----------------------------------------------------------------------
    // Programs

    fn run_program(&self, stmts: &[PStmt], sc: &Scope) -> EResult<Value> {
        let mut fr = Frame { sc: sc.clone(), base: sc.locals.len(), iters: 0 };
        match self.exec_block(stmts, &mut fr)? {
            Flow::Normal(Some(v)) | Flow::Return(v) => Ok(v),
            Flow::Normal(None) => Err("программа не вернула значения".into()),
            Flow::Break => Err("'break' вне цикла".into()),
            Flow::Continue => Err("'continue' вне цикла".into()),
        }
    }

    fn exec_block(&self, stmts: &[PStmt], fr: &mut Frame) -> EResult<Flow> {
        let mut last = None;
        for s in stmts {
            match self.exec_stmt(s, fr)? {
                Flow::Normal(Some(v)) => last = Some(v),
                Flow::Normal(None) => {}
                other => return Ok(other),
            }
        }
        Ok(Flow::Normal(last))
    }

    fn tick(&self, fr: &mut Frame) -> EResult<()> {
        fr.iters += 1;
        if fr.iters > self.max_iterations {
            return Err(format!("превышено допустимое число итераций цикла ({})", self.max_iterations));
        }
        Ok(())
    }

    fn exec_stmt(&self, s: &PStmt, fr: &mut Frame) -> EResult<Flow> {
        match s {
            PStmt::Expr(e) => Ok(Flow::Normal(Some(self.eval(e, &fr.sc)?))),
            PStmt::Assign(Target::Var(name), e) => {
                let v = self.eval(e, &fr.sc)?;
                fr.set(name, v.clone());
                Ok(Flow::Normal(Some(v)))
            }
            PStmt::Assign(Target::Indexed(name, idx), e) => {
                let origin = self.origin(&fr.sc);
                let ix = idx
                    .iter()
                    .map(|i| self.eval(i, &fr.sc)?.as_int().map(|k| k - origin))
                    .collect::<EResult<Vec<i64>>>()?;
                let v = self.eval(e, &fr.sc)?.as_num()?;
                match fr.local_pos(name) {
                    Some(i) => match &mut Rc::make_mut(&mut fr.sc.locals)[i].1 {
                        Value::Matrix(m) => set_element(m, &ix, v)?,
                        other => {
                            return Err(format!("'{name}': элементу нельзя присвоить значение, найдено: {}", other.type_name()))
                        }
                    },
                    None => {
                        let mut m = Matrix::zeros(0, 0);
                        set_element(&mut m, &ix, v)?;
                        fr.set(name, Value::Matrix(m));
                    }
                }
                Ok(Flow::Normal(Some(Value::Num(v))))
            }
            PStmt::Assign(Target::Func(..), _) => Err("в программе нельзя определять функции".into()),
            PStmt::If { cond, then, else_ } => {
                if self.eval(cond, &fr.sc)?.truthy()? {
                    self.exec_block(then, fr)
                } else if let Some(e) = else_ {
                    self.exec_block(e, fr)
                } else {
                    Ok(Flow::Normal(None))
                }
            }
            PStmt::While { cond, body } => {
                let mut last = None;
                while self.eval(cond, &fr.sc)?.truthy()? {
                    self.tick(fr)?;
                    match self.exec_block(body, fr)? {
                        Flow::Normal(v) => last = v.or(last),
                        Flow::Break => break,
                        Flow::Continue => {}
                        r @ Flow::Return(_) => return Ok(r),
                    }
                }
                Ok(Flow::Normal(last))
            }
            PStmt::For { var, iter, body } => {
                let items: Box<dyn Iterator<Item = Value>> = match self.eval(iter, &fr.sc)? {
                    Value::Range(r) => Box::new(r.values().map(Value::Num).collect::<Vec<_>>().into_iter()),
                    Value::Matrix(m) => Box::new(m.data.into_iter().map(Value::Num)),
                    Value::Num(n) => Box::new(std::iter::once(Value::Num(n))),
                    other => return Err(format!("цикл for: нельзя перебрать значения типа {}", other.type_name())),
                };
                let mut last = None;
                for v in items {
                    self.tick(fr)?;
                    fr.set(var, v);
                    match self.exec_block(body, fr)? {
                        Flow::Normal(v) => last = v.or(last),
                        Flow::Break => break,
                        Flow::Continue => {}
                        r @ Flow::Return(_) => return Ok(r),
                    }
                }
                Ok(Flow::Normal(last))
            }
            PStmt::Break => Ok(Flow::Break),
            PStmt::Continue => Ok(Flow::Continue),
            PStmt::Return(e) => Ok(Flow::Return(self.eval(e, &fr.sc)?)),
        }
    }

    /// Calls a function value with already evaluated arguments.
    pub fn call_value(&self, f: &UserFunc, args: Vec<Value>) -> EResult<Value> {
        if args.len() != f.params.len() {
            return Err(format!(
                "'{}': нужно аргументов: {}, передано: {}",
                f.name,
                f.params.len(),
                args.len()
            ));
        }
        let d = self.depth.get();
        if d >= MAX_CALL_DEPTH {
            return Err("слишком глубокая рекурсия".into());
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
        if matches!(name, "rkfixed" | "Rkadapt" | "rkadapt") {
            return self.builtin_ode(name, &vals);
        }
        if let Some(r) = crate::fitting::call_with(name, &vals, &|f, a| self.call_value(f, a)) {
            return r.map_err(|e| format!("{name}: {e}"));
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
        Err(format!("функция '{name}' не определена"))
    }

    fn builtin_matrix(&self, vals: &[Value]) -> EResult<Value> {
        if vals.len() != 3 {
            return Err("matrix(m, n, f) принимает 3 аргумента".into());
        }
        let m = vals[0].as_int()?;
        let n = vals[1].as_int()?;
        let Value::Func(f) = &vals[2] else {
            return Err("третий аргумент должен быть функцией f(i, j)".into());
        };
        if m <= 0 || n <= 0 || m * n > MAX_RANGE_LEN as i64 {
            return Err("недопустимый размер матрицы".into());
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

    /// `rkfixed(y, x1, x2, npoints, D)` and `Rkadapt(...)`: solves `y' = D(x, y)`.
    /// Returns the matrix whose first column is `x` and the others the components of `y`.
    fn builtin_ode(&self, name: &str, vals: &[Value]) -> EResult<Value> {
        if vals.len() != 5 {
            return Err(format!("{name}(y, x1, x2, npoints, D) принимает 5 аргументов"));
        }
        let scalar = matches!(vals[0], Value::Num(_));
        let init = vals[0].to_matrix()?;
        if !init.is_vector() {
            return Err("начальные условия должны быть вектором".into());
        }
        let to_f = |n: &Num| -> EResult<f64> {
            if !n.dim.is_none() {
                return Err("значение должно быть безразмерным".into());
            }
            if !n.is_real() {
                return Err("значение должно быть вещественным".into());
            }
            Ok(n.v.re)
        };
        let y0 = init.data.iter().map(to_f).collect::<EResult<Vec<f64>>>()?;
        let x1 = vals[1].as_dimless_real()?;
        let x2 = vals[2].as_dimless_real()?;
        let np = vals[3].as_int()?;
        if np < 1 || np as usize > MAX_RANGE_LEN {
            return Err("недопустимое число точек".into());
        }
        let Value::Func(f) = &vals[4] else {
            return Err("пятый аргумент должен быть функцией D(x, y)".into());
        };
        let dim = y0.len();
        let rhs = |x: f64, y: &[f64]| -> EResult<Vec<f64>> {
            let yv = if scalar {
                Value::real(y[0])
            } else {
                Value::Matrix(Matrix::column(y.iter().map(|v| Num::real(*v)).collect()))
            };
            let r = self.call_value(f, vec![Value::real(x), yv])?.to_matrix()?;
            if r.data.len() != dim || !r.is_vector() {
                return Err(format!("D(x, y) должна возвращать вектор из {dim} элементов"));
            }
            r.data.iter().map(to_f).collect()
        };
        let rows = if name == "rkfixed" {
            crate::numeric::rk4_fixed(&rhs, &y0, x1, x2, np as usize)?
        } else {
            crate::numeric::rk45_adaptive(&rhs, &y0, x1, x2, np as usize, 1e-8)?
        };
        let mut out = Matrix::zeros(rows.len(), dim + 1);
        for (k, row) in rows.iter().enumerate() {
            out.set(k, 0, Num::real(x1 + (x2 - x1) * k as f64 / np as f64));
            for (j, v) in row.iter().enumerate() {
                out.set(k, j + 1, Num::real(*v));
            }
        }
        Ok(Value::Matrix(out))
    }

    fn bound_var<'a>(&self, e: &'a Expr, form: &str) -> EResult<&'a str> {
        match e {
            Expr::Var(n) => Ok(n),
            _ => Err(format!("{form}: переменная должна быть именем")),
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
                return Err("слишком много слагаемых".into());
            }
            (a..=b).map(|k| Value::real(k as f64)).collect()
        } else {
            match self.lookup(var, sc) {
                Some(Value::Range(r)) => r.values().map(Value::Num).collect(),
                Some(Value::Matrix(m)) if m.is_vector() => m.data.iter().map(|n| Value::Num(*n)).collect(),
                _ => return Err(format!("'{var}' должна быть дискретной переменной")),
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
        let mut a = self.eval(&args[2], sc)?.as_num()?;
        let mut b = self.eval(&args[3], sc)?.as_num()?;
        unify_zero_dims(&mut a, &mut b);
        if a.dim != b.dim {
            return Err("пределы имеют разные единицы измерения".into());
        }
        if !a.is_real() || !b.is_real() {
            return Err("пределы должны быть вещественными".into());
        }
        let f = self.scalar_fn(&args[0], var, a.dim, sc);
        let out_dim = Cell::new(None::<Dim>);
        let g = |x: f64| -> EResult<C64> {
            let n = f(C64::new(x, 0.0))?;
            match out_dim.get() {
                None => out_dim.set(Some(n.dim)),
                Some(d) if d != n.dim => return Err("подынтегральная функция имеет несогласованные единицы измерения".into()),
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
                .ok_or_else(|| format!("'{var}' должна быть определена (точка, в которой берётся производная)"))?
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
            let mut a = self.eval(&args[2], sc)?.as_num()?;
            let mut b = self.eval(&args[3], sc)?.as_num()?;
            unify_zero_dims(&mut a, &mut b);
            if a.dim != b.dim {
                return Err("границы интервала имеют разные единицы измерения".into());
            }
            let f = self.scalar_fn(&args[0], var, a.dim, sc);
            let g = |x: f64| -> EResult<f64> { Ok(f(C64::new(x, 0.0))?.v.re) };
            let x = crate::numeric::brent(&g, a.v.re, b.v.re)?;
            return Ok(Value::Num(Num::with_dim(x, a.dim)));
        }
        let guess = self
            .lookup(var, sc)
            .ok_or_else(|| format!("для '{var}' нужно начальное приближение, заданное выше"))?
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
                return Err("слишком много значений".into());
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
                        "дискретную переменную '{r}' можно использовать только в определении с индексом, например {name}[{r}] := …"
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
                    set_element(&mut m, &ix, v)
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
        Expr::OnError(a, b) => {
            free_vars(a, bound, out);
            free_vars(b, bound, out);
        }
        Expr::Program(stmts) => {
            let mark = bound.len();
            block_free_vars(stmts, bound, out);
            bound.truncate(mark);
        }
    }
}

/// Program locals (assigned names and `for` variables) are bound from their first assignment on.
fn block_free_vars(stmts: &[PStmt], bound: &mut Vec<String>, out: &mut Vec<String>) {
    for s in stmts {
        match s {
            PStmt::Assign(target, e) => {
                free_vars(e, bound, out);
                match target {
                    Target::Var(n) => bound.push(n.clone()),
                    Target::Indexed(n, idx) => {
                        for i in idx {
                            free_vars(i, bound, out);
                        }
                        bound.push(n.clone());
                    }
                    Target::Func(..) => {}
                }
            }
            PStmt::Expr(e) | PStmt::Return(e) => free_vars(e, bound, out),
            PStmt::If { cond, then, else_ } => {
                free_vars(cond, bound, out);
                block_free_vars(then, bound, out);
                if let Some(e) = else_ {
                    block_free_vars(e, bound, out);
                }
            }
            PStmt::For { var, iter, body } => {
                free_vars(iter, bound, out);
                bound.push(var.clone());
                block_free_vars(body, bound, out);
            }
            PStmt::While { cond, body } => {
                free_vars(cond, bound, out);
                block_free_vars(body, bound, out);
            }
            PStmt::Break | PStmt::Continue => {}
        }
    }
}
