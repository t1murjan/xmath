//! Differentiation, integration, limits, series and sums.

use super::algebra::*;
use super::poly;
use super::sx::*;
use num_bigint::BigInt;
use num_integer::Integer;
use num_traits::{One, Signed, ToPrimitive, Zero};

// ================================================================== diff

pub fn diff(e: &Sx, x: &Sx) -> Sx {
    if !e.depends(x) {
        return zero();
    }
    match e {
        Sx::Sym(_) => {
            if e == x {
                one()
            } else {
                zero()
            }
        }
        Sx::Add(v) => sum(v.iter().map(|t| diff(t, x)).collect()),
        Sx::Mul(v) => {
            let mut terms = Vec::new();
            for i in 0..v.len() {
                let d = diff(&v[i], x);
                if d.is_zero() {
                    continue;
                }
                let mut f: Vec<Sx> = v.iter().enumerate().filter(|(j, _)| *j != i).map(|(_, t)| t.clone()).collect();
                f.push(d);
                terms.push(product(f));
            }
            sum(terms)
        }
        Sx::Pow(b, p) => {
            if !p.depends(x) {
                let db = diff(b, x);
                product(vec![(**p).clone(), pow((**b).clone(), sub((**p).clone(), one())), db])
            } else if !b.depends(x) {
                product(vec![e.clone(), func("ln", vec![(**b).clone()]), diff(p, x)])
            } else {
                let t = add(mul(diff(p, x), func("ln", vec![(**b).clone()])), div(mul((**p).clone(), diff(b, x)), (**b).clone()));
                mul(e.clone(), t)
            }
        }
        Sx::Fun(n, a) => {
            if a.len() != 1 {
                return Sx::Fun("diff".into(), vec![e.clone(), x.clone()]);
            }
            let u = &a[0];
            let du = diff(u, x);
            let outer = match n.as_str() {
                "exp" => e.clone(),
                "ln" => pow(u.clone(), int(-1)),
                "sin" => func("cos", vec![u.clone()]),
                "cos" => neg(func("sin", vec![u.clone()])),
                "tan" => pow(func("cos", vec![u.clone()]), int(-2)),
                "asin" => pow(sub(one(), pow(u.clone(), int(2))), rat(-1, 2)),
                "acos" => neg(pow(sub(one(), pow(u.clone(), int(2))), rat(-1, 2))),
                "atan" => pow(add(one(), pow(u.clone(), int(2))), int(-1)),
                "sinh" => func("cosh", vec![u.clone()]),
                "cosh" => func("sinh", vec![u.clone()]),
                "tanh" => pow(func("cosh", vec![u.clone()]), int(-2)),
                "asinh" => pow(add(pow(u.clone(), int(2)), one()), rat(-1, 2)),
                "acosh" => pow(sub(pow(u.clone(), int(2)), one()), rat(-1, 2)),
                "atanh" => pow(sub(one(), pow(u.clone(), int(2))), int(-1)),
                "abs" => div(u.clone(), e.clone()),
                "erf" => mul(div(int(2), sqrt(pi())), func("exp", vec![neg(pow(u.clone(), int(2)))])),
                "sign" | "floor" | "ceil" | "round" | "trunc" => zero(),
                _ => return Sx::Fun("diff".into(), vec![e.clone(), x.clone()]),
            };
            mul(outer, du)
        }
        Sx::Mat(m) => Sx::Mat(m.iter().map(|r| r.iter().map(|t| diff(t, x)).collect()).collect()),
        _ => zero(),
    }
}

pub fn diff_n(e: &Sx, x: &Sx, n: usize) -> Sx {
    let mut r = e.clone();
    for _ in 0..n {
        r = simplify_light(&diff(&r, x));
    }
    r
}

/// Cheap clean-up between differentiation steps.
fn simplify_light(e: &Sx) -> Sx {
    let t = trig_reduce(e);
    if t.size() > 12 {
        let c = cancel(&t);
        if c.size() < t.size() {
            return c;
        }
    }
    t
}

// ================================================================== integration

fn linear(u: &Sx, x: &Sx) -> Option<(Sx, Sx)> {
    if !u.depends(x) {
        return None;
    }
    let cs = poly_in(u, x)?;
    if cs.len() == 2 && !cs[1].is_zero() {
        Some((cs[1].clone(), cs[0].clone()))
    } else {
        None
    }
}

/// Square root assuming positivity of symbols.
fn sq(e: &Sx) -> Sx {
    match e {
        Sx::Num(_) => sqrt(e.clone()),
        Sx::Mul(v) => product(v.iter().map(sq).collect()),
        Sx::Pow(b, p) if matches!(**p, Sx::Num(_)) => pow((**b).clone(), mul(half(), (**p).clone())),
        Sx::Sym(_) => sqrt(e.clone()),
        _ => sqrt(e.clone()),
    }
}

fn sign_hint(e: &Sx) -> Option<i32> {
    match e {
        Sx::Num(r) => Some(if r.is_positive() { 1 } else if r.is_negative() { -1 } else { 0 }),
        Sx::Float(F(v)) => Some(if *v > 0.0 { 1 } else if *v < 0.0 { -1 } else { 0 }),
        Sx::Pow(_, p) if matches!(&**p, Sx::Num(r) if r.is_integer() && r.to_integer().is_even()) => Some(1),
        Sx::Mul(v) => v.iter().try_fold(1, |acc, f| sign_hint(f).map(|s| acc * s)),
        Sx::Add(v) => {
            let ss: Option<Vec<i32>> = v.iter().map(sign_hint).collect();
            let ss = ss?;
            if ss.iter().all(|s| *s >= 0) && ss.iter().any(|s| *s > 0) {
                Some(1)
            } else if ss.iter().all(|s| *s <= 0) && ss.iter().any(|s| *s < 0) {
                Some(-1)
            } else {
                None
            }
        }
        Sx::Sym(_) if is_positive(e) => Some(1),
        _ => None,
    }
}

