//! Additional Mathcad 15 built-ins: linear algebra, interpolation and
//! regression, statistics, special functions, strings and lookup.
//!
//! Spline / regression coefficient vectors (`lspline`, `pspline`, `cspline`,
//! `regress`) have the layout `[kind, n, 0, c_0, c_1, …]` where `kind` is
//! 1 (lspline), 2 (pspline), 3 (cspline) or 4 (regress). For splines `n` is the
//! number of knots and `c_i` are the second derivatives at the knots; for
//! `regress` `n` is the polynomial degree and `c_j` the coefficients in
//! ascending powers. `interp(vs, vx, vy, x)` accepts all of them.

use crate::builtins::{arity, dimless, flatten, int_arg, map1, same_dim, vector_arg};
use crate::linalg::{self, CMat, RMat};
use crate::special;
use crate::units::Dim;
use crate::value::*;
use num_complex::Complex64 as C64;


fn to_cmat(m: &Matrix) -> CMat {
    (0..m.rows).map(|i| (0..m.cols).map(|j| m.get(i, j).v).collect()).collect()
}

fn to_rmat(m: &Matrix) -> EResult<RMat> {
    if m.data.iter().any(|n| !n.is_real()) {
        return Err("матрица должна быть вещественной".into());
    }
    Ok((0..m.rows).map(|i| (0..m.cols).map(|j| m.get(i, j).v.re).collect()).collect())
}

fn from_cmat(a: &CMat, dim: Dim) -> Matrix {
    let cols = a.first().map_or(0, |r| r.len());
    Matrix::new(a.len(), cols, a.iter().flatten().map(|&z| Num::new(z, dim)).collect())
}

fn from_rmat(a: &RMat, dim: Dim) -> Matrix {
    let cols = a.first().map_or(0, |r| r.len());
    Matrix::new(a.len(), cols, a.iter().flatten().map(|&x| Num::with_dim(x, dim)).collect())
}

fn hcat(parts: &[Matrix]) -> Matrix {
    let rows = parts[0].rows;
    let cols: usize = parts.iter().map(|m| m.cols).sum();
    let mut out = Matrix::zeros(rows, cols);
    let mut c0 = 0;
    for m in parts {
        for i in 0..rows {
            for j in 0..m.cols {
                out.set(i, c0 + j, m.get(i, j));
            }
        }
        c0 += m.cols;
    }
    out
}

fn vcat(a: Matrix, b: Matrix) -> Matrix {
    let mut data = a.data;
    data.extend(b.data);
    Matrix::new(a.rows + b.rows, a.cols, data)
}

fn square_matrix(v: &Value) -> EResult<Matrix> {
    let m = v.to_matrix()?;
    if !m.is_square() {
        return Err("нужна квадратная матрица".into());
    }
    Ok(m)
}

fn col(data: Vec<Num>) -> Value {
    Value::Matrix(Matrix::column(data))
}

/// Applies `f` to a real dimensionless scalar or to each element of a matrix.
fn real_checked(v: &Value, f: impl Fn(f64) -> EResult<f64>) -> EResult<Value> {
    map1(v, |n| {
        let z = dimless(n)?;
        if z.im != 0.0 {
            return Err("аргумент должен быть вещественным".into());
        }
        Ok(Num::real(f(z.re)?))
    })
}

fn positive(x: f64) -> EResult<f64> {
    if x > 0.0 {
        Ok(x)
    } else {
        Err("аргумент должен быть положительным".into())
    }
}

// ---------------------------------------------------------------------------
// Linear algebra

fn eigen_values(args: &[Value]) -> EResult<Value> {
    arity("eigenvals", args, 1)?;
    let m = square_matrix(&args[0])?;
    let d = m.uniform_dim()?;
    let (vals, _) = linalg::eigen_system(&to_cmat(&m), false)?;
    Ok(col(vals.into_iter().map(|z| Num::new(z, d)).collect()))
}

fn eigen_vectors(args: &[Value]) -> EResult<Value> {
    arity("eigenvecs", args, 1)?;
    let m = square_matrix(&args[0])?;
    m.uniform_dim()?;
    let (_, vecs) = linalg::eigen_system(&to_cmat(&m), true)?;
    let n = m.rows;
    let mut out = Matrix::zeros(n, n);
    for (j, v) in vecs.iter().enumerate() {
        for i in 0..n {
            out.set(i, j, Num::new(v[i], Dim::NONE));
        }
    }
    Ok(Value::Matrix(out))
}

