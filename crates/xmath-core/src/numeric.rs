//! Numerical algorithms: adaptive quadrature, differentiation and root finding.

use crate::value::EResult;
use num_complex::Complex64 as C64;

const XGK: [f64; 8] = [
    0.991_455_371_120_812_6,
    0.949_107_912_342_758_5,
    0.864_864_423_359_769_1,
    0.741_531_185_599_394_4,
    0.586_087_235_467_691_1,
    0.405_845_151_377_397_2,
    0.207_784_955_007_898_5,
    0.0,
];
const WGK: [f64; 8] = [
    0.022_935_322_010_529_22,
    0.063_092_092_629_978_55,
    0.104_790_010_322_250_2,
    0.140_653_259_715_525_9,
    0.169_004_726_639_267_9,
    0.190_350_578_064_785_4,
    0.204_432_940_075_298_9,
    0.209_482_141_084_727_8,
];
const WG: [f64; 4] = [
    0.129_484_966_168_869_7,
    0.279_705_391_489_276_7,
    0.381_830_050_505_118_9,
    0.417_959_183_673_469_4,
];

/// One Gauss–Kronrod 7/15 step: (integral, error estimate).
fn gk15(f: &dyn Fn(f64) -> EResult<C64>, a: f64, b: f64) -> EResult<(C64, f64)> {
    let c = 0.5 * (a + b);
    let h = 0.5 * (b - a);
    // An integrable singularity at an endpoint (`1/sqrt(1 - τ²)` at τ = 1) makes a node that
    // rounded onto the endpoint infinite: step such a node inside by a few ulps and use that.
    let at = |x: f64| -> EResult<C64> {
        let ok = |v: &C64| v.re.is_finite() && v.im.is_finite();
        let mut y = x;
        for _ in 0..64 {
            if y <= a {
                y = toward(y, c);
            } else if y >= b {
                y = toward(y, c);
            }
            match f(y) {
                Ok(v) if ok(&v) => return Ok(v),
                other if y != x || y <= a || y >= b => {
                    let _ = other;
                    y = toward(y, c);
                    if y == x || y == c {
                        break;
                    }
                }
                other => return other,
            }
        }
        f(y)
    };
    let f = &at;
    let fc = f(c)?;
    let mut k = fc * WGK[7];
    let mut g = fc * WG[3];
    for j in 0..7 {
        let x = h * XGK[j];
        let s = f(c - x)? + f(c + x)?;
        k += s * WGK[j];
        if j % 2 == 1 {
            g += s * WG[j / 2];
        }
    }
    let k = k * h;
    let g = g * h;
    Ok((k, (k - g).norm()))
}

/// The next representable number from `x` toward `dir`.
fn toward(x: f64, dir: f64) -> f64 {
    if x == dir || !x.is_finite() {
        return x;
    }
    let bits = x.to_bits();
    if (dir > x) == (x > 0.0) {
        f64::from_bits(bits + 1)
    } else {
        f64::from_bits(bits - 1)
    }
}

const MAX_INTERVALS: usize = 2000;

fn adaptive(f: &dyn Fn(f64) -> EResult<C64>, a: f64, b: f64) -> EResult<C64> {
    // Global adaptive bisection: always refine the interval with the largest error.
    let (k, e) = gk15(f, a, b)?;
    let mut parts = vec![(a, b, k, e)];
    for _ in 0..MAX_INTERVALS {
        let total: C64 = parts.iter().map(|p| p.2).sum();
        let err: f64 = parts.iter().map(|p| p.3).sum();
        if err <= 1e-12_f64.max(1e-11 * total.norm()) {
            return Ok(total);
        }
        let (i, _) = parts
            .iter()
            .enumerate()
            .max_by(|x, y| x.1 .3.total_cmp(&y.1 .3))
            .unwrap();
        let (a, b, _, _) = parts.swap_remove(i);
        let m = 0.5 * (a + b);
        if m <= a || m >= b {
            break; // interval can't be split further
        }
        let (k1, e1) = gk15(f, a, m)?;
        let (k2, e2) = gk15(f, m, b)?;
        parts.push((a, m, k1, e1));
        parts.push((m, b, k2, e2));
    }
    let total: C64 = parts.iter().map(|p| p.2).sum();
    if !total.re.is_finite() || !total.im.is_finite() {
        return Err("интеграл не сходится".into());
    }
    Ok(total)
}