/// `int dx / q^k` with `q = a x^2 + b x + c`.
fn inv_quad_power(a: &Sx, b: &Sx, c: &Sx, x: &Sx, k: i64) -> Option<Sx> {
    let q = add(add(mul(a.clone(), pow(x.clone(), int(2))), mul(b.clone(), x.clone())), c.clone());
    let delta = sub(mul(int(4), mul(a.clone(), c.clone())), pow(b.clone(), int(2)));
    let delta = expand(&delta);
    let lin = add(mul(mul(int(2), a.clone()), x.clone()), b.clone());
    if k == 1 {
        return match sign_hint(&delta)? {
            1 => {
                let s = sq(&delta);
                Some(mul(div(int(2), s.clone()), func("atan", vec![div(lin, s)])))
            }
            -1 => {
                let d = neg(delta);
                let s = sq(&d);
                Some(mul(pow(s.clone(), int(-1)), func("ln", vec![div(sub(lin.clone(), s.clone()), add(lin, s))])))
            }
            _ => None,
        };
    }
    if sign_hint(&delta)? <= 0 {
        return None;
    }
    let prev = inv_quad_power(a, b, c, x, k - 1)?;
    let t1 = div(lin, mul(mul(int(k - 1), delta.clone()), pow(q, int(k - 1))));
    let t2 = mul(div(mul(int(2 * (2 * k - 3)), a.clone()), mul(int(k - 1), delta)), prev);
    Some(add(t1, t2))
}

/// `int (B x + C) / q^k dx` for a general quadratic `q`.
fn quad_term(bb: &Sx, cc: &Sx, a: &Sx, b: &Sx, c: &Sx, x: &Sx, k: i64) -> Option<Sx> {
    let q = add(add(mul(a.clone(), pow(x.clone(), int(2))), mul(b.clone(), x.clone())), c.clone());
    let two_a = mul(int(2), a.clone());
    let b_part = div(bb.clone(), two_a.clone());
    let mut out = Vec::new();
    if !bb.is_zero() {
        let d = if k == 1 { func("ln", vec![q.clone()]) } else { div(int(-1), mul(int(k - 1), pow(q.clone(), int(k - 1)))) };
        out.push(mul(b_part.clone(), d));
    }
    let rest = sub(cc.clone(), mul(b_part, b.clone()));
    if !rest.is_zero() {
        out.push(mul(rest, inv_quad_power(a, b, c, x, k)?));
    }
    Some(sum(out))
}

fn trig_power(name: &str, u: &Sx, n: i64, a: &Sx, x: &Sx, depth: u32) -> Option<Sx> {
    // int sin^n(u) dx, u = a x + b, n >= 2
    let (s, c) = (func("sin", vec![u.clone()]), func("cos", vec![u.clone()]));
    let lower = if n == 2 {
        if name == "sin" {
            div(sub(u.clone(), mul(s.clone(), c.clone())), int(2))
        } else {
            div(add(u.clone(), mul(s.clone(), c.clone())), int(2))
        }
    } else {
        let prev = trig_power(name, u, n - 2, a, x, depth)?;
        // reduction formula (in u)
        if name == "sin" {
            add(div(neg(mul(pow(s.clone(), int(n - 1)), c.clone())), int(n)), mul(rat(n - 1, n), prev))
        } else {
            add(div(mul(pow(c.clone(), int(n - 1)), s.clone()), int(n)), mul(rat(n - 1, n), prev))
        }
    };
    Some(div(lower, a.clone()))
}

fn int_table(f: &Sx, x: &Sx, depth: u32) -> Option<Sx> {
    if f == x {
        return Some(div(pow(x.clone(), int(2)), int(2)));
    }
    match f {
        Sx::Pow(b, n) => {
            if !n.depends(x) {
                if let Some((a, _)) = linear(b, x).or_else(|| if **b == *x { Some((one(), zero())) } else { None }) {
                    return Some(if n.is_minus_one() {
                        div(func("ln", vec![(**b).clone()]), a)
                    } else {
                        div(pow((**b).clone(), add((**n).clone(), one())), mul(add((**n).clone(), one()), a))
                    });
                }
                // sin^n, cos^n
                if let (Sx::Fun(fname, args), Some(k)) = (&**b, n.as_int()) {
                    if args.len() == 1 && (fname == "sin" || fname == "cos") {
                        if let Some((a, _)) = linear(&args[0], x) {
                            if k >= 2 {
                                return trig_power(fname, &args[0], k, &a, x, depth);
                            }
                            if k == -2 {
                                return Some(if fname == "cos" {
                                    div(func("tan", vec![args[0].clone()]), a)
                                } else {
                                    neg(div(func("cos", vec![args[0].clone()]), mul(a, func("sin", vec![args[0].clone()]))))
                                });
                            }
                        }
                    }
                }
                // 1/sqrt(c0 + c2 x^2)
                if matches!(&**n, Sx::Num(r) if *r == rat_ij(-1, 2)) {
                    if let Some(cs) = poly_in(b, x) {
                        if cs.len() == 3 && cs[1].is_zero() {
                            let (c0, c2) = (&cs[0], &cs[2]);
                            match (sign_hint(c2), sign_hint(c0)) {
                                (Some(1), _) => {
                                    let s2 = sq(c2);
                                    return Some(div(func("ln", vec![add(mul(s2.clone(), x.clone()), f_inv_sqrt_base(b))]), s2));
                                }
                                (Some(-1), Some(1)) => {
                                    let s2 = sq(&neg(c2.clone()));
                                    return Some(div(func("asin", vec![div(mul(s2.clone(), x.clone()), sq(c0))]), s2));
                                }
                                _ => {}
                            }
                        }
                    }
                }
            } else if !b.depends(x) {
                if let Some((a, _)) = linear(n, x) {
                    if b.is_num() && b.is_one() {
                        return None;
                    }
                    return Some(div(f.clone(), mul(a, func("ln", vec![(**b).clone()]))));
                }
            }
            None
        }
        Sx::Fun(name, args) if args.len() == 1 => {
            let u = &args[0];
            let (a, _) = linear(u, x)?;
            let r = match name.as_str() {
                "exp" => f.clone(),
                "sin" => neg(func("cos", vec![u.clone()])),
                "cos" => func("sin", vec![u.clone()]),
                "tan" => neg(func("ln", vec![func("cos", vec![u.clone()])])),
                "sinh" => func("cosh", vec![u.clone()]),
                "cosh" => func("sinh", vec![u.clone()]),
                "tanh" => func("ln", vec![func("cosh", vec![u.clone()])]),
                "ln" => sub(mul(u.clone(), f.clone()), u.clone()),
                "atan" => sub(mul(u.clone(), f.clone()), div(func("ln", vec![add(one(), pow(u.clone(), int(2)))]), int(2))),
                "asin" => add(mul(u.clone(), f.clone()), sqrt(sub(one(), pow(u.clone(), int(2))))),
                "acos" => sub(mul(u.clone(), f.clone()), sqrt(sub(one(), pow(u.clone(), int(2))))),
                _ => return None,
            };
            Some(div(r, a))
        }
        Sx::Mul(v) if v.len() == 2 => {
            // exp(lin) * sin/cos(lin)
            let (e1, t1) = match (&v[0], &v[1]) {
                (Sx::Fun(n1, _), Sx::Fun(n2, _)) if n1 == "exp" && (n2 == "sin" || n2 == "cos") => (&v[0], &v[1]),
                (Sx::Fun(n1, _), Sx::Fun(n2, _)) if n2 == "exp" && (n1 == "sin" || n1 == "cos") => (&v[1], &v[0]),
                _ => return None,
            };
            if let (Sx::Fun(_, ea), Sx::Fun(tn, ta)) = (e1, t1) {
                let (a, _) = linear(&ea[0], x)?;
                let (b, _) = linear(&ta[0], x)?;
                let theta = &ta[0];
                let den = add(pow(a.clone(), int(2)), pow(b.clone(), int(2)));
                let (s, c) = (func("sin", vec![theta.clone()]), func("cos", vec![theta.clone()]));
                let r = if tn == "sin" { sub(mul(a, s), mul(b, c)) } else { add(mul(a, c), mul(b, s)) };
                return Some(div(mul(e1.clone(), r), den));
            }
            None
        }
        _ => None,
    }
}

