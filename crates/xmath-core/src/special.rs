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

/// Regularized lower incomplete gamma function P(a, x) = γ(a, x) / Γ(a) (series for x < a+1,
/// continued fraction for the upper part otherwise). Mathcad's pchisq/pgamma are built on it.
pub fn gamma_p(a: f64, x: f64) -> f64 {
    if x <= 0.0 {
        return 0.0;
    }
    if a <= 0.0 {
        return f64::NAN;
    }
    let ln_pre = -x + a * x.ln() - libm::lgamma(a);
    if x < a + 1.0 {
        let mut ap = a;
        let mut term = 1.0 / a;
        let mut sum = term;
        for _ in 0..10_000 {
            ap += 1.0;
            term *= x / ap;
            sum += term;
            if term.abs() < sum.abs() * 1e-16 {
                break;
            }
        }
        (sum * ln_pre.exp()).clamp(0.0, 1.0)
    } else {
        // Lentz's continued fraction for Q(a, x)
        let tiny = 1e-300;
        let mut b = x + 1.0 - a;
        let mut c = 1.0 / tiny;
        let mut d = 1.0 / b;
        let mut h = d;
        for i in 1..10_000 {
            let an = -i as f64 * (i as f64 - a);
            b += 2.0;
            d = an * d + b;
            if d.abs() < tiny {
                d = tiny;
            }
            c = b + an / c;
            if c.abs() < tiny {
                c = tiny;
            }
            d = 1.0 / d;
            let del = d * c;
            h *= del;
            if (del - 1.0).abs() < 1e-16 {
                break;
            }
        }
        (1.0 - h * ln_pre.exp()).clamp(0.0, 1.0)
    }
}

/// The regularized incomplete beta function I_x(a, b) (continued fraction, Lentz).
pub fn beta_i(a: f64, b: f64, x: f64) -> f64 {
    if x <= 0.0 {
        return 0.0;
    }
    if x >= 1.0 {
        return 1.0;
    }
    let ln_front = libm::lgamma(a + b) - libm::lgamma(a) - libm::lgamma(b) + a * x.ln() + b * (1.0 - x).ln();
    let front = ln_front.exp();
    // the continued fraction converges quickly for x < (a+1)/(a+b+2); otherwise use symmetry
    if x < (a + 1.0) / (a + b + 2.0) {
        (front * beta_cf(a, b, x) / a).clamp(0.0, 1.0)
    } else {
        (1.0 - front * beta_cf(b, a, 1.0 - x) / b).clamp(0.0, 1.0)
    }
}

fn beta_cf(a: f64, b: f64, x: f64) -> f64 {
    let tiny = 1e-300;
    let mut c = 1.0;
    let mut d = 1.0 - (a + b) * x / (a + 1.0);
    if d.abs() < tiny {
        d = tiny;
    }
    d = 1.0 / d;
    let mut h = d;
    for m in 1..500 {
        let m = m as f64;
        let m2 = 2.0 * m;
        // even step
        let aa = m * (b - m) * x / ((a + m2 - 1.0) * (a + m2));
        d = 1.0 + aa * d;
        if d.abs() < tiny {
            d = tiny;
        }
        c = 1.0 + aa / c;
        if c.abs() < tiny {
            c = tiny;
        }
        d = 1.0 / d;
        h *= d * c;
        // odd step
        let aa = -(a + m) * (a + b + m) * x / ((a + m2) * (a + m2 + 1.0));
        d = 1.0 + aa * d;
        if d.abs() < tiny {
            d = tiny;
        }
        c = 1.0 + aa / c;
        if c.abs() < tiny {
            c = tiny;
        }
        d = 1.0 / d;
        let del = d * c;
        h *= del;
        if (del - 1.0).abs() < 1e-15 {
            break;
        }
    }
    h
}

/// Student's t distribution: Mathcad's dt, pt and qt.
pub fn t_pdf(t: f64, nu: f64) -> f64 {
    let ln = libm::lgamma((nu + 1.0) / 2.0) - libm::lgamma(nu / 2.0) - 0.5 * (nu * std::f64::consts::PI).ln();
    (ln - (nu + 1.0) / 2.0 * (1.0 + t * t / nu).ln()).exp()
}

