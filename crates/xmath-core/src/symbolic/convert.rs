//! Conversion between the syntax tree (`Expr`) and canonical symbolic values (`Sx`).

use super::calculus;
use super::matrix;
use super::sx::*;
use super::Ctx;
use crate::ast::{BinOp, Expr, UnOp};
use crate::value::Value;
use num_bigint::BigInt;
use num_traits::{One, Signed, ToPrimitive};
use std::collections::BTreeMap;

pub enum ConvErr {
    /// The expression cannot be handled symbolically (the caller evaluates it numerically).
    NotSymbolic(String),
    Error(String),
}

type CR<T> = Result<T, ConvErr>;

fn ns<T>(msg: &str) -> CR<T> {
    Err(ConvErr::NotSymbolic(msg.to_string()))
}

pub fn num_to_sx(x: f64) -> Sx {
    if x.is_finite() && x.fract() == 0.0 && x.abs() < 1e15 {
        int(x as i64)
    } else if x.is_infinite() {
        if x > 0.0 {
            inf()
        } else {
            neg(inf())
        }
    } else {
        float(x)
    }
}

pub fn value_to_sx(v: &Value) -> Option<Sx> {
    match v {
        Value::Num(n) => {
            if !n.dim.is_none() {
                return None;
            }
            let (re, im) = (n.v.re, n.v.im);
            Some(if im == 0.0 { num_to_sx(re) } else { add(num_to_sx(re), mul(num_to_sx(im), Sx::Im)) })
        }
        Value::Matrix(m) => {
            let mut rows = Vec::new();
            for r in 0..m.rows {
                let mut row = Vec::new();
                for c in 0..m.cols {
                    row.push(value_to_sx(&Value::Num(m.data[r * m.cols + c]))?);
                }
                rows.push(row);
            }
            Some(Sx::Mat(rows))
        }
        _ => None,
    }
}

pub struct Env<'a> {
    pub ctx: &'a Ctx<'a>,
    pub locals: Vec<(String, Sx)>,
    pub depth: u32,
    /// Names currently being expanded (cycle guard).
    pub active: Vec<String>,
}