fn eigen_vector(args: &[Value]) -> EResult<Value> {
    arity("eigenvec", args, 2)?;
    let m = square_matrix(&args[0])?;
    let d = m.uniform_dim()?;
    let lam = args[1].as_num()?;
    if lam.dim != d {
        return Err("Единицы измерения не совпадают".into());
    }
    let a = to_cmat(&m);
    let v = linalg::eigenvector(&a, lam.v, &[], 0);
    let scale = linalg::max_abs(&a).max(1e-300);
    for i in 0..m.rows {
        let av: C64 = (0..m.rows).map(|j| a[i][j] * v[j]).sum();
        if (av - lam.v * v[i]).norm() > 1e-6 * scale.max(lam.v.norm()) {
            return Err("число не является собственным значением матрицы".into());
        }
    }
    Ok(col(v.into_iter().map(|z| Num::new(z, Dim::NONE)).collect()))
}

fn matrix_rank(args: &[Value]) -> EResult<Value> {
    arity("rank", args, 1)?;
    let m = args[0].to_matrix()?;
    m.uniform_dim()?;
    let (_, r) = linalg::rref(&to_cmat(&m));
    Ok(Value::real(r as f64))
}

fn cholesky(args: &[Value]) -> EResult<Value> {
    arity("cholesky", args, 1)?;
    let m = square_matrix(&args[0])?;
    let d = m.uniform_dim()?;
    let dim = d.powf(0.5).ok_or("корень из этих единиц измерения представить нельзя")?;
    Ok(Value::Matrix(from_cmat(&linalg::cholesky(&to_cmat(&m))?, dim)))
}

fn lu_decomp(args: &[Value]) -> EResult<Value> {
    arity("lu", args, 1)?;
    let m = square_matrix(&args[0])?;
    let d = m.uniform_dim()?;
    let (p, l, u) = linalg::lu_decompose(&to_cmat(&m))?;
    Ok(Value::Matrix(hcat(&[from_cmat(&p, Dim::NONE), from_cmat(&l, Dim::NONE), from_cmat(&u, d)])))
}

fn qr_decomp(args: &[Value]) -> EResult<Value> {
    arity("qr", args, 1)?;
    let m = args[0].to_matrix()?;
    let d = m.uniform_dim()?;
    let (q, r) = linalg::qr_real(&to_rmat(&m)?);
    Ok(Value::Matrix(hcat(&[from_rmat(&q, Dim::NONE), from_rmat(&r, d)])))
}

fn svd_all(args: &[Value], name: &str) -> EResult<(Matrix, Vec<f64>, Matrix, Dim)> {
    arity(name, args, 1)?;
    let m = args[0].to_matrix()?;
    let d = m.uniform_dim()?;
    let (u, s, v) = linalg::svd_real(&to_rmat(&m)?);
    Ok((from_rmat(&u, Dim::NONE), s, from_rmat(&v, Dim::NONE), d))
}

fn geninv(args: &[Value]) -> EResult<Value> {
    arity("geninv", args, 1)?;
    let m = args[0].to_matrix()?;
    let d = m.uniform_dim()?;
    let a = to_rmat(&m)?;
    let (u, s, v) = linalg::svd_real(&a);
    let tol = (m.rows.max(m.cols) as f64) * f64::EPSILON * s.first().copied().unwrap_or(0.0);
    let mut out = vec![vec![0.0; m.rows]; m.cols];
    for k in 0..s.len() {
        if s[k] <= tol {
            continue;
        }
        for i in 0..m.cols {
            for j in 0..m.rows {
                out[i][j] += v[i][k] * u[j][k] / s[k];
            }
        }
    }
    Ok(Value::Matrix(from_rmat(&out, d.inv())))
}

fn rref(args: &[Value]) -> EResult<Value> {
    arity("rref", args, 1)?;
    let m = args[0].to_matrix()?;
    m.uniform_dim()?;
    Ok(Value::Matrix(from_cmat(&linalg::rref(&to_cmat(&m)).0, Dim::NONE)))
}

fn matrix_norm(args: &[Value], name: &str) -> EResult<Value> {
    arity(name, args, 1)?;
    let m = args[0].to_matrix()?;
    let d = m.uniform_dim()?;
    let x = match name {
        "norm1" => (0..m.cols).map(|j| (0..m.rows).map(|i| m.get(i, j).v.norm()).sum::<f64>()).fold(0.0, f64::max),
        "normi" => (0..m.rows).map(|i| (0..m.cols).map(|j| m.get(i, j).v.norm()).sum::<f64>()).fold(0.0, f64::max),
        _ => linalg::svd_real(&to_rmat(&m)?).1.first().copied().unwrap_or(0.0),
    };
    Ok(Value::Num(Num::with_dim(x, d)))
}

fn cond2(args: &[Value]) -> EResult<Value> {
    arity("cond2", args, 1)?;
    let m = args[0].to_matrix()?;
    m.uniform_dim()?;
    let s = linalg::svd_real(&to_rmat(&m)?).1;
    let (hi, lo) = (s.first().copied().unwrap_or(0.0), s.last().copied().unwrap_or(0.0));
    if lo <= 1e-15 * hi {
        return Err("Матрица вырождена".into());
    }
    Ok(Value::real(hi / lo))
}