/// Integrates `f` over `[a, b]`; limits beyond ±1e300 are treated as infinite.
pub fn integrate(f: &dyn Fn(f64) -> EResult<C64>, a: f64, b: f64) -> EResult<C64> {
    if a == b {
        return Ok(C64::new(0.0, 0.0));
    }
    if a > b {
        return integrate(f, b, a).map(|v| -v);
    }
    let inf = 1e300;
    let check = |v: C64| -> EResult<C64> {
        if v.re.is_finite() && v.im.is_finite() {
            Ok(v)
        } else {
            Err("подынтегральная функция не ограничена на интервале".into())
        }
    };
    match (a <= -inf, b >= inf) {
        (false, false) => adaptive(&|x| f(x).and_then(check), a, b),
        // x = a + t/(1-t), t in [0, 1)
        (false, true) => adaptive(
            &|t| {
                let x = a + t / (1.0 - t);
                Ok(f(x).and_then(check)? / ((1.0 - t) * (1.0 - t)))
            },
            0.0,
            1.0,
        ),
        // x = b - t/(1-t)
        (true, false) => adaptive(
            &|t| {
                let x = b - t / (1.0 - t);
                Ok(f(x).and_then(check)? / ((1.0 - t) * (1.0 - t)))
            },
            0.0,
            1.0,
        ),
        // x = t/(1-t^2), t in (-1, 1)
        (true, true) => adaptive(
            &|t| {
                let d = 1.0 - t * t;
                let x = t / d;
                Ok(f(x).and_then(check)? * ((1.0 + t * t) / (d * d)))
            },
            -1.0,
            1.0,
        ),
    }
}

/// First derivative by Richardson-extrapolated central differences (Ridders).
pub fn derivative(f: &dyn Fn(f64) -> EResult<C64>, x: f64) -> EResult<C64> {
    const NTAB: usize = 10;
    const CON: f64 = 1.4;
    const CON2: f64 = CON * CON;
    let mut h = 0.1 * x.abs().max(1e-3);
    let mut a = [[C64::new(0.0, 0.0); NTAB]; NTAB];
    a[0][0] = (f(x + h)? - f(x - h)?) / (2.0 * h);
    let mut best = a[0][0];
    let mut err = f64::MAX;
    for i in 1..NTAB {
        h /= CON;
        a[0][i] = (f(x + h)? - f(x - h)?) / (2.0 * h);
        let mut fac = CON2;
        for j in 1..=i {
            a[j][i] = (a[j - 1][i] * fac - a[j - 1][i - 1]) / (fac - 1.0);
            fac *= CON2;
            let e = (a[j][i] - a[j - 1][i]).norm().max((a[j][i] - a[j - 1][i - 1]).norm());
            if e <= err {
                err = e;
                best = a[j][i];
            }
        }
        if (a[i][i] - a[i - 1][i - 1]).norm() >= 2.0 * err {
            break;
        }
    }
    if !best.re.is_finite() || !best.im.is_finite() {
        return Err("производная не существует (бесконечна)".into());
    }
    Ok(best)
}

/// Secant iteration from a guess (works for complex roots too).
pub fn secant(f: &dyn Fn(C64) -> EResult<C64>, guess: C64) -> EResult<C64> {
    let mut x0 = guess;
    let delta = if guess.norm() > 0.0 { guess.norm() * 1e-4 } else { 1e-4 };
    let mut x1 = guess + delta;
    let mut f0 = f(x0)?;
    if f0.norm() == 0.0 {
        return Ok(x0);
    }
    let mut f1 = f(x1)?;
    for _ in 0..200 {
        if f1.norm() == 0.0 {
            return Ok(x1);
        }
        let denom = f1 - f0;
        if denom.norm() == 0.0 {
            break;
        }
        let x2 = x1 - f1 * (x1 - x0) / denom;
        if !x2.re.is_finite() || !x2.im.is_finite() {
            break;
        }
        x0 = x1;
        f0 = f1;
        x1 = x2;
        f1 = f(x1)?;
        if (x1 - x0).norm() <= 1e-14 * x1.norm().max(1e-300) {
            return Ok(x1);
        }
    }
    // accept if the residual is tiny compared to the function scale
    if f1.norm() <= 1e-10 * f(guess)?.norm().max(1.0) {
        return Ok(x1);
    }
    Err("решение не найдено; попробуйте другое начальное приближение".into())
}