impl<'a> Env<'a> {
    pub fn new(ctx: &'a Ctx<'a>) -> Env<'a> {
        Env { ctx, locals: Vec::new(), depth: 0, active: Vec::new() }
    }

    fn numeric_expr(&self, e: &Expr) -> Option<Sx> {
        let f = self.ctx.numeric?;
        // only when nothing symbolic is left in the expression
        let mut names = Vec::new();
        crate::eval::free_vars(e, &mut Vec::new(), &mut names);
        for n in names {
            if self.locals.iter().any(|(l, _)| *l == n) {
                return None;
            }
        }
        f(e)
    }

    fn var(&mut self, name: &str) -> CR<Sx> {
        if let Some((_, v)) = self.locals.iter().rev().find(|(n, _)| n == name) {
            return Ok(v.clone());
        }
        if let Some(def) = self.ctx.defs.vars.get(name) {
            if !self.active.iter().any(|a| a == name) && self.depth < 60 {
                if matches!(def, Expr::Range(..)) {
                    return Ok(sym(name));
                }
                self.active.push(name.to_string());
                self.depth += 1;
                let saved = std::mem::take(&mut self.locals);
                let r = self.conv(def);
                self.locals = saved;
                self.depth -= 1;
                self.active.pop();
                match r {
                    Ok(v) => return Ok(v),
                    Err(ConvErr::NotSymbolic(_)) => {
                        // numeric value of the definition
                        if let Some(v) = self.ctx.value_of.and_then(|f| f(name)) {
                            return Ok(v);
                        }
                        return Ok(sym(name));
                    }
                    Err(e) => return Err(e),
                }
            }
            return Ok(sym(name));
        }
        if self.ctx.defs.funcs.contains_key(name) {
            return Ok(sym(name));
        }
        Ok(match name {
            "π" | "pi" => pi(),
            "e" => func("exp", vec![one()]),
            "∞" | "inf" | "infinity" | "Inf" => inf(),
            "i" | "j" => Sx::Im,
            _ => sym(name),
        })
    }

    pub fn conv(&mut self, e: &Expr) -> CR<Sx> {
        Ok(match e {
            Expr::Num(x) => num_to_sx(*x),
            Expr::Imag(x) => mul(num_to_sx(*x), Sx::Im),
            Expr::Str(_) => return ns("строка"),
            Expr::Var(n) => self.var(n)?,
            Expr::Unary(UnOp::Neg, a) => neg(self.conv(a)?),
            Expr::Unary(UnOp::Not, a) => func("not", vec![self.conv(a)?]),
            Expr::Unary(UnOp::Factorial, a) => func("factorial", vec![self.conv(a)?]),
            Expr::Binary(op, a, b) => {
                let (x, y) = (self.conv(a)?, self.conv(b)?);
                match op {
                    BinOp::Add => add(x, y),
                    BinOp::Sub => sub(x, y),
                    BinOp::Mul => mul(x, y),
                    BinOp::Div => div(x, y),
                    BinOp::Pow => pow(x, y),
                    BinOp::Eq => Sx::Fun("==".into(), vec![x, y]),
                    BinOp::Ne => Sx::Fun("!=".into(), vec![x, y]),
                    BinOp::Lt => Sx::Fun("<".into(), vec![x, y]),
                    BinOp::Gt => Sx::Fun(">".into(), vec![x, y]),
                    BinOp::Le => Sx::Fun("<=".into(), vec![x, y]),
                    BinOp::Ge => Sx::Fun(">=".into(), vec![x, y]),
                    BinOp::And => Sx::Fun("and".into(), vec![x, y]),
                    BinOp::Or => Sx::Fun("or".into(), vec![x, y]),
                    BinOp::Xor => Sx::Fun("xor".into(), vec![x, y]),
                }
            }
            Expr::Matrix(rows) => {
                let mut out = Vec::new();
                for r in rows {
                    let mut row = Vec::new();
                    for c in r {
                        row.push(self.conv(c)?);
                    }
                    out.push(row);
                }
                Sx::Mat(out)
            }
            Expr::Abs(a) => {
                let v = self.conv(a)?;
                match &v {
                    Sx::Mat(m) => {
                        let (r, c) = matrix::dims(m);
                        if r == c && r > 1 {
                            match matrix::det(m) {
                                Some(d) => d,
                                None => return Err(ConvErr::Error("не удалось вычислить определитель".into())),
                            }
                        } else {
                            // vector magnitude
                            let items: Vec<Sx> = m.iter().flatten().map(|x| mul(func("abs", vec![x.clone()]), func("abs", vec![x.clone()]))).collect();
                            sqrt(sum(items))
                        }
                    }
                    _ => func("abs", vec![v]),
                }
            }
            Expr::Index(base, idx) => {
                if let Some(v) = self.numeric_expr(e) {
                    return Ok(v);
                }
                let b = self.conv(base)?;
                let mut ix = Vec::new();
                for i in idx {
                    match self.conv(i)?.as_int() {
                        Some(k) if k >= 0 => ix.push(k as usize),
                        _ => return ns("индекс"),
                    }
                }
                match (&b, ix.as_slice()) {
                    (Sx::Mat(m), [r, c]) if *r < m.len() && *c < m[0].len() => m[*r][*c].clone(),
                    (Sx::Mat(m), [k]) if m.len() == 1 && *k < m[0].len() => m[0][*k].clone(),
                    (Sx::Mat(m), [k]) if m[0].len() == 1 && *k < m.len() => m[*k][0].clone(),
                    _ => return ns("индекс"),
                }
            }
            Expr::Call(name, args) => self.call(e, name, args)?,
            Expr::OnError(..) | Expr::Program(_) | Expr::Range(..) | Expr::Vectorize(_) | Expr::MatCol(..) => {
                return match self.numeric_expr(e) {
                    Some(v) => Ok(v),
                    None => ns("выражение вычисляется только численно"),
                };
            }
        })
    }

    fn bound_var(&mut self, e: &Expr) -> CR<Sx> {
        match e {
            Expr::Var(n) => Ok(sym(n)),
            _ => Err(ConvErr::Error("переменная должна быть именем".into())),
        }
    }

    /// Converts `body` with the variable `x` kept symbolic even when it is defined.
    fn conv_bound(&mut self, body: &Expr, x: &Sx) -> CR<Sx> {
        if let Sx::Sym(n) = x {
            self.locals.push((n.clone(), x.clone()));
            let r = self.conv(body);
            self.locals.pop();
            r
        } else {
            self.conv(body)
        }
    }

    fn call(&mut self, whole: &Expr, name: &str, args: &[Expr]) -> CR<Sx> {
        // user-defined function
        if let Some((params, body)) = self.ctx.defs.funcs.get(name).cloned() {
            if params.len() != args.len() {
                return Err(ConvErr::Error(format!("функция {name} ожидает аргументов: {}", params.len())));
            }
            if self.depth > 60 || self.active.iter().filter(|a| *a == name).count() > 20 {
                return ns("глубокая рекурсия");
            }
            let mut vals = Vec::new();
            for a in args {
                vals.push(self.conv(a)?);
            }
            let saved_locals = std::mem::take(&mut self.locals);
            self.locals = params.iter().cloned().zip(vals).collect();
            self.active.push(name.to_string());
            self.depth += 1;
            let r = self.conv(&body);
            self.depth -= 1;
            self.active.pop();
            self.locals = saved_locals;
            return match r {
                Err(ConvErr::NotSymbolic(m)) => match self.numeric_expr(whole) {
                    Some(v) => Ok(v),
                    None => Err(ConvErr::NotSymbolic(m)),
                },
                other => other,
            };
        }
        let lname = name;
        match (lname, args.len()) {
            ("diff", 2) | ("diff", 3) => {
                let x = self.bound_var(&args[1])?;
                let f = self.conv_bound(&args[0], &x)?;
                let n = if args.len() == 3 {
                    match self.conv(&args[2])?.as_int() {
                        Some(k) if (0..=20).contains(&k) => k as usize,
                        _ => return Err(ConvErr::Error("порядок производной должен быть целым числом".into())),
                    }
                } else {
                    1
                };
                Ok(calculus::diff_n(&f, &x, n))
            }
            ("integrate" | "∫", 2) => {
                let x = self.bound_var(&args[1])?;
                let f = self.conv_bound(&args[0], &x)?;
                Ok(match calculus::integ(&f, &x, 0) {
                    Some(r) => super::algebra::tidy(&r),
                    None => Sx::Fun("integrate".into(), vec![f, x]),
                })
            }
            ("integrate" | "∫", 4) => {
                let x = self.bound_var(&args[1])?;
                let f = self.conv_bound(&args[0], &x)?;
                let (a, b) = (self.conv(&args[2])?, self.conv(&args[3])?);
                if let Some(r) = calculus::definite(&f, &x, &a, &b) {
                    return Ok(r);
                }
                if let Some(v) = self.numeric_expr(whole) {
                    return Ok(v);
                }
                Ok(Sx::Fun("integrate".into(), vec![f, x, a, b]))
            }
            ("sum" | "Σ" | "prod" | "Π", 4) => {
                let k = self.bound_var(&args[0])?;
                let (a, b) = (self.conv(&args[1])?, self.conv(&args[2])?);
                // the index variable is local: temporarily unbind definitions of the same name
                let body = self.conv_bound(&args[3], &k)?;
                let r = if lname == "sum" || lname == "Σ" { calculus::sum_closed(&body, &k, &a, &b) } else { calculus::prod_closed(&body, &k, &a, &b) };
                match r {
                    Some(v) => Ok(v),
                    None => match self.numeric_expr(whole) {
                        Some(v) => Ok(v),
                        None => Ok(Sx::Fun(if lname == "sum" || lname == "Σ" { "sum" } else { "prod" }.into(), vec![k, a, b, body])),
                    },
                }
            }
            ("lim" | "limit", 3) | ("lim" | "limit", 4) => {
                let x = self.bound_var(&args[1])?;
                let f = self.conv_bound(&args[0], &x)?;
                let a = self.conv(&args[2])?;
                let dir = if args.len() == 4 { self.conv(&args[3])?.as_int().unwrap_or(0) as i32 } else { 0 };
                Ok(match calculus::limit(&f, &x, &a, dir.signum()) {
                    Some(r) => r,
                    None => {
                        let mut v = vec![f, x, a];
                        if args.len() == 4 {
                            v.push(int(dir as i64));
                        }
                        Sx::Fun("lim".into(), v)
                    }
                })
            }
            ("sqrt", 1) => Ok(sqrt(self.conv(&args[0])?)),
            ("nthroot", 2) => {
                let (a, n) = (self.conv(&args[0])?, self.conv(&args[1])?);
                Ok(pow(a, pow(n, int(-1))))
            }
            ("sec", 1) => Ok(pow(func("cos", vec![self.conv(&args[0])?]), int(-1))),
            ("csc", 1) => Ok(pow(func("sin", vec![self.conv(&args[0])?]), int(-1))),
            ("cot", 1) => Ok(pow(func("tan", vec![self.conv(&args[0])?]), int(-1))),
            ("log", 1) => Ok(div(func("ln", vec![self.conv(&args[0])?]), func("ln", vec![int(10)]))),
            ("log", 2) => Ok(div(func("ln", vec![self.conv(&args[0])?]), func("ln", vec![self.conv(&args[1])?]))),
            ("exp" | "ln" | "sin" | "cos" | "tan" | "asin" | "acos" | "atan" | "sinh" | "cosh" | "tanh" | "asinh" | "acosh" | "atanh" | "abs" | "sign" | "floor" | "ceil" | "round" | "trunc" | "erf" | "factorial", 1) => {
                Ok(func(lname, vec![self.conv(&args[0])?]))
            }
            ("transpose", 1) => Ok(match self.conv(&args[0])? {
                Sx::Mat(m) => Sx::Mat(matrix::transpose(&m)),
                other => other,
            }),
            ("det", 1) => Ok(match self.conv(&args[0])? {
                Sx::Mat(m) => matrix::det(&m).ok_or_else(|| ConvErr::Error("определитель определён только для квадратных матриц".into()))?,
                other => other,
            }),
            ("tr", 1) => Ok(match self.conv(&args[0])? {
                Sx::Mat(m) => matrix::trace(&m),
                other => other,
            }),
            ("identity", 1) => Ok(match self.conv(&args[0])?.as_int() {
                Some(n) if n > 0 && n <= 50 => Sx::Mat(matrix::identity(n as usize)),
                _ => return Err(ConvErr::Error("identity: размер должен быть натуральным числом".into())),
            }),
            ("rows", 1) => Ok(match self.conv(&args[0])? {
                Sx::Mat(m) => int(m.len() as i64),
                _ => int(1),
            }),
            ("cols", 1) => Ok(match self.conv(&args[0])? {
                Sx::Mat(m) => int(m[0].len() as i64),
                _ => int(1),
            }),
            _ => {
                // numeric built-in with fully known arguments, otherwise an unevaluated call
                if let Some(v) = self.numeric_expr(whole) {
                    return Ok(v);
                }
                if crate::solve::SOLVE_NAMES.contains(&name) {
                    return ns("блок решения");
                }
                let mut vals = Vec::new();
                for a in args {
                    vals.push(self.conv(a)?);
                }
                Ok(Sx::Fun(name.to_string(), vals))
            }
        }
    }
}

