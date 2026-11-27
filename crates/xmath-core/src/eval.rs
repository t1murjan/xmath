//! Expression evaluation in Mathcad order: every region sees the definitions
//! made above / to the left of it.

use crate::ast::{BinOp, Expr, PStmt, Target, UnOp};
use crate::builtins;
use crate::units::{self, Dim};
use crate::value::*;
use num_complex::Complex64 as C64;
use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::rc::Rc;

/// Maximum nesting of user-function calls (protects against runaway recursion).
pub const MAX_CALL_DEPTH: usize = 2000;

/// Lexical scope: how many worksheet definitions are visible, plus local bindings
/// (function parameters, iteration variables).
#[derive(Clone, Debug)]
pub struct Scope {
    pub limit: usize,
    pub locals: Rc<RefCell<Vec<(String, Value)>>>,
}

impl Scope {
    pub fn with(&self, name: &str, v: Value) -> Scope {
        let mut locals = self.locals.borrow().clone();
        locals.push((name.to_string(), v));
        Scope { limit: self.limit, locals: Rc::new(RefCell::new(locals)) }
    }

    /// A scope with the same visibility and a copy of the locals.
    fn copy_here(&self) -> Scope {
        Scope { limit: self.limit, locals: Rc::new(RefCell::new(self.locals.borrow().clone())) }
    }
}

/// Maximum number of loop iterations in one program run.
pub const MAX_LOOP_ITERATIONS: u64 = 10_000_000;

/// Error text that carries a `return` met inside an expression; the value travelling with it
/// is in [`Evaluator::returns`]. No other error has exactly this text.
const RETURN_SIGNAL: &str = "return вне программы";
const BREAK_SIGNAL: &str = "'break' вне цикла";
const CONTINUE_SIGNAL: &str = "'continue' вне цикла";

/// Whether an error is one of the control-flow signals above.
fn is_flow_signal(e: &str) -> bool {
    e == RETURN_SIGNAL || e == BREAK_SIGNAL || e == CONTINUE_SIGNAL
}

pub struct Evaluator {
    /// All worksheet definitions, in evaluation order (append-only).
    pub defs: Vec<(String, Value)>,
    /// Names whose definition failed. Mathcad leaves such a name undefined, while xmath used to
    /// fall back to a built-in constant or unit of the same name (`N` became the newton after a
    /// failed definition of `N`), which silently produced a plausible wrong value.
    pub failed: std::collections::HashSet<String>,
    depth: Cell<usize>,
    /// Loop iteration limit of a single program run.
    pub max_iterations: u64,
    /// Definitions as source expressions, for symbolic evaluation (`→`).
    pub sym: crate::symbolic::Defs,
    /// Worksheet directory and file access rules (`READPRN`, `WRITEPRN`, references).
    pub env: crate::fileio::EvalEnv,
    /// Programs being run: the function they belong to (if called) and the path of the
    /// statement being executed (see [`ProgErr`]).
    prog_stack: RefCell<Vec<(Option<String>, Vec<u32>)>>,
    /// The function whose program body is about to run.
    calling: RefCell<Option<String>>,
    /// Where the innermost failing program statement is (taken by the worksheet).
    pub prog_err: RefCell<Option<ProgErr>>,
    /// Values carried out of an expression by `return` (see [`Expr::Return`]).
    returns: RefCell<Vec<Value>>,
    /// The programs being run, innermost last: the names they assign anywhere (Mathcad
    /// declares them as locals of the whole program) and those already assigned.
    frames: RefCell<Vec<FrameNames>>,
}

/// Statically collected local names of one running program.
#[derive(Default)]
struct FrameNames {
    declared: HashSet<String>,
    assigned: HashSet<String>,
    /// First local of the program's own frame (parameters below it are never hidden).
    base: usize,
}

/// The program statement where an error happened, for Mathcad's red mark: `func` is the
/// user function whose body it is (`None`: the program written in the region itself) and
/// `path` leads to the statement: an index into the block, then for `if` 0 (its statement)
/// or 1 (`otherwise`), for loops 0 (the body), and so on into nested blocks.
#[derive(Clone, Debug, PartialEq)]
pub struct ProgErr {
    pub func: Option<String>,
    pub path: Vec<u32>,
    pub msg: String,
}

impl Default for Evaluator {
    fn default() -> Self {
        Evaluator {
            defs: Vec::new(),
            failed: Default::default(),
            depth: Cell::new(0),
            max_iterations: MAX_LOOP_ITERATIONS,
            sym: Default::default(),
            env: Default::default(),
            prog_stack: RefCell::new(Vec::new()),
            calling: RefCell::new(None),
            prog_err: RefCell::new(None),
            returns: RefCell::new(Vec::new()),
            frames: RefCell::new(Vec::new()),
        }
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
        self.sc.locals.borrow().iter().rposition(|(n, _)| n == name).filter(|i| *i >= self.base)
    }