fn diag(args: &[Value]) -> EResult<Value> {
    arity("diag", args, 1)?;
    let v = vector_arg(&args[0], "diag: аргумент")?;
    let n = v.rows;
    let mut out = Matrix::zeros(n, n);
    for i in 0..n {
        out.set(i, i, v.data[i]);
    }
    Ok(Value::Matrix(out))
}

fn sort_lines(args: &[Value], origin: i64, by_column: bool) -> EResult<Value> {
    let name = if by_column { "csort" } else { "rsort" };
    arity(name, args, 2)?;
    let m = args[0].to_matrix()?;
    let k = int_arg(&args[1])? - origin;
    let limit = if by_column { m.cols } else { m.rows };
    if k < 0 || k as usize >= limit {
        return Err("индекс вне допустимых границ".into());
    }
    let k = k as usize;
    let count = if by_column { m.rows } else { m.cols };
    let key = |t: usize| if by_column { m.get(t, k) } else { m.get(k, t) };
    let keys: Vec<Num> = (0..count).map(key).collect();
    same_dim(&keys)?;
    let mut order: Vec<usize> = (0..count).collect();
    order.sort_by(|&a, &b| keys[a].v.re.total_cmp(&keys[b].v.re));
    let mut out = Matrix::zeros(m.rows, m.cols);
    for (new, &old) in order.iter().enumerate() {
        for t in 0..(if by_column { m.cols } else { m.rows }) {
            if by_column {
                out.set(new, t, m.get(old, t));
            } else {
                out.set(t, new, m.get(t, old));
            }
        }
    }
    Ok(Value::Matrix(out))
}

// ---------------------------------------------------------------------------
// Interpolation and regression

struct XY {
    x: Vec<f64>,
    dx: Dim,
    y: Vec<f64>,
    dy: Dim,
}

fn real_vec(m: &Matrix) -> EResult<(Vec<f64>, Dim)> {
    let d = same_dim(&m.data)?;
    if m.data.iter().any(|n| !n.is_real()) {
        return Err("значения должны быть вещественными".into());
    }
    Ok((m.data.iter().map(|n| n.v.re).collect(), d))
}

fn xy_args(name: &str, vx: &Value, vy: &Value, min_len: usize) -> EResult<XY> {
    let mx = vector_arg(vx, &format!("{name}: vx"))?;
    let my = vector_arg(vy, &format!("{name}: vy"))?;
    if mx.rows != my.rows || mx.rows < min_len {
        return Err(format!("vx и vy должны иметь одинаковую длину (не менее {min_len})"));
    }
    let (x, dx) = real_vec(&mx)?;
    let (y, dy) = real_vec(&my)?;
    Ok(XY { x, dx, y, dy })
}

/// Second derivatives of the interpolating cubic spline at the knots.
fn spline_m(x: &[f64], y: &[f64], kind: u8) -> EResult<Vec<f64>> {
    let n = x.len();
    if x.windows(2).any(|w| w[1] <= w[0]) {
        return Err("значения vx должны строго возрастать".into());
    }
    if n == 2 {
        return Ok(vec![0.0, 0.0]);
    }
    let kind = if n == 3 && kind == 3 { 2 } else { kind };
    let h: Vec<f64> = x.windows(2).map(|w| w[1] - w[0]).collect();
    let k = n - 2;
    let mut lower = vec![0.0; k];
    let mut diag = vec![0.0; k];
    let mut upper = vec![0.0; k];
    let mut rhs = vec![0.0; k];
    for j in 0..k {
        let i = j + 1;
        lower[j] = h[i - 1];
        diag[j] = 2.0 * (h[i - 1] + h[i]);
        upper[j] = h[i];
        rhs[j] = 6.0 * ((y[i + 1] - y[i]) / h[i] - (y[i] - y[i - 1]) / h[i - 1]);
    }
    let (a, b) = match kind {
        1 => (0.0, 0.0),
        2 => (1.0, 0.0),
        _ => (2.0, -1.0),
    };
    diag[0] += h[0] * a;
    if k > 1 {
        upper[0] += h[0] * b;
    }
    diag[k - 1] += h[n - 2] * a;
    if k > 1 {
        lower[k - 1] += h[n - 2] * b;
    }
    // Thomas algorithm
    for j in 1..k {
        let f = lower[j] / diag[j - 1];
        diag[j] -= f * upper[j - 1];
        rhs[j] -= f * rhs[j - 1];
    }
    let mut mid = vec![0.0; k];
    mid[k - 1] = rhs[k - 1] / diag[k - 1];
    for j in (0..k - 1).rev() {
        mid[j] = (rhs[j] - upper[j] * mid[j + 1]) / diag[j];
    }
    let mut m = vec![0.0; n];
    m[1..n - 1].copy_from_slice(&mid);
    m[0] = a * m[1] + b * if n > 3 { m[2] } else { 0.0 };
    m[n - 1] = a * m[n - 2] + b * if n > 3 { m[n - 3] } else { 0.0 };
    Ok(m)
}