// ------------------------------------------------------------------ Sx -> Expr

fn bigint_expr(n: &BigInt) -> Expr {
    match n.to_i64() {
        Some(v) if v.abs() < (1 << 53) => Expr::Num(v as f64),
        _ => Expr::Var(n.to_string()),
    }
}

fn rat_abs_expr(r: &Rat) -> Expr {
    let r = r.abs();
    if r.is_integer() {
        bigint_expr(r.numer())
    } else {
        Expr::Binary(BinOp::Div, Box::new(bigint_expr(r.numer())), Box::new(bigint_expr(r.denom())))
    }
}

fn clean_f(x: f64) -> f64 {
    if x == 0.0 || !x.is_finite() {
        return x;
    }
    format!("{x:.14e}").parse::<f64>().unwrap_or(x)
}

fn neg_e(e: Expr) -> Expr {
    Expr::Unary(UnOp::Neg, Box::new(e))
}

/// `-e` with the sign moved to the leftmost factor (`-x*y/2` rather than `-(x*y/2)`).
fn negate(e: Expr) -> Expr {
    match e {
        Expr::Binary(BinOp::Mul, a, b) => Expr::Binary(BinOp::Mul, Box::new(negate(*a)), b),
        Expr::Binary(BinOp::Div, a, b) => Expr::Binary(BinOp::Div, Box::new(negate(*a)), b),
        Expr::Num(x) if x > 0.0 => Expr::Num(-x),
        other => neg_e(other),
    }
}