/// Brent's method on a bracketing interval.
pub fn brent(f: &dyn Fn(f64) -> EResult<f64>, a: f64, b: f64) -> EResult<f64> {
    let (mut a, mut b) = (a, b);
    let mut fa = f(a)?;
    let mut fb = f(b)?;
    if fa == 0.0 {
        return Ok(a);
    }
    if fb == 0.0 {
        return Ok(b);
    }
    if fa.signum() == fb.signum() {
        return Err("функция имеет одинаковый знак на концах интервала".into());
    }
    let (mut c, mut fc) = (a, fa);
    let mut d = b - a;
    let mut e = d;
    for _ in 0..300 {
        if fb.signum() == fc.signum() {
            c = a;
            fc = fa;
            d = b - a;
            e = d;
        }
        if fc.abs() < fb.abs() {
            a = b;
            b = c;
            c = a;
            fa = fb;
            fb = fc;
            fc = fa;
        }
        let tol = 2.0 * f64::EPSILON * b.abs() + 1e-300;
        let m = 0.5 * (c - b);
        if m.abs() <= tol || fb == 0.0 {
            return Ok(b);
        }
        if e.abs() >= tol && fa.abs() > fb.abs() {
            let s = fb / fa;
            let (mut p, mut q);
            if a == c {
                p = 2.0 * m * s;
                q = 1.0 - s;
            } else {
                let q0 = fa / fc;
                let r = fb / fc;
                p = s * (2.0 * m * q0 * (q0 - r) - (b - a) * (r - 1.0));
                q = (q0 - 1.0) * (r - 1.0) * (s - 1.0);
            }
            if p > 0.0 {
                q = -q;
            } else {
                p = -p;
            }
            if 2.0 * p < (3.0 * m * q - (tol * q).abs()).min((e * q).abs()) {
                e = d;
                d = p / q;
            } else {
                d = m;
                e = m;
            }
        } else {
            d = m;
            e = m;
        }
        a = b;
        fa = fb;
        b += if d.abs() > tol { d } else { tol.copysign(m) };
        fb = f(b)?;
    }
    Ok(b)
}

/// All roots of a polynomial given by coefficients in ascending order
/// (Durand–Kerner / Weierstrass iteration).
pub fn polyroots(coeffs: &[C64]) -> EResult<Vec<C64>> {
    let mut c: Vec<C64> = coeffs.to_vec();
    while c.last().is_some_and(|x| x.norm() == 0.0) {
        c.pop();
    }
    let n = c.len().saturating_sub(1);
    if n == 0 {
        return Err("степень многочлена должна быть не меньше 1".into());
    }
    let lead = c[n];
    let c: Vec<C64> = c.iter().map(|x| x / lead).collect();
    let eval = |z: C64| c.iter().rev().fold(C64::new(0.0, 0.0), |acc, k| acc * z + k);
    let radius = 1.0 + c[..n].iter().map(|x| x.norm()).fold(0.0, f64::max);
    let seed = C64::new(0.4, 0.9);
    let mut z: Vec<C64> = (0..n).map(|k| seed.powu(k as u32) * radius.min(2.0)).collect();
    for _ in 0..2000 {
        let mut change: f64 = 0.0;
        for i in 0..n {
            let mut denom = C64::new(1.0, 0.0);
            for j in 0..n {
                if i != j {
                    denom *= z[i] - z[j];
                }
            }
            if denom.norm() == 0.0 {
                denom = C64::new(1e-12, 0.0);
            }
            let dz = eval(z[i]) / denom;
            z[i] -= dz;
            change = change.max(dz.norm() / z[i].norm().max(1e-300));
        }
        if change < 1e-15 {
            break;
        }
    }
    // polish tiny imaginary parts and sort by real part
    for r in &mut z {
        if r.im.abs() < 1e-12 * r.re.abs().max(1e-12) {
            r.im = 0.0;
        }
    }
    z.sort_by(|a, b| a.re.total_cmp(&b.re).then(a.im.total_cmp(&b.im)));
    // conjugate pairs have equal real parts up to rounding: put the negative imaginary part first
    for i in 1..z.len() {
        let (p, q) = (z[i - 1], z[i]);
        if (p.re - q.re).abs() <= 1e-7 * (1.0 + p.re.abs().max(q.re.abs())) && p.im > q.im {
            z.swap(i - 1, i);
        }
    }
    Ok(z)
}