fn spline(args: &[Value], kind: u8, name: &str) -> EResult<Value> {
    arity(name, args, 2)?;
    let xy = xy_args(name, &args[0], &args[1], 2)?;
    let m = spline_m(&xy.x, &xy.y, kind)?;
    let d2 = xy.dy.div(xy.dx.mul(xy.dx));
    let mut data = vec![Num::real(kind as f64), Num::real(xy.x.len() as f64), Num::zero()];
    data.extend(m.into_iter().map(|v| Num::with_dim(v, d2)));
    Ok(col(data))
}

fn spline_eval(x: &[f64], y: &[f64], m: &[f64], kind: u8, t: f64) -> f64 {
    let n = x.len();
    let slope0 = (y[1] - y[0]) / (x[1] - x[0]) - (x[1] - x[0]) * (2.0 * m[0] + m[1]) / 6.0;
    let h_last = x[n - 1] - x[n - 2];
    let slope1 = (y[n - 1] - y[n - 2]) / h_last + h_last * (m[n - 2] + 2.0 * m[n - 1]) / 6.0;
    if kind == 1 && t < x[0] {
        return y[0] + slope0 * (t - x[0]);
    }
    if kind == 1 && t > x[n - 1] {
        return y[n - 1] + slope1 * (t - x[n - 1]);
    }
    let mut i = 0;
    while i < n - 2 && t > x[i + 1] {
        i += 1;
    }
    let h = x[i + 1] - x[i];
    let (a, b) = (x[i + 1] - t, t - x[i]);
    m[i] * a * a * a / (6.0 * h)
        + m[i + 1] * b * b * b / (6.0 * h)
        + (y[i] / h - m[i] * h / 6.0) * a
        + (y[i + 1] / h - m[i + 1] * h / 6.0) * b
}

fn interp(args: &[Value]) -> EResult<Value> {
    arity("interp", args, 4)?;
    let vs = vector_arg(&args[0], "interp: vs")?;
    let xy = xy_args("interp", &args[1], &args[2], 2)?;
    if vs.rows < 4 || !vs.data[..3].iter().all(|n| n.dim.is_none()) {
        return Err("interp: неверный вектор коэффициентов vs".into());
    }
    let kind = vs.data[0].v.re.round() as u8;
    let coef: Vec<f64> = vs.data[3..].iter().map(|n| n.v.re).collect();
    if kind == 5 {
        // loess: vs = [5, n, 0, span]
        if coef.len() != 1 || vs.data[1].v.re as usize != xy.x.len() {
            return Err("interp: vs не соответствует vx и vy".into());
        }
        return map1(&args[3], |t| {
            if t.dim != xy.dx {
                return Err("Единицы измерения не совпадают".into());
            }
            Ok(Num::with_dim(crate::fitting::loess_at(&xy.x, &xy.y, coef[0], t.v.re), xy.dy))
        });
    }
    if kind == 4 {
        return map1(&args[3], |t| {
            if t.dim != xy.dx {
                return Err("Единицы измерения не совпадают".into());
            }
            Ok(Num::with_dim(coef.iter().rev().fold(0.0, |acc, c| acc * t.v.re + c), xy.dy))
        });
    }
    if !(1..=3).contains(&kind) || coef.len() != xy.x.len() {
        return Err("interp: vs не соответствует vx и vy".into());
    }
    map1(&args[3], |t| {
        if t.dim != xy.dx {
            return Err("Единицы измерения не совпадают".into());
        }
        Ok(Num::with_dim(spline_eval(&xy.x, &xy.y, &coef, kind, t.v.re), xy.dy))
    })
}

fn line_fit(xy: &XY) -> (f64, f64) {
    let n = xy.x.len() as f64;
    let (mx, my) = (xy.x.iter().sum::<f64>() / n, xy.y.iter().sum::<f64>() / n);
    let sxx: f64 = xy.x.iter().map(|x| (x - mx) * (x - mx)).sum();
    let sxy: f64 = xy.x.iter().zip(&xy.y).map(|(x, y)| (x - mx) * (y - my)).sum();
    let slope = sxy / sxx;
    (my - slope * mx, slope)
}

