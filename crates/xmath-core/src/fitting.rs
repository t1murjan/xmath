//! Curve fitting (`expfit`, `genfit`, ...), smoothing (`loess`, `ksmooth`, ...),
//! linear prediction and random number generators.
//!
//! `loess(vx, vy, span)` returns `[5, n, 0, span]`, which `interp(vs, vx, vy, x)`
//! understands (see `builtins_ext`): it evaluates the local quadratic fit at `x`.

use crate::builtins::{arity, vector_arg};
use crate::units::Dim;
use crate::value::*;
use std::cell::Cell;

type Calc<'a> = &'a dyn Fn(&UserFunc, Vec<Value>) -> EResult<Value>;

pub const NAMES: &[&str] = &[
    "expfit", "lgsfit", "logfit", "lnfit", "pwrfit", "sinfit", "linfit", "genfit", "medfit", "loess", "ksmooth",
    "medsmooth", "supsmooth", "predict", "rnd", "runif", "rnorm", "rexp", "rpois", "rbinom", "rgamma", "rchisq",
    "rbeta", "rt", "rlnorm", "rweibull", "rcauchy", "rlogis", "rgeom", "dbinom", "pbinom", "dpois", "ppois",
];

fn col(v: Vec<f64>) -> Value {
    Value::Matrix(Matrix::column(v.into_iter().map(Num::real).collect()))
}

fn col_dim(v: Vec<f64>, d: Dim) -> Value {
    Value::Matrix(Matrix::column(v.into_iter().map(|x| Num::with_dim(x, d)).collect()))
}

fn real_vec(v: &Value, what: &str) -> EResult<(Vec<f64>, Dim)> {
    let m = vector_arg(v, what)?;
    let d = crate::builtins::same_dim(&m.data)?;
    if m.data.iter().any(|n| !n.is_real()) {
        return Err(format!("{what}: значения должны быть вещественными"));
    }
    Ok((m.data.iter().map(|n| n.v.re).collect(), d))
}

fn dimless_vec(v: &Value, what: &str) -> EResult<Vec<f64>> {
    let (x, d) = real_vec(v, what)?;
    if !d.is_none() {
        return Err(format!("{what} должен быть безразмерным"));
    }
    Ok(x)
}

fn xy_dimless(name: &str, vx: &Value, vy: &Value, min_len: usize) -> EResult<(Vec<f64>, Vec<f64>)> {
    let x = dimless_vec(vx, &format!("{name}: vx"))?;
    let y = dimless_vec(vy, &format!("{name}: vy"))?;
    if x.len() != y.len() || x.len() < min_len {
        return Err(format!("vx и vy должны иметь одинаковую длину (не менее {min_len})"));
    }
    Ok((x, y))
}

// ---------------------------------------------------------------------------
// Small dense linear algebra and Levenberg-Marquardt

fn solve_small(mut a: Vec<Vec<f64>>, mut b: Vec<f64>) -> Option<Vec<f64>> {
    let n = b.len();
    for c in 0..n {
        let p = (c..n).max_by(|&i, &j| a[i][c].abs().total_cmp(&a[j][c].abs()))?;
        if a[p][c].abs() < 1e-300 || !a[p][c].is_finite() {
            return None;
        }
        a.swap(c, p);
        b.swap(c, p);
        for r in c + 1..n {
            let f = a[r][c] / a[c][c];
            for k in c..n {
                a[r][k] -= f * a[c][k];
            }
            b[r] -= f * b[c];
        }
    }
    let mut x = vec![0.0; n];
    for r in (0..n).rev() {
        let s: f64 = (r + 1..n).map(|k| a[r][k] * x[k]).sum();
        x[r] = (b[r] - s) / a[r][r];
    }
    x.iter().all(|v| v.is_finite()).then_some(x)
}

/// Model value and (optionally) its gradient with respect to the parameters.
type ModelFn<'a> = dyn FnMut(f64, &[f64]) -> EResult<(f64, Option<Vec<f64>>)> + 'a;

fn residuals(f: &mut ModelFn, x: &[f64], y: &[f64], p: &[f64]) -> EResult<(Vec<f64>, Vec<Vec<f64>>)> {
    let mut r = Vec::with_capacity(x.len());
    let mut j = Vec::with_capacity(x.len());
    for (&xi, &yi) in x.iter().zip(y) {
        let (v, g) = f(xi, p)?;
        let g = match g {
            Some(g) if g.len() == p.len() => g,
            _ => {
                let mut g = vec![0.0; p.len()];
                for k in 0..p.len() {
                    let h = 1e-6 * p[k].abs().max(1e-3);
                    let mut q = p.to_vec();
                    q[k] = p[k] + h;
                    let up = f(xi, &q)?.0;
                    q[k] = p[k] - h;
                    let dn = f(xi, &q)?.0;
                    g[k] = (up - dn) / (2.0 * h);
                }
                g
            }
        };
        r.push(yi - v);
        j.push(g);
    }
    Ok((r, j))
}