/// Right-hand side of an ODE system `y' = f(x, y)`.
pub type OdeFn<'a> = dyn Fn(f64, &[f64]) -> EResult<Vec<f64>> + 'a;

fn axpy(y: &[f64], h: f64, k: &[f64]) -> Vec<f64> {
    y.iter().zip(k).map(|(a, b)| a + h * b).collect()
}

/// Classic fourth-order Runge–Kutta with a fixed step; returns the `y` values
/// at `x1 + k (x2 - x1) / n` for `k = 0..=n`.
pub fn rk4_fixed(f: &OdeFn, y0: &[f64], x1: f64, x2: f64, n: usize) -> EResult<Vec<Vec<f64>>> {
    let h = (x2 - x1) / n as f64;
    let mut out = vec![y0.to_vec()];
    let mut y = y0.to_vec();
    for k in 0..n {
        let x = x1 + k as f64 * h;
        let k1 = f(x, &y)?;
        let k2 = f(x + 0.5 * h, &axpy(&y, 0.5 * h, &k1))?;
        let k3 = f(x + 0.5 * h, &axpy(&y, 0.5 * h, &k2))?;
        let k4 = f(x + h, &axpy(&y, h, &k3))?;
        for i in 0..y.len() {
            y[i] += h / 6.0 * (k1[i] + 2.0 * k2[i] + 2.0 * k3[i] + k4[i]);
        }
        if y.iter().any(|v| !v.is_finite()) {
            return Err("решение расходится".into());
        }
        out.push(y.clone());
    }
    Ok(out)
}