fn regress(args: &[Value]) -> EResult<Value> {
    arity("regress", args, 3)?;
    let xy = xy_args("regress", &args[0], &args[1], 2)?;
    let k = int_arg(&args[2])?;
    if k < 1 || k as usize >= xy.x.len() {
        return Err("степень многочлена должна быть от 1 до n-1".into());
    }
    let k = k as usize;
    // scale x for conditioning
    let s = xy.x.iter().fold(0.0f64, |a, x| a.max(x.abs())).max(1e-300);
    let a: RMat = xy.x.iter().map(|x| (0..=k).map(|j| (x / s).powi(j as i32)).collect()).collect();
    let c = linalg::lstsq(&a, &xy.y)?;
    let mut data = vec![Num::real(4.0), Num::real(k as f64), Num::zero()];
    for (j, cj) in c.iter().enumerate() {
        let dim = xy.dy.div(xy.dx.powf(j as f64).ok_or("такую степень единиц измерения представить нельзя")?);
        data.push(Num::with_dim(cj / s.powi(j as i32), dim));
    }
    Ok(col(data))
}

// ---------------------------------------------------------------------------
// Statistics

fn real_values(args: &[Value]) -> EResult<(Vec<f64>, Dim)> {
    let v = flatten(args)?;
    let d = same_dim(&v)?;
    if v.iter().any(|n| !n.is_real()) {
        return Err("значения должны быть вещественными".into());
    }
    Ok((v.iter().map(|n| n.v.re).collect(), d))
}

fn standardized_moment(args: &[Value], power: i32, name: &str) -> EResult<Value> {
    arity(name, args, 1)?;
    let (x, _) = real_values(args)?;
    let n = x.len() as f64;
    let need = if power == 3 { 3 } else { 4 };
    if x.len() < need {
        return Err(format!("нужно не менее {need} значений"));
    }
    let mean = x.iter().sum::<f64>() / n;
    let s = (x.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / (n - 1.0)).sqrt();
    if s == 0.0 {
        return Err("Деление на ноль".into());
    }
    let sum: f64 = x.iter().map(|v| ((v - mean) / s).powi(power)).sum();
    Ok(Value::real(if power == 3 {
        n / ((n - 1.0) * (n - 2.0)) * sum
    } else {
        n * (n + 1.0) / ((n - 1.0) * (n - 2.0) * (n - 3.0)) * sum - 3.0 * (n - 1.0).powi(2) / ((n - 2.0) * (n - 3.0))
    }))
}

fn histogram(args: &[Value]) -> EResult<Value> {
    arity("hist", args, 2)?;
    let (data, dd) = real_values(&args[1..])?;
    let (edges, last_closed) = match &args[0] {
        Value::Num(n) => {
            let k = args[0].as_int()?;
            if k < 1 || k > 1_000_000 {
                return Err("недопустимое число интервалов".into());
            }
            let _ = n;
            let lo = data.iter().cloned().fold(f64::INFINITY, f64::min);
            let hi = data.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
            ((0..=k).map(|i| lo + (hi - lo) * i as f64 / k as f64).collect::<Vec<_>>(), true)
        }
        other => {
            let (e, de) = real_values(std::slice::from_ref(other))?;
            if de != dd {
                return Err("Единицы измерения не совпадают".into());
            }
            if e.len() < 2 || e.windows(2).any(|w| w[1] < w[0]) {
                return Err("границы интервалов должны возрастать (не менее двух)".into());
            }
            (e, false)
        }
    };
    let bins = edges.len() - 1;
    let mut counts = vec![0.0; bins];
    for &x in &data {
        for b in 0..bins {
            let inside = x >= edges[b] && (x < edges[b + 1] || (last_closed && b == bins - 1 && x <= edges[b + 1]));
            if inside {
                counts[b] += 1.0;
                break;
            }
        }
    }
    Ok(col(counts.into_iter().map(Num::real).collect()))
}

fn gcd(a: i64, b: i64) -> i64 {
    let (mut a, mut b) = (a.abs(), b.abs());
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}

fn int_list(args: &[Value]) -> EResult<Vec<i64>> {
    let v = flatten(args)?;
    v.iter().map(|n| Value::Num(*n).as_int()).collect()
}

// ---------------------------------------------------------------------------
// Strings and lookup

fn str_arg<'a>(v: &'a Value, what: &str) -> EResult<&'a str> {
    match v {
        Value::Str(s) => Ok(s),
        _ => Err(format!("{what}: ожидалась строка")),
    }
}

fn closest(z: Num, items: &[Num]) -> EResult<usize> {
    let mut best = None;
    for (i, n) in items.iter().enumerate() {
        if n.dim != z.dim && !(n.v.norm() == 0.0 && n.dim.is_none()) {
            return Err("Единицы измерения не совпадают".into());
        }
        let d = (n.v - z.v).norm();
        if best.map_or(true, |(_, bd)| d < bd) {
            best = Some((i, d));
        }
    }
    best.map(|(i, _)| i).ok_or_else(|| "нет значений".to_string())
}

