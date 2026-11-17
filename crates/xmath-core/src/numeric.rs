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
        return Err("the integral does not converge".into());
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
            Err("the integrand is not finite on the interval".into())
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
        return Err("the derivative is not finite".into());
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
    Err("no solution found; try a different guess value".into())
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
        return Err("the function has the same sign at both ends of the interval".into());
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
        return Err("the polynomial must have degree 1 or higher".into());
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
    Ok(z)
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
}
