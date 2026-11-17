//! Runtime values: complex quantities with units, matrices, ranges and functions.

use crate::ast::Expr;
use crate::units::Dim;
use num_complex::Complex64 as C64;
use std::rc::Rc;

pub type EResult<T> = Result<T, String>;

/// A complex number carrying a physical dimension. The numeric part is
/// always stored in SI base units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Num {
    pub v: C64,
    pub dim: Dim,
}

impl Num {
    pub fn real(x: f64) -> Num {
        Num { v: C64::new(x, 0.0), dim: Dim::NONE }
    }

    pub fn new(v: C64, dim: Dim) -> Num {
        // `+ 0.0` turns a negative-zero imaginary part into +0, so that
        // `-8` is `-8 + 0i` and complex functions take the principal branch
        Num { v: C64::new(v.re, v.im + 0.0), dim }
    }

    pub fn with_dim(x: f64, dim: Dim) -> Num {
        Num { v: C64::new(x, 0.0), dim }
    }

    pub fn is_real(&self) -> bool {
        self.v.im == 0.0 || self.v.im.abs() <= 1e-15 * self.v.re.abs()
    }

    pub fn re(&self) -> f64 {
        self.v.re
    }

    pub fn zero() -> Num {
        Num::real(0.0)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Matrix {
    pub rows: usize,
    pub cols: usize,
    /// Row-major storage.
    pub data: Vec<Num>,
}

impl Matrix {
    pub fn new(rows: usize, cols: usize, data: Vec<Num>) -> Matrix {
        debug_assert_eq!(rows * cols, data.len());
        Matrix { rows, cols, data }
    }

    pub fn zeros(rows: usize, cols: usize) -> Matrix {
        Matrix { rows, cols, data: vec![Num::zero(); rows * cols] }
    }

    pub fn column(data: Vec<Num>) -> Matrix {
        Matrix { rows: data.len(), cols: 1, data }
    }

    pub fn get(&self, r: usize, c: usize) -> Num {
        self.data[r * self.cols + c]
    }

    pub fn set(&mut self, r: usize, c: usize, v: Num) {
        self.data[r * self.cols + c] = v;
    }

    pub fn is_vector(&self) -> bool {
        self.cols == 1 || self.rows == 1
    }

    pub fn is_square(&self) -> bool {
        self.rows == self.cols
    }

    /// Grows the matrix with zeros so that `(r, c)` is a valid index.
    pub fn ensure(&mut self, r: usize, c: usize) {
        if r < self.rows && c < self.cols {
            return;
        }
        let rows = self.rows.max(r + 1);
        let cols = self.cols.max(c + 1);
        let mut m = Matrix::zeros(rows, cols);
        for i in 0..self.rows {
            for j in 0..self.cols {
                m.set(i, j, self.get(i, j));
            }
        }
        *self = m;
    }

    pub fn transpose(&self) -> Matrix {
        let mut m = Matrix::zeros(self.cols, self.rows);
        for i in 0..self.rows {
            for j in 0..self.cols {
                m.set(j, i, self.get(i, j));
            }
        }
        m
    }

    /// The common dimension of all elements, if there is one.
    pub fn uniform_dim(&self) -> EResult<Dim> {
        let d = self.data.first().map(|n| n.dim).unwrap_or(Dim::NONE);
        if self.data.iter().all(|n| n.dim == d) {
            Ok(d)
        } else {
            Err("для этой операции все элементы матрицы должны иметь одинаковые единицы измерения".into())
        }
    }

    pub fn map(&self, f: impl Fn(Num) -> EResult<Num>) -> EResult<Matrix> {
        let data = self.data.iter().map(|n| f(*n)).collect::<EResult<Vec<_>>>()?;
        Ok(Matrix { rows: self.rows, cols: self.cols, data })
    }
}

/// A range variable `start, start+step .. end`.
#[derive(Clone, Debug, PartialEq)]
pub struct Range {
    pub start: f64,
    pub step: f64,
    pub end: f64,
    pub dim: Dim,
}

pub const MAX_RANGE_LEN: usize = 1_000_000;

impl Range {
    pub fn len(&self) -> usize {
        let n = ((self.end - self.start) / self.step + 1e-9).floor();
        if n < 0.0 {
            0
        } else {
            n as usize + 1
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn values(&self) -> impl Iterator<Item = Num> + '_ {
        (0..self.len()).map(move |k| Num::with_dim(self.start + k as f64 * self.step, self.dim))
    }
}

/// A user-defined function with the scope it was defined in.
#[derive(Debug)]
pub struct UserFunc {
    pub name: String,
    pub params: Vec<String>,
    pub body: Rc<Expr>,
    /// Number of worksheet definitions visible to the body.
    pub limit: usize,
    pub locals: Rc<Vec<(String, Value)>>,
}

#[derive(Clone, Debug)]
pub enum Value {
    Num(Num),
    Matrix(Matrix),
    Str(Rc<str>),
    Range(Range),
    Func(Rc<UserFunc>),
}

impl Value {
    pub fn real(x: f64) -> Value {
        Value::Num(Num::real(x))
    }

    pub fn bool(b: bool) -> Value {
        Value::real(if b { 1.0 } else { 0.0 })
    }

    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Num(_) => "скаляр",
            Value::Matrix(_) => "матрица",
            Value::Str(_) => "строка",
            Value::Range(_) => "диапазон",
            Value::Func(_) => "функция",
        }
    }

    pub fn as_num(&self) -> EResult<Num> {
        match self {
            Value::Num(n) => Ok(*n),
            other => Err(format!("ожидался скаляр, найдено: {}", other.type_name())),
        }
    }

    pub fn as_matrix(&self) -> EResult<&Matrix> {
        match self {
            Value::Matrix(m) => Ok(m),
            other => Err(format!("ожидался вектор или матрица, найдено: {}", other.type_name())),
        }
    }

    /// Converts scalars into 1×1 matrices.
    pub fn to_matrix(&self) -> EResult<Matrix> {
        match self {
            Value::Matrix(m) => Ok(m.clone()),
            Value::Num(n) => Ok(Matrix::new(1, 1, vec![*n])),
            Value::Range(r) => Ok(Matrix::column(r.values().collect())),
            other => Err(format!("ожидался вектор или матрица, найдено: {}", other.type_name())),
        }
    }

    pub fn as_real(&self) -> EResult<f64> {
        let n = self.as_num()?;
        if !n.is_real() {
            return Err("значение должно быть вещественным".into());
        }
        Ok(n.v.re)
    }

    pub fn as_dimless_real(&self) -> EResult<f64> {
        let n = self.as_num()?;
        if !n.dim.is_none() {
            return Err("значение должно быть безразмерным".into());
        }
        if !n.is_real() {
            return Err("значение должно быть вещественным".into());
        }
        Ok(n.v.re)
    }

    pub fn as_int(&self) -> EResult<i64> {
        let x = self.as_dimless_real()?;
        let r = x.round();
        if (x - r).abs() > 1e-9 * x.abs().max(1.0) {
            return Err(format!("ожидалось целое число, найдено {x}"));
        }
        Ok(r as i64)
    }

    pub fn truthy(&self) -> EResult<bool> {
        let n = self.as_num()?;
        Ok(n.v != C64::new(0.0, 0.0))
    }
}

// ---------------------------------------------------------------------------
// Scalar arithmetic

fn units_mismatch() -> String {
    "Единицы измерения не совпадают".into()
}

/// A plain (dimensionless) zero is compatible with any units, as in Mathcad:
/// gives both numbers the same dimension if one of them is such a zero.
pub fn unify_zero_dims(a: &mut Num, b: &mut Num) {
    if a.dim != b.dim {
        if a.dim.is_none() && a.v == C64::new(0.0, 0.0) {
            a.dim = b.dim;
        } else if b.dim.is_none() && b.v == C64::new(0.0, 0.0) {
            b.dim = a.dim;
        }
    }
}

pub fn num_add(a: Num, b: Num) -> EResult<Num> {
    if a.dim != b.dim {
        // Mathcad allows adding plain zero to anything.
        if b.v == C64::new(0.0, 0.0) && b.dim.is_none() {
            return Ok(a);
        }
        if a.v == C64::new(0.0, 0.0) && a.dim.is_none() {
            return Ok(b);
        }
        return Err(units_mismatch());
    }
    Ok(Num::new(a.v + b.v, a.dim))
}

pub fn num_sub(a: Num, b: Num) -> EResult<Num> {
    num_add(a, Num::new(-b.v, b.dim))
}

pub fn num_mul(a: Num, b: Num) -> Num {
    Num::new(a.v * b.v, a.dim.mul(b.dim))
}

pub fn num_div(a: Num, b: Num) -> EResult<Num> {
    if b.v == C64::new(0.0, 0.0) {
        return Err("Деление на ноль".into());
    }
    // real / real must be exact (the complex quotient formula is not: 1/0.2 != 5)
    let v = if a.v.im == 0.0 && b.v.im == 0.0 { C64::new(a.v.re / b.v.re, 0.0) } else { a.v / b.v };
    Ok(Num::new(v, a.dim.div(b.dim)))
}

pub fn cpow(base: C64, e: C64) -> C64 {
    let base = C64::new(base.re, base.im + 0.0);
    let e = C64::new(e.re, e.im + 0.0);
    if e.im == 0.0 {
        let p = e.re;
        if base.im == 0.0 && (base.re >= 0.0 || p.fract() == 0.0) {
            if p.fract() == 0.0 && p.abs() < 1024.0 {
                return C64::new(base.re.powi(p as i32), 0.0);
            }
            return C64::new(base.re.powf(p), 0.0);
        }
        if p.fract() == 0.0 && p.abs() < 1024.0 {
            return base.powi(p as i32);
        }
    }
    if base == C64::new(0.0, 0.0) {
        return if e.re > 0.0 { C64::new(0.0, 0.0) } else { C64::new(f64::INFINITY, 0.0) };
    }
    base.powc(e)
}

pub fn num_pow(a: Num, b: Num) -> EResult<Num> {
    if !b.dim.is_none() {
        return Err("показатель степени должен быть безразмерным".into());
    }
    let dim = if a.dim.is_none() {
        Dim::NONE
    } else {
        if !b.is_real() {
            return Err("величину с единицами нельзя возводить в комплексную степень".into());
        }
        a.dim.powf(b.v.re).ok_or("такую степень единиц измерения представить нельзя")?
    };
    if a.v == C64::new(0.0, 0.0) && b.v.re < 0.0 {
        return Err("Деление на ноль".into());
    }
    Ok(Num::new(cpow(a.v, b.v), dim))
}

fn compare(a: Num, b: Num) -> EResult<std::cmp::Ordering> {
    if a.dim != b.dim && !(b.v.re == 0.0 && b.dim.is_none()) && !(a.v.re == 0.0 && a.dim.is_none()) {
        return Err(units_mismatch());
    }
    a.v.re.partial_cmp(&b.v.re).ok_or_else(|| "нельзя сравнить NaN".into())
}

pub fn num_eq(a: Num, b: Num) -> EResult<bool> {
    if a.dim != b.dim {
        return Err(units_mismatch());
    }
    let scale = a.v.norm().max(b.v.norm());
    Ok((a.v - b.v).norm() <= 1e-15 * scale)
}

// ---------------------------------------------------------------------------
// Value arithmetic with broadcasting

fn elementwise(a: &Value, b: &Value, f: impl Fn(Num, Num) -> EResult<Num>, what: &str) -> EResult<Value> {
    match (a, b) {
        (Value::Num(x), Value::Num(y)) => Ok(Value::Num(f(*x, *y)?)),
        (Value::Matrix(m), Value::Num(y)) => Ok(Value::Matrix(m.map(|x| f(x, *y))?)),
        (Value::Num(x), Value::Matrix(m)) => Ok(Value::Matrix(m.map(|y| f(*x, y))?)),
        (Value::Matrix(m), Value::Matrix(n)) => {
            if m.rows != n.rows || m.cols != n.cols {
                return Err(format!(
                    "нельзя {what}: матрицы {}×{} и {}×{} имеют разные размеры",
                    m.rows, m.cols, n.rows, n.cols
                ));
            }
            let data = m.data.iter().zip(&n.data).map(|(x, y)| f(*x, *y)).collect::<EResult<Vec<_>>>()?;
            Ok(Value::Matrix(Matrix::new(m.rows, m.cols, data)))
        }
        (x, y) => Err(format!("нельзя {what}: {} и {}", x.type_name(), y.type_name())),
    }
}

pub fn add(a: &Value, b: &Value) -> EResult<Value> {
    if let (Value::Str(x), Value::Str(y)) = (a, b) {
        return Ok(Value::Str(format!("{x}{y}").into()));
    }
    elementwise(a, b, num_add, "сложить")
}

pub fn sub(a: &Value, b: &Value) -> EResult<Value> {
    elementwise(a, b, num_sub, "вычесть")
}

pub fn neg(a: &Value) -> EResult<Value> {
    match a {
        Value::Num(n) => Ok(Value::Num(Num::new(-n.v, n.dim))),
        Value::Matrix(m) => Ok(Value::Matrix(m.map(|n| Ok(Num::new(-n.v, n.dim)))?)),
        other => Err(format!("нельзя изменить знак: {}", other.type_name())),
    }
}

pub fn matmul(a: &Matrix, b: &Matrix) -> EResult<Matrix> {
    if a.cols != b.rows {
        return Err(format!(
            "нельзя умножить матрицу {}×{} на матрицу {}×{}",
            a.rows, a.cols, b.rows, b.cols
        ));
    }
    let mut out = Matrix::zeros(a.rows, b.cols);
    for i in 0..a.rows {
        for j in 0..b.cols {
            let mut acc = num_mul(a.get(i, 0), b.get(0, j));
            for k in 1..a.cols {
                acc = num_add(acc, num_mul(a.get(i, k), b.get(k, j)))?;
            }
            out.set(i, j, acc);
        }
    }
    Ok(out)
}

pub fn mul(a: &Value, b: &Value) -> EResult<Value> {
    match (a, b) {
        (Value::Matrix(m), Value::Matrix(n)) => {
            // vector · vector is the dot product, as in Mathcad
            if m.cols == 1 && n.cols == 1 && m.rows == n.rows {
                let mut acc = num_mul(m.data[0], conj(n.data[0]));
                for k in 1..m.rows {
                    acc = num_add(acc, num_mul(m.data[k], conj(n.data[k])))?;
                }
                return Ok(Value::Num(acc));
            }
            Ok(Value::Matrix(matmul(m, n)?))
        }
        _ => elementwise(a, b, |x, y| Ok(num_mul(x, y)), "умножить"),
    }
}

fn conj(n: Num) -> Num {
    Num::new(n.v.conj(), n.dim)
}

pub fn div(a: &Value, b: &Value) -> EResult<Value> {
    match (a, b) {
        (_, Value::Num(_)) => elementwise(a, b, num_div, "разделить"),
        // Mathcad: dividing by a non-square array (e.g. a vector) works element by element
        (Value::Num(_), Value::Matrix(m)) if !m.is_square() => elementwise(a, b, num_div, "разделить"),
        (Value::Matrix(x), Value::Matrix(m)) if !m.is_square() && x.rows == m.rows && x.cols == m.cols => {
            elementwise(a, b, num_div, "разделить")
        }
        (Value::Num(_), Value::Matrix(m)) | (Value::Matrix(_), Value::Matrix(m)) if m.is_square() => {
            mul(a, &Value::Matrix(inverse(m)?))
        }
        (x, y) => Err(format!("нельзя разделить: {} на {}", x.type_name(), y.type_name())),
    }
}

pub fn pow(a: &Value, b: &Value) -> EResult<Value> {
    match (a, b) {
        (Value::Num(x), Value::Num(y)) => Ok(Value::Num(num_pow(*x, *y)?)),
        (Value::Matrix(m), Value::Num(_)) => {
            if !m.is_square() {
                // Mathcad: a power of a non-square array is element-wise
                return elementwise(&Value::Matrix(m.clone()), b, num_pow, "возвести в степень");
            }
            let n = b.as_int().map_err(|_| "степень матрицы должна быть целым числом".to_string())?;
            let base = if n < 0 { inverse(m)? } else { m.clone() };
            let mut result = identity(m.rows);
            for _ in 0..n.unsigned_abs() {
                result = matmul(&result, &base)?;
            }
            Ok(Value::Matrix(result))
        }
        (Value::Num(_), Value::Matrix(m)) if !m.is_square() => elementwise(a, b, num_pow, "возвести в степень"),
        (x, y) => Err(format!("нельзя возвести в степень: {} и {}", x.type_name(), y.type_name())),
    }
}

pub fn cmp_op(a: &Value, b: &Value, op: crate::ast::BinOp) -> EResult<Value> {
    use crate::ast::BinOp::*;
    use std::cmp::Ordering::*;
    if let (Value::Str(x), Value::Str(y)) = (a, b) {
        let r = match op {
            Eq => x == y,
            Ne => x != y,
            Lt => x < y,
            Gt => x > y,
            Le => x <= y,
            Ge => x >= y,
            _ => unreachable!(),
        };
        return Ok(Value::bool(r));
    }
    elementwise(
        a,
        b,
        |x, y| {
            let r = match op {
                Eq => num_eq(x, y)?,
                Ne => !num_eq(x, y)?,
                Lt => compare(x, y)? == Less,
                Gt => compare(x, y)? == Greater,
                Le => compare(x, y)? != Greater,
                Ge => compare(x, y)? != Less,
                _ => unreachable!(),
            };
            Ok(Num::real(if r { 1.0 } else { 0.0 }))
        },
        "сравнить",
    )
}

// ---------------------------------------------------------------------------
// Linear algebra (complex LU with partial pivoting)

pub fn identity(n: usize) -> Matrix {
    let mut m = Matrix::zeros(n, n);
    for i in 0..n {
        m.set(i, i, Num::real(1.0));
    }
    m
}

struct Lu {
    n: usize,
    a: Vec<C64>,
    perm: Vec<usize>,
    sign: f64,
    singular: bool,
}

fn lu(m: &Matrix) -> Lu {
    let n = m.rows;
    let mut a: Vec<C64> = m.data.iter().map(|x| x.v).collect();
    let mut perm: Vec<usize> = (0..n).collect();
    let mut sign = 1.0;
    let mut singular = false;
    let scale = a.iter().map(|x| x.norm()).fold(0.0, f64::max);
    for k in 0..n {
        let mut p = k;
        let mut best = a[k * n + k].norm();
        for i in k + 1..n {
            let v = a[i * n + k].norm();
            if v > best {
                best = v;
                p = i;
            }
        }
        if best <= 1e-14 * scale || best == 0.0 {
            singular = true;
            continue;
        }
        if p != k {
            for j in 0..n {
                a.swap(k * n + j, p * n + j);
            }
            perm.swap(k, p);
            sign = -sign;
        }
        let pivot = a[k * n + k];
        for i in k + 1..n {
            let f = a[i * n + k] / pivot;
            a[i * n + k] = f;
            for j in k + 1..n {
                let t = a[k * n + j];
                a[i * n + j] -= f * t;
            }
        }
    }
    Lu { n, a, perm, sign, singular }
}

impl Lu {
    fn solve(&self, b: &[C64]) -> Vec<C64> {
        let n = self.n;
        let mut x: Vec<C64> = self.perm.iter().map(|&p| b[p]).collect();
        for i in 0..n {
            for k in 0..i {
                let t = self.a[i * n + k] * x[k];
                x[i] -= t;
            }
        }
        for i in (0..n).rev() {
            for k in i + 1..n {
                let t = self.a[i * n + k] * x[k];
                x[i] -= t;
            }
            x[i] /= self.a[i * n + i];
        }
        x
    }
}

pub fn det(m: &Matrix) -> EResult<Num> {
    if !m.is_square() {
        return Err("определитель определён только для квадратной матрицы".into());
    }
    let d = m.uniform_dim()?;
    let dim = d.powf(m.rows as f64).unwrap_or(d);
    let f = lu(m);
    if f.singular {
        return Ok(Num::new(C64::new(0.0, 0.0), dim));
    }
    let mut p = C64::new(f.sign, 0.0);
    for i in 0..f.n {
        p *= f.a[i * f.n + i];
    }
    Ok(Num::new(p, dim))
}

pub fn inverse(m: &Matrix) -> EResult<Matrix> {
    if !m.is_square() {
        return Err("обратить можно только квадратную матрицу".into());
    }
    let d = m.uniform_dim()?;
    let f = lu(m);
    if f.singular {
        return Err("Матрица вырождена".into());
    }
    let n = m.rows;
    let mut out = Matrix::zeros(n, n);
    for j in 0..n {
        let mut e = vec![C64::new(0.0, 0.0); n];
        e[j] = C64::new(1.0, 0.0);
        let col = f.solve(&e);
        for i in 0..n {
            out.set(i, j, Num::new(col[i], d.inv()));
        }
    }
    Ok(out)
}

pub fn lsolve(m: &Matrix, b: &Matrix) -> EResult<Matrix> {
    if !m.is_square() {
        return Err("матрица должна быть квадратной".into());
    }
    if b.cols != 1 || b.rows != m.rows {
        return Err("правая часть должна быть вектором с тем же числом строк, что и матрица".into());
    }
    let dm = m.uniform_dim()?;
    let db = b.uniform_dim()?;
    let f = lu(m);
    if f.singular {
        return Err("Матрица вырождена".into());
    }
    let rhs: Vec<C64> = b.data.iter().map(|x| x.v).collect();
    let x = f.solve(&rhs);
    Ok(Matrix::column(x.into_iter().map(|v| Num::new(v, db.div(dm))).collect()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mat(rows: usize, cols: usize, v: &[f64]) -> Matrix {
        Matrix::new(rows, cols, v.iter().map(|x| Num::real(*x)).collect())
    }

    #[test]
    fn linalg() {
        let m = mat(2, 2, &[4.0, 3.0, 6.0, 3.0]);
        assert!((det(&m).unwrap().v.re + 6.0).abs() < 1e-12);
        let inv = inverse(&m).unwrap();
        let p = matmul(&m, &inv).unwrap();
        assert!((p.get(0, 0).v.re - 1.0).abs() < 1e-12);
        assert!(p.get(0, 1).v.re.abs() < 1e-12);
        let x = lsolve(&m, &mat(2, 1, &[10.0, 12.0])).unwrap();
        assert!((x.data[0].v.re - 1.0).abs() < 1e-12);
        assert!((x.data[1].v.re - 2.0).abs() < 1e-12);
    }

    #[test]
    fn ranges() {
        let r = Range { start: 0.0, step: 0.1, end: 1.0, dim: Dim::NONE };
        assert_eq!(r.len(), 11);
        let r = Range { start: 10.0, step: -1.0, end: 1.0, dim: Dim::NONE };
        assert_eq!(r.len(), 10);
    }
}