fn bin(op: BinOp, a: Expr, b: Expr) -> Expr {
    Expr::Binary(op, Box::new(a), Box::new(b))
}

/// Sign and magnitude of a term.
fn split_sign(t: &Sx) -> (bool, Sx) {
    let (c, rest) = split_coeff(t);
    match c {
        Co::R(r) if r.is_negative() => (true, attach_rat(-r, rest)),
        Co::F(x) if x < 0.0 => (true, attach_f(-x, rest)),
        _ => (false, t.clone()),
    }
}

fn attach_rat(r: Rat, rest: Sx) -> Sx {
    if r.is_one() {
        rest
    } else if rest.is_one() {
        Sx::Num(r)
    } else {
        match rest {
            Sx::Mul(mut v) => {
                v.insert(0, Sx::Num(r));
                Sx::Mul(v)
            }
            other => Sx::Mul(vec![Sx::Num(r), other]),
        }
    }
}

fn attach_f(x: f64, rest: Sx) -> Sx {
    if rest.is_one() {
        float(x)
    } else {
        match rest {
            Sx::Mul(mut v) => {
                v.insert(0, float(x));
                Sx::Mul(v)
            }
            other => Sx::Mul(vec![float(x), other]),
        }
    }
}

fn exps_of(t: &Sx) -> (BTreeMap<String, f64>, bool) {
    let mut m = BTreeMap::new();
    let mut nonconst = false;
    let fs: Vec<Sx> = match split_coeff(t).1 {
        Sx::Mul(v) => v,
        other => vec![other],
    };
    for f in fs {
        match &f {
            Sx::Sym(n) if !is_constant_name(n) => {
                *m.entry(n.clone()).or_insert(0.0) += 1.0;
                nonconst = true;
            }
            Sx::Pow(b, e) => {
                if let (Sx::Sym(n), Some(k)) = (&**b, e.num_f64()) {
                    *m.entry(n.clone()).or_insert(0.0) += k;
                }
                nonconst = true;
            }
            Sx::Num(_) | Sx::Float(_) => {}
            Sx::Im => {}
            _ => nonconst = true,
        }
    }
    (m, nonconst && !t.is_const())
}

