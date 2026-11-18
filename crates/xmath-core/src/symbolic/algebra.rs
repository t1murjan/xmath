//! Algebraic operations on `Sx`: expand, collect, coefficients, rational-function
//! normal forms, factorisation, partial fractions and simplification.

use super::poly::{self, P};
use super::sx::*;
use num_bigint::BigInt;
use num_integer::Integer;
use num_traits::{One, Signed, ToPrimitive, Zero};
use std::collections::BTreeMap;

// ======================================================================= expand

pub fn expand(e: &Sx) -> Sx {
    match e {
        Sx::Add(v) => sum(v.iter().map(expand).collect()),
        Sx::Mul(v) => distribute(v.iter().map(expand).collect()),
        Sx::Pow(b, x) => {
            let b = expand(b);
            let x = expand(x);
            expand_pow(b, x)
        }
        Sx::Fun(n, a) => expand_fun(n, a.iter().map(expand).collect()),
        Sx::Mat(m) => Sx::Mat(m.iter().map(|r| r.iter().map(expand).collect()).collect()),
        _ => e.clone(),
    }
}

fn terms_of(e: Sx) -> Vec<Sx> {
    match e {
        Sx::Add(v) => v,
        other => vec![other],
    }
}

fn distribute(fs: Vec<Sx>) -> Sx {
    if fs.iter().any(Sx::is_mat) {
        return product(fs);
    }
    let mut acc: Vec<Sx> = vec![one()];
    for f in fs {
        let terms = terms_of(f);
        let mut next = Vec::with_capacity(acc.len() * terms.len());
        for a in &acc {
            for t in &terms {
                next.push(mul(a.clone(), t.clone()));
            }
        }
        acc = terms_of(sum(next));
        if acc.len() > 20_000 {
            break;
        }
    }
    sum(acc)
}

fn expand_pow(b: Sx, x: Sx) -> Sx {
    if let Sx::Num(r) = &x {
        if r.is_integer() {
            if let Some(n) = r.to_integer().to_i64() {
                if n > 1 && n <= 100 {
                    if let Sx::Add(ref terms) = b {
                        if (terms.len() as f64).powi(2) * (n as f64) < 100_000.0 {
                            // binary exponentiation
                            let mut result = one();
                            let mut base = b.clone();
                            let mut k = n;
                            while k > 0 {
                                if k & 1 == 1 {
                                    result = distribute(vec![result, base.clone()]);
                                }
                                k >>= 1;
                                if k > 0 {
                                    base = distribute(vec![base.clone(), base]);
                                }
                            }
                            return result;
                        }
                    }
                }
            }
        }
    }
    if let Sx::Add(ts) = &x {
        // b^(p+q) = b^p * b^q
        if !matches!(b, Sx::Mat(_)) {
            return distribute(ts.iter().map(|t| expand_pow(b.clone(), t.clone())).collect());
        }
    }
    pow(b, x)
}

fn sin_cos_n(n: i64, a: &Sx) -> (Sx, Sx) {
    // (sin(n a), cos(n a)) expanded in sin a, cos a
    let (s1, c1) = (func("sin", vec![a.clone()]), func("cos", vec![a.clone()]));
    if n == 1 {
        return (s1, c1);
    }
    let (s, c) = sin_cos_n(n - 1, a);
    let sn = expand(&add(mul(s.clone(), c1.clone()), mul(c.clone(), s1.clone())));
    let cn = expand(&sub(mul(c, c1), mul(s, s1)));
    (sn, cn)
}