/// Levenberg-Marquardt least squares for `y ~ f(x, p)`.
fn lm(f: &mut ModelFn, x: &[f64], y: &[f64], p0: &[f64]) -> EResult<Vec<f64>> {
    let n = p0.len();
    let mut p = p0.to_vec();
    let (mut r, mut j) = residuals(f, x, y, &p)?;
    let mut sse: f64 = r.iter().map(|v| v * v).sum();
    if !sse.is_finite() {
        return Err("модель дала нечисловое значение при начальном приближении".into());
    }
    let mut lambda = 1e-3;
    for _ in 0..500 {
        if sse < 1e-30 {
            break;
        }
        let mut a = vec![vec![0.0; n]; n];
        let mut g = vec![0.0; n];
        for (ri, ji) in r.iter().zip(&j) {
            for k in 0..n {
                g[k] += ji[k] * ri;
                for l in 0..n {
                    a[k][l] += ji[k] * ji[l];
                }
            }
        }
        let mut accepted = false;
        while lambda < 1e14 {
            let mut aa = a.clone();
            for k in 0..n {
                aa[k][k] += lambda * a[k][k].max(1e-12);
            }
            if let Some(d) = solve_small(aa, g.clone()) {
                let q: Vec<f64> = p.iter().zip(&d).map(|(a, b)| a + b).collect();
                if let Ok((r2, j2)) = residuals(f, x, y, &q) {
                    let sse2: f64 = r2.iter().map(|v| v * v).sum();
                    if sse2.is_finite() && sse2 <= sse {
                        let done = sse - sse2 <= 1e-14 * sse;
                        p = q;
                        r = r2;
                        j = j2;
                        sse = sse2;
                        lambda = (lambda * 0.3).max(1e-14);
                        accepted = !done;
                        break;
                    }
                }
            }
            lambda *= 10.0;
        }
        if !accepted {
            break;
        }
    }
    Ok(p)
}

fn guess_arg(name: &str, args: &[Value], idx: usize, n: usize) -> EResult<Vec<f64>> {
    let g = dimless_vec(&args[idx], &format!("{name}: vg"))?;
    if g.len() != n {
        return Err(format!("{name}: vg должен содержать {n} элемента(ов)"));
    }
    Ok(g)
}

fn fit_model(
    name: &str,
    args: &[Value],
    np: usize,
    model: fn(f64, &[f64]) -> f64,
) -> EResult<Value> {
    arity(name, args, 3)?;
    let (x, y) = xy_dimless(name, &args[0], &args[1], np)?;
    let g = guess_arg(name, args, 2, np)?;
    let mut f = |x: f64, p: &[f64]| Ok((model(x, p), None));
    Ok(col(lm(&mut f, &x, &y, &g)?))
}

fn expfit(args: &[Value]) -> EResult<Value> {
    if args.len() == 3 {
        return fit_model("expfit", args, 3, |x, p| p[0] * (p[1] * x).exp() + p[2]);
    }
    arity("expfit", args, 2)?;
    let (x, y) = xy_dimless("expfit", &args[0], &args[1], 3)?;
    let n = x.len() as f64;
    let x0 = x.iter().cloned().fold(f64::INFINITY, f64::min);
    let xr = x.iter().cloned().fold(f64::NEG_INFINITY, f64::max) - x0;
    if xr <= 0.0 {
        return Err("значения vx должны различаться".into());
    }
    let xs: Vec<f64> = x.iter().map(|v| v - x0).collect();
    // for a fixed b the model is linear in (a, c): scan b, keep the best
    let mut best: Option<(f64, [f64; 3])> = None;
    for k in -150i32..=150 {
        let b = if k == 0 { 1e-3 / xr } else { k as f64 / 150.0 * 40.0 / xr };
        let e: Vec<f64> = xs.iter().map(|v| (b * v).exp()).collect();
        let (se, see) = (e.iter().sum::<f64>(), e.iter().map(|v| v * v).sum::<f64>());
        let sey: f64 = e.iter().zip(&y).map(|(a, b)| a * b).sum();
        let sy: f64 = y.iter().sum();
        let det = see * n - se * se;
        if det.abs() < 1e-300 || !det.is_finite() {
            continue;
        }
        let a = (sey * n - se * sy) / det;
        let c = (see * sy - se * sey) / det;
        let sse: f64 = e.iter().zip(&y).map(|(ei, yi)| (yi - a * ei - c).powi(2)).sum();
        if sse.is_finite() && best.map_or(true, |(s, _)| sse < s) {
            best = Some((sse, [a, b, c]));
        }
    }
    let (_, p0) = best.ok_or("не удалось подобрать параметры")?;
    let mut f = |x: f64, p: &[f64]| Ok((p[0] * (p[1] * x).exp() + p[2], None));
    let p = lm(&mut f, &xs, &y, &p0)?;
    Ok(col(vec![p[0] * (p[1] * x0).exp().recip(), p[1], p[2]]))
}