fn term_order(a: &Sx, b: &Sx) -> std::cmp::Ordering {
    let ((ea, na), (eb, nb)) = (exps_of(a), exps_of(b));
    let (da, db): (f64, f64) = (ea.values().sum(), eb.values().sum());
    let (ia, ib) = (a.has_im(), b.has_im());
    nb.cmp(&na)
        .then(ia.cmp(&ib))
        .then(db.total_cmp(&da))
        .then_with(|| {
            let names: std::collections::BTreeSet<&String> = ea.keys().chain(eb.keys()).collect();
            for n in names {
                let (x, y) = (ea.get(n).copied().unwrap_or(0.0), eb.get(n).copied().unwrap_or(0.0));
                let c = y.total_cmp(&x);
                if c != std::cmp::Ordering::Equal {
                    return c;
                }
            }
            std::cmp::Ordering::Equal
        })
        .then_with(|| split_sign(a).0.cmp(&split_sign(b).0))
        .then_with(|| a.cmp(b))
}

trait HasIm {
    fn has_im(&self) -> bool;
}
impl HasIm for Sx {
    fn has_im(&self) -> bool {
        match self {
            Sx::Im => true,
            Sx::Add(v) | Sx::Mul(v) | Sx::Fun(_, v) => v.iter().any(|x| x.has_im()),
            Sx::Pow(a, b) => a.has_im() || b.has_im(),
            _ => false,
        }
    }
}