fn expand_fun(name: &str, args: Vec<Sx>) -> Sx {
    if args.len() == 1 {
        let a = &args[0];
        match name {
            "exp" => {
                if let Sx::Add(ts) = a {
                    return product(ts.iter().map(|t| func("exp", vec![t.clone()])).collect());
                }
            }
            "sin" | "cos" => {
                if let Sx::Add(ts) = a {
                    let first = ts[0].clone();
                    let rest = sum(ts[1..].to_vec());
                    let (s1, c1) = (expand_fun("sin", vec![first.clone()]), expand_fun("cos", vec![first]));
                    let (s2, c2) = (expand_fun("sin", vec![rest.clone()]), expand_fun("cos", vec![rest]));
                    return expand(&if name == "sin" { add(mul(s1, c2), mul(c1, s2)) } else { sub(mul(c1, c2), mul(s1, s2)) });
                }
                if let Sx::Mul(v) = a {
                    if let Some(Sx::Num(c)) = v.first() {
                        if c.is_integer() {
                            if let Some(n) = c.to_integer().to_i64() {
                                if (2..=8).contains(&n) {
                                    let rest = product(v[1..].to_vec());
                                    if pi_multiple(&rest).is_none() {
                                        let (s, co) = sin_cos_n(n, &rest);
                                        return if name == "sin" { s } else { co };
                                    }
                                }
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
    func(name, args)
}

// ================================================================ polynomial views

/// Coefficients (ascending) of `e` as a polynomial in `x`, each independent of `x`.
pub fn poly_in(e: &Sx, x: &Sx) -> Option<Vec<Sx>> {
    let e = expand(e);
    let mut by: BTreeMap<usize, Vec<Sx>> = BTreeMap::new();
    for t in terms_of(e) {
        let (k, c) = split_power(&t, x)?;
        by.entry(k).or_default().push(c);
    }
    let n = by.keys().next_back().copied().unwrap_or(0);
    Some((0..=n).map(|k| by.remove(&k).map(sum).unwrap_or_else(zero)).collect())
}

/// `term = c * x^k` with `c` independent of `x`.
fn split_power(t: &Sx, x: &Sx) -> Option<(usize, Sx)> {
    if !t.depends(x) {
        return Some((0, t.clone()));
    }
    let factors: Vec<Sx> = match t {
        Sx::Mul(v) => v.clone(),
        other => vec![other.clone()],
    };
    let mut k = 0usize;
    let mut rest = Vec::new();
    for f in factors {
        if &f == x {
            k += 1;
        } else if let Sx::Pow(b, e) = &f {
            if **b == *x {
                match &**e {
                    Sx::Num(r) if r.is_integer() && !r.is_negative() => k += r.to_integer().to_usize()?,
                    _ => return None,
                }
            } else if f.depends(x) {
                return None;
            } else {
                rest.push(f);
            }
        } else if f.depends(x) {
            return None;
        } else {
            rest.push(f);
        }
    }
    Some((k, product(rest)))
}

/// Rational-coefficient polynomial in `x`.
pub fn to_p(e: &Sx, x: &Sx) -> Option<P> {
    let cs = poly_in(e, x)?;
    let mut out = Vec::with_capacity(cs.len());
    for c in cs {
        match c {
            Sx::Num(r) => out.push(r),
            _ => return None,
        }
    }
    Some(poly::trim(out))
}

pub fn from_p(p: &P, x: &Sx) -> Sx {
    sum(p.iter().enumerate().map(|(k, c)| mul(Sx::Num(c.clone()), pow(x.clone(), int(k as i64)))).collect())
}

// ================================================================ collect / coeffs

pub fn collect(e: &Sx, x: &Sx) -> Sx {
    if let Sx::Mat(m) = e {
        return Sx::Mat(m.iter().map(|r| r.iter().map(|v| collect(v, x)).collect()).collect());
    }
    let e = expand(e);
    let mut by: BTreeMap<usize, Vec<Sx>> = BTreeMap::new();
    let mut other = Vec::new();
    for t in terms_of(e) {
        match split_power(&t, x) {
            Some((k, c)) => by.entry(k).or_default().push(c),
            None => other.push(t),
        }
    }
    let mut out = other;
    for (k, cs) in by {
        let c = sum(cs);
        let xp = pow(x.clone(), int(k as i64));
        out.push(if k == 0 {
            c
        } else if matches!(c, Sx::Add(_)) {
            Sx::Mul(sorted(vec![c, xp]))
        } else {
            mul(c, xp)
        });
    }
    // sum() would re-merge only identical terms, so the grouped form is kept
    let mut res = out;
    res.sort();
    match res.len() {
        0 => zero(),
        1 => res.pop().unwrap(),
        _ => Sx::Add(res),
    }
}

fn sorted(mut v: Vec<Sx>) -> Vec<Sx> {
    v.sort();
    v
}

pub fn coeffs(e: &Sx, x: &Sx) -> Option<Sx> {
    let cs = poly_in(e, x)?;
    Some(Sx::Mat(cs.into_iter().map(|c| vec![c]).collect()))
}

// ================================================================ atoms

#[derive(Default)]
pub struct Atoms {
    pub pairs: Vec<(Sx, Sx)>,
}

impl Atoms {
    fn placeholder(&mut self, e: &Sx) -> Sx {
        if let Some((p, _)) = self.pairs.iter().find(|(_, o)| o == e) {
            return p.clone();
        }
        let p = Sx::Sym(format!("§{}", self.pairs.len()));
        self.pairs.push((p.clone(), e.clone()));
        p
    }
    pub fn restore(&self, e: &Sx) -> Sx {
        if self.pairs.is_empty() {
            return e.clone();
        }
        subs_many(e, &self.pairs)
    }
}

/// Replaces non-polynomial subexpressions by placeholder symbols.
pub fn abstract_atoms(e: &Sx, at: &mut Atoms) -> Sx {
    match e {
        Sx::Num(_) | Sx::Sym(_) => e.clone(),
        Sx::Float(_) => e.clone(),
        Sx::Im => at.placeholder(e),
        Sx::Add(v) => sum(v.iter().map(|x| abstract_atoms(x, at)).collect()),
        Sx::Mul(v) => product(v.iter().map(|x| abstract_atoms(x, at)).collect()),
        Sx::Pow(b, x) => match &**x {
            Sx::Num(r) if r.is_integer() => pow(abstract_atoms(b, at), (**x).clone()),
            _ => at.placeholder(e),
        },
        Sx::Fun(..) | Sx::Mat(_) => at.placeholder(e),
    }
}

// ================================================================ together / cancel

/// Numerator and denominator of a rational expression (no cancellation).
pub fn num_den(e: &Sx) -> (Sx, Sx) {
    match e {
        Sx::Num(r) => (Sx::Num(Rat::from_integer(r.numer().clone())), Sx::Num(Rat::from_integer(r.denom().clone()))),
        Sx::Mul(v) => {
            let (mut ns, mut ds) = (Vec::new(), Vec::new());
            for f in v {
                let (n, d) = num_den(f);
                ns.push(n);
                ds.push(d);
            }
            (product(ns), product(ds))
        }
        Sx::Pow(b, x) => {
            if let Sx::Num(r) = &**x {
                if r.is_integer() {
                    let (n, d) = num_den(b);
                    return if r.is_negative() {
                        let k = -r.clone();
                        (pow(d, Sx::Num(k.clone())), pow(n, Sx::Num(k)))
                    } else {
                        (pow(n, (**x).clone()), pow(d, (**x).clone()))
                    };
                }
            }
            if let Sx::Num(r) = &**x {
                if r.is_negative() {
                    return (one(), pow((**b).clone(), Sx::Num(-r.clone())));
                }
            }
            (e.clone(), one())
        }
        Sx::Add(v) => {
            let parts: Vec<(Sx, Sx)> = v.iter().map(num_den).collect();
            if parts.iter().all(|(_, d)| d.is_one()) {
                return (e.clone(), one());
            }
            // common denominator: lcm of integer parts, max power of the other bases
            let mut lcm_int = BigInt::one();
            let mut bases: BTreeMap<Sx, i64> = BTreeMap::new();
            let dens: Vec<(BigInt, BTreeMap<Sx, i64>)> = parts
                .iter()
                .map(|(_, d)| {
                    let mut ip = BigInt::one();
                    let mut m: BTreeMap<Sx, i64> = BTreeMap::new();
                    let fs: Vec<Sx> = match d {
                        Sx::Mul(v) => v.clone(),
                        other => vec![other.clone()],
                    };
                    for f in fs {
                        match f {
                            Sx::Num(r) => ip *= r.to_integer(),
                            Sx::Pow(b, x) => match x.as_int() {
                                Some(k) if k > 0 => *m.entry(*b).or_insert(0) += k,
                                _ => *m.entry(Sx::Pow(b, x)).or_insert(0) += 1,
                            },
                            other => *m.entry(other).or_insert(0) += 1,
                        }
                    }
                    (ip, m)
                })
                .collect();
            for (ip, m) in &dens {
                lcm_int = lcm_int.lcm(ip);
                for (b, k) in m {
                    let e = bases.entry(b.clone()).or_insert(0);
                    *e = (*e).max(*k);
                }
            }
            let mut common: Vec<Sx> = vec![Sx::Num(Rat::from_integer(lcm_int.clone()))];
            for (b, k) in &bases {
                common.push(pow(b.clone(), int(*k)));
            }
            let mut nums = Vec::new();
            for ((n, _), (ip, m)) in parts.iter().zip(&dens) {
                let mut f = vec![n.clone(), Sx::Num(Rat::from_integer(&lcm_int / ip))];
                for (b, k) in &bases {
                    let have = m.get(b).copied().unwrap_or(0);
                    if *k - have > 0 {
                        f.push(pow(b.clone(), int(*k - have)));
                    }
                }
                nums.push(product(f));
            }
            (sum(nums), product(common))
        }
        Sx::Fun(n, a) if n == "exp" && a.len() == 1 && leading_neg(&a[0]) => (one(), func("exp", vec![neg(a[0].clone())])),
        _ => (e.clone(), one()),
    }
}

fn leading_neg(e: &Sx) -> bool {
    match e {
        Sx::Num(r) => r.is_negative(),
        Sx::Float(F(x)) => *x < 0.0,
        Sx::Mul(v) => v.first().is_some_and(leading_neg),
        _ => false,
    }
}

/// Combines everything into a single fraction (recursing into function arguments).
pub fn together(e: &Sx) -> Sx {
    let inner = match e {
        Sx::Fun(..) | Sx::Mat(_) | Sx::Pow(..) | Sx::Add(_) | Sx::Mul(_) => map_children(e, &mut |c| together(c)),
        _ => e.clone(),
    };
    let (n, d) = num_den(&inner);
    if d.is_one() {
        inner
    } else {
        mul(n, pow(d, int(-1)))
    }
}

fn mv_gcd_cancel(n: &Sx, d: &Sx) -> Option<(Sx, Sx)> {
    // n, d expanded polynomials over atoms
    let mut at = Atoms::default();
    let (na, da) = (abstract_atoms(n, &mut at), abstract_atoms(d, &mut at));
    let mut vars = std::collections::BTreeSet::new();
    na.symbols(&mut vars);
    da.symbols(&mut vars);
    if vars.is_empty() {
        return None;
    }
    if vars.len() == 1 {
        let x = Sx::Sym(vars.iter().next().unwrap().clone());
        let (pn, pd) = (to_p(&na, &x)?, to_p(&da, &x)?);
        let g = poly::pgcd(&pn, &pd);
        if poly::deg(&g) < 1 {
            return None;
        }
        let (qn, _) = poly::pdivrem(&pn, &g);
        let (qd, _) = poly::pdivrem(&pd, &g);
        return Some((at.restore(&from_p(&qn, &x)), at.restore(&from_p(&qd, &x))));
    }
    // several variables: cancel common irreducible factors
    let (cn, fnn) = factor_poly(&na)?;
    let (cd, fd) = factor_poly(&da)?;
    let mut fd = fd;
    let mut num_f: Vec<(Sx, usize)> = Vec::new();
    let mut changed = false;
    for (f, m) in fnn {
        if let Some(slot) = fd.iter_mut().find(|(g, _)| *g == f) {
            let k = m.min(slot.1);
            slot.1 -= k;
            if k > 0 {
                changed = true;
            }
            if m > k {
                num_f.push((f, m - k));
            }
        } else {
            num_f.push((f, m));
        }
    }
    if !changed {
        return None;
    }
    let n2 = product(std::iter::once(Sx::Num(cn)).chain(num_f.into_iter().map(|(f, m)| pow(f, int(m as i64)))).collect());
    let d2 = product(std::iter::once(Sx::Num(cd)).chain(fd.into_iter().filter(|(_, m)| *m > 0).map(|(f, m)| pow(f, int(m as i64)))).collect());
    Some((at.restore(&expand(&n2)), at.restore(&expand(&d2))))
}

/// Rational function in canonical `numerator / denominator` form with common factors cancelled.
pub fn cancel(e: &Sx) -> Sx {
    let t = together(e);
    let (n, d) = num_den(&t);
    if d.is_one() {
        return t;
    }
    let (n, d) = (expand(&n), expand(&d));
    let (n, d) = mv_gcd_cancel(&n, &d).unwrap_or((n, d));
    // move numeric content of the denominator into the numerator
    let (c, rest) = split_coeff(&d);
    let (n2, d2) = match c {
        Co::R(r) if !r.is_one() => (mul(n, Sx::Num(r.recip())), rest),
        _ => (n, d),
    };
    mul(n2, pow(d2, int(-1)))
}

// ================================================================ multivariate factoring

type Mono = Vec<u32>;
type MP = BTreeMap<Mono, Rat>;

fn mp_from_sx(e: &Sx, vars: &[String]) -> Option<MP> {
    let mut out: MP = BTreeMap::new();
    for t in terms_of(expand(e)) {
        let (c, rest) = split_coeff(&t);
        let c = match c {
            Co::R(r) => r,
            _ => return None,
        };
        let mut mono = vec![0u32; vars.len()];
        let fs: Vec<Sx> = match rest {
            Sx::Mul(v) => v,
            one if one.is_one() => vec![],
            other => vec![other],
        };
        for f in fs {
            let (name, k) = match &f {
                Sx::Sym(n) => (n.clone(), 1u32),
                Sx::Pow(b, x) => match (&**b, x.as_int()) {
                    (Sx::Sym(n), Some(k)) if k > 0 => (n.clone(), k as u32),
                    _ => return None,
                },
                _ => return None,
            };
            let i = vars.iter().position(|v| *v == name)?;
            mono[i] += k;
        }
        let e = out.entry(mono).or_insert_with(Rat::zero);
        *e += c;
    }
    out.retain(|_, c| !c.is_zero());
    Some(out)
}

fn mp_to_sx(m: &MP, vars: &[String]) -> Sx {
    sum(m
        .iter()
        .map(|(mono, c)| {
            let mut f = vec![Sx::Num(c.clone())];
            for (i, k) in mono.iter().enumerate() {
                if *k > 0 {
                    f.push(pow(Sx::Sym(vars[i].clone()), int(*k as i64)));
                }
            }
            product(f)
        })
        .collect())
}

fn mp_mul(a: &MP, b: &MP) -> MP {
    let mut out: MP = BTreeMap::new();
    for (ma, ca) in a {
        for (mb, cb) in b {
            let m: Mono = ma.iter().zip(mb).map(|(x, y)| x + y).collect();
            *out.entry(m).or_insert_with(Rat::zero) += ca * cb;
        }
    }
    out.retain(|_, c| !c.is_zero());
    out
}

/// Exact division by lexicographic reduction; `None` when `b` does not divide `a`.
fn mp_divexact(a: &MP, b: &MP) -> Option<MP> {
    let mut r = a.clone();
    let mut q: MP = BTreeMap::new();
    let (lb, cb) = b.iter().next_back()?;
    let mut guard = 0;
    while let Some((la, ca)) = r.iter().next_back().map(|(k, v)| (k.clone(), v.clone())) {
        guard += 1;
        if guard > 100_000 {
            return None;
        }
        if la.iter().zip(lb).any(|(x, y)| x < y) {
            return None;
        }
        let m: Mono = la.iter().zip(lb).map(|(x, y)| x - y).collect();
        let c = &ca / cb;
        let term: MP = BTreeMap::from([(m.clone(), c.clone())]);
        let sub = mp_mul(&term, b);
        for (k, v) in sub {
            let e = r.entry(k.clone()).or_insert_with(Rat::zero);
            *e -= v;
            if e.is_zero() {
                r.remove(&k);
            }
        }
        *q.entry(m).or_insert_with(Rat::zero) += c;
    }
    Some(q)
}

fn kron_encode(m: &MP, d: u32) -> P {
    let mut p: P = Vec::new();
    for (mono, c) in m {
        let mut e: usize = 0;
        let mut mulr: usize = 1;
        for k in mono {
            e += *k as usize * mulr;
            mulr *= d as usize;
        }
        if p.len() <= e {
            p.resize(e + 1, Rat::zero());
        }
        p[e] = c.clone();
    }
    poly::trim(p)
}

fn kron_decode(p: &P, d: u32, nv: usize) -> MP {
    let mut m: MP = BTreeMap::new();
    for (e, c) in p.iter().enumerate() {
        if c.is_zero() {
            continue;
        }
        let mut rest = e;
        let mut mono = Vec::with_capacity(nv);
        for _ in 0..nv {
            mono.push((rest % d as usize) as u32);
            rest /= d as usize;
        }
        m.insert(mono, c.clone());
    }
    m
}

/// Factors a polynomial in the symbols (and placeholders) it contains:
/// `(content, [(irreducible factor, multiplicity)])`.
pub fn factor_poly(e: &Sx) -> Option<(Rat, Vec<(Sx, usize)>)> {
    let e = expand(e);
    let mut vars_set = std::collections::BTreeSet::new();
    e.symbols(&mut vars_set);
    let vars: Vec<String> = vars_set.into_iter().collect();
    if vars.is_empty() {
        return match e {
            Sx::Num(r) => Some((r, Vec::new())),
            _ => None,
        };
    }
    if vars.len() == 1 {
        let x = Sx::Sym(vars[0].clone());
        let p = to_p(&e, &x)?;
        let (c, fs) = poly::factor_q(&p);
        return Some((c, fs.into_iter().map(|(f, m)| (from_p(&f, &x), m)).collect()));
    }
    let mp = mp_from_sx(&e, &vars)?;
    // content and monomial gcd
    let mut cont = BigInt::zero();
    let mut den = BigInt::one();
    for c in mp.values() {
        den = den.lcm(c.denom());
    }
    for c in mp.values() {
        cont = cont.gcd(&(c.numer() * (&den / c.denom())));
    }
    let lead_neg = mp.iter().next_back().is_some_and(|(_, c)| c.is_negative());
    let mut content = Rat::new(cont, den);
    if lead_neg {
        content = -content;
    }
    let mut min_exp = vec![u32::MAX; vars.len()];
    for mono in mp.keys() {
        for (i, k) in mono.iter().enumerate() {
            min_exp[i] = min_exp[i].min(*k);
        }
    }
    let mut factors: Vec<(Sx, usize)> = Vec::new();
    for (i, k) in min_exp.iter().enumerate() {
        if *k > 0 {
            factors.push((Sx::Sym(vars[i].clone()), *k as usize));
        }
    }
    let reduced: MP = mp
        .iter()
        .map(|(mono, c)| (mono.iter().zip(&min_exp).map(|(a, b)| a - b).collect::<Mono>(), c / &content))
        .collect();
    let total: u32 = reduced.keys().map(|m| m.iter().sum::<u32>()).max().unwrap_or(0);
    if total == 0 {
        return Some((content, factors));
    }
    // single variable left?
    let used: Vec<usize> = (0..vars.len()).filter(|i| reduced.keys().any(|m| m[*i] > 0)).collect();
    if used.len() == 1 {
        let x = Sx::Sym(vars[used[0]].clone());
        let p = to_p(&mp_to_sx(&reduced, &vars), &x)?;
        let (c, fs) = poly::factor_q(&p);
        let content = content * c;
        factors.extend(fs.into_iter().map(|(f, m)| (from_p(&f, &x), m)));
        return Some((content, factors));
    }
    let d = reduced.keys().flat_map(|m| m.iter().copied()).max().unwrap_or(1) + 1;
    let p = kron_encode(&reduced, d);
    let (_c, fs) = poly::factor_q(&p);
    if fs.len() <= 1 && fs.first().map_or(true, |(_, m)| *m == 1) {
        factors.push((mp_to_sx(&reduced, &vars), 1));
        return Some((content, factors));
    }
    // all irreducible univariate factors, repeated by multiplicity
    let mut pieces: Vec<P> = Vec::new();
    for (f, m) in &fs {
        for _ in 0..*m {
            pieces.push(f.clone());
        }
    }
    if pieces.len() > 12 {
        factors.push((mp_to_sx(&reduced, &vars), 1));
        return Some((content, factors));
    }
    let mut rest = reduced.clone();
    let mut remaining: Vec<usize> = (0..pieces.len()).collect();
    let mut found: Vec<MP> = Vec::new();
    let mut size = 1;
    while size <= remaining.len() && !remaining.is_empty() {
        let mut progressed = false;
        let m = remaining.len();
        for comb in subsets(m, size) {
            let mut prod_p: P = vec![Rat::one()];
            for &i in &comb {
                prod_p = poly::pmul(&prod_p, &pieces[remaining[i]]);
            }
            let (_, prim) = poly::primitive(&prod_p);
            let cand = kron_decode(&prim, d, vars.len());
            if cand.is_empty() {
                continue;
            }
            // the decoded polynomial must encode back to the same univariate image
            if poly::trim(kron_encode(&cand, d)) != prim {
                continue;
            }
            if let Some(q) = mp_divexact(&rest, &cand) {
                // make sure the quotient factors consistently (it is rest/cand)
                rest = q;
                found.push(cand);
                let take: Vec<usize> = comb.clone();
                let mut keep = Vec::new();
                for (j, r) in remaining.iter().enumerate() {
                    if !take.contains(&j) {
                        keep.push(*r);
                    }
                }
                remaining = keep;
                progressed = true;
                break;
            }
        }
        if !progressed {
            size += 1;
        }
    }
    // the unit remainder
    let rest_sx = mp_to_sx(&rest, &vars);
    let mut content = content;
    match rest_sx {
        Sx::Num(r) => content *= r,
        other => {
            if !other.is_one() {
                found.push(rest);
            }
        }
    }
    // merge equal factors
    let mut merged: Vec<(Sx, usize)> = Vec::new();
    for f in found {
        let (c, fp) = {
            // normalise sign so that the leading coefficient is positive
            let lead = f.iter().next_back().map(|(_, c)| c.clone()).unwrap_or_else(Rat::one);
            if lead.is_negative() {
                (-Rat::one(), f.iter().map(|(k, v)| (k.clone(), -v.clone())).collect::<MP>())
            } else {
                (Rat::one(), f)
            }
        };
        content *= c;
        let sx = mp_to_sx(&fp, &vars);
        if let Some(slot) = merged.iter_mut().find(|(g, _)| *g == sx) {
            slot.1 += 1;
        } else {
            merged.push((sx, 1));
        }
    }
    factors.extend(merged);
    Some((content, factors))
}

fn subsets(m: usize, k: usize) -> Vec<Vec<usize>> {
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

// ================================================================ factor

/// Factors an expression (rational function) over the rationals.
pub fn factor(e: &Sx) -> Sx {
    if let Sx::Mat(m) = e {
        return Sx::Mat(m.iter().map(|r| r.iter().map(factor).collect()).collect());
    }
    let floats = contains_float(e);
    let e0 = if floats { rationalize(e) } else { e.clone() };
    let mut at = Atoms::default();
    let ab = abstract_atoms(&e0, &mut at);
    let t = together(&ab);
    let (n, d) = num_den(&t);
    let r = match (factor_poly(&n), factor_poly(&d)) {
        (Some((cn, fnn)), Some((cd, fd))) => {
            let mut fd = fd;
            let mut num_f: Vec<(Sx, i64)> = Vec::new();
            for (f, m) in fnn {
                if let Some(slot) = fd.iter_mut().find(|(g, _)| *g == f) {
                    let k = m.min(slot.1);
                    slot.1 -= k;
                    if m > k {
                        num_f.push((f, (m - k) as i64));
                    }
                } else {
                    num_f.push((f, m as i64));
                }
            }
            let mut parts: Vec<Sx> = vec![Sx::Num(cn / cd)];
            for (f, m) in num_f {
                parts.push(pow(f, int(m)));
            }
            for (f, m) in fd {
                if m > 0 {
                    parts.push(pow(f, int(-(m as i64))));
                }
            }
            product(parts)
        }
        _ => ab.clone(),
    };
    let r = at.restore(&r);
    if floats {
        floatify(&r, 17)
    } else {
        r
    }
}

// ================================================================ partial fractions

fn solve_linear_rat(mut a: Vec<Vec<Rat>>, mut b: Vec<Rat>) -> Option<Vec<Rat>> {
    let n = a.len();
    for col in 0..n {
        let piv = (col..n).find(|&r| !a[r][col].is_zero())?;
        a.swap(col, piv);
        b.swap(col, piv);
        let p = a[col][col].clone();
        for r in 0..n {
            if r != col && !a[r][col].is_zero() {
                let f = &a[r][col] / &p;
                for c in col..n {
                    let v = &a[col][c] * &f;
                    a[r][c] -= v;
                }
                let v = &b[col] * &f;
                b[r] -= v;
            }
        }
    }
    Some((0..n).map(|i| &b[i] / &a[i][i]).collect())
}

pub fn parfrac(e: &Sx, x: &Sx) -> Option<Sx> {
    let t = together(e);
    let (n, d) = num_den(&t);
    let (pn, pd) = (to_p(&n, x)?, to_p(&d, x)?);
    if poly::deg(&pd) < 1 {
        return Some(e.clone());
    }
    let (q, r) = poly::pdivrem(&pn, &pd);
    let (c, factors) = poly::factor_q(&pd);
    let r = poly::pscale(&r, &c.recip());
    let total = poly::deg(&pd) as usize;
    // unknowns: for each factor f^m, k=1..m: coefficients of A_k (deg < deg f)
    let mut cols: Vec<(usize, usize, usize)> = Vec::new(); // (factor index, k, coefficient index)
    for (i, (f, m)) in factors.iter().enumerate() {
        for k in 1..=*m {
            for j in 0..(poly::deg(f) as usize) {
                cols.push((i, k, j));
            }
        }
    }
    if cols.len() != total {
        return None;
    }
    let prod_all: P = factors.iter().fold(vec![Rat::one()], |acc, (f, m)| poly::pmul(&acc, &poly::ppow(f, *m)));
    let mut a = vec![vec![Rat::zero(); total]; total];
    for (ci, &(i, k, j)) in cols.iter().enumerate() {
        // x^j * prod_all / f_i^k
        let (qq, _) = poly::pdivrem(&prod_all, &poly::ppow(&factors[i].0, k));
        let mut shifted = vec![Rat::zero(); j];
        shifted.extend(qq);
        for (row, v) in shifted.iter().enumerate() {
            if row < total {
                a[row][ci] = v.clone();
            }
        }
    }
    let mut b = vec![Rat::zero(); total];
    for (i, v) in r.iter().enumerate() {
        if i < total {
            b[i] = v.clone();
        }
    }
    let sol = solve_linear_rat(a, b)?;
    let mut terms = vec![from_p(&q, x)];
    for (i, (f, m)) in factors.iter().enumerate() {
        for k in 1..=*m {
            let coef: P = poly::trim(
                cols.iter()
                    .enumerate()
                    .filter(|(_, (fi, kk, _))| *fi == i && *kk == k)
                    .map(|(ci, _)| sol[ci].clone())
                    .collect(),
            );
            if coef.is_empty() {
                continue;
            }
            terms.push(mul(from_p(&coef, x), pow(from_p(f, x), int(-(k as i64)))));
        }
    }
    Some(sum(terms))
}

// ================================================================ simplify

fn split_factor_power(f: &Sx) -> (Sx, Sx) {
    match f {
        Sx::Pow(b, e) => ((**b).clone(), (**e).clone()),
        other => (other.clone(), one()),
    }
}

/// Trigonometric identities: sin²+cos²=1, sin/cos=tan.
pub fn trig_reduce(e: &Sx) -> Sx {
    let e = map_children(e, &mut |c| trig_reduce(c));
    match &e {
        Sx::Add(ts) => trig_sum(ts).unwrap_or(e),
        Sx::Mul(fs) => trig_mul(fs).unwrap_or(e),
        _ => e,
    }
}

fn trig_mul(fs: &[Sx]) -> Option<Sx> {
    // sin(a)^p * cos(a)^q with opposite signs -> tan(a)^k ...
    let mut sin_of: Vec<(Sx, i64, usize)> = Vec::new();
    for (i, f) in fs.iter().enumerate() {
        let (b, e) = split_factor_power(f);
        if let (Sx::Fun(n, a), Some(k)) = (&b, e.as_int()) {
            if n == "sin" && a.len() == 1 {
                sin_of.push((a[0].clone(), k, i));
            }
        }
    }
    for (arg, p, i) in sin_of {
        for (j, f) in fs.iter().enumerate() {
            let (b, e) = split_factor_power(f);
            if let (Sx::Fun(n, a), Some(q)) = (&b, e.as_int()) {
                if n == "cos" && a.len() == 1 && a[0] == arg && p * q < 0 {
                    let k = p.abs().min(q.abs());
                    let sgn = if p > 0 { 1 } else { -1 };
                    let mut parts: Vec<Sx> = fs.iter().enumerate().filter(|(t, _)| *t != i && *t != j).map(|(_, f)| f.clone()).collect();
                    parts.push(pow(func("tan", vec![arg.clone()]), int(sgn * k)));
                    let rp = p - sgn * k;
                    let rq = q + sgn * k;
                    if rp != 0 {
                        parts.push(pow(func("sin", vec![arg.clone()]), int(rp)));
                    }
                    if rq != 0 {
                        parts.push(pow(func("cos", vec![arg.clone()]), int(rq)));
                    }
                    return Some(product(parts));
                }
            }
        }
    }
    None
}

fn trig_sum(ts: &[Sx]) -> Option<Sx> {
    // find term pairs c*R*sin(a)^2 + c*R*cos(a)^2
    #[derive(Clone)]
    struct Info {
        idx: usize,
        arg: Sx,
        rest: Sx,
        coef: Co,
        is_sin: bool,
    }
    let mut infos: Vec<Info> = Vec::new();
    for (idx, t) in ts.iter().enumerate() {
        let (coef, rest) = split_coeff(t);
        let fs: Vec<Sx> = match &rest {
            Sx::Mul(v) => v.clone(),
            other => vec![other.clone()],
        };
        for (k, f) in fs.iter().enumerate() {
            if let Sx::Pow(b, e) = f {
                if e.as_int() == Some(2) {
                    if let Sx::Fun(n, a) = &**b {
                        if (n == "sin" || n == "cos") && a.len() == 1 {
                            let others: Vec<Sx> = fs.iter().enumerate().filter(|(j, _)| *j != k).map(|(_, x)| x.clone()).collect();
                            infos.push(Info { idx, arg: a[0].clone(), rest: product(others), coef: coef.clone(), is_sin: n == "sin" });
                        }
                    }
                }
            }
        }
    }
    for a in &infos {
        if !a.is_sin {
            continue;
        }
        for b in &infos {
            if b.is_sin || b.arg != a.arg || b.rest != a.rest || a.idx == b.idx {
                continue;
            }
            let (Co::R(ca), Co::R(cb)) = (&a.coef, &b.coef) else { continue };
            // ca sin² + cb cos² = ca + (cb - ca) cos²
            let mut out: Vec<Sx> = ts.iter().enumerate().filter(|(i, _)| *i != a.idx && *i != b.idx).map(|(_, t)| t.clone()).collect();
            out.push(mul(Sx::Num(ca.clone()), a.rest.clone()));
            let diff = cb - ca;
            if !diff.is_zero() {
                out.push(product(vec![Sx::Num(diff), a.rest.clone(), pow(func("cos", vec![a.arg.clone()]), int(2))]));
            }
            return Some(sum(out));
        }
    }
    // k + c sin² with k = -c  ->  k cos² ; k + c cos² with k = -c -> k sin²
    let konst = ts.iter().find_map(|t| if let Sx::Num(r) = t { Some(r.clone()) } else { None });
    if let Some(k) = konst {
        for a in &infos {
            if let Co::R(c) = &a.coef {
                if a.rest.is_one() && *c == -k.clone() {
                    let other = if a.is_sin { "cos" } else { "sin" };
                    let mut out: Vec<Sx> = ts.iter().enumerate().filter(|(i, t)| *i != a.idx && !matches!(t, Sx::Num(_))).map(|(_, t)| t.clone()).collect();
                    out.push(mul(Sx::Num(k), pow(func(other, vec![a.arg.clone()]), int(2))));
                    return Some(sum(out));
                }
            }
        }
    }
    None
}

/// Simplification: picks the smallest of several equivalent forms.
pub fn simplify(e: &Sx) -> Sx {
    if let Sx::Mat(m) = e {
        return Sx::Mat(m.iter().map(|r| r.iter().map(simplify).collect()).collect());
    }
    let base = trig_reduce(e);
    let mut cands: Vec<Sx> = vec![e.clone(), base.clone()];
    let floats = contains_float(&base);
    if !floats {
        cands.push(cancel(&base));
        let ex = expand(&base);
        cands.push(trig_reduce(&ex));
        cands.push(ex);
        let c = cancel(&expand(&base));
        cands.push(c.clone());
        if !c.is_const() {
            cands.push(trig_reduce(&cancel(&trig_reduce(&expand(&base)))));
        }
    } else {
        cands.push(expand(&base));
    }
    let mut best = cands[0].clone();
    for c in cands.into_iter().skip(1) {
        if c.size() < best.size() {
            best = c;
        }
    }
    best
}

/// Exact `x` such that `x*x == r`? (helper for the roots module)
pub fn is_perfect_square(r: &Rat) -> Option<Rat> {
    if r.is_negative() {
        return None;
    }
    let n = r.numer().sqrt();
    let d = r.denom().sqrt();
    if &n * &n == *r.numer() && &d * &d == *r.denom() {
        Some(Rat::new(n, d))
    } else {
        None
    }
}

/// Presentation clean-up: function arguments are replaced by their smallest equivalent form.
pub fn tidy(e: &Sx) -> Sx {
    match e {
        Sx::Fun(n, args) if !matches!(n.as_str(), "integrate" | "diff" | "sum" | "prod" | "lim") => {
            let new: Vec<Sx> = args
                .iter()
                .map(|a| {
                    let a = tidy(a);
                    let mut best = a.clone();
                    if matches!(a, Sx::Add(_) | Sx::Mul(_)) && !a.is_const() || matches!(a, Sx::Mul(_)) {
                        for c in [factor(&a), cancel(&a)] {
                            if c.size() < best.size() {
                                best = c;
                            }
                        }
                    }
                    best
                })
                .collect();
            func(n, new)
        }
        other => map_children(other, &mut |c| tidy(c)),
    }
}
