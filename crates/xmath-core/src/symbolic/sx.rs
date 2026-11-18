//! Canonical symbolic expressions (`Sx`): exact rationals, symbols, sums, products,
//! powers and function applications.  The smart constructors (`sum`, `product`,
//! `pow`, `func`) keep every value in canonical form: flattened, like terms combined,
//! numbers folded, trigonometric/exponential identities for exact arguments applied.

use num_bigint::BigInt;
use num_complex::Complex64 as C64;
use num_integer::Integer;
use num_rational::BigRational;
use num_traits::{One, Signed, ToPrimitive, Zero};
use std::cell::RefCell;
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

pub type Rat = BigRational;

#[derive(Clone, Copy, Debug)]
pub struct F(pub f64);

impl PartialEq for F {
    fn eq(&self, o: &F) -> bool {
        self.0.to_bits() == o.0.to_bits()
    }
}
impl Eq for F {}
impl PartialOrd for F {
    fn partial_cmp(&self, o: &F) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}
impl Ord for F {
    fn cmp(&self, o: &F) -> Ordering {
        self.0.total_cmp(&o.0)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Sx {
    Num(Rat),
    Float(F),
    Sym(String),
    /// The imaginary unit.
    Im,
    Add(Vec<Sx>),
    Mul(Vec<Sx>),
    Pow(Box<Sx>, Box<Sx>),
    Fun(String, Vec<Sx>),
    Mat(Vec<Vec<Sx>>),
}

impl Sx {
    fn rank(&self) -> u8 {
        match self {
            Sx::Num(_) => 0,
            Sx::Float(_) => 1,
            Sx::Im => 2,
            Sx::Sym(_) => 3,
            Sx::Fun(..) => 4,
            Sx::Pow(..) => 5,
            Sx::Mul(_) => 6,
            Sx::Add(_) => 7,
            Sx::Mat(_) => 8,
        }
    }
}

impl PartialOrd for Sx {
    fn partial_cmp(&self, o: &Sx) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}

impl Ord for Sx {
    fn cmp(&self, o: &Sx) -> Ordering {
        match (self, o) {
            (Sx::Num(a), Sx::Num(b)) => a.cmp(b),
            (Sx::Float(a), Sx::Float(b)) => a.cmp(b),
            (Sx::Sym(a), Sx::Sym(b)) => a.cmp(b),
            (Sx::Im, Sx::Im) => Ordering::Equal,
            (Sx::Add(a), Sx::Add(b)) | (Sx::Mul(a), Sx::Mul(b)) => a.cmp(b),
            (Sx::Pow(a, b), Sx::Pow(c, d)) => a.cmp(c).then_with(|| b.cmp(d)),
            (Sx::Fun(a, x), Sx::Fun(b, y)) => a.cmp(b).then_with(|| x.cmp(y)),
            (Sx::Mat(a), Sx::Mat(b)) => a.cmp(b),
            _ => self.rank().cmp(&o.rank()),
        }
    }
}

// ------------------------------------------------------------------ constants

pub fn rat_i(n: i64) -> Rat {
    Rat::from_integer(BigInt::from(n))
}
pub fn rat_ij(n: i64, d: i64) -> Rat {
    Rat::new(BigInt::from(n), BigInt::from(d))
}
pub fn int(n: i64) -> Sx {
    Sx::Num(rat_i(n))
}
pub fn rat(n: i64, d: i64) -> Sx {
    Sx::Num(rat_ij(n, d))
}
pub fn num(r: Rat) -> Sx {
    Sx::Num(r)
}
pub fn sym(s: &str) -> Sx {
    Sx::Sym(s.to_string())
}
pub fn float(x: f64) -> Sx {
    Sx::Float(F(x))
}
pub fn pi() -> Sx {
    sym("π")
}
pub fn zero() -> Sx {
    int(0)
}
pub fn one() -> Sx {
    int(1)
}
pub fn half() -> Sx {
    rat(1, 2)
}
pub fn inf() -> Sx {
    sym("∞")
}

impl Sx {
    pub fn is_zero(&self) -> bool {
        matches!(self, Sx::Num(r) if r.is_zero()) || matches!(self, Sx::Float(F(x)) if *x == 0.0)
    }
    pub fn is_one(&self) -> bool {
        matches!(self, Sx::Num(r) if r.is_one())
    }
    pub fn is_minus_one(&self) -> bool {
        matches!(self, Sx::Num(r) if *r == rat_i(-1))
    }
    pub fn as_rat(&self) -> Option<&Rat> {
        match self {
            Sx::Num(r) => Some(r),
            _ => None,
        }
    }
    pub fn as_int(&self) -> Option<i64> {
        match self {
            Sx::Num(r) if r.is_integer() => r.to_integer().to_i64(),
            _ => None,
        }
    }
    pub fn is_num(&self) -> bool {
        matches!(self, Sx::Num(_) | Sx::Float(_))
    }
    pub fn is_float(&self) -> bool {
        matches!(self, Sx::Float(_))
    }
    pub fn is_sym(&self, s: &str) -> bool {
        matches!(self, Sx::Sym(n) if n == s)
    }
    pub fn is_mat(&self) -> bool {
        matches!(self, Sx::Mat(_))
    }
    /// A rational (or float) number as `f64`.
    pub fn num_f64(&self) -> Option<f64> {
        match self {
            Sx::Num(r) => r.to_f64(),
            Sx::Float(F(x)) => Some(*x),
            _ => None,
        }
    }
    pub fn contains(&self, pat: &Sx) -> bool {
        if self == pat {
            return true;
        }
        match self {
            Sx::Add(v) | Sx::Mul(v) | Sx::Fun(_, v) => v.iter().any(|x| x.contains(pat)),
            Sx::Pow(a, b) => a.contains(pat) || b.contains(pat),
            Sx::Mat(m) => m.iter().flatten().any(|x| x.contains(pat)),
            _ => false,
        }
    }
    pub fn has_sym(&self, name: &str) -> bool {
        match self {
            Sx::Sym(n) => n == name,
            Sx::Add(v) | Sx::Mul(v) | Sx::Fun(_, v) => v.iter().any(|x| x.has_sym(name)),
            Sx::Pow(a, b) => a.has_sym(name) || b.has_sym(name),
            Sx::Mat(m) => m.iter().flatten().any(|x| x.has_sym(name)),
            _ => false,
        }
    }
    /// Does the expression depend on `x` (a symbol)?
    pub fn depends(&self, x: &Sx) -> bool {
        self.contains(x)
    }
    pub fn symbols(&self, out: &mut BTreeSet<String>) {
        match self {
            Sx::Sym(n) => {
                if !is_constant_name(n) {
                    out.insert(n.clone());
                }
            }
            Sx::Add(v) | Sx::Mul(v) | Sx::Fun(_, v) => v.iter().for_each(|x| x.symbols(out)),
            Sx::Pow(a, b) => {
                a.symbols(out);
                b.symbols(out)
            }
            Sx::Mat(m) => m.iter().flatten().for_each(|x| x.symbols(out)),
            _ => {}
        }
    }
    pub fn free_symbols(&self) -> BTreeSet<String> {
        let mut s = BTreeSet::new();
        self.symbols(&mut s);
        s
    }
    /// Number of nodes (a size measure for choosing the simplest form).
    pub fn size(&self) -> usize {
        match self {
            Sx::Add(v) | Sx::Mul(v) | Sx::Fun(_, v) => 1 + v.iter().map(Sx::size).sum::<usize>(),
            Sx::Pow(a, b) => 1 + a.size() + b.size(),
            Sx::Mat(m) => 1 + m.iter().flatten().map(Sx::size).sum::<usize>(),
            Sx::Num(r) => {
                if r.is_integer() {
                    1
                } else {
                    3
                }
            }
            _ => 1,
        }
    }
    pub fn is_const(&self) -> bool {
        self.free_symbols().is_empty()
    }
}

pub fn is_constant_name(n: &str) -> bool {
    matches!(n, "π" | "∞")
}

// -------------------------------------------------------------- assumptions

#[derive(Clone, Default, Debug)]
pub struct Assumptions {
    pub positive: BTreeSet<String>,
    pub negative: BTreeSet<String>,
    pub real: BTreeSet<String>,
}

thread_local! {
    static ASSUME: RefCell<Assumptions> = RefCell::new(Assumptions::default());
}

pub fn set_assumptions(a: Assumptions) {
    ASSUME.with(|c| *c.borrow_mut() = a);
}
pub fn assumptions() -> Assumptions {
    ASSUME.with(|c| c.borrow().clone())
}

/// Is `e` known to be positive (numbers, π, assumed symbols and products/sums/powers of them)?
pub fn is_positive(e: &Sx) -> bool {
    match e {
        Sx::Num(r) => r.is_positive(),
        Sx::Float(F(x)) => *x > 0.0,
        Sx::Sym(n) => n == "π" || ASSUME.with(|c| c.borrow().positive.contains(n)),
        Sx::Mul(v) | Sx::Add(v) => v.iter().all(is_positive),
        Sx::Pow(b, e) => is_positive(b) && matches!(**e, Sx::Num(_) | Sx::Float(_)) || (is_positive(b) && is_real(e)),
        Sx::Fun(n, a) if n == "exp" => a.iter().all(is_real),
        Sx::Fun(n, _) if n == "abs" => false,
        _ => false,
    }
}
pub fn is_negative(e: &Sx) -> bool {
    match e {
        Sx::Num(r) => r.is_negative(),
        Sx::Float(F(x)) => *x < 0.0,
        Sx::Sym(n) => ASSUME.with(|c| c.borrow().negative.contains(n)),
        _ => false,
    }
}
/// Is `e` known to be real?
pub fn is_real(e: &Sx) -> bool {
    match e {
        Sx::Num(_) | Sx::Float(_) => true,
        Sx::Sym(n) => n == "π" || ASSUME.with(|c| {
            let c = c.borrow();
            c.real.contains(n) || c.positive.contains(n) || c.negative.contains(n)
        }),
        Sx::Add(v) | Sx::Mul(v) => v.iter().all(is_real),
        Sx::Pow(b, e) => is_real(b) && matches!(**e, Sx::Num(ref r) if r.is_integer()),
        _ => false,
    }
}

// -------------------------------------------------------------- coefficients

#[derive(Clone, Debug)]
pub enum Co {
    R(Rat),
    F(f64),
}

impl Co {
    pub fn one() -> Co {
        Co::R(Rat::one())
    }
    pub fn zero() -> Co {
        Co::R(Rat::zero())
    }
    pub fn f64(&self) -> f64 {
        match self {
            Co::R(r) => r.to_f64().unwrap_or(f64::NAN),
            Co::F(x) => *x,
        }
    }
    pub fn add(&self, o: &Co) -> Co {
        match (self, o) {
            (Co::R(a), Co::R(b)) => Co::R(a + b),
            _ => Co::F(self.f64() + o.f64()),
        }
    }
    pub fn mul(&self, o: &Co) -> Co {
        match (self, o) {
            (Co::R(a), Co::R(b)) => Co::R(a * b),
            _ => Co::F(self.f64() * o.f64()),
        }
    }
    pub fn is_zero(&self) -> bool {
        match self {
            Co::R(r) => r.is_zero(),
            Co::F(x) => *x == 0.0,
        }
    }
    pub fn is_one(&self) -> bool {
        matches!(self, Co::R(r) if r.is_one())
    }
    pub fn to_sx(&self) -> Sx {
        match self {
            Co::R(r) => Sx::Num(r.clone()),
            Co::F(x) => Sx::Float(F(*x)),
        }
    }
    pub fn of(e: &Sx) -> Option<Co> {
        match e {
            Sx::Num(r) => Some(Co::R(r.clone())),
            Sx::Float(F(x)) => Some(Co::F(*x)),
            _ => None,
        }
    }
}

/// Splits a term into its numeric coefficient and the remaining (coefficient free) part.
pub fn split_coeff(t: &Sx) -> (Co, Sx) {
    match t {
        Sx::Num(r) => (Co::R(r.clone()), one()),
        Sx::Float(F(x)) => (Co::F(*x), one()),
        Sx::Mul(v) => {
            if let Some(c) = v.first().and_then(Co::of) {
                let rest: Vec<Sx> = v[1..].to_vec();
                let rest = match rest.len() {
                    0 => one(),
                    1 => rest.into_iter().next().unwrap(),
                    _ => Sx::Mul(rest),
                };
                (c, rest)
            } else {
                (Co::one(), t.clone())
            }
        }
        _ => (Co::one(), t.clone()),
    }
}

fn attach(c: Co, rest: Sx) -> Sx {
    if c.is_zero() {
        return zero();
    }
    if rest.is_one() {
        return c.to_sx();
    }
    if c.is_one() {
        return rest;
    }
    match rest {
        Sx::Mul(mut v) => {
            v.insert(0, c.to_sx());
            Sx::Mul(v)
        }
        other => Sx::Mul(vec![c.to_sx(), other]),
    }
}

// ------------------------------------------------------------------- sum

pub fn add(a: Sx, b: Sx) -> Sx {
    sum(vec![a, b])
}
pub fn neg(a: Sx) -> Sx {
    product(vec![int(-1), a])
}
pub fn sub(a: Sx, b: Sx) -> Sx {
    sum(vec![a, neg(b)])
}
pub fn mul(a: Sx, b: Sx) -> Sx {
    product(vec![a, b])
}
pub fn div(a: Sx, b: Sx) -> Sx {
    product(vec![a, pow(b, int(-1))])
}
pub fn powi(a: Sx, n: i64) -> Sx {
    pow(a, int(n))
}
pub fn sqrt(a: Sx) -> Sx {
    pow(a, half())
}

pub fn sum(v: Vec<Sx>) -> Sx {
    let mut work: Vec<Sx> = Vec::with_capacity(v.len());
    let mut has_mat = false;
    for t in v {
        match t {
            Sx::Add(items) => work.extend(items),
            Sx::Mat(_) => {
                has_mat = true;
                work.push(t)
            }
            other => work.push(other),
        }
    }
    if has_mat {
        return super::matrix::mat_sum(work);
    }
    let mut konst = Co::zero();
    let mut groups: BTreeMap<Sx, Co> = BTreeMap::new();
    for t in work {
        let (c, rest) = split_coeff(&t);
        if rest.is_one() {
            konst = konst.add(&c);
        } else {
            let e = groups.entry(rest).or_insert_with(Co::zero);
            *e = e.add(&c);
        }
    }
    let mut out: Vec<Sx> = Vec::new();
    if !konst.is_zero() {
        out.push(konst.to_sx());
    }
    for (rest, c) in groups {
        if c.is_zero() {
            continue;
        }
        out.push(attach(c, rest));
    }
    out.sort();
    match out.len() {
        0 => zero(),
        1 => out.pop().unwrap(),
        _ => Sx::Add(out),
    }
}

// ---------------------------------------------------- exact complex numbers

/// `a + b i` with rational `a`, `b` (numbers, the imaginary unit and sums of them).
pub fn cplx_of(e: &Sx) -> Option<(Rat, Rat)> {
    match e {
        Sx::Num(r) => Some((r.clone(), Rat::zero())),
        Sx::Im => Some((Rat::zero(), Rat::one())),
        Sx::Mul(v) if v.len() == 2 && v[1] == Sx::Im => match &v[0] {
            Sx::Num(c) => Some((Rat::zero(), c.clone())),
            _ => None,
        },
        Sx::Add(v) => {
            let mut z = (Rat::zero(), Rat::zero());
            for t in v {
                let (a, b) = cplx_of(t)?;
                z.0 += a;
                z.1 += b;
            }
            Some(z)
        }
        _ => None,
    }
}

pub fn from_cplx(a: Rat, b: Rat) -> Sx {
    let mut terms = Vec::new();
    if !a.is_zero() {
        terms.push(Sx::Num(a));
    }
    if !b.is_zero() {
        terms.push(if b.is_one() { Sx::Im } else { Sx::Mul(vec![Sx::Num(b), Sx::Im]) });
    }
    terms.sort();
    match terms.len() {
        0 => zero(),
        1 => terms.pop().unwrap(),
        _ => Sx::Add(terms),
    }
}

fn cmul(x: &(Rat, Rat), y: &(Rat, Rat)) -> (Rat, Rat) {
    (&x.0 * &y.0 - &x.1 * &y.1, &x.0 * &y.1 + &x.1 * &y.0)
}

/// `(a + b i)^n` for an integer `n`.
fn cpow(z: &(Rat, Rat), n: i64) -> Option<(Rat, Rat)> {
    if n.abs() > 400 {
        return None;
    }
    let mut base = z.clone();
    if n < 0 {
        let norm = &z.0 * &z.0 + &z.1 * &z.1;
        if norm.is_zero() {
            return None;
        }
        base = (&z.0 / &norm, -&z.1 / &norm);
    }
    let mut r = (Rat::one(), Rat::zero());
    for _ in 0..n.abs() {
        r = cmul(&r, &base);
    }
    Some(r)
}

/// Principal square root of `a + b i` (b != 0).
fn csqrt(a: &Rat, b: &Rat) -> Sx {
    let m = pow(Sx::Num(a * a + b * b), half());
    let re = pow(mul(half(), add(m.clone(), Sx::Num(a.clone()))), half());
    let im = pow(mul(half(), sub(m, Sx::Num(a.clone()))), half());
    add(re, mul(if b.is_negative() { int(-1) } else { int(1) }, mul(im, Sx::Im)))
}

// --------------------------------------------------------------- product

pub fn product(v: Vec<Sx>) -> Sx {
    mul_list(v, 0)
}

fn mul_list(v: Vec<Sx>, depth: u32) -> Sx {
    let mut work: Vec<Sx> = Vec::with_capacity(v.len());
    let mut has_mat = false;
    for t in v {
        match t {
            Sx::Mul(items) => work.extend(items),
            Sx::Mat(_) => {
                has_mat = true;
                work.push(t)
            }
            other => work.push(other),
        }
    }
    if has_mat {
        return super::matrix::mat_product(work);
    }
    // exact complex numbers multiply out: (1+2i)(3-4i) = 11+2i
    if work.iter().any(|f| matches!(f, Sx::Add(_)) && cplx_of(f).is_some_and(|z| !z.1.is_zero())) {
        let mut z = (Rat::one(), Rat::zero());
        let mut others = Vec::new();
        for f in work {
            match cplx_of(&f) {
                Some(c) => z = cmul(&z, &c),
                None => others.push(f),
            }
        }
        work = others;
        work.push(from_cplx(z.0, z.1));
        if work.len() == 1 {
            return work.pop().unwrap();
        }
    }
    let mut coeff = Co::one();
    let mut groups: BTreeMap<Sx, Vec<Sx>> = BTreeMap::new();
    for f in work {
        match f {
            Sx::Num(r) => coeff = coeff.mul(&Co::R(r)),
            Sx::Float(F(x)) => coeff = coeff.mul(&Co::F(x)),
            Sx::Pow(b, e) => groups.entry(*b).or_default().push(*e),
            other => groups.entry(other).or_default().push(one()),
        }
    }
    if coeff.is_zero() {
        return zero();
    }
    let mut out: Vec<Sx> = Vec::new();
    let mut again = false;
    let mut exps_f: Vec<(Sx, Sx)> = Vec::new();
    for (b, es) in groups {
        let e = sum(es);
        if e.is_zero() {
            continue;
        }
        exps_f.push((b, e));
    }
    // numeric radicals with equal exponents combine: sqrt(2)*sqrt(3) = sqrt(6)
    let mut merged: Vec<(Sx, Sx)> = Vec::new();
    for (b, e) in exps_f {
        if let (Sx::Num(bn), Sx::Num(en)) = (&b, &e) {
            if bn.is_integer() && bn.is_positive() && !en.is_integer() {
                if let Some(slot) = merged.iter_mut().find(|(b2, e2)| matches!((b2, e2), (Sx::Num(x), Sx::Num(y)) if y == en && x.is_integer() && x.is_positive())) {
                    slot.0 = Sx::Num(bn * slot.0.as_rat().unwrap());
                    continue;
                }
            }
        }
        merged.push((b, e));
    }
    for (b, e) in merged {
        let p = pow(b, e);
        match p {
            Sx::Num(r) => {
                coeff = coeff.mul(&Co::R(r));
                again = again || !out.is_empty();
            }
            Sx::Float(F(x)) => coeff = coeff.mul(&Co::F(x)),
            Sx::Mul(items) => {
                again = true;
                out.extend(items)
            }
            other => out.push(other),
        }
    }
    // exp(a)*exp(b) = exp(a+b)
    let exps: Vec<usize> = out.iter().enumerate().filter(|(_, f)| matches!(f, Sx::Fun(n, a) if n == "exp" && a.len() == 1)).map(|(i, _)| i).collect();
    if exps.len() > 1 {
        let mut args = Vec::new();
        for &i in exps.iter().rev() {
            if let Sx::Fun(_, a) = out.remove(i) {
                args.push(a.into_iter().next().unwrap());
            }
        }
        out.push(func("exp", vec![sum(args)]));
        again = true;
    }
    if again && depth < 6 {
        let mut all = out;
        all.push(coeff.to_sx());
        return mul_list(all, depth + 1);
    }
    if coeff.is_zero() {
        return zero();
    }
    out.sort();
    if out.is_empty() {
        return coeff.to_sx();
    }
    if coeff.is_one() && out.len() == 1 {
        return out.pop().unwrap();
    }
    if !coeff.is_one() {
        out.insert(0, coeff.to_sx());
    }
    Sx::Mul(out)
}

// ----------------------------------------------------------------- power

pub fn factor_int(n: &BigInt) -> Vec<(BigInt, u32)> {
    let mut n = n.clone();
    let mut out: Vec<(BigInt, u32)> = Vec::new();
    if n <= BigInt::one() {
        return out;
    }
    let push = |p: BigInt, out: &mut Vec<(BigInt, u32)>| {
        if let Some(l) = out.last_mut() {
            if l.0 == p {
                l.1 += 1;
                return;
            }
        }
        out.push((p, 1));
    };
    let mut p = 2u64;
    while p < 100_000 {
        let bp = BigInt::from(p);
        if &bp * &bp > n {
            break;
        }
        while (&n % &bp).is_zero() {
            n /= &bp;
            push(bp.clone(), &mut out);
        }
        p += if p == 2 { 1 } else { 2 };
    }
    if n > BigInt::one() {
        // a large cofactor: detect perfect powers
        for k in (2..=8u32).rev() {
            let r = n.nth_root(k);
            if r.pow(k) == n && r > BigInt::one() {
                for _ in 0..k {
                    push(r.clone(), &mut out);
                }
                n = BigInt::one();
                break;
            }
        }
        if n > BigInt::one() {
            push(n, &mut out);
        }
    }
    out
}

/// `n^(a/q)` for a positive integer `n` and `0 <= a < q`: returns `(coefficient, radicand)`
/// with `n^(a/q) = coefficient * radicand^(1/q)`.
fn int_root_part(n: &BigInt, a: u32, q: u32) -> (BigInt, BigInt) {
    let fac = factor_int(n);
    let mut out = BigInt::one();
    let mut rad = BigInt::one();
    for (p, m) in fac {
        let t = m * a;
        out *= p.pow(t / q);
        rad *= p.pow(t % q);
    }
    (out, rad)
}

fn rat_pow_rat(b: &Rat, e: &Rat) -> Option<Sx> {
    // integer exponents
    if e.is_integer() {
        let k = e.to_integer().to_i64()?;
        if k.abs() > 100_000 {
            return None;
        }
        if b.is_zero() {
            return None;
        }
        let r = if k >= 0 { num_traits::pow::pow(b.clone(), k as usize) } else { num_traits::pow::pow(b.recip(), (-k) as usize) };
        return Some(Sx::Num(r));
    }
    let q = e.denom().to_u32()?;
    if q > 64 || b.is_zero() {
        return None;
    }
    if b.is_negative() {
        if q == 2 {
            // (-a)^(p/2) = i^p * a^(p/2)
            let a = -b.clone();
            let p = e.numer().to_i64()?;
            let pos = rat_pow_rat(&a, e)?;
            let ip = pow(Sx::Im, int(p));
            return Some(mul(ip, pos));
        }
        return None;
    }
    let m = e.floor();
    let f = e - &m; // 0 < f < 1
    let a = f.numer().to_u32()?;
    let base_m = match rat_pow_rat(b, &m)? {
        Sx::Num(r) => r,
        _ => return None,
    };
    let (nn, dd) = (b.numer().clone(), b.denom().clone());
    // b^f = nn^f * dd^-f = nn^f * dd^(q-a)/q / dd  (rationalised denominator)
    let (c1, r1) = int_root_part(&nn, a, q);
    let (c2, r2) = int_root_part(&dd, q - a, q);
    let coeff = base_m * Rat::new(c1 * c2, dd);
    let rad = &r1 * &r2;
    if rad.is_one() {
        return Some(Sx::Num(coeff));
    }
    let root = Sx::Pow(Box::new(Sx::Num(Rat::from_integer(rad))), Box::new(Sx::Num(Rat::new(BigInt::one(), BigInt::from(q)))));
    Some(if coeff.is_one() { root } else { Sx::Mul(vec![Sx::Num(coeff), root]) })
}

pub fn pow(b: Sx, e: Sx) -> Sx {
    // numbers
    if let Sx::Num(en) = &e {
        if en.is_zero() {
            return one();
        }
        if en.is_one() {
            return b;
        }
    }
    if b.is_one() {
        return one();
    }
    if let Sx::Mat(_) = b {
        return super::matrix::mat_pow(b, e);
    }
    match (&b, &e) {
        (Sx::Num(bn), Sx::Num(en)) => {
            if let Some(r) = rat_pow_rat(bn, en) {
                return r;
            }
        }
        (Sx::Float(_) | Sx::Num(_), Sx::Float(_)) | (Sx::Float(_), Sx::Num(_)) => {
            let (x, y) = (b.num_f64().unwrap(), e.num_f64().unwrap());
            if x >= 0.0 || y.fract() == 0.0 {
                return float(x.powf(y));
            }
            let z = C64::new(x, 0.0).powf(y);
            return sum(vec![float(z.re), mul(float(z.im), Sx::Im)]);
        }
        _ => {}
    }
    if b.is_zero() {
        if let Some(r) = e.as_rat() {
            if r.is_positive() {
                return zero();
            }
            flag_error("деление на ноль");
        }
    }
    if let Some((a, bi)) = cplx_of(&b) {
        if !bi.is_zero() {
            if let Some(n) = e.as_int() {
                if let Some(z) = cpow(&(a.clone(), bi.clone()), n) {
                    return from_cplx(z.0, z.1);
                }
            } else if e == half() {
                return csqrt(&a, &bi);
            }
        }
    }
    match b {
        Sx::Im => {
            if let Some(k) = e.as_int() {
                return match k.rem_euclid(4) {
                    0 => one(),
                    1 => Sx::Im,
                    2 => int(-1),
                    _ => neg(Sx::Im),
                };
            }
            Sx::Pow(Box::new(Sx::Im), Box::new(e))
        }
        Sx::Pow(c, d) => {
            let int_e = matches!(&e, Sx::Num(r) if r.is_integer());
            let pos_base = matches!(&*c, Sx::Num(r) if r.is_positive()) || is_positive(&c);
            if int_e || pos_base && is_real(&d) && is_real(&e) {
                pow(*c, mul(*d, e))
            } else {
                // (x^d)^e with real x>=0 assumed only for d=1/2... keep nested
                Sx::Pow(Box::new(Sx::Pow(c, d)), Box::new(e))
            }
        }
        Sx::Mul(fs) => {
            let int_e = matches!(&e, Sx::Num(r) if r.is_integer());
            if int_e {
                product(fs.into_iter().map(|f| pow(f, e.clone())).collect())
            } else {
                // pull out positive numeric / positive-symbol factors
                let (pos, rest): (Vec<Sx>, Vec<Sx>) = fs.into_iter().partition(|f| matches!(f, Sx::Num(r) if r.is_positive()) || is_positive(f));
                if pos.is_empty() {
                    Sx::Pow(Box::new(Sx::Mul(rest)), Box::new(e))
                } else {
                    let mut parts: Vec<Sx> = pos.into_iter().map(|f| pow(f, e.clone())).collect();
                    if !rest.is_empty() {
                        parts.push(Sx::Pow(Box::new(product(rest)), Box::new(e)));
                    }
                    product(parts)
                }
            }
        }
        Sx::Fun(ref n, ref a) if n == "exp" && a.len() == 1 && is_real(&e) => func("exp", vec![mul(a[0].clone(), e)]),
        Sx::Fun(ref n, ref a) if n == "abs" && a.len() == 1 && matches!(&e, Sx::Num(r) if r.is_integer() && r.to_integer().is_even()) => {
            pow(a[0].clone(), e)
        }
        Sx::Sym(ref n) if n == "π" => Sx::Pow(Box::new(b), Box::new(e)),
        other => {
            // (x^2)^(1/2) with positive x handled above; a positive symbol to a numeric power stays
            Sx::Pow(Box::new(other), Box::new(e))
        }
    }
}

// ------------------------------------------------------------- functions

/// `c` such that `arg = c*π`, if so.
pub fn pi_multiple(arg: &Sx) -> Option<Rat> {
    match arg {
        Sx::Num(r) if r.is_zero() => Some(Rat::zero()),
        Sx::Sym(n) if n == "π" => Some(Rat::one()),
        Sx::Mul(v) if v.len() == 2 => match (&v[0], &v[1]) {
            (Sx::Num(c), Sx::Sym(n)) if n == "π" => Some(c.clone()),
            _ => None,
        },
        _ => None,
    }
}

fn sqrt_i(n: i64) -> Sx {
    sqrt(int(n))
}

/// Exact `sin(c*π)`.
fn sin_pi(c: &Rat) -> Option<Sx> {
    let two = rat_i(2);
    let mut r = c % &two;
    if r.is_negative() {
        r += &two;
    }
    // r in [0, 2)
    let sign = if r >= rat_i(1) {
        r -= rat_i(1);
        -1
    } else {
        1
    };
    // sin(r*π) with r in [0,1)
    let v = match (r.numer().to_i64()?, r.denom().to_i64()?) {
        (0, _) => zero(),
        (1, 6) | (5, 6) => half(),
        (1, 4) | (3, 4) => mul(half(), sqrt_i(2)),
        (1, 3) | (2, 3) => mul(half(), sqrt_i(3)),
        (1, 2) => one(),
        (1, 12) | (11, 12) => mul(rat(1, 4), sub(sqrt_i(6), sqrt_i(2))),
        (5, 12) | (7, 12) => mul(rat(1, 4), add(sqrt_i(6), sqrt_i(2))),
        _ => return None,
    };
    Some(if sign < 0 { neg(v) } else { v })
}

fn cos_pi(c: &Rat) -> Option<Sx> {
    sin_pi(&(c + rat_ij(1, 2)))
}

fn atan_special(x: &Sx) -> Option<Sx> {
    let sq3 = sqrt_i(3);
    if x.is_zero() {
        return Some(zero());
    }
    if x.is_one() {
        return Some(mul(rat(1, 4), pi()));
    }
    if *x == sq3 {
        return Some(mul(rat(1, 3), pi()));
    }
    if *x == mul(rat(1, 3), sq3) {
        return Some(mul(rat(1, 6), pi()));
    }
    None
}

fn asin_special(x: &Sx) -> Option<Sx> {
    if x.is_zero() {
        return Some(zero());
    }
    if x.is_one() {
        return Some(mul(half(), pi()));
    }
    if *x == half() {
        return Some(mul(rat(1, 6), pi()));
    }
    if *x == mul(half(), sqrt_i(2)) {
        return Some(mul(rat(1, 4), pi()));
    }
    if *x == mul(half(), sqrt_i(3)) {
        return Some(mul(rat(1, 3), pi()));
    }
    None
}

/// Is the leading numeric coefficient negative (for parity rules)?
fn leading_negative(e: &Sx) -> bool {
    match e {
        Sx::Num(r) => r.is_negative(),
        Sx::Float(F(x)) => *x < 0.0,
        Sx::Mul(v) => matches!(v.first(), Some(Sx::Num(r)) if r.is_negative()) || matches!(v.first(), Some(Sx::Float(F(x))) if *x < 0.0),
        Sx::Add(v) => v.first().map_or(false, |t| leading_negative(t)) && v.iter().all(leading_negative_or_zero),
        _ => false,
    }
}
fn leading_negative_or_zero(e: &Sx) -> bool {
    leading_negative(e)
}

pub fn eval_float_fun(name: &str, x: f64) -> Option<f64> {
    Some(match name {
        "sin" => x.sin(),
        "cos" => x.cos(),
        "tan" => x.tan(),
        "asin" => x.asin(),
        "acos" => x.acos(),
        "atan" => x.atan(),
        "sinh" => x.sinh(),
        "cosh" => x.cosh(),
        "tanh" => x.tanh(),
        "asinh" => x.asinh(),
        "acosh" => x.acosh(),
        "atanh" => x.atanh(),
        "exp" => x.exp(),
        "ln" if x > 0.0 => x.ln(),
        "abs" => x.abs(),
        "floor" => x.floor(),
        "ceil" => x.ceil(),
        "round" => x.round(),
        "trunc" => x.trunc(),
        "sign" => {
            if x > 0.0 {
                1.0
            } else if x < 0.0 {
                -1.0
            } else {
                0.0
            }
        }
        _ => return None,
    })
}

pub fn func(name: &str, args: Vec<Sx>) -> Sx {
    if args.iter().any(Sx::is_mat) && args.len() == 1 && !matches!(name, "det" | "transpose" | "tr" | "inverse") {
        // elementwise
        if let Sx::Mat(m) = &args[0] {
            return Sx::Mat(m.iter().map(|r| r.iter().map(|x| func(name, vec![x.clone()])).collect()).collect());
        }
    }
    if args.len() == 1 {
        let a = &args[0];
        if let Sx::Float(F(x)) = a {
            if let Some(v) = eval_float_fun(name, *x) {
                return float(v);
            }
        }
        match name {
            "exp" => {
                if a.is_zero() {
                    return one();
                }
                if let Sx::Fun(n, inner) = a {
                    if n == "ln" && inner.len() == 1 {
                        return inner[0].clone();
                    }
                }
                // exp(c*ln(x)) = x^c ; exp(sum with ln terms)
                if let Sx::Mul(v) = a {
                    if v.len() == 2 {
                        if let Sx::Fun(n, inner) = &v[1] {
                            if n == "ln" && inner.len() == 1 && matches!(v[0], Sx::Num(_)) {
                                return Sx::Pow(Box::new(inner[0].clone()), Box::new(v[0].clone()));
                            }
                        }
                    }
                }
                if let Sx::Add(v) = a {
                    // pull ln terms out: exp(a + ln(x)) = x*exp(a)
                    let (lns, rest): (Vec<Sx>, Vec<Sx>) = v.iter().cloned().partition(|t| matches!(t, Sx::Fun(n, _) if n == "ln"));
                    if !lns.is_empty() {
                        let mut parts: Vec<Sx> = lns.into_iter().map(|t| if let Sx::Fun(_, a) = t { a[0].clone() } else { unreachable!() }).collect();
                        parts.push(func("exp", vec![sum(rest)]));
                        return product(parts);
                    }
                }
                // exp(i*c*π)
                if let Sx::Mul(v) = a {
                    if v.len() == 3 && v[1] == Sx::Im && v[2].is_sym("π") {
                        if let Sx::Num(c) = &v[0] {
                            if let (Some(co), Some(si)) = (cos_pi(c), sin_pi(c)) {
                                return add(co, mul(Sx::Im, si));
                            }
                        }
                    }
                }
                if let Sx::Mul(v) = a {
                    if v.len() == 2 && v[0] == Sx::Im && v[1].is_sym("π") {
                        return int(-1);
                    }
                }
                if *a == mul(Sx::Im, pi()) {
                    return int(-1);
                }
            }
            "ln" => {
                if a.is_one() {
                    return zero();
                }
                if let Sx::Fun(n, inner) = a {
                    if n == "exp" && inner.len() == 1 && is_real(&inner[0]) {
                        return inner[0].clone();
                    }
                }
                if let Sx::Num(r) = a {
                    if r.is_negative() {
                        return add(func("ln", vec![Sx::Num(-r.clone())]), mul(Sx::Im, pi()));
                    }
                }
                if let Sx::Float(F(x)) = a {
                    if *x < 0.0 {
                        return add(float(x.abs().ln()), mul(float(std::f64::consts::PI), Sx::Im));
                    }
                }
                if let Sx::Pow(b, e) = a {
                    if is_positive(b) && is_real(e) {
                        return mul((**e).clone(), func("ln", vec![(**b).clone()]));
                    }
                }
                if let Sx::Mul(v) = a {
                    if v.iter().all(is_positive) {
                        return sum(v.iter().map(|f| func("ln", vec![f.clone()])).collect());
                    }
                }
            }
            "sin" | "cos" | "tan" => {
                if let Some(c) = pi_multiple(a) {
                    match name {
                        "sin" => {
                            if let Some(v) = sin_pi(&c) {
                                return v;
                            }
                        }
                        "cos" => {
                            if let Some(v) = cos_pi(&c) {
                                return v;
                            }
                        }
                        _ => {
                            if let (Some(s), Some(co)) = (sin_pi(&c), cos_pi(&c)) {
                                if !co.is_zero() {
                                    return div(s, co);
                                }
                            }
                        }
                    }
                }
                if leading_negative(a) {
                    let pos = neg(a.clone());
                    let r = func(name, vec![pos]);
                    return if name == "cos" { r } else { neg(r) };
                }
            }
            "sinh" | "tanh" | "asin" | "atan" | "asinh" | "atanh" => {
                if a.is_zero() {
                    return zero();
                }
                if leading_negative(a) {
                    return neg(func(name, vec![neg(a.clone())]));
                }
                if name == "asin" {
                    if let Some(v) = asin_special(a) {
                        return v;
                    }
                }
                if name == "atan" {
                    if let Some(v) = atan_special(a) {
                        return v;
                    }
                }
            }
            "cosh" => {
                if a.is_zero() {
                    return one();
                }
                if leading_negative(a) {
                    return func(name, vec![neg(a.clone())]);
                }
            }
            "acos" => {
                if a.is_one() {
                    return zero();
                }
                if a.is_zero() {
                    return mul(half(), pi());
                }
                if let Some(v) = asin_special(&neg(a.clone())).filter(|_| leading_negative(a)) {
                    // acos(-x) = π - acos(x) = π/2 + asin(x)
                    return add(mul(half(), pi()), neg(v));
                }
                if let Some(v) = asin_special(a) {
                    return sub(mul(half(), pi()), v);
                }
            }
            "abs" => {
                match a {
                    Sx::Num(r) => return Sx::Num(r.abs()),
                    Sx::Float(F(x)) => return float(x.abs()),
                    Sx::Im => return one(),
                    Sx::Fun(n, _) if n == "abs" => return a.clone(),
                    _ => {}
                }
                if is_positive(a) {
                    return a.clone();
                }
                if is_negative(a) {
                    return neg(a.clone());
                }
                if let Sx::Mul(v) = a {
                    if let Some(Sx::Num(c)) = v.first() {
                        let rest = product(v[1..].to_vec());
                        return mul(Sx::Num(c.abs()), func("abs", vec![rest]));
                    }
                    if v.iter().any(|f| is_positive(f) || matches!(f, Sx::Im)) {
                        let (known, rest): (Vec<Sx>, Vec<Sx>) = v.iter().cloned().partition(|f| is_positive(f) || matches!(f, Sx::Im));
                        let k: Vec<Sx> = known.into_iter().map(|f| if f == Sx::Im { one() } else { f }).collect();
                        return mul(product(k), func("abs", vec![product(rest)]));
                    }
                }
                if let Sx::Pow(b, e) = a {
                    if is_real(e) && matches!(**e, Sx::Num(_)) {
                        return pow(func("abs", vec![(**b).clone()]), (**e).clone());
                    }
                }
                if leading_negative(a) {
                    return func("abs", vec![neg(a.clone())]);
                }
            }
            "sign" => {
                if let Sx::Num(r) = a {
                    return int(if r.is_positive() { 1 } else if r.is_negative() { -1 } else { 0 });
                }
            }
            "factorial" => {
                if let Some(n) = a.as_int() {
                    if (0..=2000).contains(&n) {
                        let mut r = BigInt::one();
                        for k in 2..=n {
                            r *= BigInt::from(k);
                        }
                        return Sx::Num(Rat::from_integer(r));
                    }
                }
            }
            "floor" | "ceil" | "round" | "trunc" => {
                if let Sx::Num(r) = a {
                    let v = match name {
                        "floor" => r.floor(),
                        "ceil" => r.ceil(),
                        "round" => r.round(),
                        _ => r.trunc(),
                    };
                    return Sx::Num(v);
                }
            }
            _ => {}
        }
    }
    Sx::Fun(name.to_string(), args)
}

// ------------------------------------------------------- generic traversal

/// Rebuilds a node with `f` applied to every direct child (through the smart constructors).
pub fn map_children(e: &Sx, f: &mut dyn FnMut(&Sx) -> Sx) -> Sx {
    match e {
        Sx::Add(v) => sum(v.iter().map(|x| f(x)).collect()),
        Sx::Mul(v) => product(v.iter().map(|x| f(x)).collect()),
        Sx::Pow(a, b) => {
            let (a, b) = (f(a), f(b));
            pow(a, b)
        }
        Sx::Fun(n, v) => func(n, v.iter().map(|x| f(x)).collect()),
        Sx::Mat(m) => Sx::Mat(m.iter().map(|r| r.iter().map(|x| f(x)).collect()).collect()),
        other => other.clone(),
    }
}

/// Replaces every occurrence of `pat` (any subexpression) by `val`.
pub fn subs(e: &Sx, pat: &Sx, val: &Sx) -> Sx {
    if e == pat {
        return val.clone();
    }
    map_children(e, &mut |c| subs(c, pat, val))
}

pub fn subs_many(e: &Sx, pairs: &[(Sx, Sx)]) -> Sx {
    for (p, v) in pairs {
        if e == p {
            return v.clone();
        }
    }
    map_children(e, &mut |c| subs_many(c, pairs))
}

// ------------------------------------------------------ numeric evaluation

pub fn eval_c(e: &Sx) -> Option<C64> {
    Some(match e {
        Sx::Num(r) => C64::new(r.to_f64()?, 0.0),
        Sx::Float(F(x)) => C64::new(*x, 0.0),
        Sx::Im => C64::new(0.0, 1.0),
        Sx::Sym(n) => match n.as_str() {
            "π" => C64::new(std::f64::consts::PI, 0.0),
            "∞" => C64::new(f64::INFINITY, 0.0),
            _ => return None,
        },
        Sx::Add(v) => {
            let mut s = C64::new(0.0, 0.0);
            for t in v {
                s += eval_c(t)?;
            }
            s
        }
        Sx::Mul(v) => {
            let mut s = C64::new(1.0, 0.0);
            for t in v {
                s *= eval_c(t)?;
            }
            s
        }
        Sx::Pow(b, x) => {
            let (b, x) = (eval_c(b)?, eval_c(x)?);
            if x.im == 0.0 && x.re.fract() == 0.0 && x.re.abs() < 1e6 {
                b.powi(x.re as i32)
            } else if x.im == 0.0 {
                b.powf(x.re)
            } else {
                b.powc(x)
            }
        }
        Sx::Fun(n, a) => {
            if a.len() == 1 {
                let x = eval_c(&a[0])?;
                match n.as_str() {
                    "sin" => x.sin(),
                    "cos" => x.cos(),
                    "tan" => x.tan(),
                    "asin" => x.asin(),
                    "acos" => x.acos(),
                    "atan" => x.atan(),
                    "sinh" => x.sinh(),
                    "cosh" => x.cosh(),
                    "tanh" => x.tanh(),
                    "asinh" => x.asinh(),
                    "acosh" => x.acosh(),
                    "atanh" => x.atanh(),
                    "exp" => x.exp(),
                    "ln" => x.ln(),
                    "abs" => C64::new(x.norm(), 0.0),
                    "sign" => C64::new(if x.re > 0.0 { 1.0 } else if x.re < 0.0 { -1.0 } else { 0.0 }, 0.0),
                    "floor" => C64::new(x.re.floor(), 0.0),
                    "ceil" => C64::new(x.re.ceil(), 0.0),
                    "round" => C64::new(x.re.round(), 0.0),
                    "trunc" => C64::new(x.re.trunc(), 0.0),
                    "factorial" => {
                        let mut r = 1.0;
                        for k in 2..=(x.re as u64).min(170) {
                            r *= k as f64;
                        }
                        C64::new(r, 0.0)
                    }
                    "erf" => C64::new(libm::erf(x.re), 0.0),
                    "gamma" => C64::new(libm::tgamma(x.re), 0.0),
                    _ => return None,
                }
            } else {
                return None;
            }
        }
        Sx::Mat(_) => return None,
    })
}

pub fn eval_f64(e: &Sx) -> Option<f64> {
    let z = eval_c(e)?;
    if z.im.abs() <= 1e-12 * z.re.abs().max(1.0) {
        Some(z.re)
    } else {
        None
    }
}

/// Replaces every exact number and constant by a float (exponents are kept exact when
/// integers), keeping symbols.
pub fn floatify(e: &Sx, digits: usize) -> Sx {
    let round = |x: f64| -> f64 {
        if x == 0.0 || !x.is_finite() {
            return x;
        }
        let d = digits.clamp(1, 17) as i32;
        let mag = x.abs().log10().floor() as i32;
        let scale = 10f64.powi(d - 1 - mag);
        if scale.is_finite() && scale > 0.0 {
            let r = (x * scale).round() / scale;
            // reparse through text to drop binary noise
            format!("{:.*e}", (d - 1) as usize, r).parse::<f64>().unwrap_or(r)
        } else {
            x
        }
    };
    fn go(e: &Sx, round: &dyn Fn(f64) -> f64, in_exp: bool) -> Sx {
        match e {
            Sx::Num(r) => {
                if in_exp && r.is_integer() {
                    e.clone()
                } else {
                    float(round(r.to_f64().unwrap_or(f64::NAN)))
                }
            }
            Sx::Float(F(x)) => float(round(*x)),
            Sx::Sym(n) if n == "π" && !in_exp => float(round(std::f64::consts::PI)),
            Sx::Pow(b, x) => {
                let (b2, x2) = (go(b, round, false), go(x, round, true));
                pow(b2, x2)
            }
            Sx::Fun(n, a) => {
                let args: Vec<Sx> = a.iter().map(|x| go(x, round, false)).collect();
                if n == "exp" && args.len() == 1 && matches!(a[0], Sx::Num(ref r) if r.is_one()) {
                    return float(round(std::f64::consts::E));
                }
                func(n, args)
            }
            Sx::Add(v) => sum(v.iter().map(|x| go(x, round, false)).collect()),
            Sx::Mul(v) => product(v.iter().map(|x| go(x, round, false)).collect()),
            Sx::Mat(m) => Sx::Mat(m.iter().map(|r| r.iter().map(|x| go(x, round, false)).collect()).collect()),
            other => other.clone(),
        }
    }
    let r = go(e, &round, false);
    fn round_leaves(e: &Sx, round: &dyn Fn(f64) -> f64) -> Sx {
        match e {
            Sx::Float(F(x)) => float(round(*x)),
            other => map_children(other, &mut |c| round_leaves(c, round)),
        }
    }
    round_leaves(&r, &round)
}

/// Converts every float to the nearest "nice" rational (the shortest decimal text).
pub fn rationalize(e: &Sx) -> Sx {
    match e {
        Sx::Float(F(x)) => match float_to_rat(*x) {
            Some(r) => Sx::Num(r),
            None => e.clone(),
        },
        other => map_children(other, &mut |c| rationalize(c)),
    }
}

pub fn float_to_rat(x: f64) -> Option<Rat> {
    if !x.is_finite() {
        return None;
    }
    let s = format!("{x:e}");
    let (mant, exp) = s.split_once('e')?;
    let exp: i32 = exp.parse().ok()?;
    let neg = mant.starts_with('-');
    let mant = mant.trim_start_matches('-');
    let (ip, fp) = mant.split_once('.').unwrap_or((mant, ""));
    let digits: BigInt = format!("{ip}{fp}").parse().ok()?;
    let e10 = exp - fp.len() as i32;
    let ten = BigInt::from(10);
    let r = if e10 >= 0 { Rat::from_integer(digits * ten.pow(e10 as u32)) } else { Rat::new(digits, ten.pow((-e10) as u32)) };
    Some(if neg { -r } else { r })
}

pub fn contains_float(e: &Sx) -> bool {
    match e {
        Sx::Float(_) => true,
        Sx::Add(v) | Sx::Mul(v) | Sx::Fun(_, v) => v.iter().any(contains_float),
        Sx::Pow(a, b) => contains_float(a) || contains_float(b),
        Sx::Mat(m) => m.iter().flatten().any(contains_float),
        _ => false,
    }
}

/// Greatest common divisor / lcm on rationals' components.
pub fn bigint_gcd(a: &BigInt, b: &BigInt) -> BigInt {
    a.gcd(b)
}

// ------------------------------------------------------------ error flag

thread_local! {
    static ERR: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// Records a problem found inside a smart constructor (which cannot fail).
pub fn flag_error(msg: &str) {
    ERR.with(|c| {
        let mut c = c.borrow_mut();
        if c.is_none() {
            *c = Some(msg.to_string());
        }
    });
}

pub fn take_error() -> Option<String> {
    ERR.with(|c| c.borrow_mut().take())
}

/// Runs `f`, returning the result and the problem flagged while it ran (the flag is cleared).
pub fn capture<T>(f: impl FnOnce() -> T) -> (T, Option<String>) {
    let saved = take_error();
    let r = f();
    let e = take_error();
    if let Some(s) = saved {
        flag_error(&s);
    }
    (r, e)
}