fn f_inv_sqrt_base(b: &Sx) -> Sx {
    sqrt(b.clone())
}

fn rational_parts(f: &Sx, x: &Sx) -> Option<(poly::P, poly::P)> {
    let t = together(f);
    let (n, d) = num_den(&t);
    let (pn, pd) = (to_p(&n, x)?, to_p(&d, x)?);
    if pd.is_empty() {
        return None;
    }
    Some((pn, pd))
}

fn int_rational(f: &Sx, x: &Sx, depth: u32) -> Option<Sx> {
    let (_pn, pd) = rational_parts(f, x)?;
    if poly::deg(&pd) < 1 {
        return None;
    }
    let pf = parfrac(f, x)?;
    let mut out = Vec::new();
    for t in match pf {
        Sx::Add(v) => v,
        other => vec![other],
    } {
        out.push(int_pf_term(&t, x, depth)?);
    }
    Some(sum(out))
}

fn int_pf_term(t: &Sx, x: &Sx, depth: u32) -> Option<Sx> {
    if let Some(r) = int_table(t, x, depth) {
        return Some(r);
    }
    let (n, d) = num_den(t);
    let (pn, pd) = (to_p(&n, x)?, to_p(&d, x)?);
    if poly::deg(&pd) == 0 {
        // polynomial term
        let c = &pd[0];
        let mut out = Vec::new();
        for (k, co) in pn.iter().enumerate() {
            out.push(mul(Sx::Num(co / c), div(pow(x.clone(), int(k as i64 + 1)), int(k as i64 + 1))));
        }
        return Some(sum(out));
    }
    let (c, fs) = poly::factor_q(&pd);
    if fs.len() != 1 {
        return None;
    }
    let (q, k) = (&fs[0].0, fs[0].1);
    if poly::deg(q) == 1 {
        // (A x + B)/(c (a x + b)^k): rewrite through substitution
        let a = Sx::Num(q[1].clone());
        let b = Sx::Num(q[0].clone());
        let u = add(mul(a.clone(), x.clone()), b);
        // A x + B = (A/a) (a x + b) + (B - A b/a)
        let (pa, pb) = (pn.get(1).cloned().unwrap_or_else(Rat::zero), pn.first().cloned().unwrap_or_else(Rat::zero));
        let big_a = Sx::Num(pa.clone() / &c);
        let rest = Sx::Num(pb / &c - pa * q[0].clone() / q[1].clone() / &c);
        let r1 = if k == 1 { div(big_a.clone(), a.clone()) } else { zero() };
        let mut out = Vec::new();
        // (A/a) * int (u)^(1-k) / a ... computed directly
        let ia = |p: i64| -> Sx {
            // int u^p dx = u^(p+1)/((p+1) a), p=-1 -> ln(u)/a
            if p == -1 {
                div(func("ln", vec![u.clone()]), a.clone())
            } else {
                div(pow(u.clone(), int(p + 1)), mul(int(p + 1), a.clone()))
            }
        };
        let _ = r1;
        out.push(mul(div(big_a, a.clone()), ia(1 - k as i64)));
        out.push(mul(rest, ia(-(k as i64))));
        return Some(sum(out));
    }
    if poly::deg(q) == 2 {
        let (a, b, cq) = (Sx::Num(q[2].clone()), Sx::Num(q[1].clone()), Sx::Num(q[0].clone()));
        let (bb, cc) = (Sx::Num(pn.get(1).cloned().unwrap_or_else(Rat::zero) / &c), Sx::Num(pn.first().cloned().unwrap_or_else(Rat::zero) / &c));
        if poly::deg(&pn) > 1 {
            return None;
        }
        return quad_term(&bb, &cc, &a, &b, &cq, x, k as i64);
    }
    None
}

/// Rational function with symbolic parameters and a quadratic denominator.
fn int_quad_symbolic(f: &Sx, x: &Sx) -> Option<Sx> {
    let t = together(f);
    let (n, d) = num_den(&t);
    let cn = poly_in(&n, x)?;
    let (c, base, k) = {
        // d = c * q^k
        let (cf, rest) = split_coeff(&d);
        let _ = cf;
        let mut k = 1;
        let mut q = rest.clone();
        if let Sx::Pow(b, e) = &rest {
            k = e.as_int()?;
            q = (**b).clone();
        }
        (split_coeff(&d).0, q, k)
    };
    let cq = poly_in(&base, x)?;
    if cq.len() != 3 || cn.len() > 2 || k < 1 {
        return None;
    }
    let cs = c.to_sx();
    let (bb, cc) = (div(cn.get(1).cloned().unwrap_or_else(zero), cs.clone()), div(cn[0].clone(), cs));
    quad_term(&bb, &cc, &cq[2], &cq[1], &cq[0], x, k)
}