fn lnfit(args: &[Value]) -> EResult<Value> {
    arity("lnfit", args, 2)?;
    let (x, y) = xy_dimless("lnfit", &args[0], &args[1], 2)?;
    if x.iter().any(|&v| v <= 0.0) {
        return Err("значения vx должны быть положительными".into());
    }
    let lx: Vec<f64> = x.iter().map(|v| v.ln()).collect();
    let n = lx.len() as f64;
    let (mx, my) = (lx.iter().sum::<f64>() / n, y.iter().sum::<f64>() / n);
    let sxx: f64 = lx.iter().map(|v| (v - mx).powi(2)).sum();
    let sxy: f64 = lx.iter().zip(&y).map(|(a, b)| (a - mx) * (b - my)).sum();
    let a = sxy / sxx;
    Ok(col(vec![a, my - a * mx]))
}

fn median_of(v: &[f64]) -> f64 {
    let mut s = v.to_vec();
    s.sort_by(f64::total_cmp);
    let n = s.len();
    if n == 0 {
        0.0
    } else if n % 2 == 1 {
        s[n / 2]
    } else {
        0.5 * (s[n / 2 - 1] + s[n / 2])
    }
}

/// Median-median line, `[intercept; slope]`.
fn medfit(args: &[Value]) -> EResult<Value> {
    arity("medfit", args, 2)?;
    let (x, y) = xy_dimless("medfit", &args[0], &args[1], 3)?;
    let mut idx: Vec<usize> = (0..x.len()).collect();
    idx.sort_by(|&a, &b| x[a].total_cmp(&x[b]));
    let k = (x.len() / 3).max(1);
    let (lo, hi) = (&idx[..k], &idx[x.len() - k..]);
    let med = |s: &[usize], v: &[f64]| median_of(&s.iter().map(|&i| v[i]).collect::<Vec<_>>());
    let (xl, yl, xh, yh) = (med(lo, &x), med(lo, &y), med(hi, &x), med(hi, &y));
    if xh == xl {
        return Err("значения vx должны различаться".into());
    }
    let slope = (yh - yl) / (xh - xl);
    let mid = &idx[k..x.len() - k];
    let (xm, ym) = if mid.is_empty() { (med(&idx, &x), med(&idx, &y)) } else { (med(mid, &x), med(mid, &y)) };
    let icpt = ((yl - slope * xl) + (ym - slope * xm) + (yh - slope * xh)) / 3.0;
    Ok(col(vec![icpt, slope]))
}

fn linfit(args: &[Value], calc: Calc) -> EResult<Value> {
    arity("linfit", args, 3)?;
    let (x, y) = xy_dimless("linfit", &args[0], &args[1], 1)?;
    let Value::Func(f) = &args[2] else {
        return Err("третий аргумент должен быть функцией F(x), возвращающей вектор".into());
    };
    let mut a: Vec<Vec<f64>> = Vec::new();
    for &xi in &x {
        let r = calc(f, vec![Value::real(xi)])?.to_matrix()?;
        if !r.is_vector() || r.data.iter().any(|n| !n.dim.is_none() || !n.is_real()) {
            return Err("F(x) должна возвращать вещественный вектор".into());
        }
        a.push(r.data.iter().map(|n| n.v.re).collect());
    }
    let m = a[0].len();
    if a.iter().any(|r| r.len() != m) || m == 0 || m > x.len() {
        return Err("F(x) должна возвращать вектор постоянной длины (не больше числа точек)".into());
    }
    Ok(col(crate::linalg::lstsq(&a, &y)?))
}

fn genfit(args: &[Value], calc: Calc) -> EResult<Value> {
    arity("genfit", args, 4)?;
    let (x, y) = xy_dimless("genfit", &args[0], &args[1], 1)?;
    let g = dimless_vec(&args[2], "genfit: vg")?;
    let Value::Func(f) = &args[3] else {
        return Err("четвёртый аргумент должен быть функцией F(x, u)".into());
    };
    let np = g.len();
    let mut model = |xi: f64, p: &[f64]| -> EResult<(f64, Option<Vec<f64>>)> {
        let u = Value::Matrix(Matrix::column(p.iter().map(|v| Num::real(*v)).collect()));
        let r = calc(f, vec![Value::real(xi), u])?.to_matrix()?;
        if r.data.iter().any(|n| !n.dim.is_none() || !n.is_real()) {
            return Err("F(x, u) должна возвращать безразмерные вещественные значения".into());
        }
        let v: Vec<f64> = r.data.iter().map(|n| n.v.re).collect();
        match v.len() {
            1 => Ok((v[0], None)),
            l if l == np + 1 => Ok((v[0], Some(v[1..].to_vec()))),
            _ => Err(format!("F(x, u) должна возвращать скаляр или вектор из {} элементов", np + 1)),
        }
    };
    Ok(col(lm(&mut model, &x, &y, &g)?))
}

