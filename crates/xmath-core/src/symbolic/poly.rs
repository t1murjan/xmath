//! Univariate polynomials over the rationals: arithmetic, gcd, square-free
//! decomposition, factorisation and numeric roots.

use super::sx::{rat_i, Rat};
use num_bigint::BigInt;
use num_complex::Complex64 as C64;
use num_integer::Integer;
use num_traits::{One, Signed, ToPrimitive, Zero};

/// Coefficients in ascending order, no trailing zeros (the zero polynomial is empty).
pub type P = Vec<Rat>;

pub fn trim(mut p: P) -> P {
    while p.last().is_some_and(|c| c.is_zero()) {
        p.pop();
    }
    p
}

pub fn deg(p: &P) -> isize {
    p.len() as isize - 1
}

pub fn padd(a: &P, b: &P) -> P {
    let n = a.len().max(b.len());
    trim((0..n).map(|i| a.get(i).cloned().unwrap_or_else(Rat::zero) + b.get(i).cloned().unwrap_or_else(Rat::zero)).collect())
}

pub fn psub(a: &P, b: &P) -> P {
    let n = a.len().max(b.len());
    trim((0..n).map(|i| a.get(i).cloned().unwrap_or_else(Rat::zero) - b.get(i).cloned().unwrap_or_else(Rat::zero)).collect())
}

pub fn pmul(a: &P, b: &P) -> P {
    if a.is_empty() || b.is_empty() {
        return Vec::new();
    }
    let mut out = vec![Rat::zero(); a.len() + b.len() - 1];
    for (i, x) in a.iter().enumerate() {
        if x.is_zero() {
            continue;
        }
        for (j, y) in b.iter().enumerate() {
            out[i + j] += x * y;
        }
    }
    trim(out)
}