pub fn to_expr(e: &Sx) -> Expr {
    match e {
        Sx::Num(r) => {
            if r.is_negative() {
                if r.is_integer() {
                    neg_e(rat_abs_expr(r))
                } else {
                    bin(BinOp::Div, neg_e(bigint_expr(&r.numer().abs())), bigint_expr(r.denom()))
                }
            } else {
                rat_abs_expr(r)
            }
        }
        Sx::Float(F(x)) => {
            if x.is_infinite() {
                if *x > 0.0 {
                    Expr::Var("∞".into())
                } else {
                    neg_e(Expr::Var("∞".into()))
                }
            } else {
                Expr::Num(clean_f(*x))
            }
        }
        Sx::Sym(s) => Expr::Var(s.clone()),
        Sx::Im => Expr::Imag(1.0),
        Sx::Add(ts) => {
            let mut ts = ts.clone();
            ts.sort_by(term_order);
            let mut acc: Option<Expr> = None;
            for t in &ts {
                let (neg, mag) = split_sign(t);
                let m = to_expr(&mag);
                acc = Some(match acc {
                    None => {
                        if neg {
                            negate(m)
                        } else {
                            m
                        }
                    }
                    Some(a) => bin(if neg { BinOp::Sub } else { BinOp::Add }, a, m),
                });
            }
            acc.unwrap_or(Expr::Num(0.0))
        }
        Sx::Mul(fs) => mul_expr(fs),
        Sx::Pow(b, p) => pow_expr(b, p),
        Sx::Fun(n, a) => match (n.as_str(), a.as_slice()) {
            ("exp", [x]) if x.is_one() => Expr::Var("e".into()),
            ("abs", [x]) => Expr::Abs(Box::new(to_expr(x))),
            ("undefined", []) => Expr::Var("undefined".into()),
            ("==", [x, y]) => bin(BinOp::Eq, to_expr(x), to_expr(y)),
            ("!=", [x, y]) => bin(BinOp::Ne, to_expr(x), to_expr(y)),
            ("<", [x, y]) => bin(BinOp::Lt, to_expr(x), to_expr(y)),
            (">", [x, y]) => bin(BinOp::Gt, to_expr(x), to_expr(y)),
            ("<=", [x, y]) => bin(BinOp::Le, to_expr(x), to_expr(y)),
            (">=", [x, y]) => bin(BinOp::Ge, to_expr(x), to_expr(y)),
            ("and", [x, y]) => bin(BinOp::And, to_expr(x), to_expr(y)),
            ("or", [x, y]) => bin(BinOp::Or, to_expr(x), to_expr(y)),
            ("xor", [x, y]) => bin(BinOp::Xor, to_expr(x), to_expr(y)),
            ("not", [x]) => Expr::Unary(UnOp::Not, Box::new(to_expr(x))),
            ("factorial", [x]) => Expr::Unary(UnOp::Factorial, Box::new(to_expr(x))),
            _ => Expr::Call(n.clone(), a.iter().map(to_expr).collect()),
        },
        Sx::Mat(m) => Expr::Matrix(m.iter().map(|r| r.iter().map(to_expr).collect()).collect()),
    }
}

fn pow_expr(b: &Sx, p: &Sx) -> Expr {
    if let Sx::Num(r) = p {
        if *r == rat_ij(1, 2) {
            return Expr::Call("sqrt".into(), vec![to_expr(b)]);
        }
        if r.is_negative() {
            let pos = pow_expr_pos(b, &Sx::Num(-r.clone()));
            return bin(BinOp::Div, Expr::Num(1.0), pos);
        }
    }
    bin(BinOp::Pow, to_expr(b), to_expr(p))
}

fn pow_expr_pos(b: &Sx, p: &Sx) -> Expr {
    if p.is_one() {
        return to_expr(b);
    }
    pow_expr(b, p)
}