// ---------------------------------------------------------------------------
// Smoothing

/// Gaussian elimination that fails on (relatively) tiny pivots.
fn solve_checked(mut a: Vec<Vec<f64>>, mut b: Vec<f64>) -> Option<Vec<f64>> {
    let n = b.len();
    let scale = a.iter().flatten().fold(0.0f64, |m, v| m.max(v.abs()));
    for c in 0..n {
        let p = (c..n).max_by(|&i, &j| a[i][c].abs().total_cmp(&a[j][c].abs()))?;
        if !(a[p][c].abs() > 1e-9 * scale) {
            return None;
        }
        a.swap(c, p);
        b.swap(c, p);
        for r in c + 1..n {
            let f = a[r][c] / a[c][c];
            for k in c..n {
                a[r][k] -= f * a[c][k];
            }
            b[r] -= f * b[c];
        }
    }
    let mut x = vec![0.0; n];
    for r in (0..n).rev() {
        let s: f64 = (r + 1..n).map(|k| a[r][k] * x[k]).sum();
        x[r] = (b[r] - s) / a[r][r];
    }
    x.iter().all(|v| v.is_finite()).then_some(x)
}

/// Local quadratic regression with tricube weights, evaluated at `t`.
/// Falls back to a local line, then to a weighted mean, when the local
/// problem is ill-conditioned (too few points with non-zero weight);
/// outside the data range a local line is used.
pub fn loess_at(x: &[f64], y: &[f64], span: f64, t: f64) -> f64 {
    let n = x.len();
    // at least 4 neighbours so that 3 of them carry weight
    let q = ((span * n as f64).ceil() as usize).clamp(4.min(n), n);
    let dist: Vec<f64> = x.iter().map(|v| (v - t).abs()).collect();
    let mut sorted = dist.clone();
    sorted.sort_by(f64::total_cmp);
    let mut h = sorted[q - 1];
    if span > 1.0 {
        h *= span;
    }
    // widen the window until at least min(3, n) points have clear weight
    let need = 3.min(n);
    let span_x = sorted[n - 1].max(1e-300);
    if h <= 0.0 {
        h = span_x * 1e-3;
    }
    let weights = |h: f64| -> Vec<f64> {
        dist.iter()
            .map(|&d| {
                let u = d / h;
                if u < 1.0 { (1.0 - u * u * u).powi(3) } else { 0.0 }
            })
            .collect()
    };
    let mut w = weights(h);
    for _ in 0..60 {
        if w.iter().filter(|&&v| v > 1e-3).count() >= need {
            break;
        }
        h *= 1.5;
        w = weights(h);
    }
    let sw: f64 = w.iter().sum();
    if !(sw > 0.0) {
        return y.iter().sum::<f64>() / n as f64;
    }
    // scaled coordinate keeps the normal equations well conditioned
    let u: Vec<f64> = x.iter().map(|v| (v - t) / h).collect();
    let mut s = [0.0f64; 5];
    let mut b = [0.0f64; 3];
    for i in 0..n {
        let mut pw = w[i];
        for sk in s.iter_mut() {
            *sk += pw;
            pw *= u[i];
        }
        b[0] += w[i] * y[i];
        b[1] += w[i] * y[i] * u[i];
        b[2] += w[i] * y[i] * u[i] * u[i];
    }
    let inside = n > 0 && t >= x.iter().cloned().fold(f64::INFINITY, f64::min) && t <= x.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    if inside && n >= 3 {
        let quad = vec![vec![s[0], s[1], s[2]], vec![s[1], s[2], s[3]], vec![s[2], s[3], s[4]]];
        if let Some(c) = solve_checked(quad, b.to_vec()) {
            // guard against wild results: stay within a generous data envelope
            let (lo, hi) = y.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(l, h), &v| (l.min(v), h.max(v)));
            let pad = (hi - lo).max(1e-300);
            if c[0] >= lo - pad && c[0] <= hi + pad {
                return c[0];
            }
        }
    }
    if n >= 2 {
        if let Some(c) = solve_checked(vec![vec![s[0], s[1]], vec![s[1], s[2]]], vec![b[0], b[1]]) {
            return c[0];
        }
    }
    b[0] / sw
}