pub fn t_cdf(t: f64, nu: f64) -> f64 {
    let x = nu / (nu + t * t);
    let tail = 0.5 * beta_i(nu / 2.0, 0.5, x);
    if t >= 0.0 {
        1.0 - tail
    } else {
        tail
    }
}

/// The quantile of Student's t distribution: the `t` with `t_cdf(t) == p`.
pub fn t_quantile(p: f64, nu: f64) -> f64 {
    if !(0.0..=1.0).contains(&p) {
        return f64::NAN;
    }
    if p == 0.0 {
        return f64::NEG_INFINITY;
    }
    if p == 1.0 {
        return f64::INFINITY;
    }
    if p == 0.5 {
        return 0.0;
    }
    let mut lo = -1e3;
    let mut hi = 1e3;
    for _ in 0..200 {
        let mid = 0.5 * (lo + hi);
        if t_cdf(mid, nu) < p {
            lo = mid;
        } else {
            hi = mid;
        }
        if hi - lo <= 1e-14 * hi.abs().max(1.0) {
            break;
        }
    }
    // bisection alone reaches the resolution of f64 long before the loop ends
    0.5 * (lo + hi)
}

/// The F distribution: Mathcad's dF, pF and qF.
pub fn f_pdf(x: f64, d1: f64, d2: f64) -> f64 {
    if x <= 0.0 {
        return 0.0;
    }
    // f(x) = sqrt(((d1 x)^d1 d2^d2) / (d1 x + d2)^(d1+d2)) / (x · B(d1/2, d2/2))
    let ln_beta = libm::lgamma(d1 / 2.0) + libm::lgamma(d2 / 2.0) - libm::lgamma((d1 + d2) / 2.0);
    (0.5 * (d1 * (d1 * x).ln() + d2 * d2.ln() - (d1 + d2) * (d1 * x + d2).ln()) - x.ln() - ln_beta).exp()
}

pub fn f_cdf(x: f64, d1: f64, d2: f64) -> f64 {
    if x <= 0.0 {
        return 0.0;
    }
    beta_i(d1 / 2.0, d2 / 2.0, d1 * x / (d1 * x + d2))
}

/// The quantile of the F distribution: the `x` with `f_cdf(x) == p`.
pub fn f_quantile(p: f64, d1: f64, d2: f64) -> f64 {
    if !(0.0..=1.0).contains(&p) {
        return f64::NAN;
    }
    if p == 0.0 {
        return 0.0;
    }
    if p == 1.0 {
        return f64::INFINITY;
    }
    let mut lo = 0.0;
    let mut hi = 1000.0;
    while f_cdf(hi, d1, d2) < p {
        hi *= 2.0;
        if hi > 1e300 {
            return f64::INFINITY;
        }
    }
    for _ in 0..200 {
        let mid = 0.5 * (lo + hi);
        if f_cdf(mid, d1, d2) < p {
            lo = mid;
        } else {
            hi = mid;
        }
        if hi - lo <= 1e-15 * hi.max(1.0) {
            break;
        }
    }
    // bisection alone reaches the resolution of f64 long before the loop ends
    0.5 * (lo + hi)
}

/// The chi-square distribution with `k` degrees of freedom: Mathcad's dchisq, pchisq, qchisq.
pub fn chisq_pdf(x: f64, k: f64) -> f64 {
    if x < 0.0 {
        return 0.0;
    }
    let h = k / 2.0;
    if x == 0.0 {
        return if h < 1.0 { f64::INFINITY } else if h == 1.0 { 0.5 } else { 0.0 };
    }
    ((h - 1.0) * x.ln() - x / 2.0 - h * std::f64::consts::LN_2 - libm::lgamma(h)).exp()
}

pub fn chisq_cdf(x: f64, k: f64) -> f64 {
    if x <= 0.0 {
        return 0.0;
    }
    gamma_p(k / 2.0, x / 2.0)
}