fn candidates(f: &Sx, x: &Sx, out: &mut Vec<Sx>) {
    let mut push = |u: &Sx| {
        if u != x && u.depends(x) && !matches!(u, Sx::Num(_)) && !out.contains(u) {
            out.push(u.clone());
        }
    };
    match f {
        Sx::Fun(_, a) => {
            push(f);
            for u in a {
                push(u);
            }
            for u in a {
                candidates(u, x, out);
            }
        }
        Sx::Pow(b, e) => {
            push(b);
            if e.depends(x) {
                push(e);
            }
            candidates(b, x, out);
            candidates(e, x, out);
        }
        Sx::Add(v) | Sx::Mul(v) => {
            for u in v {
                candidates(u, x, out);
            }
        }
        _ => {}
    }
}

fn int_subst(f: &Sx, x: &Sx, depth: u32) -> Option<Sx> {
    if depth >= 4 {
        return None;
    }
    let mut cands = Vec::new();
    candidates(f, x, &mut cands);
    cands.sort_by_key(|u| std::cmp::Reverse(u.size()));
    let t = Sx::Sym("§u".into());
    for u in cands.into_iter().take(10) {
        let du = diff(&u, x);
        if du.is_zero() {
            continue;
        }
        let q = cancel(&div(f.clone(), du));
        let q = trig_reduce(&q);
        let qt = subs(&q, &u, &t);
        if qt.depends(x) {
            // try the simplified forms too
            let qt2 = subs(&simplify(&q), &u, &t);
            if qt2.depends(x) {
                continue;
            }
            if let Some(g) = integ(&qt2, &t, depth + 1) {
                return Some(subs(&g, &t, &u));
            }
            continue;
        }
        if let Some(g) = integ(&qt, &t, depth + 1) {
            return Some(subs(&g, &t, &u));
        }
    }
    None
}

fn int_parts(f: &Sx, x: &Sx, depth: u32) -> Option<Sx> {
    if depth >= 4 {
        return None;
    }
    let fs: Vec<Sx> = match f {
        Sx::Mul(v) => v.clone(),
        _ => return None,
    };
    let (polys, others): (Vec<Sx>, Vec<Sx>) = fs.into_iter().partition(|t| t == x || matches!(t, Sx::Pow(b, e) if **b == *x && e.as_int().is_some_and(|k| k >= 1)));
    if polys.is_empty() || others.len() != 1 {
        return None;
    }
    let pm = product(polys);
    let g = &others[0];
    let family_e = matches!(g, Sx::Fun(n, a) if a.len() == 1 && matches!(n.as_str(), "exp" | "sin" | "cos" | "sinh" | "cosh") && linear(&a[0], x).is_some());
    let family_l = matches!(g, Sx::Fun(n, a) if a.len() == 1 && matches!(n.as_str(), "ln" | "atan" | "asin" | "acos") && linear(&a[0], x).is_some());
    if family_e {
        let v = int_table(g, x, depth)?;
        let dp = diff(&pm, x);
        let rest = integ(&expand(&mul(dp, v.clone())), x, depth + 1)?;
        return Some(sub(mul(pm, v), rest));
    }
    if family_l {
        let v = integ(&pm, x, depth + 1)?;
        let dg = diff(g, x);
        let rest = integ(&cancel(&mul(v.clone(), dg)), x, depth + 1)?;
        return Some(sub(mul(g.clone(), v), rest));
    }
    None
}

/// Does `f(x)` hold at a numeric sample (used to double-check antiderivatives)?
fn verify(f: &Sx, big_f: &Sx, x: &Sx) -> bool {
    let syms = f.free_symbols();
    let mut pairs: Vec<(Sx, Sx)> = Vec::new();
    for (i, s) in syms.iter().enumerate() {
        if Sx::Sym(s.clone()) != *x {
            pairs.push((Sx::Sym(s.clone()), float(1.3 + 0.37 * i as f64)));
        }
    }
    let d = diff(big_f, x);
    for t in [0.37, 0.71, 1.9] {
        let mut p = pairs.clone();
        p.push((x.clone(), float(t)));
        let a = eval_c(&subs_many(f, &p));
        let b = eval_c(&subs_many(&d, &p));
        match (a, b) {
            (Some(a), Some(b)) => {
                if !a.re.is_finite() || !b.re.is_finite() {
                    continue;
                }
                if (a - b).norm() > 1e-6 * a.norm().max(b.norm()).max(1.0) {
                    return false;
                }
            }
            _ => continue,
        }
    }
    true
}

pub fn integ(f: &Sx, x: &Sx, depth: u32) -> Option<Sx> {
    if depth > 8 {
        return None;
    }
    if !f.depends(x) {
        return Some(mul(f.clone(), x.clone()));
    }
    if let Sx::Mat(m) = f {
        let mut out = Vec::new();
        for r in m {
            out.push(r.iter().map(|t| integ(t, x, depth)).collect::<Option<Vec<_>>>()?);
        }
        return Some(Sx::Mat(out));
    }
    match f {
        Sx::Add(ts) => {
            let mut out = Vec::new();
            for t in ts {
                out.push(integ(t, x, depth + 1)?);
            }
            return Some(sum(out));
        }
        Sx::Mul(fs) => {
            let (indep, dep): (Vec<Sx>, Vec<Sx>) = fs.iter().cloned().partition(|t| !t.depends(x));
            if !indep.is_empty() {
                let rest = integ(&product(dep), x, depth + 1)?;
                return Some(mul(product(indep), rest));
            }
        }
        _ => {}
    }
    let try_rules = |g: &Sx| -> Option<Sx> {
        if let Some(r) = int_table(g, x, depth) {
            return Some(r);
        }
        if let Some(r) = int_rational(g, x, depth) {
            return Some(r);
        }
        if let Some(r) = int_quad_symbolic(g, x) {
            return Some(r);
        }
        if let Some(r) = int_subst(g, x, depth) {
            return Some(r);
        }
        if let Some(r) = int_parts(g, x, depth) {
            return Some(r);
        }
        None
    };
    let check = |r: Sx| -> Option<Sx> {
        if verify(f, &r, x) {
            Some(r)
        } else {
            None
        }
    };
    if let Some(r) = try_rules(f) {
        if let Some(r) = check(r) {
            return Some(r);
        }
    }
    let ex = expand(f);
    if &ex != f {
        if let Some(r) = integ(&ex, x, depth + 1) {
            return check(r);
        }
    }
    let tg = cancel(f);
    if &tg != f {
        if let Some(r) = try_rules(&tg) {
            return check(r);
        }
    }
    None
}