fn loess(args: &[Value]) -> EResult<Value> {
    arity("loess", args, 3)?;
    let (x, _, y, _) = {
        let (x, dx) = real_vec(&args[0], "loess: vx")?;
        let (y, dy) = real_vec(&args[1], "loess: vy")?;
        (x, dx, y, dy)
    };
    if x.len() != y.len() || x.len() < 3 {
        return Err("vx и vy должны иметь одинаковую длину (не менее 3)".into());
    }
    let span = args[2].as_dimless_real()?;
    if span <= 0.0 {
        return Err("span должен быть положительным".into());
    }
    Ok(col(vec![5.0, x.len() as f64, 0.0, span]))
}

fn ksmooth(args: &[Value]) -> EResult<Value> {
    arity("ksmooth", args, 3)?;
    let (x, dx) = real_vec(&args[0], "ksmooth: vx")?;
    let (y, dy) = real_vec(&args[1], "ksmooth: vy")?;
    let b = args[2].as_num()?;
    if b.dim != dx {
        return Err("b и vx имеют разные единицы измерения".into());
    }
    if x.len() != y.len() || x.is_empty() {
        return Err("vx и vy должны иметь одинаковую длину".into());
    }
    let sigma = 0.3706506 * b.v.re;
    if sigma <= 0.0 {
        return Err("b должно быть положительным".into());
    }
    let out = x
        .iter()
        .map(|&t| {
            let (mut sw, mut sy) = (0.0, 0.0);
            for (xi, yi) in x.iter().zip(&y) {
                let w = (-0.5 * ((xi - t) / sigma).powi(2)).exp();
                sw += w;
                sy += w * yi;
            }
            sy / sw
        })
        .collect();
    Ok(col_dim(out, dy))
}

fn medsmooth(args: &[Value]) -> EResult<Value> {
    arity("medsmooth", args, 2)?;
    let (y, dy) = real_vec(&args[0], "medsmooth: vy")?;
    let w = args[1].as_int()?;
    if w < 1 || w as usize > y.len() {
        return Err("размер окна должен быть от 1 до длины вектора".into());
    }
    let half = (w as usize) / 2;
    let n = y.len();
    let out = (0..n)
        .map(|i| {
            let k = half.min(i).min(n - 1 - i);
            median_of(&y[i - k..=i + k])
        })
        .collect();
    Ok(col_dim(out, dy))
}

/// Running-line smoother with window `w` points; returns the smooth and the
/// cross-validated absolute residuals.
fn running_line(x: &[f64], y: &[f64], w: usize) -> (Vec<f64>, Vec<f64>) {
    let n = x.len();
    let w = w.clamp(3.min(n), n);
    let mut s = vec![0.0; n];
    let mut cv = vec![0.0; n];
    for i in 0..n {
        let lo = i.saturating_sub(w / 2).min(n - w);
        let hi = lo + w;
        let m = w as f64;
        let mx = x[lo..hi].iter().sum::<f64>() / m;
        let my = y[lo..hi].iter().sum::<f64>() / m;
        let sxx: f64 = x[lo..hi].iter().map(|v| (v - mx).powi(2)).sum();
        let sxy: f64 = (lo..hi).map(|k| (x[k] - mx) * (y[k] - my)).sum();
        let slope = if sxx > 0.0 { sxy / sxx } else { 0.0 };
        s[i] = my + slope * (x[i] - mx);
        let leverage = 1.0 / m + if sxx > 0.0 { (x[i] - mx).powi(2) / sxx } else { 0.0 };
        cv[i] = (y[i] - s[i]).abs() / (1.0 - leverage).max(1e-3);
    }
    (s, cv)
}

fn running_mean(v: &[f64], w: usize) -> Vec<f64> {
    let n = v.len();
    let w = w.clamp(1, n);
    (0..n)
        .map(|i| {
            let lo = i.saturating_sub(w / 2).min(n - w);
            v[lo..lo + w].iter().sum::<f64>() / w as f64
        })
        .collect()
}