    fn set(&mut self, name: &str, v: Value) {
        let pos = self.local_pos(name);
        let mut locals = self.sc.locals.borrow_mut();
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

/// Stores `v` at the (zero based) index `ix` of a numeric matrix or of a table with mixed
/// cells (a table cell may itself be a matrix: Mathcad's nested arrays).
fn set_cell(container: &mut Value, ix: &[i64], v: Value) -> EResult<()> {
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
    match container {
        Value::Table(t) => {
            t.set(r as usize, c as usize, v);
            Ok(())
        }
        Value::Matrix(m) => match v {
            Value::Num(n) => set_element(m, ix, n),
            other => {
                // a vector in a cell makes the container a nested array
                let cells: Vec<Value> = m.data.iter().map(|n| Value::Num(*n)).collect();
                let mut t = crate::value::Table::new(m.rows, m.cols, cells);
                t.set(r as usize, c as usize, other);
                *container = Value::Table(Box::new(t));
                Ok(())
            }
        },
        other => Err(format!("'{}' нельзя индексировать", other.type_name())),
    }
}

/// Replaces column `k` (zero based) of `m` with the vector `v`, growing the matrix.
fn assign_column(m: &mut Matrix, k: i64, v: &Value) -> EResult<()> {
    let Value::Matrix(col) = v else {
        return Err(format!("справа от определения столбца должен быть вектор, найдено: {}", v.type_name()));
    };
    if !col.is_vector() {
        return Err("справа от определения столбца должен быть вектор".into());
    }
    if k < 0 {
        return Err("индекс столбца вне допустимых границ".into());
    }
    let (k, n) = (k as usize, col.data.len());
    if (n.max(m.rows)) * (k + 1) > MAX_RANGE_LEN {
        return Err("матрица получилась бы слишком большой".into());
    }
    let mut out = Matrix::zeros(n.max(m.rows), (m.cols).max(k + 1));
    for r in 0..m.rows {
        for c in 0..m.cols {
            out.set(r, c, m.get(r, c));
        }
    }
    for (r, x) in col.data.iter().enumerate() {
        out.set(r, k, *x);
    }
    *m = out;
    Ok(())
}

/// Collects the names a program assigns anywhere (its static local scope) and its loop
/// variables; function definitions are not values and are left out.
fn collect_locals(stmts: &[PStmt], out: &mut HashSet<String>) {
    for s in stmts {
        match s {
            PStmt::Assign(target, _) => match target {
                Target::Var(n) | Target::Indexed(n, _) | Target::MatCol(n, _) => {
                    out.insert(n.clone());
                }
                Target::Vector(names) => out.extend(names.iter().cloned()),
                Target::Func(..) => {}
            },
            PStmt::For { var, body, .. } => {
                out.insert(var.clone());
                collect_locals(body, out);
            }
            PStmt::While { body, .. } => collect_locals(body, out),
            PStmt::If { then, else_, .. } => {
                collect_locals(then, out);
                if let Some(e) = else_ {
                    collect_locals(e, out);
                }
            }
            PStmt::Expr(e) | PStmt::Return(e) => collect_expr_locals(e, out),
            PStmt::Break | PStmt::Continue => {}
        }
    }
}

/// Names assigned by a local assignment written as an expression (`X[(k ← 2)] := …`).
fn collect_expr_locals(e: &Expr, out: &mut HashSet<String>) {
    match e {
        Expr::LocalDefine(t, v) => {
            match &**t {
                Target::Var(n) | Target::Indexed(n, _) | Target::MatCol(n, _) => {
                    out.insert(n.clone());
                }
                Target::Vector(names) => out.extend(names.iter().cloned()),
                Target::Func(..) => {}
            }
            collect_expr_locals(v, out);
        }
        Expr::Unary(_, a) | Expr::Abs(a) | Expr::Vectorize(a) | Expr::Return(a) => collect_expr_locals(a, out),
        Expr::Binary(_, a, b) | Expr::OnError(a, b) | Expr::MatCol(a, b) => {
            collect_expr_locals(a, out);
            collect_expr_locals(b, out);
        }
        Expr::Call(_, args) => args.iter().for_each(|a| collect_expr_locals(a, out)),
        Expr::Index(b, idx) => {
            collect_expr_locals(b, out);
            idx.iter().for_each(|i| collect_expr_locals(i, out));
        }
        Expr::Range(a, b, c) => {
            collect_expr_locals(a, out);
            if let Some(b) = b {
                collect_expr_locals(b, out);
            }
            collect_expr_locals(c, out);
        }
        Expr::Matrix(rows) => rows.iter().flatten().for_each(|c| collect_expr_locals(c, out)),
        Expr::Program(stmts) => collect_locals(stmts, out),
        Expr::Break | Expr::Continue => {}
        _ => {}
    }
}

/// The real number a value holds (for Mathcad's image `and`/`or`, a min/max of pixels).
fn num_of(v: &Value) -> Option<f64> {
    let n = v.as_num().ok()?;
    if n.dim.is_none() && n.v.im == 0.0 {
        Some(n.v.re)
    } else {
        None
    }
}

/// The boolean operation of a `logic` call: the binary operator or the name seen in
/// Mathcad's apply form (`and(A, B)`, `implies(P, Q)`).
#[derive(Clone, Copy)]
pub enum LogicOp {
    And,
    Or,
    Xor,
    Implies,
    Equiv,
}

impl LogicOp {
    fn apply(self, x: bool, y: bool) -> bool {
        match self {
            LogicOp::And => x && y,
            LogicOp::Or => x || y,
            LogicOp::Xor => x != y,
            LogicOp::Implies => !x || y,
            LogicOp::Equiv => x == y,
        }
    }
}

pub trait IntoLogic {
    fn into_logic(self) -> LogicOp;
}

impl IntoLogic for BinOp {
    fn into_logic(self) -> LogicOp {
        match self {
            BinOp::And => LogicOp::And,
            BinOp::Or => LogicOp::Or,
            _ => LogicOp::Xor,
        }
    }
}

impl IntoLogic for &str {
    fn into_logic(self) -> LogicOp {
        match self {
            "and" => LogicOp::And,
            "or" => LogicOp::Or,
            "xor" => LogicOp::Xor,
            "implies" | "⇒" => LogicOp::Implies,
            _ => LogicOp::Equiv,
        }
    }
}

/// Whether the expression calls `name` as a function.
fn calls_name(e: &Expr, name: &str) -> bool {
    match e {
        Expr::Call(n, args) => n == name || args.iter().any(|a| calls_name(a, name)),
        Expr::Unary(_, a) | Expr::Abs(a) | Expr::Vectorize(a) | Expr::Return(a) => calls_name(a, name),
        Expr::Binary(_, a, b) | Expr::OnError(a, b) | Expr::MatCol(a, b) => calls_name(a, name) || calls_name(b, name),
        Expr::Index(b, idx) => calls_name(b, name) || idx.iter().any(|i| calls_name(i, name)),
        Expr::Range(a, b, c) => {
            calls_name(a, name) || calls_name(c, name) || b.as_ref().is_some_and(|b| calls_name(b, name))
        }
        Expr::Matrix(rows) => rows.iter().flatten().any(|c| calls_name(c, name)),
        Expr::LocalDefine(_, v) => calls_name(v, name),
        Expr::Program(stmts) => stmts.iter().any(|s| match s {
            PStmt::Assign(_, e) | PStmt::Expr(e) | PStmt::Return(e) => calls_name(e, name),
            PStmt::If { cond, then, else_ } => {
                calls_name(cond, name)
                    || then.iter().any(|s| pstmt_calls(s, name))
                    || else_.as_ref().is_some_and(|b| b.iter().any(|s| pstmt_calls(s, name)))
            }
            PStmt::For { iter, body, .. } => {
                calls_name(iter, name) || body.iter().any(|s| pstmt_calls(s, name))
            }
            PStmt::While { cond, body } => {
                calls_name(cond, name) || body.iter().any(|s| pstmt_calls(s, name))
            }
            PStmt::Break | PStmt::Continue => false,
        }),
        Expr::Break | Expr::Continue | Expr::Num(_) | Expr::Imag(_) | Expr::Str(_) | Expr::Var(_) => false,
    }
}

fn pstmt_calls(s: &PStmt, name: &str) -> bool {
    calls_name(&Expr::Program(vec![s.clone()]), name)
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
        // Mathcad's not-a-number value, tested with `IsNaN`
        "NaN" => Num::new(C64::new(f64::NAN, f64::NAN), Dim::NONE),
        // Catalan's constant, which Mathcad knows only symbolically (`Catalan → float`)
        "Catalan" => Num::real(0.915_965_594_177_219),
        "TOL" => Num::real(1e-3),
        "CTOL" => Num::real(1e-3),
        "ORIGIN" => Num::real(0.0),
        // imaginary unit when `i`/`j` has no definition above (Mathcad worksheets use it in
        // function bodies defined above a later `i := sqrt(-1)`)
        "i" | "j" => Num::new(C64::new(0.0, 1.0), Dim::NONE),
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
        Scope { limit: self.defs.len(), locals: Rc::new(RefCell::new(Vec::new())) }
    }

    pub fn lookup(&self, name: &str, sc: &Scope) -> Option<Value> {
        if let Some(v) = sc.locals.borrow().iter().rev().find(|(n, _)| n == name).map(|(_, v)| v.clone()) {
            return Some(v);
        }
        if let Some((_, v)) = self.defs[..sc.limit].iter().rev().find(|(n, _)| n == name) {
            return Some(v.clone());
        }
        if self.failed.contains(name) {
            return None;
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
                // Mathcad's transpose `A^T` (T is the operator unless a variable T exists)
                // (T is also tesla: a matrix "to the power of tesla" means the transpose)
                if *op == BinOp::Pow && matches!(&**b, Expr::Var(t) if t == "T") {
                    let t_is_number = self.lookup("T", sc).is_some_and(|t| t.as_num().is_ok_and(|n| n.dim.is_none()));
                    if !t_is_number {
                        let v = self.eval(a, sc)?;
                        if matches!(v, Value::Matrix(_)) || self.lookup("T", sc).is_none() {
                            return Ok(Value::Matrix(v.to_matrix()?.transpose()));
                        }
                    }
                }
                match op {
                    BinOp::And | BinOp::Or | BinOp::Xor => {
                        // element by element when an operand is a matrix, as Mathcad does
                        let x = self.eval(a, sc)?;
                        let y = self.eval(b, sc)?;
                        return self.logic(*op, &x, &y);
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
                if let Value::Table(t) = &b {
                    let origin = self.origin(sc);
                    let ix = idx
                        .iter()
                        .map(|i| self.eval(i, sc)?.as_int().map(|k| k - origin))
                        .collect::<EResult<Vec<i64>>>()?;
                    let out_of_range = || "индекс вне допустимых границ".to_string();
                    return match ix.as_slice() {
                        [k] if t.cols == 1 => {
                            let r = usize::try_from(*k).map_err(|_| out_of_range())?;
                            t.cells.get(r).cloned().ok_or_else(out_of_range)
                        }
                        [r, c] => {
                            let r = usize::try_from(*r).map_err(|_| out_of_range())?;
                            let c = usize::try_from(*c).map_err(|_| out_of_range())?;
                            if r >= t.rows || c >= t.cols {
                                return Err(out_of_range());
                            }
                            Ok(t.get(r, c).clone())
                        }
                        _ => Err("для таблицы нужны два индекса".into()),
                    };
                }
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
                // Mathcad 12+ evaluates the elements column by column, which matters when a
                // cell assigns a local: `[P ← 1, Q; Q ← 2, P]` reads P only in the second column.
                let mut data = vec![Num::zero(); nrows * ncols];
                let mut mixed: Option<Vec<Value>> = None;
                for c in 0..ncols {
                    for r in 0..nrows {
                        let v = self.eval(&rows[r][c], sc)?;
                        match &v {
                            Value::Num(n) => data[r * ncols + c] = *n,
                            Value::Str(_) => {
                                // a data table with a column of names: keep every cell
                                let cells = mixed.get_or_insert_with(|| {
                                    data.iter().map(|n| Value::Num(*n)).collect::<Vec<_>>()
                                });
                                cells[r * ncols + c] = v.clone();
                            }
                            other => {
                                return Err(format!("элементы матрицы должны быть числами или строками, найдено: {}", other.type_name()))
                            }
                        }
                    }
                }
                match mixed {
                    Some(cells) => Ok(Value::Table(Box::new(crate::value::Table::new(nrows, ncols, cells)))),
                    None => Ok(Value::Matrix(Matrix::new(nrows, ncols, data))),
                }
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
                // `return`/`break`/`continue` in the failing branch leave the whole program
                Err(e) if is_flow_signal(&e) => Err(e),
                Err(_) => self.eval(b, sc),
            },
            Expr::LocalDefine(target, e) => self.local_assign(target, e, sc),
            Expr::Break => Err(BREAK_SIGNAL.to_string()),
            Expr::Continue => Err(CONTINUE_SIGNAL.to_string()),
            Expr::Return(e) => {
                let v = self.eval(e, sc)?;
                self.returns.borrow_mut().push(v);
                Err(RETURN_SIGNAL.to_string())
            }
            Expr::Program(stmts) => self.run_program(stmts, sc),
            Expr::Vectorize(a) => self.veval(a, sc),
            Expr::MatCol(m, j) => {
                let b = self.eval(m, sc)?;
                if let Value::Table(t) = &b {
                    let origin = self.origin(sc);
                    let k = self.eval(j, sc)?.as_int()? - origin;
                    let k = usize::try_from(k)
                        .ok()
                        .filter(|k| *k < t.cols)
                        .ok_or_else(|| format!("столбец {} вне допустимых границ (в таблице {} столбцов)", k + origin, t.cols))?;
                    let cells: Vec<Value> = (0..t.rows).map(|r| t.get(r, k).clone()).collect();
                    let col = crate::value::Table::new(t.rows, 1, cells);
                    return match col.numeric() {
                        Some(m) => Ok(Value::Matrix(m)),
                        None => Ok(Value::Table(Box::new(col))),
                    };
                }
                let m = match &b {
                    Value::Matrix(m) => m,
                    Value::Num(_) => return Err("оператор столбца M⟨j⟩ применим только к матрицам".into()),
                    other => return Err(format!("оператор столбца M⟨j⟩ применим только к матрицам, найдено: {}", other.type_name())),
                };
                let origin = self.origin(sc);
                let k = self.eval(j, sc)?.as_int()? - origin;
                let k = usize::try_from(k)
                    .ok()
                    .filter(|k| *k < m.cols)
                    .ok_or_else(|| format!("столбец {} вне допустимых границ (в матрице {} столбцов)", k + origin, m.cols))?;
                Ok(Value::Matrix(Matrix::column((0..m.rows).map(|r| m.get(r, k)).collect())))
            }
        }
    }

    // -----------------------------------------------------------------------
    // Vectorize

    /// Evaluates `e` element by element: matrix operands of the same size are
    /// combined index by index (`*`, `/`, `^` and function calls included),
    /// scalars are broadcast.
    fn veval(&self, e: &Expr, sc: &Scope) -> EResult<Value> {
        match e {
            Expr::Vectorize(a) => self.veval(a, sc),
            Expr::Unary(UnOp::Not, a) => {
                let v = self.veval(a, sc)?;
                self.vmap(&[v], &mut |x| Ok(Value::bool(!x[0].truthy()?)))
            }
            Expr::Unary(op, a) => {
                let v = self.veval(a, sc)?;
                self.vmap(&[v], &mut |x| match op {
                    UnOp::Neg => neg(&x[0]),
                    _ => builtins::call("factorial", &[x[0].clone()], 0).unwrap(),
                })
            }
            Expr::Binary(BinOp::And | BinOp::Or | BinOp::Xor, ..) => self.eval(e, sc),
            Expr::Binary(op, a, b) => {
                let x = self.veval(a, sc)?;
                let y = self.veval(b, sc)?;
                self.vmap(&[x, y], &mut |v| match op {
                    BinOp::Add => add(&v[0], &v[1]),
                    BinOp::Sub => sub(&v[0], &v[1]),
                    BinOp::Mul => match self.offset_temperature(&v[0], b, sc) {
                        Some(t) => Ok(t),
                        None => mul(&v[0], &v[1]),
                    },
                    BinOp::Div => div(&v[0], &v[1]),
                    BinOp::Pow => pow(&v[0], &v[1]),
                    _ => cmp_op(&v[0], &v[1], *op),
                })
            }
            Expr::Call(name, args) => {
                // special forms, matrix builders and matrix operations see their arguments
                // whole: `vectorize(augment(a, b))` and `vectorize(A/reverse(A))` are Mathcad
                // expressions of the corpus
                if matches!(
                    name.as_str(),
                    "if" | "sum"
                        | "Σ"
                        | "prod"
                        | "Π"
                        | "integrate"
                        | "∫"
                        | "diff"
                        | "root"
                        | "matrix"
                        | "rkfixed"
                        | "Rkadapt"
                        | "rkadapt"
                        | "augment"
                        | "stack"
                        | "submatrix"
                        | "transpose"
                        | "reverse"
                        | "sort"
                        | "csort"
                        | "rsort"
                        | "identity"
                        | "rows"
                        | "cols"
                        | "length"
                        | "last"
                        | "tr"
                        | "det"
                        | "norme"
                        | "cross"
                ) {
                    return self.eval(e, sc);
                }
                let vals = args.iter().map(|a| self.veval(a, sc)).collect::<EResult<Vec<_>>>()?;
                let names: Vec<String> = (0..vals.len()).map(|i| format!("\u{1}v{i}")).collect();
                let call_args: Vec<Expr> = names.iter().map(|n| Expr::Var(n.clone())).collect();
                self.vmap(&vals, &mut |x| {
                    let mut s2 = sc.clone();
                    for (n, v) in names.iter().zip(x) {
                        s2 = s2.with(n, v.clone());
                    }
                    self.call(name, &call_args, &s2)
                })
            }
            _ => self.eval(e, sc),
        }
    }

    /// Applies `f` to every index of the matrix arguments (all of the same size);
    /// scalar arguments are repeated. Without matrix arguments `f` is called once.
    fn vmap(&self, vals: &[Value], f: &mut dyn FnMut(&[Value]) -> EResult<Value>) -> EResult<Value> {
        let mut shape: Option<(usize, usize)> = None;
        let mut mats: Vec<Option<Matrix>> = Vec::new();
        for v in vals {
            let m = match v {
                Value::Matrix(m) => Some(m.clone()),
                Value::Range(r) => Some(Matrix::column(r.values().collect())),
                _ => None,
            };
            if let Some(m) = &m {
                match shape {
                    None => shape = Some((m.rows, m.cols)),
                    Some(sh) if sh != (m.rows, m.cols) => {
                        return Err(format!(
                            "vectorize: размеры операндов не совпадают ({}×{} и {}×{})",
                            sh.0, sh.1, m.rows, m.cols
                        ))
                    }
                    _ => {}
                }
            }
            mats.push(m);
        }
        let Some((rows, cols)) = shape else {
            return f(vals);
        };
        let mut out = Vec::with_capacity(rows * cols);
        for k in 0..rows * cols {
            let args: Vec<Value> = vals
                .iter()
                .zip(&mats)
                .map(|(v, m)| match m {
                    Some(m) => Value::Num(m.data[k]),
                    None => v.clone(),
                })
                .collect();
            let r = f(&args)?;
            out.push(r.as_num().map_err(|_| "vectorize: результат для элемента должен быть скаляром".to_string())?);
        }
        Ok(Value::Matrix(Matrix::new(rows, cols, out)))
    }

    // -----------------------------------------------------------------------
    // Programs

    fn run_program(&self, stmts: &[PStmt], sc: &Scope) -> EResult<Value> {
        let inner = sc.copy_here();
        let base = inner.locals.borrow().len();
        let mut fr = Frame { sc: inner, base, iters: 0 };
        // Mathcad 13 made the scope of a program static: a name assigned anywhere in it is a
        // local of the whole program, and reading it before the assignment gives 0 (Mathcad
        // 11/12 read the worksheet variable instead).
        let mut names = FrameNames { base, ..Default::default() };
        collect_locals(stmts, &mut names.declared);
        for n in &names.declared {
            // a function parameter (or an enclosing local) of the same name keeps its value:
            // `sqroot(a, ε, maxiter) := { for i ∈ 0..maxiter { … maxiter ← i … } }`
            let exists = fr.sc.locals.borrow().iter().any(|(m, _)| m == n);
            if !exists {
                fr.set(n, Value::real(0.0));
            }
        }
        self.frames.borrow_mut().push(names);
        let func = self.calling.borrow_mut().take();
        self.prog_stack.borrow_mut().push((func, Vec::new()));
        let r = self.exec_block(stmts, &mut fr);
        self.prog_stack.borrow_mut().pop();
        self.frames.borrow_mut().pop();
        match r {
            Err(e) if e == RETURN_SIGNAL => return self.returns.borrow_mut().pop().ok_or_else(|| RETURN_SIGNAL.to_string()),
            Err(e) => return Err(e),
            Ok(Flow::Normal(Some(v))) | Ok(Flow::Return(v)) => Ok(v),
            Ok(Flow::Normal(None)) => Err("программа не вернула значения".into()),
            Ok(Flow::Break) => Err("'break' вне цикла".into()),
            Ok(Flow::Continue) => Err("'continue' вне цикла".into()),
        }
    }

    /// Path step of the running program (statement index or branch).
    fn path_push(&self, k: u32) {
        if let Some((_, p)) = self.prog_stack.borrow_mut().last_mut() {
            p.push(k);
        }
    }

    fn path_pop(&self) {
        if let Some((_, p)) = self.prog_stack.borrow_mut().last_mut() {
            p.pop();
        }
    }

    /// Remembers the innermost failing statement (the first one recorded wins).
    fn note_err(&self, msg: &str) {
        if self.prog_err.borrow().is_some() {
            return;
        }
        if let Some((func, path)) = self.prog_stack.borrow().last() {
            *self.prog_err.borrow_mut() = Some(ProgErr { func: func.clone(), path: path.clone(), msg: msg.to_string() });
        }
    }

    /// Runs `f` as the branch / body `k` of the current statement.
    fn in_branch(&self, k: u32, f: impl FnOnce() -> EResult<Flow>) -> EResult<Flow> {
        self.path_push(k);
        let r = f();
        self.path_pop();
        r
    }

    /// Runs a block. Consecutive `if` statements form a chain (as in Mathcad):
    /// an `otherwise` runs only when its own condition and the conditions of all
    /// `if`s before it in the chain were false.
    fn exec_block(&self, stmts: &[PStmt], fr: &mut Frame) -> EResult<Flow> {
        let mut last = None;
        let mut fired = false;
        for (i, s) in stmts.iter().enumerate() {
            self.path_push(i as u32);
            let r = match s {
                PStmt::If { cond, then, else_ } => (|| {
                    Ok(if self.eval(cond, &fr.sc)?.truthy()? {
                        fired = true;
                        self.in_branch(0, || self.exec_block(then, fr))?
                    } else if let (Some(e), false) = (else_, fired) {
                        fired = true;
                        self.in_branch(1, || self.exec_block(e, fr))?
                    } else {
                        Flow::Normal(None)
                    })
                })(),
                _ => {
                    fired = false;
                    self.exec_stmt(s, fr)
                }
            };
            if let Err(e) = &r {
                if !is_flow_signal(e) {
                    self.note_err(e);
                }
            }
            self.path_pop();
            let flow = r?;
            match flow {
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
            PStmt::Assign(target, e) => Ok(Flow::Normal(Some(self.local_assign(target, e, &fr.sc)?))),
            PStmt::If { cond, then, else_ } => {
                if self.eval(cond, &fr.sc)?.truthy()? {
                    self.in_branch(0, || self.exec_block(then, fr))
                } else if let Some(e) = else_ {
                    self.in_branch(1, || self.exec_block(e, fr))
                } else {
                    Ok(Flow::Normal(None))
                }
            }
            PStmt::While { cond, body } => {
                let mut last = None;
                while self.eval(cond, &fr.sc)?.truthy()? {
                    self.tick(fr)?;
                    match self.in_branch(0, || self.exec_block(body, fr)) {
                        Err(e) if e == BREAK_SIGNAL => break,
                        Err(e) if e == CONTINUE_SIGNAL => continue,
                        r => match r? {
                            Flow::Normal(v) => last = v.or(last),
                            Flow::Break => break,
                            Flow::Continue => {}
                            r @ Flow::Return(_) => return Ok(r),
                        },
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
                    self.mark_assigned(var);
                    match self.in_branch(0, || self.exec_block(body, fr)) {
                        Err(e) if e == BREAK_SIGNAL => break,
                        Err(e) if e == CONTINUE_SIGNAL => continue,
                        r => match r? {
                            Flow::Normal(v) => last = v.or(last),
                            Flow::Break => break,
                            Flow::Continue => {}
                            r @ Flow::Return(_) => return Ok(r),
                        },
                    }
                }
                Ok(Flow::Normal(last))
            }
            PStmt::Break => Ok(Flow::Break),
            PStmt::Continue => Ok(Flow::Continue),
            PStmt::Return(e) => Ok(Flow::Return(self.eval(e, &fr.sc)?)),
        }
    }

    // -----------------------------------------------------------------------
    // Local assignments

    /// Sets a local name of the running program (the innermost binding, else a new one).
    fn set_local(&self, name: &str, v: Value, sc: &Scope) {
        let mut locals = sc.locals.borrow_mut();
        match locals.iter().rposition(|(n, _)| n == name) {
            Some(i) => locals[i].1 = v,
            None => locals.push((name.to_string(), v)),
        }
    }

    /// A declared local that the program has not assigned yet (it holds the 0 placeholder).
    fn local_pending(&self, name: &str) -> bool {
        self.frames
            .borrow()
            .last()
            .is_some_and(|f| f.declared.contains(name) && !f.assigned.contains(name))
    }

    fn local_value(&self, name: &str, sc: &Scope) -> Option<Value> {
        sc.locals.borrow().iter().rev().find(|(n, _)| n == name).map(|(_, v)| v.clone())
    }

    /// Records that the running program has assigned `name` (Mathcad's scope is static, so an
    /// assignment inside a branch the program never reaches still declares the name).
    fn mark_assigned(&self, name: &str) {
        if let Some(f) = self.frames.borrow_mut().last_mut() {
            f.assigned.insert(name.to_string());
        }
    }

    /// Hides declared but not yet assigned locals while the right side of a local assignment is
    /// evaluated: Mathcad 13+ reads the worksheet value of a name there until the program
    /// assigns it (a program like `{ s ← s + 1 }` starts from the worksheet `s`).
    fn hide_unassigned(&self, sc: &Scope) -> Vec<(usize, (String, Value))> {
        let (pending, base): (Vec<String>, usize) = {
            let frames = self.frames.borrow();
            match frames.last() {
                Some(f) if !f.declared.is_empty() => {
                    (f.declared.difference(&f.assigned).cloned().collect(), f.base)
                }
                _ => return Vec::new(),
            }
        };
        if pending.is_empty() {
            return Vec::new();
        }
        let mut locals = sc.locals.borrow_mut();
        let mut removed = Vec::new();
        for i in (base..locals.len()).rev() {
            if pending.iter().any(|p| *p == locals[i].0) {
                removed.push((i, locals.remove(i)));
            }
        }
        removed
    }

    fn restore_hidden(&self, sc: &Scope, removed: Vec<(usize, (String, Value))>) {
        if removed.is_empty() {
            return;
        }
        let mut locals = sc.locals.borrow_mut();
        for (i, kv) in removed.into_iter().rev() {
            locals.insert(i, kv);
        }
    }

    /// `target ← body` inside a program (or as an expression, `X[(k ← 2)] := …`); the value of
    /// the assignment is the value of its right side.
    fn local_assign(&self, target: &Target, body: &Expr, sc: &Scope) -> EResult<Value> {
        if let Target::Func(name, params) = target {
            let f = UserFunc {
                name: name.clone(),
                params: params.clone(),
                body: Rc::new(body.clone()),
                limit: sc.limit,
                locals: Rc::new(sc.locals.borrow().clone()),
            };
            let v = Value::Func(Rc::new(f));
            self.set_local(name, v.clone(), sc);
            self.mark_assigned(name);
            return Ok(v);
        }
        // `Q1 ← Q1(j)` makes a name that is also called here a value: Mathcad refuses it
        if let Target::Var(name) = target {
            if calls_name(body, name) {
                return Err(format!("функция '{name}' ссылается на себя непоследовательно"));
            }
        }
        let hidden = self.hide_unassigned(sc);
        let value = self.eval(body, sc);
        self.restore_hidden(sc, hidden);
        let v = value?;
        match target {
            Target::Var(name) => {
                self.set_local(name, v.clone(), sc);
                self.mark_assigned(name);
            }
            Target::Indexed(name, idx) => {
                let origin = self.origin(sc);
                // the right side is evaluated first (Mathcad evaluates such a statement from
                // right to left), so the index sees the local it may have just set
                let ix = idx
                    .iter()
                    .map(|i| self.eval(i, sc)?.as_int().map(|k| k - origin))
                    .collect::<EResult<Vec<i64>>>()?;
                let mut container = match self.local_value(name, sc).filter(|_| !self.local_pending(name)) {
                    Some(Value::Matrix(m)) => Value::Matrix(m),
                    Some(v @ Value::Table(_)) => v,
                    Some(other) => {
                        return Err(format!("'{name}': элементу нельзя присвоить значение, найдено: {}", other.type_name()))
                    }
                    None => Value::Matrix(Matrix::zeros(0, 0)),
                };
                set_cell(&mut container, &ix, v.clone())?;
                self.set_local(name, container, sc);
                self.mark_assigned(name);
            }
            Target::MatCol(name, j) => {
                let origin = self.origin(sc);
                let k = self.eval(j, sc)?.as_int()? - origin;
                let mut m = match self.local_value(name, sc).filter(|_| !self.local_pending(name)) {
                    Some(Value::Matrix(m)) => m,
                    Some(other) => {
                        return Err(format!("'{name}': столбцу нельзя присвоить значение, найдено: {}", other.type_name()))
                    }
                    None => Matrix::zeros(0, 0),
                };
                assign_column(&mut m, k, &v)?;
                self.set_local(name, Value::Matrix(m), sc);
                self.mark_assigned(name);
            }
            Target::Vector(names) => {
                let cols = match &v {
                    Value::Matrix(m) if m.is_vector() => m.data.clone(),
                    other => {
                        return Err(format!(
                            "справа от определения [{}] ← … должен быть вектор из {} элементов, найдено: {}",
                            names.join("; "),
                            names.len(),
                            other.type_name()
                        ))
                    }
                };
                if cols.len() != names.len() {
                    return Err(format!(
                        "размер вектора ({}) не совпадает с числом определяемых имён ({})",
                        cols.len(),
                        names.len()
                    ));
                }
                for (n, x) in names.iter().zip(&cols) {
                    self.set_local(n, Value::Num(*x), sc);
                    self.mark_assigned(n);
                }
            }
            Target::Func(..) => unreachable!(),
        }
        Ok(v)
    }

    /// Boolean `and`/`or`/`xor` and the connectives: scalars, or matrices element by element.
    fn logic(&self, op: impl IntoLogic, a: &Value, b: &Value) -> EResult<Value> {
        let op = op.into_logic();
        // Mathcad's image functions: `and` of two pixels is their minimum and `or` their
        // maximum (for the 0/1 and 0/255 images of the corpus that is the Boolean operation)
        let images = matches!(a, Value::Matrix(_)) || matches!(b, Value::Matrix(_));
        self.vmap(&[a.clone(), b.clone()], &mut |v| {
            if images {
                if let (Some(x), Some(y)) = (num_of(&v[0]), num_of(&v[1])) {
                    let r = match op {
                        LogicOp::And => x.min(y),
                        LogicOp::Or => x.max(y),
                        _ => f64::from(u8::from(op.apply(x != 0.0, y != 0.0))),
                    };
                    return Ok(Value::real(r));
                }
            }
            let (x, y) = (v[0].truthy()?, v[1].truthy()?);
            Ok(Value::bool(op.apply(x, y)))
        })
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
        let sc = Scope { limit: f.limit, locals: Rc::new(RefCell::new(locals)) };
        if matches!(*f.body, Expr::Program(_)) {
            *self.calling.borrow_mut() = Some(f.name.clone());
        }
        self.depth.set(d + 1);
        let r = self.eval(&f.body, &sc);
        self.depth.set(d);
        match r {
            // `f(x) := 1/x on error (return error("…"))`
            Err(e) if e == RETURN_SIGNAL => self.returns.borrow_mut().pop().ok_or(e),
            r => r,
        }
    }

    fn eval_args(&self, args: &[Expr], sc: &Scope) -> EResult<Vec<Value>> {
        args.iter().map(|a| self.eval(a, sc)).collect()
    }

    fn call(&self, name: &str, args: &[Expr], sc: &Scope) -> EResult<Value> {
        // `and(A, B)` — the apply form of the boolean operators — and the connectives
        if matches!(name, "and" | "or" | "xor" | "implies" | "equiv" | "⇒" | "⇔") && args.len() == 2 {
            let vals = self.eval_args(args, sc)?;
            return self.logic(name, &vals[0], &vals[1]);
        }
        // user-defined functions (and function-valued parameters) come first
        let user = self.lookup(name, sc);
        if let Some(Value::Func(f)) = &user {
            let vals = self.eval_args(args, sc)?;
            return self.call_value(f, vals);
        }
        if crate::solve::is_solve_name(name) {
            return self.solve_block(name, args, sc);
        }
        if let Some(r) = self.special_form(name, args, sc) {
            return r;
        }
        let vals = self.eval_args(args, sc)?;
        if let Some(r) = crate::fileio::call(&self.env, name, &vals) {
            return r;
        }
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
        self.define_value(target, expr).map(|_| ())
    }

    /// Like [`define`](Self::define), returns the defined value (for a vector
    /// definition: the whole right-hand side).
    pub fn define_value(&mut self, target: &Target, expr: &Expr) -> EResult<Value> {
        let r = self.define_value_inner(target, expr);
        let name = defined_name(target);
        if r.is_ok() {
            self.failed.remove(&name);
        } else {
            self.failed.insert(name);
        }
        r
    }

    fn define_value_inner(&mut self, target: &Target, expr: &Expr) -> EResult<Value> {
        let sc = self.top_scope();
        // `WRITEPRN("file") := M`: writes a file instead of defining a function
        if let Target::Func(name, params) = target {
            if crate::fileio::is_write_name(name) && params.len() == 1 {
                let path = crate::fileio::target_path(&params[0], &|n| self.lookup(n, &sc))?;
                let v = self.eval(expr, &sc)?;
                crate::fileio::write(&self.env, name, &path, &v)?;
                return Ok(v);
            }
        }
        if let Target::Vector(names) = target {
            let v = self.eval(expr, &sc)?;
            let m = match &v {
                Value::Matrix(m) if m.is_vector() => m,
                Value::Num(_) if names.len() == 1 => {
                    self.defs.push((names[0].clone(), v.clone()));
                    return Ok(v);
                }
                other => {
                    return Err(format!(
                        "справа от определения [{}] := … должен быть вектор из {} элементов, найдено: {}",
                        names.join("; "),
                        names.len(),
                        other.type_name()
                    ))
                }
            };
            if m.data.len() != names.len() {
                return Err(format!(
                    "размер вектора ({}) не совпадает с числом определяемых имён ({})",
                    m.data.len(),
                    names.len()
                ));
            }
            for (n, x) in names.iter().zip(&m.data) {
                self.defs.push((n.clone(), Value::Num(*x)));
            }
            return Ok(v);
        }
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
                let mut container = match self.lookup(name, &sc) {
                    Some(Value::Matrix(m)) => Value::Matrix(m),
                    Some(v @ Value::Table(_)) => v,
                    _ => Value::Matrix(Matrix::zeros(0, 0)),
                };
                let mut m = match &container {
                    Value::Matrix(m) => m.clone(),
                    _ => Matrix::zeros(0, 0),
                };
                let origin = self.origin(&sc);
                let mut all: Vec<&Expr> = idx.iter().collect();
                all.push(expr);
                let ranges = self.range_vars(&all, &sc);
                // `a[j + 1] := f(a[j])` over a range is sequential in Mathcad: later elements
                // see the earlier ones, so the vector under construction is visible in the scope
                let mut used = Vec::new();
                free_vars(expr, &mut Vec::new(), &mut used);
                let self_ref = !ranges.is_empty() && used.iter().any(|u| u == name);
                self.for_each_binding(&ranges, &sc, &mut |s| {
                    let s2;
                    let s = if self_ref {
                        s2 = s.with(name, Value::Matrix(m.clone()));
                        &s2
                    } else {
                        s
                    };
                    let ix = idx
                        .iter()
                        .map(|i| self.eval(i, s)?.as_int().map(|k| k - origin))
                        .collect::<EResult<Vec<i64>>>()?;
                    let cell = self.eval(expr, s)?;
                    if matches!(cell, Value::Num(_)) && matches!(container, Value::Matrix(_)) {
                        set_element(&mut m, &ix, cell.as_num()?)?;
                    } else {
                        // a vector cell makes this a nested array
                        if matches!(container, Value::Matrix(_)) {
                            let cells: Vec<Value> = m.data.iter().map(|n| Value::Num(*n)).collect();
                            container = Value::Table(Box::new(crate::value::Table::new(m.rows, m.cols, cells)));
                            m = Matrix::zeros(0, 0);
                        }
                        set_cell(&mut container, &ix, cell)?;
                    }
                    Ok(())
                })?;
                // the numeric elements accumulate in `m`; a table keeps its own cells
                let container = match container {
                    t @ Value::Table(_) => t,
                    _ => Value::Matrix(m),
                };
                let value = match container {
                    Value::Table(t) => t.into_value(),
                    Value::Matrix(m) => Value::Matrix(m),
                    other => other,
                };
                (name.clone(), value)
            }
            Target::MatCol(name, j) => {
                let mut m = match self.lookup(name, &sc) {
                    Some(Value::Matrix(m)) => m,
                    _ => Matrix::zeros(0, 0),
                };
                let origin = self.origin(&sc);
                // `v^<k> := A*v^<k - 1>` over a range builds the columns in order, each one
                // seeing the columns already assigned
                let ranges = self.range_vars(&[expr, j], &sc);
                let mut used = Vec::new();
                free_vars(expr, &mut Vec::new(), &mut used);
                let self_ref = !ranges.is_empty() && used.iter().any(|u| u == name);
                self.for_each_binding(&ranges, &sc, &mut |s| {
                    let s2;
                    let s = if self_ref {
                        s2 = s.with(name, Value::Matrix(m.clone()));
                        &s2
                    } else {
                        s
                    };
                    let k = self.eval(j, s)?.as_int()? - origin;
                    let v = self.eval(expr, s)?;
                    assign_column(&mut m, k, &v)
                })?;
                (name.clone(), Value::Matrix(m))
            }
            Target::Vector(_) => unreachable!(),
        };
        self.defs.push((name, value.clone()));
        Ok(value)
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
        Expr::Num(_) | Expr::Imag(_) | Expr::Str(_) | Expr::Break | Expr::Continue => {}
        Expr::Unary(_, a) | Expr::Abs(a) | Expr::Vectorize(a) => free_vars(a, bound, out),
        Expr::MatCol(a, b) => {
            free_vars(a, bound, out);
            free_vars(b, bound, out);
        }
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
        Expr::LocalDefine(target, v) => {
            free_vars(v, bound, out);
            match &**target {
                Target::Var(n) => bound.push(n.clone()),
                Target::Indexed(n, idx) => {
                    for i in idx {
                        free_vars(i, bound, out);
                    }
                    bound.push(n.clone());
                }
                _ => {}
            }
        }
        Expr::Return(a) => free_vars(a, bound, out),
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
                    Target::MatCol(n, j) => {
                        free_vars(j, bound, out);
                        bound.push(n.clone());
                    }
                    Target::Func(..) | Target::Vector(_) => {}
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

/// The name a target defines (`v[i] := …` and `v(x) := …` both define `v`).
fn defined_name(t: &Target) -> String {
    match t {
        Target::Var(n) | Target::Func(n, _) | Target::Indexed(n, _) | Target::MatCol(n, _) => n.clone(),
        Target::Vector(ns) => ns.first().cloned().unwrap_or_default(),
    }
}