fn pole_inside(f: &Sx, x: &Sx, a: f64, b: f64) -> bool {
    // look at negative powers and tan: base zero inside (a, b)?
    let mut bases = Vec::new();
    fn collect_b(e: &Sx, out: &mut Vec<Sx>) {
        match e {
            Sx::Pow(b, p) => {
                if matches!(&**p, Sx::Num(r) if r.is_negative()) {
                    out.push((**b).clone());
                }
                collect_b(b, out);
                collect_b(p, out);
            }
            Sx::Fun(n, a) => {
                if n == "tan" {
                    out.push(func("cos", a.clone()));
                }
                a.iter().for_each(|t| collect_b(t, out));
            }
            Sx::Add(v) | Sx::Mul(v) => v.iter().for_each(|t| collect_b(t, out)),
            _ => {}
        }
    }
    collect_b(f, &mut bases);
    let n = 2000;
    for base in bases {
        let mut prev: Option<f64> = None;
        for k in 0..=n {
            let t = a + (b - a) * (k as f64) / (n as f64);
            let v = eval_f64(&subs(&base, x, &float(t)));
            if let Some(v) = v {
                if v.abs() < 1e-12 {
                    return true;
                }
                if let Some(p) = prev {
                    if p.signum() != v.signum() {
                        return true;
                    }
                }
                prev = Some(v);
            }
        }
    }
    false
}

/// Antiderivative evaluated between the bounds. The exact attempt is bounded in time: it is a
/// fast path, not the only way to integrate.
pub fn definite(f: &Sx, x: &Sx, lo: &Sx, hi: &Sx) -> Option<Sx> {
    if crate::symbolic::poly::budget_expired() {
        return None;
    }
    crate::symbolic::poly::ensure_budget();
    definite_inner(f, x, lo, hi)
}

fn definite_inner(f: &Sx, x: &Sx, lo: &Sx, hi: &Sx) -> Option<Sx> {
    let big = integ(f, x, 0)?;
    let (a, b) = (eval_f64(lo), eval_f64(hi));
    if let (Some(a), Some(b)) = (a, b) {
        if a.is_finite() && b.is_finite() && pole_inside(f, x, a.min(b), a.max(b)) {
            return None;
        }
    }
    let at = |p: &Sx, sign: i32| -> Option<Sx> {
        let v = bound_value(&big, x, p)?;
        Some(if sign < 0 { neg(v) } else { v })
    };
    let hi_v = at(hi, 1)?;
    let lo_v = at(lo, -1)?;
    Some(sum(vec![hi_v, lo_v]))
}

fn bound_value(big: &Sx, x: &Sx, p: &Sx) -> Option<Sx> {
    let is_inf = |s: &Sx| s.is_sym("∞") || matches!(s, Sx::Mul(v) if v.len() == 2 && v[1].is_sym("∞") && v[0].is_minus_one());
    if is_inf(p) {
        let dir = if p.is_sym("∞") { 1 } else { -1 };
        return match limit(big, x, p, dir) {
            Some(v) if !has_inf(&v) => Some(v),
            _ => None,
        };
    }
    let (v, err) = capture(|| subs(big, x, p));
    if err.is_some() {
        // the antiderivative may have a removable/one-sided limit
        return match limit(big, x, p, 0) {
            Some(v) if !has_inf(&v) => Some(v),
            _ => None,
        };
    }
    Some(v)
}

fn has_inf(e: &Sx) -> bool {
    e.has_sym("∞")
}

// ================================================================== limits

#[derive(Clone, Debug)]
enum L {
    Fin(Sx),
    PInf,
    NInf,
    Undef,
}

fn l_of(e: Sx) -> L {
    if e.is_sym("∞") {
        L::PInf
    } else if matches!(&e, Sx::Mul(v) if v.len() == 2 && v[0].is_minus_one() && v[1].is_sym("∞")) {
        L::NInf
    } else {
        L::Fin(e)
    }
}

fn l_to_sx(l: &L) -> Option<Sx> {
    match l {
        L::Fin(v) => Some(v.clone()),
        L::PInf => Some(inf()),
        L::NInf => Some(neg(inf())),
        L::Undef => None,
    }
}

fn sign_of(v: &Sx) -> Option<i32> {
    let f = eval_f64(v)?;
    Some(if f > 0.0 { 1 } else if f < 0.0 { -1 } else { 0 })
}

trait ContainsSqrt {
    fn contains_sqrt(&self) -> bool;
}
impl ContainsSqrt for Sx {
    fn contains_sqrt(&self) -> bool {
        match self {
            Sx::Pow(b, p) => matches!(&**p, Sx::Num(r) if !r.is_integer()) || b.contains_sqrt(),
            Sx::Add(v) | Sx::Mul(v) | Sx::Fun(_, v) => v.iter().any(|t| t.contains_sqrt()),
            _ => false,
        }
    }
}

struct Lim<'a> {
    x: &'a Sx,
    /// point (Fin) or infinity
    at: L,
    dir: i32,
}