/// Friedman's super smoother.
fn supsmooth(args: &[Value]) -> EResult<Value> {
    arity("supsmooth", args, 2)?;
    let (x, dx) = real_vec(&args[0], "supsmooth: vx")?;
    let (y, dy) = real_vec(&args[1], "supsmooth: vy")?;
    let _ = dx;
    let n = x.len();
    if n != y.len() || n < 3 {
        return Err("vx и vy должны иметь одинаковую длину (не менее 3)".into());
    }
    let mut idx: Vec<usize> = (0..n).collect();
    idx.sort_by(|&a, &b| x[a].total_cmp(&x[b]));
    let xs: Vec<f64> = idx.iter().map(|&i| x[i]).collect();
    let ys: Vec<f64> = idx.iter().map(|&i| y[i]).collect();
    let spans = [0.05, 0.2, 0.5];
    let width = |sp: f64| ((sp * n as f64).round() as usize).max(3) | 1;
    let mut smooths = Vec::new();
    let mut resid = Vec::new();
    for &sp in &spans {
        let (s, cv) = running_line(&xs, &ys, width(sp));
        smooths.push(s);
        resid.push(running_mean(&cv, width(0.2)));
    }
    let mut chosen = vec![0.0; n];
    for i in 0..n {
        let mut k = 0;
        for j in 1..3 {
            if resid[j][i] < resid[k][i] {
                k = j;
            }
        }
        // bias adjustment towards the midrange span when the best residual is
        // not clearly better than that of the widest span
        if k < 2 && resid[k][i] < resid[2][i] && resid[k][i] > 0.0 {
            let r = ((resid[k][i] / resid[2][i]).max(1e-12)).powf(10.0 - 1.0).min(1.0);
            chosen[i] = spans[k] + (spans[2] - spans[k]) * r;
        } else {
            chosen[i] = spans[k];
        }
    }
    let chosen = running_mean(&chosen, width(0.2));
    let mut fin = vec![0.0; n];
    for i in 0..n {
        let c = chosen[i].clamp(spans[0], spans[2]);
        fin[i] = if c <= spans[1] {
            let f = (c - spans[0]) / (spans[1] - spans[0]);
            smooths[0][i] + f * (smooths[1][i] - smooths[0][i])
        } else {
            let f = (c - spans[1]) / (spans[2] - spans[1]);
            smooths[1][i] + f * (smooths[2][i] - smooths[1][i])
        };
    }
    let (out, _) = running_line(&xs, &fin, width(0.05));
    let mut res = vec![0.0; n];
    for (k, &i) in idx.iter().enumerate() {
        res[i] = out[k];
    }
    Ok(col_dim(res, dy))
}

/// Linear prediction with the Burg algorithm: extrapolates `npred` points
/// from the last `m` samples.
fn predict(args: &[Value]) -> EResult<Value> {
    arity("predict", args, 3)?;
    let (v, dv) = real_vec(&args[0], "predict: v")?;
    let m = args[1].as_int()?;
    let npred = args[2].as_int()?;
    if m < 1 || npred < 1 || v.len() < 2 * m as usize {
        return Err("v должен содержать не менее 2·m значений, m и n — положительны".into());
    }
    let m = m as usize;
    // Burg: forward / backward prediction errors
    let mut f = v.clone();
    let mut b = v.clone();
    let mut a = vec![0.0; m + 1];
    a[0] = 1.0;
    for k in 1..=m {
        let (mut num, mut den) = (0.0, 0.0);
        for i in k..v.len() {
            num += f[i] * b[i - 1];
            den += f[i] * f[i] + b[i - 1] * b[i - 1];
        }
        let refl = if den > 0.0 { -2.0 * num / den } else { 0.0 };
        let old = a.clone();
        for i in 1..=k {
            a[i] = old[i] + refl * old[k - i];
        }
        let (fo, bo) = (f.clone(), b.clone());
        for i in k..v.len() {
            f[i] = fo[i] + refl * bo[i - 1];
            b[i] = bo[i - 1] + refl * fo[i];
        }
    }
    let mut ext = v.clone();
    for _ in 0..npred {
        let t = ext.len();
        let next: f64 = -(1..=m).map(|j| a[j] * ext[t - j]).sum::<f64>();
        ext.push(next);
    }
    Ok(col_dim(ext[v.len()..].to_vec(), dv))
}

// ---------------------------------------------------------------------------
// Random numbers (fixed seed: worksheets are reproducible)

thread_local! {
    static RNG: Cell<u64> = const { Cell::new(0x9E37_79B9_7F4A_7C15) };
}

fn next_u64() -> u64 {
    RNG.with(|s| {
        // splitmix64
        let z = s.get().wrapping_add(0x9E37_79B9_7F4A_7C15);
        s.set(z);
        let mut z = z;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    })
}

/// Uniform in the open interval (0, 1).
fn uniform() -> f64 {
    ((next_u64() >> 11) as f64 + 0.5) / (1u64 << 53) as f64
}

fn normal() -> f64 {
    (-2.0 * uniform().ln()).sqrt() * (2.0 * std::f64::consts::PI * uniform()).cos()
}

fn gamma_sample(shape: f64) -> f64 {
    if shape < 1.0 {
        return gamma_sample(shape + 1.0) * uniform().powf(1.0 / shape);
    }
    let d = shape - 1.0 / 3.0;
    let c = 1.0 / (9.0 * d).sqrt();
    loop {
        let (x, u) = (normal(), uniform());
        let v = (1.0 + c * x).powi(3);
        if v > 0.0 && u.ln() < 0.5 * x * x + d - d * v + d * v.ln() {
            return d * v;
        }
    }
}