/// The quantile of the chi-square distribution: the `x` with `chisq_cdf(x) == p`.
pub fn chisq_quantile(p: f64, k: f64) -> f64 {
    if !(0.0..=1.0).contains(&p) {
        return f64::NAN;
    }
    if p == 0.0 {
        return 0.0;
    }
    if p == 1.0 {
        return f64::INFINITY;
    }
    let mut lo = 0.0;
    let mut hi = k + 10.0 * (2.0 * k).sqrt() + 100.0;
    while chisq_cdf(hi, k) < p {
        hi *= 2.0;
        if hi > 1e300 {
            return f64::INFINITY;
        }
    }
    // bisection, then a few Newton steps for the last digits
    for _ in 0..200 {
        let mid = 0.5 * (lo + hi);
        if chisq_cdf(mid, k) < p {
            lo = mid;
        } else {
            hi = mid;
        }
        if hi - lo <= 1e-15 * hi.max(1.0) {
            break;
        }
    }
    let mut x = 0.5 * (lo + hi);
    for _ in 0..4 {
        let d = chisq_pdf(x, k);
        if !(d.is_finite() && d > 0.0) {
            break;
        }
        let step = (chisq_cdf(x, k) - p) / d;
        let next = x - step;
        if !(next > 0.0 && next.is_finite()) {
            break;
        }
        x = next;
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
    fn student_t() {
        // the textbook two-sided 95% value for 10 degrees of freedom, and its symmetry
        assert!((t_quantile(0.975, 10.0) - 2.228_138_852).abs() < 1e-8);
        assert!((t_cdf(-2.228_138_852, 10.0) - 0.025).abs() < 1e-8);
        // the density at zero has a closed form: Γ((ν+1)/2)/(sqrt(νπ)·Γ(ν/2))
        let closed = |nu: f64| (libm::lgamma((nu + 1.0) / 2.0) - 0.5 * (nu * std::f64::consts::PI).ln() - libm::lgamma(nu / 2.0)).exp();
        for nu in [1.0, 5.0, 30.0] {
            assert!((t_pdf(0.0, nu) - closed(nu)).abs() < 1e-14, "nu={nu}");
        }
        // Cauchy is t with one degree of freedom: at(0) = 1/π
        assert!((t_pdf(0.0, 1.0) - 1.0 / std::f64::consts::PI).abs() < 1e-14);
        for p in [0.01, 0.2, 0.5, 0.8, 0.99] {
            let q = t_quantile(p, 7.0);
            assert!((t_cdf(q, 7.0) - p).abs() < 1e-12, "{p} -> {q}");
        }
    }

    #[test]
    fn f_distribution() {
        // F(0.95; 5, 10) = 3.32583 and F(0.99; 2, 7) = 9.54658 from the tables
        assert!((f_quantile(0.95, 5.0, 10.0) - 3.325_835).abs() < 1e-5);
        assert!((f_quantile(0.99, 2.0, 7.0) - 9.546_578).abs() < 1e-5);
        for p in [0.05, 0.5, 0.9] {
            let q = f_quantile(p, 3.0, 12.0);
            assert!((f_cdf(q, 3.0, 12.0) - p).abs() < 1e-12, "{p} -> {q}");
        }
        // the density integrates to the cdf: check one step against a Riemann sum
        let (a, b, n) = (0.05f64, 4.0f64, 20_000);
        let h = (b - a) / n as f64;
        let sum: f64 = (0..n).map(|i| f_pdf(a + (i as f64 + 0.5) * h, 3.0, 12.0) * h).sum();
        assert!((sum - (f_cdf(b, 3.0, 12.0) - f_cdf(a, 3.0, 12.0))).abs() < 1e-6);
    }

    #[test]
    fn chi_square() {
        // Mathcad 15 itself stored qchisq(1 - 0.1/2, 35) = 49.801846855417033 in a corpus file;
        // its own quantile is a numeric root accurate to ~5e-8 relative, so 1e-5 here is enough
        assert!((chisq_quantile(0.95, 35.0) - 49.801_846_855_417_033).abs() < 1e-5);
        for p in [1e-6, 0.01, 0.3, 0.95, 0.999] {
            let q = chisq_quantile(p, 35.0);
            assert!((chisq_cdf(q, 35.0) - p).abs() < 1e-12, "{p} -> {q}");
        }
        assert!((chisq_quantile(0.95, 11.0) - 19.675_137_572_682_488).abs() < 1e-9);
        assert!((chisq_quantile(0.5, 2.0) - 1.386_294_361_119_890_6).abs() < 1e-12);
        // Mathcad's own stored quantile has a ~5e-8 relative error: its cdf is 0.94999997
        assert!((chisq_cdf(49.801_846_855_417_033, 35.0) - 0.95).abs() < 1e-7);
        assert!((chisq_pdf(1.0, 1.0) - 0.241_970_724_519_143_37).abs() < 1e-14);
        assert!((chisq_pdf(2.0, 4.0) - 0.183_939_720_585_721_17).abs() < 1e-14);
        assert_eq!(chisq_cdf(-1.0, 3.0), 0.0);
        assert_eq!(chisq_quantile(0.0, 3.0), 0.0);
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

// ---------------------------------------------------------------------------
// Special functions that Mathcad evaluates only through the symbolic engine
// (`→ float`): the dilogarithm, digamma, polygamma, the zeta function, the
// exponential and sine integrals, the Fresnel integrals and the hypergeometric
// series.

/// Bernoulli numbers B2, B4, … (used by the asymptotic expansions below).
const BERNOULLI: [f64; 8] = [
    1.0 / 6.0,
    -1.0 / 30.0,
    1.0 / 42.0,
    -1.0 / 30.0,
    5.0 / 66.0,
    -691.0 / 2730.0,
    7.0 / 6.0,
    -3617.0 / 510.0,
];

/// The dilogarithm Li₂(x).
pub fn dilog(x: f64) -> f64 {
    const PI2_6: f64 = 1.6449340668482264;
    if x <= -1.0 {
        // -Li₂(x/(x-1)) - ½ ln²(1-x)
        let y = x / (x - 1.0);
        return -dilog(y) - 0.5 * (1.0 - x).ln().powi(2);
    }
    if x > 0.5 {
        // π²/6 - ln x ln(1-x) - Li₂(1-x)
        return PI2_6 - x.ln() * (1.0 - x).ln() - dilog(1.0 - x);
    }
    let mut term = x;
    let mut sum = x;
    let mut k = 2.0;
    while k < 1e7 {
        term *= x;
        let add = term / (k * k);
        sum += add;
        if add.abs() < 1e-18 * sum.abs().max(1.0) {
            break;
        }
        k += 1.0;
    }
    sum
}

/// The digamma function ψ(x).
pub fn digamma(mut x: f64) -> f64 {
    if x <= 0.0 {
        return f64::NAN;
    }
    let mut shift = 0.0;
    while x < 8.0 {
        shift -= 1.0 / x;
        x += 1.0;
    }
    // ψ(x) ≈ ln x - 1/(2x) - Σ B₂ₖ / (2k x²ᵏ)
    let mut s = x.ln() - 0.5 / x;
    let mut xp = x * x;
    for (i, b) in BERNOULLI.iter().enumerate() {
        let k = (i + 1) as f64;
        let term = b / (2.0 * k) / xp;
        s -= term;
        xp *= x * x;
        if term.abs() < 1e-20 * s.abs().max(1.0) {
            break;
        }
    }
    s + shift
}

/// The Hurwitz zeta function ζ(s, q) = Σ (q+k)^{-s}.
pub fn hurwitz_zeta(s: f64, q: f64) -> f64 {
    if s == 1.0 {
        return f64::INFINITY;
    }
    let n = 20usize;
    let mut sum: f64 = (0..n).map(|k| (q + k as f64).powf(-s)).sum();
    let nn = q + n as f64;
    sum += nn.powf(1.0 - s) / (s - 1.0) + 0.5 * nn.powf(-s);
    for (i, b) in BERNOULLI.iter().enumerate() {
        let k = (i + 1) as f64;
        let mut num = 1.0;
        for j in 0..(2 * (i + 1) - 1) {
            num *= s + j as f64;
        }
        let mut den = 1.0;
        for j in 1..=(2 * (i + 1)) {
            den *= j as f64;
        }
        let term = b * num / den * nn.powf(-s - 2.0 * k + 1.0);
        sum += term;
        if term.abs() < 1e-20 * sum.abs().max(1.0) {
            break;
        }
    }
    sum
}

/// The polygamma function ψ⁽ⁿ⁾(x) = (-1)^(n+1) n! ζ(n+1, x), n ≥ 1.
pub fn polygamma(n: u32, x: f64) -> f64 {
    if x <= 0.0 {
        return f64::NAN;
    }
    if n == 0 {
        return digamma(x);
    }
    let mut fact = 1.0;
    for k in 1..=n {
        fact *= k as f64;
    }
    let sign = if n % 2 == 1 { 1.0 } else { -1.0 };
    sign * fact * hurwitz_zeta(n as f64 + 1.0, x)
}

/// The cosine integral Ci(x).
pub fn ci(x: f64) -> f64 {
    const EULER: f64 = 0.577_215_664_901_532_9;
    if x <= 0.0 {
        return f64::NAN;
    }
    if x > 40.0 {
        let f = sine_integral_aux(x, 0);
        let g = sine_integral_aux(x, 1);
        return f * x.sin() / x - g * x.cos() / x;
    }
    let mut sum = 0.0;
    let mut term = 1.0;
    let mut k = 1.0;
    while k < 500.0 {
        term *= -x * x / ((2.0 * k - 1.0) * (2.0 * k));
        let add = term / (2.0 * k);
        sum += add;
        if add.abs() < 1e-18 * sum.abs().max(1.0) {
            break;
        }
        k += 1.0;
    }
    EULER + x.ln() + sum
}

/// The Riemann zeta function ζ(s).
pub fn zeta(s: f64) -> f64 {
    if s == 1.0 {
        return f64::INFINITY;
    }
    if s < 0.0 {
        // reflection: ζ(s) = 2^s π^(s-1) sin(πs/2) Γ(1-s) ζ(1-s)
        let pi = std::f64::consts::PI;
        return 2f64.powf(s) * pi.powf(s - 1.0) * (pi * s / 2.0).sin() * libm::tgamma(1.0 - s) * zeta(1.0 - s);
    }
    hurwitz_zeta(s, 1.0)
}

/// The exponential integral Ei(x) (Cauchy principal value for x < 0).
pub fn ei(x: f64) -> f64 {
    const EULER: f64 = 0.5772156649015329;
    if x.abs() < 40.0 {
        let mut sum = 0.0;
        let mut term = 1.0;
        for k in 1..200 {
            term *= x / k as f64;
            let add = term / k as f64;
            sum += add;
            if add.abs() < 1e-18 * sum.abs().max(1.0) {
                break;
            }
        }
        return EULER + x.abs().ln() + sum;
    }
    // asymptotic expansion
    let mut sum = 1.0;
    let mut term = 1.0;
    for k in 1..40 {
        term *= k as f64 / x;
        sum += term;
        if term.abs() > 1e300 {
            break;
        }
    }
    x.exp() / x * sum
}

/// The sine integral Si(x).
pub fn si(x: f64) -> f64 {
    const PI_2: f64 = std::f64::consts::FRAC_PI_2;
    if x.abs() > 40.0 {
        // Si(x) = π/2 - f(x) cos x / x - g(x) sin x / x
        let f = sine_integral_aux(x, 0);
        let g = sine_integral_aux(x, 1);
        let sign = if x > 0.0 { 1.0 } else { -1.0 };
        return sign * PI_2 - f * x.cos() / x - g * x.sin() / x;
    }
    let mut sum = x;
    let mut term = x;
    let mut k = 1.0;
    while k < 500.0 {
        term *= -x * x / ((2.0 * k) * (2.0 * k + 1.0));
        let add = term / (2.0 * k + 1.0);
        sum += add;
        if add.abs() < 1e-18 * sum.abs().max(1.0) {
            break;
        }
        k += 1.0;
    }
    sum
}

/// `f` (odd = 0) and `g` (odd = 1) of the asymptotic Si/Ci expansions.
fn sine_integral_aux(x: f64, odd: i32) -> f64 {
    let mut sum = 0.0;
    let mut term = if odd == 0 { 1.0 } else { 1.0 / x };
    for k in 0..30 {
        if k > 0 {
            term *= ((2 * k) as f64 + odd as f64) * ((2 * k - 1) as f64 + odd as f64) * 0.0;
        }
        let _ = term;
        // (2k+odd)! / x^(2k+odd)
        let mut f = 1.0;
        for j in 1..=(2 * k + odd) {
            f *= j as f64;
        }
        term = f / x.powi(2 * k + odd);
        if k % 2 == 1 {
            term = -term;
        }
        sum += term;
        if term.abs() > 1e250 {
            break;
        }
    }
    sum
}

/// The Fresnel integrals C(x) (`c = true`) and S(x).
pub fn fresnel(x: f64, c: bool) -> f64 {
    const PI_2: f64 = std::f64::consts::FRAC_PI_2;
    if x.abs() < 6.0 {
        let mut sum = 0.0;
        let mut k = 0.0;
        while k < 200.0 {
            let mut fact = 1.0;
            let top = if c { 2.0 * k } else { 2.0 * k + 1.0 };
            let mut j = 1.0;
            while j <= top {
                fact *= j;
                j += 1.0;
            }
            let pow = if c { 4.0 * k + 1.0 } else { 4.0 * k + 3.0 };
            let mut term = PI_2.powf(top) * x.powf(pow) / (fact * pow);
            if k as i32 % 2 == 1 {
                term = -term;
            }
            sum += term;
            if term.abs() < 1e-18 * sum.abs().max(1.0) {
                break;
            }
            k += 1.0;
        }
        return sum;
    }
    // asymptotic: C, S → 1/2 as x → ∞
    let sign = if x > 0.0 { 1.0 } else { -1.0 };
    let ax = x.abs();
    let (s, co) = ((PI_2 * ax * ax).sin(), (PI_2 * ax * ax).cos());
    let f = 1.0 / (std::f64::consts::PI * ax);
    let g = 1.0 / (std::f64::consts::PI.powi(2) * ax.powi(3));
    if c {
        0.5 * (1.0 + sign) + sign * (f * s - g * co)
    } else {
        0.5 * (1.0 + sign) - sign * (f * co + g * s)
    }
}

/// The generalised hypergeometric series pFq(a; b; z). Outside the unit disc the series
/// diverges, so the Gauss function ₂F₁ is continued with the Pfaff transformation.
pub fn hypergeom(a: &[f64], b: &[f64], z: f64) -> f64 {
    if z.abs() >= 1.0 && a.len() == 2 && b.len() == 1 && (z - 1.0).abs() > 1e-12 {
        // ₂F₁(a, b; c; z) = (1-z)^(-a) ₂F₁(a, c-b; c; z/(z-1))
        let w = z / (z - 1.0);
        return (1.0 - z).powf(-a[0]) * hypergeom(&[a[0], b[0] - a[1]], b, w);
    }
    let mut term = 1.0;
    let mut sum = 1.0;
    for k in 1..10000 {
        let mut t = term * z / k as f64;
        for p in a {
            t *= p + (k - 1) as f64;
        }
        for q in b {
            let d = q + (k - 1) as f64;
            if d == 0.0 {
                return f64::NAN;
            }
            t /= d;
        }
        term = t;
        sum += term;
        if term.abs() < 1e-17 * sum.abs().max(1.0) {
            break;
        }
    }
    sum
}