impl Lim<'_> {
    fn point_f64(&self) -> Option<f64> {
        match &self.at {
            L::Fin(a) => eval_f64(a),
            _ => None,
        }
    }

    /// The sign of `b(x)` near the limit point, for a base tending to zero.
    fn sign_near(&self, b: &Sx) -> Option<i32> {
        match &self.at {
            L::PInf | L::NInf => {
                let big = if matches!(self.at, L::PInf) { 1e6 } else { -1e6 };
                sign_of(&subs(b, self.x, &float(big)))
            }
            L::Fin(_) => {
                let a = self.point_f64()?;
                let mut signs = Vec::new();
                for h in [1e-7, 1e-5] {
                    if self.dir >= 0 {
                        signs.push(sign_of(&subs(b, self.x, &float(a + h)))?);
                    }
                    if self.dir <= 0 {
                        signs.push(sign_of(&subs(b, self.x, &float(a - h)))?);
                    }
                }
                if signs.iter().all(|s| *s == signs[0]) && signs[0] != 0 {
                    Some(signs[0])
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    fn eval(&self, e: &Sx) -> L {
        if !e.depends(self.x) {
            return l_of(e.clone());
        }
        if e == self.x {
            return self.at.clone();
        }
        match e {
            Sx::Add(v) => {
                let mut fin = Vec::new();
                let (mut pinf, mut ninf) = (false, false);
                for t in v {
                    match self.eval(t) {
                        L::Fin(c) => fin.push(c),
                        L::PInf => pinf = true,
                        L::NInf => ninf = true,
                        L::Undef => return L::Undef,
                    }
                }
                match (pinf, ninf) {
                    (true, true) => L::Undef,
                    (true, false) => L::PInf,
                    (false, true) => L::NInf,
                    _ => L::Fin(sum(fin)),
                }
            }
            Sx::Mul(v) => {
                let mut fin = Vec::new();
                let mut infs = Vec::new();
                for t in v {
                    match self.eval(t) {
                        L::Fin(c) => fin.push(c),
                        L::PInf => infs.push(1),
                        L::NInf => infs.push(-1),
                        L::Undef => return L::Undef,
                    }
                }
                let c = product(fin);
                if infs.is_empty() {
                    return L::Fin(c);
                }
                if c.is_zero() {
                    return L::Undef;
                }
                let s = match sign_of(&c) {
                    Some(s) => s,
                    None => return L::Undef,
                };
                let total: i32 = s * infs.iter().product::<i32>();
                if total > 0 {
                    L::PInf
                } else {
                    L::NInf
                }
            }
            Sx::Pow(b, p) => {
                if p.depends(self.x) {
                    // b^p = exp(p ln b)
                    let rew = func("exp", vec![mul((**p).clone(), func("ln", vec![(**b).clone()]))]);
                    return self.eval(&rew);
                }
                let lb = self.eval(b);
                let pe = eval_f64(p);
                match lb {
                    L::Fin(v) => {
                        if v.is_zero() {
                            match pe {
                                Some(pf) if pf > 0.0 => L::Fin(zero()),
                                Some(pf) if pf < 0.0 => {
                                    let sgn = match self.sign_near(b) {
                                        Some(s) => s,
                                        None => return L::Undef,
                                    };
                                    let even = p.as_int().is_some_and(|k| k % 2 == 0);
                                    if even || sgn > 0 && p.as_int().is_none() {
                                        L::PInf
                                    } else if p.as_int().is_some() {
                                        if sgn > 0 {
                                            L::PInf
                                        } else {
                                            L::NInf
                                        }
                                    } else {
                                        L::Undef
                                    }
                                }
                                _ => L::Undef,
                            }
                        } else {
                            let (r, err) = capture(|| pow(v, (**p).clone()));
                            if err.is_some() {
                                L::Undef
                            } else {
                                L::Fin(r)
                            }
                        }
                    }
                    L::PInf => match pe {
                        Some(pf) if pf > 0.0 => L::PInf,
                        Some(pf) if pf < 0.0 => L::Fin(zero()),
                        _ => L::Undef,
                    },
                    L::NInf => match (pe, p.as_int()) {
                        (Some(pf), Some(k)) if pf > 0.0 => {
                            if k % 2 == 0 {
                                L::PInf
                            } else {
                                L::NInf
                            }
                        }
                        (Some(pf), Some(_)) if pf < 0.0 => L::Fin(zero()),
                        _ => L::Undef,
                    },
                    L::Undef => L::Undef,
                }
            }
            Sx::Fun(name, args) if args.len() == 1 => {
                let l = self.eval(&args[0]);
                match (&l, name.as_str()) {
                    (L::Fin(v), "ln") if v.is_zero() => {
                        if self.sign_near(&args[0]) == Some(1) {
                            L::NInf
                        } else {
                            L::Undef
                        }
                    }
                    (L::Fin(v), "tan") if pi_multiple(&sub(v.clone(), mul(half(), pi()))).is_some_and(|c| c.is_integer()) => L::Undef,
                    (L::Fin(v), _) => {
                        let (r, err) = capture(|| func(name, vec![v.clone()]));
                        if err.is_some() {
                            L::Undef
                        } else {
                            L::Fin(r)
                        }
                    }
                    (L::PInf, "exp" | "ln" | "sinh" | "cosh" | "abs" | "asinh" | "acosh") => L::PInf,
                    (L::PInf, "atan") => L::Fin(mul(half(), pi())),
                    (L::PInf, "tanh" | "erf" | "sign") => L::Fin(one()),
                    (L::NInf, "exp" | "tanh_" | "sech") => L::Fin(zero()),
                    (L::NInf, "atan") => L::Fin(neg(mul(half(), pi()))),
                    (L::NInf, "tanh" | "erf") => L::Fin(int(-1)),
                    (L::NInf, "sinh" | "asinh") => L::NInf,
                    (L::NInf, "cosh" | "abs") => L::PInf,
                    _ => L::Undef,
                }
            }
            _ => L::Undef,
        }
    }
}

fn limit_rec(e: &Sx, x: &Sx, at: &L, dir: i32, depth: u32) -> L {
    let ctx = Lim { x, at: at.clone(), dir };
    let (direct, err) = capture(|| ctx.eval(e));
    if err.is_none() {
        if !matches!(direct, L::Undef) {
            return direct;
        }
    }
    if depth > 8 {
        return L::Undef;
    }
    // L'Hopital on N/D
    let t = together(e);
    let (n, d) = num_den(&t);
    if !d.is_one() {
        let (ln, ld) = (ctx.eval(&n), ctx.eval(&d));
        let zero_n = matches!(&ln, L::Fin(v) if v.is_zero());
        let zero_d = matches!(&ld, L::Fin(v) if v.is_zero());
        let inf_n = matches!(ln, L::PInf | L::NInf);
        let inf_d = matches!(ld, L::PInf | L::NInf);
        if (zero_n && zero_d) || (inf_n && inf_d) {
            let r = cancel(&div(diff(&n, x), diff(&d, x)));
            let r = trig_reduce(&r);
            return limit_rec(&r, x, at, dir, depth + 1);
        }
        // finite nonzero over zero etc. were handled by the direct evaluation (when possible)
    }
    // product 0 * inf : z / (1/w)
    if let Sx::Mul(v) = e {
        let ls: Vec<L> = v.iter().map(|t| ctx.eval(t)).collect();
        if let Some(i) = ls.iter().position(|l| matches!(l, L::Fin(c) if c.is_zero())) {
            if ls.iter().any(|l| matches!(l, L::PInf | L::NInf)) {
                let z = v[i].clone();
                let w = product(v.iter().enumerate().filter(|(j, _)| *j != i).map(|(_, t)| t.clone()).collect());
                // z / (1/w) and w / (1/z)
                for (n_, d_) in [(z.clone(), pow(w.clone(), int(-1))), (w.clone(), pow(z.clone(), int(-1)))] {
                    let r = cancel(&div(diff(&n_, x), diff(&d_, x)));
                    let l = limit_rec(&r, x, at, dir, depth + 1);
                    if !matches!(l, L::Undef) {
                        return l;
                    }
                }
                return L::Undef;
            }
        }
    }
    // power indeterminate forms: b^p = exp(p ln b)
    if let Sx::Pow(b, p) = e {
        if p.depends(x) {
            let ex = mul((**p).clone(), func("ln", vec![(**b).clone()]));
            let l = limit_rec(&ex, x, at, dir, depth + 1);
            return match l {
                L::Fin(v) => L::Fin(func("exp", vec![v])),
                L::PInf => L::PInf,
                L::NInf => L::Fin(zero()),
                L::Undef => L::Undef,
            };
        }
    }
    if let Sx::Fun(name, args) = e {
        if args.len() == 1 {
            let l = limit_rec(&args[0], x, at, dir, depth + 1);
            return match l {
                L::Fin(v) => {
                    let (r, err) = capture(|| func(name, vec![v]));
                    if err.is_some() {
                        L::Undef
                    } else {
                        L::Fin(r)
                    }
                }
                L::Undef => L::Undef,
                other => Lim { x, at: other.clone(), dir: 0 }.eval(&Sx::Fun(name.clone(), vec![Sx::Sym("§l".into())])).pipe_inf(&other, name),
            };
        }
    }
    // difference with a square root: multiply by the conjugate
    if let Sx::Add(ts) = e {
        if ts.len() == 2 && ts.iter().any(|t| t.contains_sqrt()) {
            let (t1, t2) = (ts[0].clone(), ts[1].clone());
            let numr = expand(&sub(pow(t1.clone(), int(2)), pow(t2.clone(), int(2))));
            let den = sub(t1, t2);
            if !den.is_zero() {
                let r = cancel(&div(numr, den));
                if &r != e {
                    let l = limit_rec(&r, x, at, dir, depth + 1);
                    if !matches!(l, L::Undef) {
                        return l;
                    }
                }
            }
        }
    }
    // sums of indeterminate parts: combine fractions, or evaluate the parts with LH
    if let Sx::Add(ts) = e {
        let ls: Vec<L> = ts.iter().map(|t| limit_rec(t, x, at, dir, depth + 1)).collect();
        if ls.iter().all(|l| !matches!(l, L::Undef)) {
            let mut fin = Vec::new();
            let (mut pi_, mut ni) = (false, false);
            for l in ls {
                match l {
                    L::Fin(v) => fin.push(v),
                    L::PInf => pi_ = true,
                    L::NInf => ni = true,
                    L::Undef => {}
                }
            }
            return match (pi_, ni) {
                (true, true) => {
                    if !d.is_one() {
                        L::Undef
                    } else {
                        L::Undef
                    }
                }
                (true, false) => L::PInf,
                (false, true) => L::NInf,
                _ => L::Fin(sum(fin)),
            };
        }
    }
    if let Sx::Mul(v) = e {
        let ls: Vec<L> = v.iter().map(|t| limit_rec(t, x, at, dir, depth + 1)).collect();
        if ls.iter().all(|l| !matches!(l, L::Undef)) {
            let mix = Sx::Mul(v.clone());
            let _ = mix;
            // recompute through the generic product rule with the resolved limits
            let mut fin = Vec::new();
            let mut infs = Vec::new();
            for l in &ls {
                match l {
                    L::Fin(c) => fin.push(c.clone()),
                    L::PInf => infs.push(1),
                    L::NInf => infs.push(-1),
                    L::Undef => {}
                }
            }
            let c = product(fin);
            if infs.is_empty() {
                return L::Fin(c);
            }
            if !c.is_zero() {
                if let Some(s) = sign_of(&c) {
                    return if s * infs.iter().product::<i32>() > 0 { L::PInf } else { L::NInf };
                }
            }
        }
    }
    L::Undef
}

trait PipeInf {
    fn pipe_inf(self, l: &L, name: &str) -> L;
}
impl PipeInf for L {
    fn pipe_inf(self, _l: &L, _name: &str) -> L {
        self
    }
}

/// `lim_{x -> at} e`; `dir` is 1 (from the right), -1 (from the left) or 0 (two-sided).
pub fn limit(e: &Sx, x: &Sx, at: &Sx, dir: i32) -> Option<Sx> {
    let point = l_of(at.clone());
    let (r, err) = capture(|| {
        let r = limit_rec(e, x, &point, dir, 0);
        // two-sided limits at a point: both one-sided limits must agree for infinities
        if dir == 0 && matches!(point, L::Fin(_)) && matches!(r, L::PInf | L::NInf) {
            let (a, b) = (limit_rec(e, x, &point, 1, 0), limit_rec(e, x, &point, -1, 0));
            return match (l_to_sx(&a), l_to_sx(&b)) {
                (Some(p), Some(q)) if p == q => a,
                _ => L::Undef,
            };
        }
        r
    });
    if err.is_some() {
        return None;
    }
    l_to_sx(&r)
}

// ================================================================== series

pub fn series(e: &Sx, x: &Sx, a: &Sx, n: usize) -> Option<Sx> {
    let n = n.max(1);
    let at_point = |f: &Sx| -> Option<Sx> {
        let (v, err) = capture(|| subs(f, x, a));
        if err.is_none() && !v.has_sym("∞") && !matches!(&v, Sx::Fun(n, _) if n == "undefined") {
            return Some(v);
        }
        limit(f, x, a, 0).filter(|v| !v.has_sym("∞"))
    };
    let shift = |k: usize| -> Sx { pow(sub(x.clone(), a.clone()), int(k as i64)) };
    for m in 0..=6usize {
        let g = if m == 0 { e.clone() } else { cancel(&mul(e.clone(), shift(m))) };
        // Taylor coefficients of g
        let total = n + m;
        let mut terms = Vec::new();
        let mut cur = g.clone();
        let mut fact = BigInt::one();
        let mut ok = true;
        for k in 0..total {
            if k > 0 {
                cur = simplify_light(&diff(&cur, x));
                fact *= BigInt::from(k);
            }
            match at_point(&cur) {
                Some(v) => {
                    let c = simplify(&div(v, Sx::Num(Rat::from_integer(fact.clone()))));
                    if !c.is_zero() {
                        if (k as i64) - (m as i64) < n as i64 {
                            terms.push(mul(c, pow(sub(x.clone(), a.clone()), int(k as i64 - m as i64))));
                        }
                    }
                }
                None => {
                    ok = false;
                    break;
                }
            }
        }
        if ok {
            return Some(sum(terms));
        }
    }
    None
}

#[allow(dead_code)]
fn trunc_series(e: &Sx, x: &Sx, a: &Sx, n: usize, _m: usize) -> Sx {
    // terms of order (x-a)^k with k < n
    let shifted = if a.is_zero() { e.clone() } else { e.clone() };
    let t = Sx::Sym("§t".into());
    let _ = t;
    let _ = (x, a);
    let terms: Vec<Sx> = match shifted {
        Sx::Add(v) => v,
        other => vec![other],
    };
    let mut out = Vec::new();
    for term in terms {
        let k = term_order(&term, x, a);
        if k.map_or(true, |k| k < n as i64) {
            out.push(term);
        }
    }
    sum(out)
}

fn term_order(t: &Sx, x: &Sx, a: &Sx) -> Option<i64> {
    let base = if a.is_zero() { x.clone() } else { sub(x.clone(), a.clone()) };
    let fs: Vec<Sx> = match t {
        Sx::Mul(v) => v.clone(),
        other => vec![other.clone()],
    };
    let mut k = 0i64;
    for f in fs {
        if f == base {
            k += 1;
        } else if let Sx::Pow(b, e) = &f {
            if **b == base {
                k += e.as_int()?;
            }
        }
    }
    Some(k)
}

// ================================================================== sums and products

fn bernoulli_plus(m: usize) -> Rat {
    // B_m with B_1 = +1/2
    let mut b: Vec<Rat> = Vec::new();
    for n in 0..=m {
        let mut s = Rat::zero();
        for k in 0..n {
            s += binom(n + 1, k) * &b[k];
        }
        let v = if n == 0 { Rat::one() } else { -s / rat_i(n as i64 + 1) };
        b.push(v);
    }
    let v = b[m].clone();
    if m == 1 {
        -v
    } else {
        v
    }
}

fn binom(n: usize, k: usize) -> Rat {
    let mut r = Rat::one();
    for i in 0..k {
        r = r * rat_i((n - i) as i64) / rat_i(i as i64 + 1);
    }
    r
}

/// `sum_{k=1}^{n} k^m` as an expression in `n`.
fn power_sum(m: usize, n: &Sx) -> Sx {
    let mut terms = Vec::new();
    for j in 0..=m {
        let c = binom(m + 1, j) * bernoulli_plus(j) / rat_i(m as i64 + 1);
        terms.push(mul(Sx::Num(c), pow(n.clone(), int((m + 1 - j) as i64))));
    }
    sum(terms)
}

pub fn sum_closed(body: &Sx, k: &Sx, lo: &Sx, hi: &Sx) -> Option<Sx> {
    // finite numeric range
    if let (Some(a), Some(b)) = (lo.as_int(), hi.as_int()) {
        if b < a {
            return Some(zero());
        }
        if b - a <= 2000 {
            return Some(sum((a..=b).map(|i| subs(body, k, &int(i))).collect()));
        }
    }
    // polynomial in k
    if let Some(cs) = poly_in(body, k) {
        if !hi.has_sym("∞") && cs.len() <= 12 {
            let lo_m1 = sub(lo.clone(), one());
            let mut terms = Vec::new();
            for (m, c) in cs.iter().enumerate() {
                if c.is_zero() {
                    continue;
                }
                let s = if m == 0 {
                    sub(hi.clone(), lo_m1.clone())
                } else {
                    sub(power_sum(m, hi), power_sum(m, &lo_m1))
                };
                terms.push(mul(c.clone(), s));
            }
            return Some(expand(&sum(terms)));
        }
    }
    // geometric: c * r^k
    let (c, rest) = split_ind(body, k);
    let rest = match &rest {
        Sx::Pow(r, e) if **e == neg(k.clone()) => pow(pow((**r).clone(), int(-1)), k.clone()),
        other => other.clone(),
    };
    if let Sx::Pow(r, e) = &rest {
        if !r.depends(k) && **e == *k {
            if hi.has_sym("∞") {
                let rv = eval_f64(r)?;
                if rv.abs() < 1.0 {
                    return Some(mul(c, div(pow((**r).clone(), lo.clone()), sub(one(), (**r).clone()))));
                }
                return None;
            }
            return Some(mul(c, div(sub(pow((**r).clone(), add(hi.clone(), one())), pow((**r).clone(), lo.clone())), sub((**r).clone(), one()))));
        }
    }
    if hi.has_sym("∞") {
        // 1/k^2 and exp series
        if lo.is_one() {
            if rest == pow(k.clone(), int(-2)) {
                return Some(mul(c, div(pow(pi(), int(2)), int(6))));
            }
            if rest == pow(k.clone(), int(-4)) {
                return Some(mul(c, div(pow(pi(), int(4)), int(90))));
            }
        }
    }
    None
}

fn split_ind(e: &Sx, k: &Sx) -> (Sx, Sx) {
    match e {
        Sx::Mul(v) => {
            let (i, d): (Vec<Sx>, Vec<Sx>) = v.iter().cloned().partition(|t| !t.depends(k));
            (product(i), product(d))
        }
        other => (one(), other.clone()),
    }
}

pub fn prod_closed(body: &Sx, k: &Sx, lo: &Sx, hi: &Sx) -> Option<Sx> {
    if let (Some(a), Some(b)) = (lo.as_int(), hi.as_int()) {
        if b < a {
            return Some(one());
        }
        if b - a <= 500 {
            return Some(product((a..=b).map(|i| subs(body, k, &int(i))).collect()));
        }
    }
    if body == k && lo.is_one() {
        return Some(func("factorial", vec![hi.clone()]));
    }
    if !body.depends(k) {
        return Some(pow(body.clone(), add(sub(hi.clone(), lo.clone()), one())));
    }
    None
}

pub fn gcd_big(a: &BigInt, b: &BigInt) -> BigInt {
    a.gcd(b)
}

#[allow(dead_code)]
fn unused(_: &BigInt) -> Option<u32> {
    BigInt::zero().to_u32()
}