fn poisson_sample(l: f64) -> f64 {
    if l < 30.0 {
        let (lim, mut k, mut p) = ((-l).exp(), 0.0, 1.0);
        loop {
            p *= uniform();
            if p <= lim {
                return k;
            }
            k += 1.0;
        }
    }
    (l + l.sqrt() * normal()).round().max(0.0)
}

fn binomial_sample(n: f64, p: f64) -> f64 {
    if n <= 10_000.0 {
        (0..n as u64).filter(|_| uniform() < p).count() as f64
    } else {
        (n * p + (n * p * (1.0 - p)).sqrt() * normal()).round().clamp(0.0, n)
    }
}

fn random(name: &str, args: &[Value]) -> EResult<Value> {
    if name == "rnd" {
        arity(name, args, 1)?;
        let x = args[0].as_num()?;
        return Ok(Value::Num(Num::with_dim(uniform() * x.v.re, x.dim)));
    }
    let npar = match name {
        "rexp" | "rpois" | "rgamma" | "rchisq" | "rt" | "rgeom" => 1,
        "rbinom" | "runif" | "rnorm" | "rbeta" | "rlnorm" | "rweibull" | "rlogis" | "rcauchy" => 2,
        _ => 0,
    };
    arity(name, args, 1 + npar)?;
    let m = args[0].as_int()?;
    if m < 1 || m as usize > MAX_RANGE_LEN {
        return Err("число значений должно быть положительным".into());
    }
    let mut p = Vec::new();
    let mut dim = Dim::NONE;
    for (i, a) in args[1..].iter().enumerate() {
        let n = a.as_num()?;
        if !n.is_real() {
            return Err("параметры должны быть вещественными".into());
        }
        if i == 0 && matches!(name, "runif" | "rnorm" | "rlogis" | "rcauchy") {
            dim = n.dim;
        } else if matches!(name, "runif") && i == 1 && n.dim != dim {
            // allow a plain 0 limit
            if !(n.dim.is_none() && n.v.re == 0.0) && !(dim.is_none() && p[0] == 0.0) {
                return Err("Единицы измерения не совпадают".into());
            }
            if dim.is_none() {
                dim = n.dim;
            }
        } else if matches!(name, "rnorm" | "rlogis" | "rcauchy") && i == 1 && n.dim != dim {
            return Err("Единицы измерения не совпадают".into());
        } else if !matches!(name, "rnorm" | "rlogis" | "rcauchy" | "runif") && !n.dim.is_none() {
            return Err("параметры должны быть безразмерными".into());
        }
        p.push(n.v.re);
    }
    let pos = |i: usize, what: &str| -> EResult<f64> {
        if p[i] > 0.0 {
            Ok(p[i])
        } else {
            Err(format!("{what} должен быть положительным"))
        }
    };
    let pi = std::f64::consts::PI;
    let mut out = Vec::with_capacity(m as usize);
    match name {
        "runif" => {
            let (a, b) = (p[0], p[1]);
            (0..m).for_each(|_| out.push(a + (b - a) * uniform()));
        }
        "rnorm" => {
            let s = pos(1, "σ")?;
            (0..m).for_each(|_| out.push(p[0] + s * normal()));
        }
        "rexp" => {
            let r = pos(0, "параметр")?;
            (0..m).for_each(|_| out.push(-uniform().ln() / r));
        }
        "rpois" => {
            let l = pos(0, "параметр")?;
            (0..m).for_each(|_| out.push(poisson_sample(l)));
        }
        "rbinom" => {
            let (n, pr) = (p[0].round(), p[1]);
            if n < 0.0 || !(0.0..=1.0).contains(&pr) {
                return Err("недопустимые параметры (n ≥ 0, 0 ≤ p ≤ 1)".into());
            }
            (0..m).for_each(|_| out.push(binomial_sample(n, pr)));
        }
        "rgamma" => {
            let s = pos(0, "параметр формы")?;
            (0..m).for_each(|_| out.push(gamma_sample(s)));
        }
        "rchisq" => {
            let d = pos(0, "число степеней свободы")?;
            (0..m).for_each(|_| out.push(2.0 * gamma_sample(d / 2.0)));
        }
        "rt" => {
            let d = pos(0, "число степеней свободы")?;
            (0..m).for_each(|_| out.push(normal() / (2.0 * gamma_sample(d / 2.0) / d).sqrt()));
        }
        "rbeta" => {
            let (s1, s2) = (pos(0, "s1")?, pos(1, "s2")?);
            (0..m).for_each(|_| {
                let (a, b) = (gamma_sample(s1), gamma_sample(s2));
                out.push(a / (a + b));
            });
        }
        "rlnorm" => {
            let s = pos(1, "σ")?;
            (0..m).for_each(|_| out.push((p[0] + s * normal()).exp()));
        }
        "rweibull" => {
            let (s, r) = (pos(0, "s")?, pos(1, "параметр масштаба")?);
            (0..m).for_each(|_| out.push(r * (-uniform().ln()).powf(1.0 / s)));
        }
        "rcauchy" => {
            let g = pos(1, "параметр масштаба")?;
            (0..m).for_each(|_| out.push(p[0] + g * (pi * (uniform() - 0.5)).tan()));
        }
        "rlogis" => {
            let s = pos(1, "параметр масштаба")?;
            (0..m).for_each(|_| {
                let u = uniform();
                out.push(p[0] + s * (u / (1.0 - u)).ln());
            });
        }
        "rgeom" => {
            let pr = p[0];
            if !(0.0 < pr && pr <= 1.0) {
                return Err("p должно быть в (0, 1]".into());
            }
            (0..m).for_each(|_| out.push((uniform().ln() / (1.0 - pr).ln()).floor().max(0.0)));
        }
        _ => unreachable!(),
    }
    Ok(col_dim(out, dim))
}

