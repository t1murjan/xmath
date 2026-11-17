//! Special functions: modified Bessel functions and the normal quantile.

/// Modified Bessel function of the first kind `I_n(x)` (integer order).
/// Uses the power series, whose terms are all positive for `x >= 0`.
pub fn bessel_i(n: i64, x: f64) -> f64 {
    let nn = n.unsigned_abs() as f64;
    if x.is_nan() {
        return f64::NAN;
    }
    let sign = if x < 0.0 && n % 2 != 0 { -1.0 } else { 1.0 };
    let x = x.abs();
    if x == 0.0 {
        return if n == 0 { 1.0 } else { 0.0 };
    }
    let half = 0.5 * x;
    let mut term = (nn * half.ln() - libm::lgamma(nn + 1.0)).exp();
    let q = half * half;
    let mut sum = term;
    for k in 0..100_000 {
        let kf = k as f64;
        term *= q / ((kf + 1.0) * (kf + 1.0 + nn));
        sum += term;
        if term <= 1e-17 * sum {
            break;
        }
    }
    sign * sum
}

/// Modified Bessel function of the second kind `K_n(x)` for `x > 0`,
/// by the (double-exponentially convergent) trapezoidal rule applied to
/// `K_ν(x) = ∫₀^∞ exp(-x cosh t) cosh(νt) dt`.
pub fn bessel_k(n: i64, x: f64) -> f64 {
    if x.is_nan() || x < 0.0 {
        return f64::NAN;
    }
    if x == 0.0 {
        return f64::INFINITY;
    }
    let nu = n.unsigned_abs() as f64;
    let h = 0.2;
    let f = |t: f64| {
        let e = -x * t.cosh();
        0.5 * ((e + nu * t).exp() + (e - nu * t).exp())
    };
    let mut sum = 0.5 * f(0.0);
    for k in 1..5000 {
        let t = k as f64 * h;
        let term = f(t);
        sum += term;
        if x * t.sinh() > nu && term <= 1e-18 * sum {
            break;
        }
    }
    sum * h
}

/// Quantile (inverse CDF) of the standard normal distribution, `0 < p < 1`.
pub fn qnorm_std(p: f64) -> f64 {
    const A: [f64; 6] = [
        -3.969_683_028_665_376e1,
        2.209_460_984_245_205e2,
        -2.759_285_104_469_687e2,
        1.383_577_518_672_69e2,
        -3.066_479_806_614_716e1,
        2.506_628_277_459_239,
    ];
    const B: [f64; 5] = [
        -5.447_609_879_822_406e1,
        1.615_858_368_580_409e2,
        -1.556_989_798_598_866e2,
        6.680_131_188_771_972e1,
        -1.328_068_155_288_572e1,
    ];
    const C: [f64; 6] = [
        -7.784_894_002_430_293e-3,
        -3.223_964_580_411_365e-1,
        -2.400_758_277_161_838,
        -2.549_732_539_343_734,
        4.374_664_141_464_968,
        2.938_163_982_698_783,
    ];
    const D: [f64; 4] = [
        7.784_695_709_041_462e-3,
        3.224_671_290_700_398e-1,
        2.445_134_137_142_996,
        3.754_408_661_907_416,
    ];
    let plow = 0.02425;
    let tail = |q: f64| {
        (((((C[0] * q + C[1]) * q + C[2]) * q + C[3]) * q + C[4]) * q + C[5])
            / ((((D[0] * q + D[1]) * q + D[2]) * q + D[3]) * q + 1.0)
    };
    let mut x = if p < plow {
        tail((-2.0 * p.ln()).sqrt())
    } else if p <= 1.0 - plow {
        let q = p - 0.5;
        let r = q * q;
        (((((A[0] * r + A[1]) * r + A[2]) * r + A[3]) * r + A[4]) * r + A[5]) * q
            / (((((B[0] * r + B[1]) * r + B[2]) * r + B[3]) * r + B[4]) * r + 1.0)
    } else {
        -tail((-2.0 * (1.0 - p).ln()).sqrt())
    };
    // one Halley refinement step
    for _ in 0..2 {
        let e = 0.5 * libm::erfc(-x / std::f64::consts::SQRT_2) - p;
        let u = e * (2.0 * std::f64::consts::PI).sqrt() * (0.5 * x * x).exp();
        x -= u / (1.0 + 0.5 * x * u);
    }
    x
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modified_bessel() {
        assert!((bessel_i(0, 1.0) - 1.266_065_877_752_008_4).abs() < 1e-14);
        assert!((bessel_i(1, 1.0) - 0.565_159_103_992_485).abs() < 1e-14);
        assert!((bessel_i(2, 2.0) - 0.688_948_447_698_738).abs() < 1e-13);
        assert!((bessel_i(1, -1.0) + 0.565_159_103_992_485).abs() < 1e-14);
        assert!((bessel_k(0, 1.0) - 0.421_024_438_240_708_3).abs() < 1e-13);
        assert!((bessel_k(1, 1.0) - 0.601_907_230_197_234_6).abs() < 1e-13);
        assert!((bessel_k(2, 3.0) - 0.061_510_458_471_742_0).abs() < 1e-13);
        assert!((bessel_k(0, 0.01) - 4.721_244_730_161_08).abs() < 1e-11);
    }

    #[test]
    fn quantile() {
        for p in [1e-10, 0.001, 0.02, 0.3, 0.5, 0.9, 0.999, 1.0 - 1e-9] {
            let x = qnorm_std(p);
            let back = 0.5 * libm::erfc(-x / std::f64::consts::SQRT_2);
            assert!((back - p).abs() < 1e-14 * p.max(1e-3), "{p}: {back}");
        }
        assert!((qnorm_std(0.975) - 1.959_963_984_540_054).abs() < 1e-12);
    }
}