/// Adaptive Dormand–Prince 5(4) integration; the solution is reported at
/// `n + 1` equally spaced points.
pub fn rk45_adaptive(f: &OdeFn, y0: &[f64], x1: f64, x2: f64, n: usize, tol: f64) -> EResult<Vec<Vec<f64>>> {
    const C: [f64; 7] = [0.0, 0.2, 0.3, 0.8, 8.0 / 9.0, 1.0, 1.0];
    const A: [[f64; 6]; 7] = [
        [0.0; 6],
        [0.2, 0.0, 0.0, 0.0, 0.0, 0.0],
        [3.0 / 40.0, 9.0 / 40.0, 0.0, 0.0, 0.0, 0.0],
        [44.0 / 45.0, -56.0 / 15.0, 32.0 / 9.0, 0.0, 0.0, 0.0],
        [19372.0 / 6561.0, -25360.0 / 2187.0, 64448.0 / 6561.0, -212.0 / 729.0, 0.0, 0.0],
        [9017.0 / 3168.0, -355.0 / 33.0, 46732.0 / 5247.0, 49.0 / 176.0, -5103.0 / 18656.0, 0.0],
        [35.0 / 384.0, 0.0, 500.0 / 1113.0, 125.0 / 192.0, -2187.0 / 6784.0, 11.0 / 84.0],
    ];
    const B4: [f64; 7] =
        [5179.0 / 57600.0, 0.0, 7571.0 / 16695.0, 393.0 / 640.0, -92097.0 / 339200.0, 187.0 / 2100.0, 1.0 / 40.0];
    let dim = y0.len();
    let seg = (x2 - x1) / n as f64;
    let mut out = vec![y0.to_vec()];
    let mut y = y0.to_vec();
    let mut h = seg / 4.0;
    let mut steps = 0usize;
    for k in 0..n {
        let x_end = x1 + (k + 1) as f64 * seg;
        let mut x = x1 + k as f64 * seg;
        while (x_end - x) * seg.signum() > 1e-14 * seg.abs() {
            steps += 1;
            if steps > 2_000_000 {
                return Err("слишком много шагов интегрирования".into());
            }
            if (x + h - x_end) * seg.signum() > 0.0 {
                h = x_end - x;
            }
            let mut ks: Vec<Vec<f64>> = Vec::with_capacity(7);
            for s in 0..7 {
                let mut ys = y.clone();
                for (j, kj) in ks.iter().enumerate() {
                    for i in 0..dim {
                        ys[i] += h * A[s][j] * kj[i];
                    }
                }
                ks.push(f(x + C[s] * h, &ys)?);
            }
            // 5th-order solution is ys of stage 7 (FSAL): y + h * sum A[6][j] k_j
            let mut y5 = y.clone();
            for j in 0..6 {
                for i in 0..dim {
                    y5[i] += h * A[6][j] * ks[j][i];
                }
            }
            let mut err: f64 = 0.0;
            for i in 0..dim {
                let mut y4 = y[i];
                for j in 0..7 {
                    y4 += h * B4[j] * ks[j][i];
                }
                let sc = 1e-10 + tol * y[i].abs().max(y5[i].abs());
                err = err.max((y5[i] - y4).abs() / sc);
            }
            if !err.is_finite() {
                return Err("решение расходится".into());
            }
            if err <= 1.0 {
                x += h;
                y = y5;
            }
            let fac = if err == 0.0 { 5.0 } else { (0.9 * err.powf(-0.2)).clamp(0.2, 5.0) };
            h *= fac;
        }
        out.push(y.clone());
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(f: impl Fn(f64) -> f64) -> impl Fn(f64) -> EResult<C64> {
        move |x| Ok(C64::new(f(x), 0.0))
    }

    #[test]
    fn quadrature() {
        let v = integrate(&r(|x| x * x), 0.0, 3.0).unwrap();
        assert!((v.re - 9.0).abs() < 1e-12);
        let v = integrate(&r(|x| (-x * x).exp()), -1e307, 1e307).unwrap();
        assert!((v.re - std::f64::consts::PI.sqrt()).abs() < 1e-9);
        let v = integrate(&r(|x| 1.0 / (x * x)), 1.0, 1e307).unwrap();
        assert!((v.re - 1.0).abs() < 1e-9);
        let v = integrate(&r(|x| x.sqrt()), 0.0, 1.0).unwrap();
        assert!((v.re - 2.0 / 3.0).abs() < 1e-10);
    }

    #[test]
    fn derivatives() {
        let d = derivative(&r(f64::sin), 1.0).unwrap();
        assert!((d.re - 1f64.cos()).abs() < 1e-12);
    }

    #[test]
    fn roots() {
        let x = brent(&|x| Ok(x * x - 2.0), 0.0, 2.0).unwrap();
        assert!((x - 2f64.sqrt()).abs() < 1e-14);
        let x = secant(&|x| Ok(x * x - 2.0), C64::new(1.0, 0.0)).unwrap();
        assert!((x.re - 2f64.sqrt()).abs() < 1e-12);
        let x = secant(&|x| Ok(x * x + 1.0), C64::new(0.5, 0.5)).unwrap();
        assert!((x - C64::new(0.0, 1.0)).norm() < 1e-12);
        let p = polyroots(&[C64::new(-6.0, 0.0), C64::new(11.0, 0.0), C64::new(-6.0, 0.0), C64::new(1.0, 0.0)])
            .unwrap();
        for (z, e) in p.iter().zip([1.0, 2.0, 3.0]) {
            assert!((z.re - e).abs() < 1e-10 && z.im == 0.0, "{p:?}");
        }
    }

    #[test]
    fn ode() {
        // y' = y, y(0) = 1
        let f = |_x: f64, y: &[f64]| -> EResult<Vec<f64>> { Ok(vec![y[0]]) };
        let r = rk4_fixed(&f, &[1.0], 0.0, 1.0, 50).unwrap();
        assert!((r[50][0] - std::f64::consts::E).abs() < 1e-7);
        let r = rk45_adaptive(&f, &[1.0], 0.0, 1.0, 4, 1e-10).unwrap();
        assert!((r[4][0] - std::f64::consts::E).abs() < 1e-8);
        assert!((r[2][0] - 0.5f64.exp()).abs() < 1e-8);
    }
}