thread_local! {
    /// Deadline of the current symbolic attempt and whether it was hit (see [`start_budget`]).
    static BUDGET: std::cell::Cell<Option<std::time::Instant>> = const { std::cell::Cell::new(None) };
    static EXPIRED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Starts a fresh budget for one worksheet evaluation. Exact gcds of polynomials whose
/// coefficients came from floating-point numbers grow without bound (a 177 kB Mathcad sample
/// drove one for minutes), so the symbolic attempts give up and the caller falls back to a
/// numeric method.
///
/// The budget belongs to the evaluation, not to a single call: a worksheet whose function is an
/// integral evaluated at hundreds of points would otherwise pay the full budget every time.
pub fn reset_budget(d: std::time::Duration) {
    BUDGET.with(|b| b.set(Some(std::time::Instant::now() + d)));
    EXPIRED.with(|e| e.set(false));
}

/// Starts the budget if the evaluation did not (symbolic evaluation outside a worksheet).
pub fn ensure_budget() {
    BUDGET.with(|b| {
        if b.get().is_none() {
            b.set(Some(std::time::Instant::now() + std::time::Duration::from_millis(1500)));
        }
    });
}

/// True when the attempt has run out of time; sticky for the duration of the attempt.
pub fn budget_expired() -> bool {
    let hit = BUDGET.with(|b| b.get().is_some_and(|t| std::time::Instant::now() >= t));
    if hit {
        EXPIRED.with(|e| e.set(true));
    }
    hit
}

/// Ends the budget (the evaluation is over) and reports whether it was hit.
pub fn end_budget() -> bool {
    BUDGET.with(|b| b.set(None));
    EXPIRED.with(|e| e.replace(false))
}

pub fn pscale(a: &P, c: &Rat) -> P {
    trim(a.iter().map(|x| x * c).collect())
}

pub fn pdivrem(a: &P, b: &P) -> (P, P) {
    assert!(!b.is_empty());
    let mut r = a.clone();
    let db = b.len();
    if r.len() < db {
        return (Vec::new(), r);
    }
    let mut q = vec![Rat::zero(); r.len() - db + 1];
    let lb = b.last().unwrap().clone();
    while r.len() >= db {
        if budget_expired() {
            return (Vec::new(), r);
        }
        let c = r.last().unwrap() / &lb;
        let shift = r.len() - db;
        for (i, y) in b.iter().enumerate() {
            r[shift + i] -= &c * y;
        }
        q[shift] = c;
        r.pop();
        r = trim(r);
        if r.is_empty() {
            break;
        }
    }
    (trim(q), r)
}

pub fn pderiv(a: &P) -> P {
    trim(a.iter().enumerate().skip(1).map(|(i, c)| c * rat_i(i as i64)).collect())
}

pub fn peval(a: &P, x: &Rat) -> Rat {
    let mut r = Rat::zero();
    for c in a.iter().rev() {
        r = r * x + c;
    }
    r
}

pub fn pmonic(a: &P) -> P {
    match a.last() {
        Some(l) if !l.is_one() => pscale(a, &l.recip()),
        _ => a.clone(),
    }
}

pub fn pgcd(a: &P, b: &P) -> P {
    let (mut a, mut b) = (a.clone(), b.clone());
    while !b.is_empty() {
        if budget_expired() {
            return vec![Rat::one()];
        }
        let (_, r) = pdivrem(&a, &b);
        a = b;
        b = r;
    }
    pmonic(&a)
}

pub fn ppow(a: &P, n: usize) -> P {
    let mut r = vec![Rat::one()];
    for _ in 0..n {
        r = pmul(&r, a);
    }
    r
}

/// The integer primitive part of `p` and the rational scale `c` with `p = c * prim`.
/// The leading coefficient of `prim` is positive.
pub fn primitive(p: &P) -> (Rat, P) {
    if p.is_empty() {
        return (Rat::zero(), Vec::new());
    }
    let mut l = BigInt::one();
    for c in p {
        l = l.lcm(c.denom());
    }
    let ints: Vec<BigInt> = p.iter().map(|c| c.numer() * (&l / c.denom())).collect();
    let mut g = BigInt::zero();
    for c in &ints {
        g = g.gcd(c);
    }
    let sign = if ints.last().unwrap().is_negative() { -BigInt::one() } else { BigInt::one() };
    let g = g * &sign;
    let prim: P = ints.iter().map(|c| Rat::from_integer(c / &g)).collect();
    (Rat::new(g, l), prim)
}

/// Square-free decomposition (Yun): `p = c * prod f_i^i`.
pub fn squarefree(p: &P) -> Vec<(P, usize)> {
    let mut out = Vec::new();
    if deg(p) < 1 {
        return out;
    }
    let d = pderiv(p);
    let g = pgcd(p, &d);
    if deg(&g) == 0 {
        return vec![(pmonic(p), 1)];
    }
    let mut w = pdivrem(p, &g).0;
    let mut y = pdivrem(&d, &g).0;
    let mut z = psub(&y, &pderiv(&w));
    let mut i = 1;
    while deg(&w) > 0 {
        let gg = pgcd(&w, &z);
        if deg(&gg) > 0 {
            out.push((gg.clone(), i));
        }
        w = pdivrem(&w, &gg).0;
        y = pdivrem(&z, &gg).0;
        z = psub(&y, &pderiv(&w));
        i += 1;
        if i > 200 {
            break;
        }
    }
    out
}

fn divisors(n: &BigInt) -> Option<Vec<BigInt>> {
    let n = n.abs();
    if n.is_zero() {
        return None;
    }
    let fac = super::sx::factor_int(&n);
    let mut ds = vec![BigInt::one()];
    for (p, m) in fac {
        let cur = ds.clone();
        let mut pk = BigInt::one();
        for _ in 0..m {
            pk *= &p;
            for d in &cur {
                ds.push(d * &pk);
            }
        }
        if ds.len() > 20_000 {
            return None;
        }
    }
    Some(ds)
}

/// Rational roots of an integer polynomial.
pub fn rational_roots(p: &P) -> Vec<Rat> {
    let mut roots = Vec::new();
    let mut p = p.clone();
    // zero root
    while !p.is_empty() && p[0].is_zero() {
        p.remove(0);
        if !roots.contains(&Rat::zero()) {
            roots.push(Rat::zero());
        }
    }
    if deg(&p) < 1 {
        return roots;
    }
    let (_, prim) = primitive(&p);
    let a0 = prim[0].numer().clone();
    let an = prim.last().unwrap().numer().clone();
    let (Some(dn), Some(dd)) = (divisors(&a0), divisors(&an)) else { return roots };
    // coefficients that came from floating-point numbers have enormous numerators, so the
    // candidate set explodes; the budget keeps this bounded like the gcd above
    if dn.len().saturating_mul(dd.len()) > 20_000 {
        return roots;
    }
    for n in &dn {
        if budget_expired() {
            break;
        }
        for d in &dd {
            if budget_expired() {
                break;
            }
            if !n.gcd(d).is_one() {
                continue;
            }
            for s in [1, -1] {
                let cand = Rat::new(n * BigInt::from(s), d.clone());
                if peval(&prim, &cand).is_zero() && !roots.contains(&cand) {
                    roots.push(cand);
                }
            }
        }
    }
    roots.sort();
    roots
}

// ------------------------------------------------------------ numeric roots

pub fn to_c(p: &P) -> Vec<C64> {
    p.iter().map(|c| C64::new(c.to_f64().unwrap_or(f64::NAN), 0.0)).collect()
}

/// All complex roots (Aberth–Ehrlich iteration), with multiplicity.
pub fn roots_numeric(p: &P) -> Vec<C64> {
    let n = deg(p);
    if n < 1 {
        return Vec::new();
    }
    let n = n as usize;
    let c = to_c(p);
    let lc = c[n];
    let a: Vec<C64> = c.iter().map(|x| x / lc).collect();
    let eval = |z: C64| -> (C64, C64) {
        let mut f = C64::new(0.0, 0.0);
        let mut d = C64::new(0.0, 0.0);
        for k in (0..=n).rev() {
            d = d * z + f;
            f = f * z + a[k];
        }
        (f, d)
    };
    let radius = 1.0 + a[..n].iter().map(|x| x.norm()).fold(0.0, f64::max);
    let mut z: Vec<C64> = (0..n)
        .map(|k| {
            let ang = 2.0 * std::f64::consts::PI * (k as f64) / (n as f64) + 0.4;
            C64::from_polar(radius * 0.7, ang)
        })
        .collect();
    for _ in 0..500 {
        let mut worst: f64 = 0.0;
        for i in 0..n {
            let (f, d) = eval(z[i]);
            if f.norm() == 0.0 {
                continue;
            }
            let ratio = f / d;
            let mut s = C64::new(0.0, 0.0);
            for j in 0..n {
                if j != i {
                    s += 1.0 / (z[i] - z[j]);
                }
            }
            let w = ratio / (1.0 - ratio * s);
            if w.is_nan() {
                continue;
            }
            z[i] -= w;
            worst = worst.max(w.norm() / z[i].norm().max(1.0));
        }
        if worst < 1e-15 {
            break;
        }
    }
    // Newton polish
    for zi in z.iter_mut() {
        for _ in 0..3 {
            let (f, d) = eval(*zi);
            if d.norm() > 0.0 {
                let w = f / d;
                if !w.is_nan() {
                    *zi -= w;
                }
            }
        }
        if zi.im.abs() < 1e-11 * zi.re.abs().max(1.0) {
            zi.im = 0.0;
        }
    }
    z
}

fn is_near_int(x: f64) -> Option<i128> {
    let r = x.round();
    if (x - r).abs() <= 1e-6 * x.abs().max(1.0) * 1e-2 + 1e-7 && r.abs() < 1e15 {
        Some(r as i128)
    } else {
        None
    }
}

/// Splits an integer polynomial (square-free, primitive, no rational roots, degree >= 4)
/// into factors by matching products of numeric roots with integer polynomials.
fn recombine(p: &P) -> Vec<P> {
    let n = deg(p) as usize;
    if n < 4 || n > 14 {
        return vec![p.clone()];
    }
    let roots = roots_numeric(p);
    let lc = p.last().unwrap().numer().clone();
    let lc_divs: Vec<BigInt> = match divisors(&lc) {
        Some(d) if d.len() <= 64 => d,
        _ => vec![BigInt::one()],
    };
    // subsets containing root 0 (the factor holding root 0 is found first)
    let m = n - 1;
    for size in 1..=(n - 2) {
        for comb in combinations(m, size) {
            let subset: Vec<usize> = std::iter::once(0).chain(comb.iter().map(|&i| i + 1)).collect();
            if let Some(f) = try_subset(p, &roots, &subset, &lc_divs) {
                let (q, _) = pdivrem(p, &f);
                let (_, q) = primitive(&q);
                let mut out = vec![f];
                out.extend(recombine_or_one(&q));
                return out;
            }
        }
    }
    vec![p.clone()]
}

fn combinations(m: usize, k: usize) -> Vec<Vec<usize>> {
    let mut out = Vec::new();
    let mut cur = Vec::new();
    fn go(start: usize, m: usize, k: usize, cur: &mut Vec<usize>, out: &mut Vec<Vec<usize>>) {
        if cur.len() == k {
            out.push(cur.clone());
            return;
        }
        for i in start..m {
            cur.push(i);
            go(i + 1, m, k, cur, out);
            cur.pop();
        }
    }
    go(0, m, k, &mut cur, &mut out);
    out
}

fn recombine_or_one(q: &P) -> Vec<P> {
    match deg(q) {
        d if d <= 1 => vec![q.clone()],
        2 | 3 => vec![q.clone()],
        _ => recombine(q),
    }
}

fn try_subset(p: &P, roots: &[C64], subset: &[usize], lc_divs: &[BigInt]) -> Option<P> {
    // product of (x - r)
    let mut c: Vec<C64> = vec![C64::new(1.0, 0.0)];
    for &i in subset {
        let r = roots[i];
        let mut nc = vec![C64::new(0.0, 0.0); c.len() + 1];
        for (k, ck) in c.iter().enumerate() {
            nc[k + 1] += ck;
            nc[k] -= ck * r;
        }
        c = nc;
    }
    if c.iter().any(|z| z.im.abs() > 1e-6 * z.re.abs().max(1.0)) {
        return None;
    }
    for d in lc_divs {
        let df = d.to_f64()?;
        let mut ints = Vec::with_capacity(c.len());
        let mut ok = true;
        for z in &c {
            match is_near_int(z.re * df) {
                Some(v) => ints.push(Rat::from_integer(BigInt::from(v))),
                None => {
                    ok = false;
                    break;
                }
            }
        }
        if !ok {
            continue;
        }
        let f = trim(ints);
        let (_, f) = primitive(&f);
        if deg(&f) < 1 {
            continue;
        }
        let (_, r) = pdivrem(p, &f);
        if r.is_empty() {
            return Some(f);
        }
    }
    None
}

/// Factors a square-free integer polynomial into irreducible integer polynomials.
fn factor_squarefree(g: &P) -> Vec<P> {
    let mut rest = g.clone();
    let mut out = Vec::new();
    // roots
    let rr = rational_roots(&rest);
    for r in rr {
        let lin = vec![-r.clone(), Rat::one()];
        let (_, lin) = primitive(&lin);
        while deg(&rest) >= 1 {
            let (q, rem) = pdivrem(&rest, &lin);
            if rem.is_empty() {
                rest = q;
                out.push(lin.clone());
            } else {
                break;
            }
        }
    }
    if deg(&rest) >= 1 {
        let (_, rest) = primitive(&rest);
        match deg(&rest) {
            1..=3 => out.push(rest),
            _ => out.extend(recombine(&rest)),
        }
    }
    out
}

/// Factorisation over the rationals: `p = c * prod f_i^m_i` with primitive integer
/// irreducible `f_i` (positive leading coefficient).
pub fn factor_q(p: &P) -> (Rat, Vec<(P, usize)>) {
    if p.is_empty() {
        return (Rat::zero(), Vec::new());
    }
    let (c, prim) = primitive(p);
    if deg(&prim) < 1 {
        return (c, Vec::new());
    }
    let mut factors: Vec<(P, usize)> = Vec::new();
    // square-free parts (scale each to a primitive integer polynomial)
    let mut cc = c;
    for (sf, m) in squarefree(&prim) {
        let (s, sfp) = primitive(&sf);
        let _ = s;
        for f in factor_squarefree(&sfp) {
            if let Some(slot) = factors.iter_mut().find(|(g, _)| *g == f) {
                slot.1 += m;
            } else {
                factors.push((f, m));
            }
        }
    }
    // recover the constant: p / prod
    let mut prod = vec![Rat::one()];
    for (f, m) in &factors {
        prod = pmul(&prod, &ppow(f, *m));
    }
    if let (Some(a), Some(b)) = (p.last(), prod.last()) {
        cc = a / b;
    }
    factors.sort_by(|a, b| deg(&a.0).cmp(&deg(&b.0)).then_with(|| a.0.cmp(&b.0)));
    (cc, factors)
}

pub fn from_ints(v: &[i64]) -> P {
    trim(v.iter().map(|&i| rat_i(i)).collect())
}

#[allow(dead_code)]
pub fn to_i64s(p: &P) -> Vec<i64> {
    p.iter().map(|c| c.to_integer().to_i64().unwrap_or(0)).collect()
}