// Discrete distributions

fn ln_choose(n: f64, k: f64) -> f64 {
    libm::lgamma(n + 1.0) - libm::lgamma(k + 1.0) - libm::lgamma(n - k + 1.0)
}

fn dbinom_pmf(k: f64, n: f64, p: f64) -> f64 {
    if k < 0.0 || k > n || k.fract() != 0.0 {
        return 0.0;
    }
    if p == 0.0 {
        return if k == 0.0 { 1.0 } else { 0.0 };
    }
    if p == 1.0 {
        return if k == n { 1.0 } else { 0.0 };
    }
    (ln_choose(n, k) + k * p.ln() + (n - k) * (1.0 - p).ln()).exp()
}

fn dpois_pmf(k: f64, l: f64) -> f64 {
    if k < 0.0 || k.fract() != 0.0 {
        return 0.0;
    }
    (k * l.ln() - l - libm::lgamma(k + 1.0)).exp()
}

fn discrete(name: &str, args: &[Value]) -> EResult<Value> {
    let binom = name.ends_with("binom");
    arity(name, args, if binom { 3 } else { 2 })?;
    let par: Vec<f64> = args[1..].iter().map(|a| a.as_dimless_real()).collect::<EResult<_>>()?;
    if binom && (par[0] < 0.0 || !(0.0..=1.0).contains(&par[1])) {
        return Err("недопустимые параметры (n ≥ 0, 0 ≤ p ≤ 1)".into());
    }
    if !binom && par[0] <= 0.0 {
        return Err("параметр должен быть положительным".into());
    }
    let cumulative = name.starts_with('p');
    let pmf = |k: f64| if binom { dbinom_pmf(k, par[0], par[1]) } else { dpois_pmf(k, par[0]) };
    crate::builtins::real_fn(&args[0], |x| {
        if !cumulative {
            return pmf(x);
        }
        let top = x.floor();
        if top < 0.0 {
            return 0.0;
        }
        let top = if binom { top.min(par[0]) } else { top };
        (0..=top as u64).map(|k| pmf(k as f64)).sum::<f64>().min(1.0)
    })
}

// ---------------------------------------------------------------------------

/// Built-ins that need only evaluated arguments.
pub fn call(name: &str, args: &[Value]) -> Option<EResult<Value>> {
    Some(match name {
        "expfit" => expfit(args),
        "lgsfit" => fit_model(name, args, 3, |x, p| p[0] / (1.0 + p[1] * (-p[2] * x).exp())),
        "logfit" => fit_model(name, args, 3, |x, p| p[0] * (x + p[1]).ln() + p[2]),
        "pwrfit" => fit_model(name, args, 3, |x, p| p[0] * x.powf(p[1]) + p[2]),
        "sinfit" => fit_model(name, args, 3, |x, p| p[0] * (x + p[1]).sin() + p[2]),
        "lnfit" => lnfit(args),
        "medfit" => medfit(args),
        "loess" => loess(args),
        "ksmooth" => ksmooth(args),
        "medsmooth" => medsmooth(args),
        "supsmooth" => supsmooth(args),
        "predict" => predict(args),
        "dbinom" | "pbinom" | "dpois" | "ppois" => discrete(name, args),
        n if n.starts_with('r') && NAMES.contains(&n) => random(n, args),
        _ => return None,
    })
}

/// `linfit` and `genfit`, which call user functions through `calc`.
pub fn call_with(name: &str, args: &[Value], calc: Calc) -> Option<EResult<Value>> {
    Some(match name {
        "linfit" => linfit(args, calc),
        "genfit" => genfit(args, calc),
        _ => return None,
    })
}