fn lookup(args: &[Value]) -> EResult<Value> {
    arity("lookup", args, 3)?;
    let z = args[0].as_num()?;
    let a = args[1].to_matrix()?;
    let b = args[2].to_matrix()?;
    if a.data.len() != b.data.len() {
        return Err("A и B должны иметь одинаковый размер".into());
    }
    Ok(Value::Num(b.data[closest(z, &a.data)?]))
}

fn match_fn(args: &[Value], origin: i64) -> EResult<Value> {
    arity("match", args, 2)?;
    let z = args[0].as_num()?;
    let a = args[1].to_matrix()?;
    let mut hits = Vec::new();
    for i in 0..a.rows {
        for j in 0..a.cols {
            let n = a.get(i, j);
            if n.dim == z.dim && (n.v - z.v).norm() <= 1e-12 * n.v.norm().max(z.v.norm()) {
                hits.push((i, j));
            }
        }
    }
    if hits.is_empty() {
        return Err("значение не найдено".into());
    }
    let idx = |k: usize| Num::real((k as i64 + origin) as f64);
    if a.is_vector() {
        let k = |&(i, j): &(usize, usize)| idx(i.max(j));
        if hits.len() == 1 {
            return Ok(Value::Num(k(&hits[0])));
        }
        return Ok(col(hits.iter().map(k).collect()));
    }
    let mut out = Matrix::zeros(hits.len(), 2);
    for (r, &(i, j)) in hits.iter().enumerate() {
        out.set(r, 0, idx(i));
        out.set(r, 1, idx(j));
    }
    Ok(Value::Matrix(out))
}

fn vh_lookup(args: &[Value], origin: i64, vertical: bool) -> EResult<Value> {
    let name = if vertical { "vlookup" } else { "hlookup" };
    arity(name, args, 3)?;
    let z = args[0].as_num()?;
    let a = args[1].to_matrix()?;
    let k = int_arg(&args[2])? - origin;
    let (keys, limit): (Vec<Num>, usize) = if vertical {
        ((0..a.rows).map(|i| a.get(i, 0)).collect(), a.cols)
    } else {
        ((0..a.cols).map(|j| a.get(0, j)).collect(), a.rows)
    };
    if k < 0 || k as usize >= limit {
        return Err("индекс вне допустимых границ".into());
    }
    let hit = closest(z, &keys)?;
    Ok(Value::Num(if vertical { a.get(hit, k as usize) } else { a.get(k as usize, hit) }))
}

fn str2num(args: &[Value]) -> EResult<Value> {
    arity("str2num", args, 1)?;
    let s = str_arg(&args[0], "str2num")?;
    let e = crate::parser::parse_expr(s.trim()).map_err(|_| "не удалось преобразовать строку в число".to_string())?;
    let ev = crate::eval::Evaluator::new();
    let v = ev.eval(&e, &ev.top_scope()).map_err(|_| "не удалось преобразовать строку в число".to_string())?;
    Ok(Value::Num(v.as_num().map_err(|_| "не удалось преобразовать строку в число".to_string())?))
}

// ---------------------------------------------------------------------------