fn mul_expr(fs: &[Sx]) -> Expr {
    let mut nums: Vec<Expr> = Vec::new();
    let mut dens: Vec<Expr> = Vec::new();
    let mut negative = false;
    let mut num_c: Option<Expr> = None;
    let mut den_c: Option<Expr> = None;
    for f in fs {
        match f {
            Sx::Num(r) => {
                if r.is_negative() {
                    negative = !negative;
                }
                let a = r.abs();
                if !a.numer().is_one() || (a.is_integer() && fs.len() == 1) {
                    num_c = Some(bigint_expr(a.numer()));
                }
                if !a.is_integer() {
                    den_c = Some(bigint_expr(a.denom()));
                }
            }
            Sx::Float(F(x)) => {
                if *x < 0.0 {
                    negative = !negative;
                }
                num_c = Some(Expr::Num(clean_f(x.abs())));
            }
            Sx::Pow(b, p) if matches!(&**p, Sx::Num(r) if r.is_negative() && *r != rat_ij(-1, 2) || (*r == rat_ij(-1, 2))) => {
                let k = match &**p {
                    Sx::Num(r) => -r.clone(),
                    _ => unreachable!(),
                };
                dens.push(pow_expr_pos(b, &Sx::Num(k)));
            }
            Sx::Pow(b, p) if matches!(&**p, Sx::Mul(v) if matches!(v.first(), Some(Sx::Num(r)) if r.is_negative())) => {
                dens.push(pow_expr_pos(b, &neg(p.as_ref().clone())));
            }
            other => {
                // an addition factor is parenthesised by the printer
                let mut sorted_factor = to_expr(other);
                if let Sx::Add(_) = other {
                    sorted_factor = to_expr(other);
                }
                nums.push(sorted_factor);
            }
        }
    }
    // symbols alphabetically (by base and power), then everything else, the imaginary unit last
    let key = |f: &Sx| -> (u8, String, i64) {
        match f {
            Sx::Sym(n) => (0, n.clone(), 1),
            Sx::Pow(b, p) => match (&**b, p.as_int()) {
                (Sx::Sym(n), Some(k)) => (0, n.clone(), k),
                _ => (1, String::new(), 0),
            },
            Sx::Im => (3, String::new(), 0),
            Sx::Add(t) => {
                let c = t.iter().find_map(|x| if let Sx::Num(r) = x { r.to_integer().to_i64() } else { None }).unwrap_or(0);
                (2, String::new(), f.size() as i64 * 1_000_000 + c.abs() * 2 + i64::from(c > 0))
            }
            _ => (1, String::new(), 0),
        }
    };
    let mut keyed: Vec<((u8, String, i64), Expr)> = Vec::new();
    for f in fs {
        if matches!(f, Sx::Num(_) | Sx::Float(_)) {
            continue;
        }
        let is_den = matches!(f, Sx::Pow(_, p) if matches!(&**p, Sx::Num(r) if r.is_negative()) || matches!(&**p, Sx::Mul(v) if matches!(v.first(), Some(Sx::Num(r)) if r.is_negative())));
        if !is_den {
            keyed.push((key(f), to_expr(f)));
        }
    }
    keyed.sort_by(|a, b| a.0.cmp(&b.0));
    nums = keyed.into_iter().map(|(_, e)| e).collect();
    // `c i` is one imaginary literal
    if fs.iter().any(|f| matches!(f, Sx::Im)) {
        if let Some(Expr::Num(c)) = &num_c {
            if let Some(pos) = nums.iter().position(|e| matches!(e, Expr::Imag(_))) {
                nums.remove(pos);
                nums.push(Expr::Imag(*c));
                num_c = None;
            }
        }
    }
    if let Some(c) = num_c {
        nums.insert(0, c);
    }
    if let Some(c) = den_c {
        dens.insert(0, c);
    }
    let fold = |v: Vec<Expr>| -> Option<Expr> {
        let mut it = v.into_iter();
        let first = it.next()?;
        Some(it.fold(first, |a, b| bin(BinOp::Mul, a, b)))
    };
    if negative {
        match nums.first_mut() {
            Some(first) => {
                let e = std::mem::replace(first, Expr::Num(0.0));
                *first = match e {
                    Expr::Num(x) => Expr::Num(-x),
                    other => neg_e(other),
                };
            }
            None => nums.push(Expr::Num(-1.0)),
        }
    }
    let n = fold(nums).unwrap_or(Expr::Num(1.0));
    match fold(dens) {
        Some(d) => bin(BinOp::Div, n, d),
        None => n,
    }
}