/// Calls the built-in `name` if it is defined in this module.
pub fn call(name: &str, args: &[Value], origin: i64) -> Option<EResult<Value>> {
    let r = match name {
        "eigenvals" => eigen_values(args),
        "eigenvecs" => eigen_vectors(args),
        "eigenvec" => eigen_vector(args),
        "rank" => matrix_rank(args),
        "cholesky" => cholesky(args),
        "lu" => lu_decomp(args),
        "qr" => qr_decomp(args),
        "svd" => svd_all(args, name).map(|(u, _, v, _)| Value::Matrix(vcat(u, v))),
        "svds" => svd_all(args, name).map(|(_, s, _, d)| col(s.into_iter().map(|x| Num::with_dim(x, d)).collect())),
        "geninv" => geninv(args),
        "rref" => rref(args),
        "norm1" | "norm2" | "normi" => matrix_norm(args, name),
        "cond2" => cond2(args),
        "diag" => diag(args),
        "csort" => sort_lines(args, origin, true),
        "rsort" => sort_lines(args, origin, false),
        "lspline" => spline(args, 1, name),
        "pspline" => spline(args, 2, name),
        "cspline" => spline(args, 3, name),
        "interp" => interp(args),
        "slope" | "intercept" | "line" | "corr" => (|| {
            arity(name, args, 2)?;
            let xy = xy_args(name, &args[0], &args[1], 2)?;
            let (b, a) = line_fit(&xy);
            let slope = Num::with_dim(a, xy.dy.div(xy.dx));
            let icpt = Num::with_dim(b, xy.dy);
            Ok(match name {
                "slope" => Value::Num(slope),
                "intercept" => Value::Num(icpt),
                "line" => col(vec![icpt, slope]),
                _ => {
                    let n = xy.x.len() as f64;
                    let (mx, my) = (xy.x.iter().sum::<f64>() / n, xy.y.iter().sum::<f64>() / n);
                    let sxx: f64 = xy.x.iter().map(|x| (x - mx).powi(2)).sum();
                    let syy: f64 = xy.y.iter().map(|y| (y - my).powi(2)).sum();
                    let sxy: f64 = xy.x.iter().zip(&xy.y).map(|(x, y)| (x - mx) * (y - my)).sum();
                    Value::real(sxy / (sxx * syy).sqrt())
                }
            })
        })(),
        "regress" => regress(args),
        "cvar" => (|| {
            arity(name, args, 2)?;
            let a = flatten(&args[..1])?;
            let b = flatten(&args[1..])?;
            if a.len() != b.len() {
                return Err("массивы должны иметь одинаковый размер".into());
            }
            let (da, db) = (same_dim(&a)?, same_dim(&b)?);
            let n = a.len() as f64;
            let ma: C64 = a.iter().map(|x| x.v).sum::<C64>() / n;
            let mb: C64 = b.iter().map(|x| x.v).sum::<C64>() / n;
            let s: C64 = a.iter().zip(&b).map(|(x, y)| (x.v - ma) * (y.v - mb).conj()).sum();
            Ok(Value::Num(Num::new(s / n, da.mul(db))))
        })(),
        "skew" => standardized_moment(args, 3, name),
        "kurt" => standardized_moment(args, 4, name),
        "hist" => histogram(args),
        "gmean" | "hmean" => (|| {
            let (x, d) = real_values(args)?;
            let n = x.len() as f64;
            if x.iter().any(|&v| v <= 0.0) {
                return Err("значения должны быть положительными".into());
            }
            Ok(Value::Num(Num::with_dim(
                if name == "gmean" {
                    (x.iter().map(|v| v.ln()).sum::<f64>() / n).exp()
                } else {
                    n / x.iter().map(|v| 1.0 / v).sum::<f64>()
                },
                d,
            )))
        })(),
        "mode" => (|| {
            let (mut x, d) = real_values(args)?;
            x.sort_by(f64::total_cmp);
            let (mut best, mut best_n, mut i) = (0.0, 0, 0);
            while i < x.len() {
                let j = x[i..].iter().take_while(|&&v| v == x[i]).count();
                if j > best_n {
                    best = x[i];
                    best_n = j;
                }
                i += j;
            }
            if best_n < 2 {
                return Err("в данных нет моды".into());
            }
            Ok(Value::Num(Num::with_dim(best, d)))
        })(),
        "gcd" | "lcm" => (|| {
            let v = int_list(args)?;
            if name == "gcd" {
                return Ok(Value::real(v.iter().fold(0, |a, &b| gcd(a, b)) as f64));
            }
            let mut acc: i64 = 1;
            for &b in &v {
                if b == 0 {
                    return Ok(Value::real(0.0));
                }
                acc = (acc / gcd(acc, b)).checked_mul(b.abs()).ok_or("переполнение")?;
            }
            Ok(Value::real(acc as f64))
        })(),
        "Φ" | "heaviside" => (|| {
            arity(name, args, 1)?;
            map1(&args[0], |n| Ok(Num::real(if n.v.re >= 0.0 { 1.0 } else { 0.0 })))
        })(),
        "δ" | "kronecker" => (|| {
            arity(name, args, 2)?;
            Ok(Value::bool(int_arg(&args[0])? == int_arg(&args[1])?))
        })(),
        "sinc" => (|| {
            arity(name, args, 1)?;
            real_checked(&args[0], |x| Ok(if x == 0.0 { 1.0 } else { x.sin() / x }))
        })(),
        "csgn" => (|| {
            arity(name, args, 1)?;
            map1(&args[0], |n| {
                let z = dimless(n)?;
                Ok(Num::real(if z.re > 0.0 || (z.re == 0.0 && z.im > 0.0) {
                    1.0
                } else if z.re < 0.0 || (z.re == 0.0 && z.im < 0.0) {
                    -1.0
                } else {
                    0.0
                }))
            })
        })(),
        "qnorm" => (|| {
            arity(name, args, 3)?;
            let (mu, sigma) = (args[1].as_real()?, args[2].as_real()?);
            if sigma <= 0.0 {
                return Err("σ должна быть положительной".into());
            }
            real_checked(&args[0], |p| {
                if p <= 0.0 || p >= 1.0 {
                    return Err("вероятность должна быть между 0 и 1".into());
                }
                Ok(mu + sigma * special::qnorm_std(p))
            })
        })(),
        "dchisq" | "pchisq" | "qchisq" => (|| {
            arity(name, args, 2)?;
            let k = args[1].as_real()?;
            if k <= 0.0 {
                return Err("число степеней свободы должно быть положительным".into());
            }
            real_checked(&args[0], |x| match name {
                "dchisq" => {
                    if x < 0.0 {
                        return Err("аргумент должен быть неотрицательным".into());
                    }
                    Ok(special::chisq_pdf(x, k))
                }
                "pchisq" => Ok(special::chisq_cdf(x, k)),
                _ => {
                    if !(0.0..=1.0).contains(&x) {
                        return Err("вероятность должна быть между 0 и 1".into());
                    }
                    Ok(special::chisq_quantile(x, k))
                }
            })
        })(),
        "dunif" | "punif" => (|| {
            arity(name, args, 3)?;
            let (a, b) = (args[1].as_real()?, args[2].as_real()?);
            if b <= a {
                return Err("должно выполняться a < b".into());
            }
            real_checked(&args[0], |x| {
                Ok(if name == "dunif" {
                    if x >= a && x <= b {
                        1.0 / (b - a)
                    } else {
                        0.0
                    }
                } else {
                    ((x - a) / (b - a)).clamp(0.0, 1.0)
                })
            })
        })(),
        "dexp" | "pexp" => (|| {
            arity(name, args, 2)?;
            let r = args[1].as_real()?;
            if r <= 0.0 {
                return Err("параметр должен быть положительным".into());
            }
            real_checked(&args[0], |x| {
                Ok(match (name, x < 0.0) {
                    (_, true) => 0.0,
                    ("dexp", _) => r * (-r * x).exp(),
                    _ => 1.0 - (-r * x).exp(),
                })
            })
        })(),
        "J0" | "J1" | "Y0" | "Y1" | "I0" | "I1" | "K0" | "K1" => (|| {
            arity(name, args, 1)?;
            let order = if name.ends_with('0') { 0 } else { 1 };
            real_checked(&args[0], |x| bessel(&name[..1], order, x))
        })(),
        "Jn" | "Yn" | "In" | "Kn" => (|| {
            arity(name, args, 2)?;
            let m = int_arg(&args[0])?;
            real_checked(&args[1], |x| bessel(&name[..1], m, x))
        })(),
        "substr" => (|| {
            arity(name, args, 3)?;
            let s = str_arg(&args[0], name)?;
            let start = int_arg(&args[1])? - origin;
            let len = int_arg(&args[2])?;
            let count = s.chars().count() as i64;
            if start < 0 || start > count || len < 0 {
                return Err("индекс вне допустимых границ".into());
            }
            Ok(Value::Str(s.chars().skip(start as usize).take(len as usize).collect::<String>().into()))
        })(),
        "search" => (|| {
            arity(name, args, 3)?;
            let s: Vec<char> = str_arg(&args[0], name)?.chars().collect();
            let sub: Vec<char> = str_arg(&args[1], name)?.chars().collect();
            let start = int_arg(&args[2])? - origin;
            if start < 0 {
                return Err("индекс вне допустимых границ".into());
            }
            let found = (start as usize..=s.len().saturating_sub(sub.len()))
                .find(|&i| i + sub.len() <= s.len() && s[i..i + sub.len()] == sub[..]);
            Ok(Value::real(found.map_or(-1.0, |i| (i as i64 + origin) as f64)))
        })(),
        "str2num" => str2num(args),
        "vec2str" => (|| {
            arity(name, args, 1)?;
            let v = vector_arg(&args[0], "vec2str: аргумент")?;
            let mut s = String::new();
            for n in &v.data {
                let c = Value::Num(*n).as_int()?;
                s.push(u32::try_from(c).ok().and_then(char::from_u32).ok_or("недопустимый код символа")?);
            }
            Ok(Value::Str(s.into()))
        })(),
        "str2vec" => (|| {
            arity(name, args, 1)?;
            let s = str_arg(&args[0], name)?;
            Ok(col(s.chars().map(|c| Num::real(c as u32 as f64)).collect()))
        })(),
        "lookup" => lookup(args),
        "match" => match_fn(args, origin),
        "vlookup" => vh_lookup(args, origin, true),
        "hlookup" => vh_lookup(args, origin, false),
        _ => return crate::fitting::call(name, args),
    };
    Some(r)
}

fn bessel(kind: &str, n: i64, x: f64) -> EResult<f64> {
    let order = i32::try_from(n).map_err(|_| "порядок слишком велик".to_string())?;
    Ok(match kind {
        "J" => libm::jn(order, x),
        "Y" => libm::yn(order, positive(x)?),
        "I" => special::bessel_i(n, x),
        _ => special::bessel_k(n, positive(x)?),
    })
}
