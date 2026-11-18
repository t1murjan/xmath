//! Solving equations and systems.

use super::algebra::*;
use super::poly;
use super::sx::*;
use num_traits::{One, Signed, ToPrimitive, Zero};

pub struct Solved {
    pub tuples: Vec<Vec<Sx>>,
    pub note: Option<String>,
}

/// `lhs - rhs` of an equation, or the expression itself (`expr = 0`).
pub fn equation_zero(e: &Sx) -> Sx {
    match e {
        Sx::Fun(n, a) if n == "==" && a.len() == 2 => sub(a[0].clone(), a[1].clone()),
        other => other.clone(),
    }
}

fn sort_roots(mut v: Vec<Sx>) -> Vec<Sx> {
    v.dedup();
    let mut keyed: Vec<(Option<(f64, f64)>, Sx)> = v.into_iter().map(|r| (eval_c(&r).map(|z| (z.re, z.im)), r)).collect();
    keyed.sort_by(|a, b| match (&a.0, &b.0) {
        (Some(x), Some(y)) => {
            let (ra, rb) = (x.1.abs() < 1e-12, y.1.abs() < 1e-12);
            rb.cmp(&ra).then(x.0.total_cmp(&y.0)).then(x.1.total_cmp(&y.1))
        }
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        _ => a.1.cmp(&b.1),
    });
    let mut out: Vec<Sx> = Vec::new();
    for (_, r) in keyed {
        if !out.contains(&r) {
            out.push(r);
        }
    }
    out
}

fn is_zero_expr(e: &Sx) -> bool {
    if e.is_zero() {
        return true;
    }
    let s = simplify(e);
    if s.is_zero() {
        return true;
    }
    matches!(eval_c(&s), Some(z) if z.norm() < 1e-9) && s.is_const()
}

/// Exact roots of a polynomial with symbolic coefficients (ascending) of degree <= 2, or
/// rational-coefficient polynomials of any degree (cubics and quartics exactly).
fn poly_roots(cs: &[Sx], note: &mut Option<String>) -> Option<Vec<Sx>> {
    let n = cs.len() - 1;
    match n {
        0 => Some(Vec::new()),
        1 => Some(vec![simplify_root(div(neg(cs[0].clone()), cs[1].clone()))]),
        2 => {
            let (a, b, c) = (&cs[2], &cs[1], &cs[0]);
            let disc = expand(&sub(pow(b.clone(), int(2)), mul(int(4), mul(a.clone(), c.clone()))));
            let s = sqrt_sx(&disc);
            let two_a = mul(int(2), a.clone());
            if disc.is_zero() {
                return Some(vec![simplify_root(div(neg(b.clone()), two_a))]);
            }
            Some(vec![
                simplify_root(div(sub(neg(b.clone()), s.clone()), two_a.clone())),
                simplify_root(div(add(neg(b.clone()), s), two_a)),
            ])
        }
        _ => {
            let rat: Option<Vec<Rat>> = cs.iter().map(|c| c.as_rat().cloned()).collect();
            let Some(r) = rat else { return None };
            let p = poly::trim(r);
            let (_, fs) = poly::factor_q(&p);
            let mut roots = Vec::new();
            for (f, _) in fs {
                let fc: Vec<Sx> = f.iter().map(|c| Sx::Num(c.clone())).collect();
                match f.len() - 1 {
                    1 | 2 => roots.extend(poly_roots(&fc, note)?),
                    3 => roots.extend(cubic_roots(&f)),
                    4 => roots.extend(quartic_roots(&f)),
                    _ => {
                        *note = Some("корни многочлена высокой степени найдены приближённо".into());
                        for z in poly::roots_numeric(&f) {
                            roots.push(c_to_sx(z));
                        }
                    }
                }
            }
            Some(roots)
        }
    }
}

fn c_to_sx(z: num_complex::Complex64) -> Sx {
    if z.im == 0.0 {
        float(z.re)
    } else {
        add(float(z.re), mul(float(z.im), Sx::Im))
    }
}

/// `sqrt` of a symbolic quantity that pulls out perfect squares.
pub fn sqrt_sx(e: &Sx) -> Sx {
    match e {
        Sx::Num(r) if r.is_negative() => mul(Sx::Im, sqrt_sx(&Sx::Num(-r.clone()))),
        Sx::Mul(v) => product(v.iter().map(sqrt_sx).collect()),
        Sx::Pow(b, p) if matches!(**p, Sx::Num(ref r) if r.is_integer()) && is_positive(b) => pow((**b).clone(), mul(half(), (**p).clone())),
        Sx::Pow(b, p) if matches!(**p, Sx::Num(ref r) if r.is_integer() && r.to_integer().to_i64().is_some_and(|k| k % 2 == 0)) => {
            // (b^2k)^(1/2) = |b|^k ; assume the principal choice b^k as Mathcad does for symbols
            pow((**b).clone(), mul(half(), (**p).clone()))
        }
        _ => sqrt(e.clone()),
    }
}

fn simplify_root(e: Sx) -> Sx {
    let s = simplify(&e);
    if s.size() <= e.size() {
        s
    } else {
        e
    }
}

fn cbrt(e: Sx) -> Sx {
    pow(e, rat(1, 3))
}

fn cubic_roots(f: &poly::P) -> Vec<Sx> {
    // a x^3 + b x^2 + c x + d
    let (d, c, b, a) = (f[0].clone(), f[1].clone(), f[2].clone(), f[3].clone());
    let three = rat_i(3);
    let p = (&three * &a * &c - &b * &b) / (&three * &a * &a);
    let q = (rat_i(2) * &b * &b * &b - rat_i(9) * &a * &b * &c + rat_i(27) * &a * &a * &d) / (rat_i(27) * &a * &a * &a);
    let shift = -(&b / (&three * &a));
    let delta = (&q / rat_i(2)).pow(2) + (&p / &three).pow(3);
    let sd = sqrt_sx(&Sx::Num(delta));
    let mut u0 = add(Sx::Num(-&q / rat_i(2)), sd.clone());
    if u0.is_zero() {
        u0 = sub(Sx::Num(-&q / rat_i(2)), sd);
    }
    let w = mul(half(), add(int(-1), mul(Sx::Im, sqrt(int(3)))));
    let wc = mul(half(), sub(int(-1), mul(Sx::Im, sqrt(int(3)))));
    let mut roots = Vec::new();
    let cs = [(one(), one()), (w.clone(), wc.clone()), (wc, w)];
    for (ck, cj) in cs {
        let (u, v) = if u0.is_zero() {
            (zero(), zero())
        } else {
            let u = cbrt(u0.clone());
            let v = div(Sx::Num(-&p / &three), u.clone());
            (mul(ck, u), mul(cj, v))
        };
        let t = add(u, v);
        let x = add(t, Sx::Num(shift.clone()));
        roots.push(clean_complex(x));
    }
    roots
}

/// Simplifies a root expression: real values collapse to a real expression.
fn clean_complex(x: Sx) -> Sx {
    if let Some(z) = eval_c(&x) {
        if z.im.abs() < 1e-12 * z.re.abs().max(1.0) {
            // numerically real: present it through the simplified exact form if possible
            let s = expand(&x);
            if let Some(zs) = eval_c(&s) {
                if (zs - z).norm() < 1e-9 && !s.has_sym("§") {
                    return s;
                }
            }
        }
    }
    expand(&x)
}

fn quartic_roots(f: &poly::P) -> Vec<Sx> {
    let a = f[4].clone();
    let (b, c, d, e) = (&f[3] / &a, &f[2] / &a, &f[1] / &a, &f[0] / &a);
    // depressed: y^4 + p y^2 + q y + r with x = y - b/4
    let b2 = &b * &b;
    let p = &c - rat_i(3) * &b2 / rat_i(8);
    let q = &d - &b * &c / rat_i(2) + &b2 * &b / rat_i(8);
    let r = &e - &b * &d / rat_i(4) + &b2 * &c / rat_i(16) - rat_i(3) * &b2 * &b2 / rat_i(256);
    let shift = Sx::Num(-&b / rat_i(4));
    let mut ys: Vec<Sx> = Vec::new();
    if q.is_zero() {
        // biquadratic: y^2 = (-p ± sqrt(p^2 - 4r))/2
        let disc = &p * &p - rat_i(4) * &r;
        let sd = sqrt_sx(&Sx::Num(disc));
        for sgn in [-1, 1] {
            let z = div(add(Sx::Num(-p.clone()), mul(int(sgn), sd.clone())), int(2));
            ys.push(sqrt_sx_general(&z));
            ys.push(neg(sqrt_sx_general(&z)));
        }
    } else {
        // resolvent: 8 m^3 + 8 p m^2 + (2p^2 - 8 r) m - q^2 = 0
        let res = vec![-(&q * &q), rat_i(2) * &p * &p - rat_i(8) * &r, rat_i(8) * &p, rat_i(8)];
        let res = poly::trim(res);
        let (_, rf) = poly::primitive(&res);
        let mut m: Option<Sx> = None;
        for rt in poly::rational_roots(&rf) {
            if !rt.is_zero() {
                m = Some(Sx::Num(rt));
                break;
            }
        }
        let m = match m {
            Some(m) => m,
            None => {
                let cands = cubic_roots(&rf);
                cands
                    .into_iter()
                    .find(|c| eval_c(c).is_some_and(|z| z.im.abs() < 1e-9 && z.re > 1e-9))
                    .unwrap_or_else(|| float(poly::roots_numeric(&rf).iter().map(|z| z.re).fold(f64::MIN, f64::max)))
            }
        };
        let s = sqrt_general(&mul(int(2), m.clone()));
        let qq = Sx::Num(q.clone());
        for s1 in [1, -1] {
            // y^2 + (p/2 + m) = s1*(s y - q/(2 s))... roots of y^2 - s1 s y + (p/2 + m + s1 q/(2 s))
            let bq = mul(int(-s1), s.clone());
            let cq = add(add(Sx::Num(&p / rat_i(2)), m.clone()), mul(int(s1), div(qq.clone(), mul(int(2), s.clone()))));
            let disc = sub(pow(bq.clone(), int(2)), mul(int(4), cq));
            let sd = sqrt_sx_general(&expand(&disc));
            ys.push(div(sub(neg(bq.clone()), sd.clone()), int(2)));
            ys.push(div(add(neg(bq), sd), int(2)));
        }
    }
    ys.into_iter().map(|y| clean_complex(add(y, shift.clone()))).collect()
}

fn sqrt_general(e: &Sx) -> Sx {
    sqrt_sx(&simplify(e))
}

fn sqrt_sx_general(e: &Sx) -> Sx {
    sqrt_sx(&simplify(e))
}

fn drop_denominator_roots(roots: Vec<Sx>, den: &Sx, x: &Sx) -> Vec<Sx> {
    if den.is_one() || !den.depends(x) {
        return roots;
    }
    roots.into_iter().filter(|r| !is_zero_expr(&subs(den, x, r))).collect()
}

/// Inverse of an atom `a(x) = value` for a single outer function.
fn invert_atom(atom: &Sx, value: &Sx, x: &Sx) -> Option<Vec<Sx>> {
    match atom {
        Sx::Fun(n, a) if a.len() == 1 => {
            let inner = &a[0];
            let sols_u: Vec<Sx> = match n.as_str() {
                "exp" => vec![func("ln", vec![value.clone()])],
                "ln" => vec![func("exp", vec![value.clone()])],
                "sin" => vec![func("asin", vec![value.clone()])],
                "cos" => vec![func("acos", vec![value.clone()])],
                "tan" => vec![func("atan", vec![value.clone()])],
                "sinh" => vec![func("asinh", vec![value.clone()])],
                "cosh" => vec![func("acosh", vec![value.clone()])],
                "tanh" => vec![func("atanh", vec![value.clone()])],
                "asin" => vec![func("sin", vec![value.clone()])],
                "acos" => vec![func("cos", vec![value.clone()])],
                "atan" => vec![func("tan", vec![value.clone()])],
                "abs" => vec![value.clone(), neg(value.clone())],
                _ => return None,
            };
            let mut out = Vec::new();
            for u in sols_u {
                let mut note = None;
                out.extend(solve_poly_in(&sub(inner.clone(), u), x, &mut note)?);
            }
            Some(out)
        }
        Sx::Pow(b, p) if !p.depends(x) => {
            // b^p = value  ->  b = value^(1/p) (principal), plus the other real root for even integer p
            let mut bs = vec![pow(value.clone(), pow((**p).clone(), int(-1)))];
            if let Some(k) = p.as_int() {
                if k % 2 == 0 {
                    bs.push(neg(bs[0].clone()));
                }
            }
            let mut out = Vec::new();
            for bv in bs {
                let mut note = None;
                out.extend(solve_poly_in(&sub((**b).clone(), bv), x, &mut note)?);
            }
            Some(out)
        }
        Sx::Pow(b, p) if !b.depends(x) => {
            // b^p = value -> p = ln(value)/ln(b)
            let rhs = log_ratio(value, b);
            let mut note = None;
            solve_poly_in(&sub((**p).clone(), rhs), x, &mut note)
        }
        _ => None,
    }
}

/// `ln(v)/ln(b)`, exact when `v` is a power of `b`.
fn log_ratio(v: &Sx, b: &Sx) -> Sx {
    if let (Sx::Num(vr), Sx::Num(br)) = (v, b) {
        if br.is_positive() && !br.is_one() && vr.is_positive() {
            for k in 1..=64i64 {
                for sign in [1, -1] {
                    if let Some(Sx::Num(p)) = rat_pow_exact(br, sign * k) {
                        if p == *vr {
                            return int(sign * k);
                        }
                    }
                }
            }
        }
    }
    div(func("ln", vec![v.clone()]), func("ln", vec![b.clone()]))
}

fn rat_pow_exact(b: &Rat, k: i64) -> Option<Sx> {
    Some(pow(Sx::Num(b.clone()), int(k))).filter(|r| matches!(r, Sx::Num(_)))
}

/// Solves `e = 0` for `x` when `e` is a polynomial (or rational) in `x`.
fn solve_poly_in(e: &Sx, x: &Sx, note: &mut Option<String>) -> Option<Vec<Sx>> {
    let t = together(e);
    let (n, d) = num_den(&t);
    let cs = poly_in(&n, x)?;
    if cs.iter().all(|c| c.is_zero()) {
        return None;
    }
    let roots = poly_roots(&cs, note)?;
    Some(drop_denominator_roots(roots, &d, x))
}

fn numeric_real_roots(f: &Sx, x: &Sx) -> Vec<f64> {
    let lo = -100.0;
    let hi = 100.0;
    let n = 4000;
    let eval = |t: f64| eval_f64(&subs(f, x, &float(t)));
    let mut roots: Vec<f64> = Vec::new();
    let mut prev: Option<(f64, f64)> = None;
    for k in 0..=n {
        let t = lo + (hi - lo) * k as f64 / n as f64;
        let v = eval(t);
        if let (Some((pt, pv)), Some(v)) = (prev, v) {
            if pv == 0.0 {
                if !roots.iter().any(|r| (r - pt).abs() < 1e-9) {
                    roots.push(pt);
                }
            } else if pv.signum() != v.signum() && v.is_finite() && pv.is_finite() {
                let (mut a, mut b, mut fa) = (pt, t, pv);
                for _ in 0..200 {
                    let m = 0.5 * (a + b);
                    match eval(m) {
                        Some(fm) => {
                            if fm == 0.0 {
                                a = m;
                                b = m;
                                break;
                            }
                            if fa.signum() == fm.signum() {
                                a = m;
                                fa = fm;
                            } else {
                                b = m;
                            }
                        }
                        None => break,
                    }
                }
                let r = 0.5 * (a + b);
                // reject poles (|f| stays large)
                if eval(r).is_some_and(|fv| fv.abs() < 1e-6) && !roots.iter().any(|q| (q - r).abs() < 1e-7) {
                    roots.push(r);
                }
            }
        }
        if v.is_some() {
            prev = v.map(|vv| (t, vv));
        } else {
            prev = None;
        }
    }
    roots
}

fn clean_float(x: f64) -> Sx {
    float(x)
}

/// Solves a single equation for `x`.
pub fn solve_one(eq: &Sx, x: &Sx) -> Result<Solved, String> {
    let e = equation_zero(eq);
    let mut note = None;
    if !e.depends(x) {
        return if is_zero_expr(&e) { Err("уравнение выполняется при любом значении переменной".into()) } else { Ok(Solved { tuples: Vec::new(), note: None }) };
    }
    let floats = contains_float(&e);
    let e = if floats { rationalize(&e) } else { e };
    let mut roots: Option<Vec<Sx>> = solve_poly_in(&e, x, &mut note);
    if roots.is_none() {
        // one outer atom: f(x) = g with g independent of x
        let mut at = Atoms::default();
        let ab = abstract_atoms(&together(&e), &mut at);
        let (n, d) = num_den(&ab);
        let _ = d;
        let cands: Vec<(Sx, Sx)> = at.pairs.iter().filter(|(_, o)| o.depends(x)).cloned().collect();
        if cands.len() == 1 && !n.depends(x) {
            let (ph, orig) = &cands[0];
            if let Some(cs) = poly_in(&n, ph) {
                if let Some(us) = poly_roots(&cs, &mut note) {
                    let mut all = Vec::new();
                    let mut ok = true;
                    for u in us {
                        match invert_atom(orig, &u, x) {
                            Some(v) => all.extend(v),
                            None => {
                                ok = false;
                                break;
                            }
                        }
                    }
                    if ok {
                        roots = Some(all);
                    }
                }
            }
        }
    }
    if roots.is_none() {
        // numeric fallback
        if e.free_symbols().iter().all(|s| Sx::Sym(s.clone()) == *x) {
            let rs = numeric_real_roots(&e, x);
            note = Some("уравнение решено численно, найдены вещественные корни на отрезке [-100; 100]".into());
            roots = Some(rs.into_iter().map(clean_float).collect());
        } else {
            return Err("не удалось решить уравнение в символьном виде".into());
        }
    }
    let mut rs = roots.unwrap();
    if floats {
        rs = rs.into_iter().map(|r| floatify(&r, 17)).collect();
    }
    Ok(Solved { tuples: sort_roots(rs).into_iter().map(|r| vec![r]).collect(), note })
}

/// Solves a system by elimination.
pub fn solve_system(eqs: &[Sx], vars: &[Sx]) -> Result<Solved, String> {
    let zs: Vec<Sx> = eqs.iter().map(|e| equation_zero(e)).collect();
    let mut note = None;
    let floats = zs.iter().any(contains_float);
    let zs: Vec<Sx> = if floats { zs.iter().map(rationalize).collect() } else { zs };
    let sols = elim(zs, vars.to_vec(), &mut note, 0)?;
    // order the columns as `vars`
    let mut out = Vec::new();
    for s in sols {
        let row: Vec<Sx> = vars
            .iter()
            .map(|v| s.iter().find(|(k, _)| k == v).map(|(_, val)| if floats { floatify(val, 17) } else { val.clone() }).unwrap_or_else(|| v.clone()))
            .collect();
        out.push(row);
    }
    Ok(Solved { tuples: out, note })
}

type Assign = Vec<(Sx, Sx)>;

fn elim(eqs: Vec<Sx>, vars: Vec<Sx>, note: &mut Option<String>, depth: u32) -> Result<Vec<Assign>, String> {
    if depth > 12 {
        return Err("не удалось решить систему".into());
    }
    let eqs: Vec<Sx> = eqs.into_iter().map(|e| simplify(&expand(&together(&e)))).filter(|e| !e.is_zero()).collect();
    if vars.is_empty() {
        // all remaining equations must hold
        return if eqs.iter().all(is_zero_expr) { Ok(vec![Vec::new()]) } else { Ok(Vec::new()) };
    }
    if eqs.is_empty() {
        // free variables stay symbolic
        return Ok(vec![Vec::new()]);
    }
    // an equation in a single unknown
    for (i, e) in eqs.iter().enumerate() {
        let used: Vec<&Sx> = vars.iter().filter(|v| e.depends(v)).collect();
        if used.len() == 1 {
            let v = used[0].clone();
            let sol = solve_one(e, &v)?;
            if sol.note.is_some() {
                *note = sol.note.clone();
            }
            let mut out = Vec::new();
            for t in sol.tuples {
                let val = t[0].clone();
                let rest: Vec<Sx> = eqs.iter().enumerate().filter(|(j, _)| *j != i).map(|(_, q)| subs(q, &v, &val)).collect();
                let rv: Vec<Sx> = vars.iter().filter(|w| **w != v).cloned().collect();
                for mut a in elim(rest, rv, note, depth + 1)? {
                    a.push((v.clone(), subs_many(&val, &a.clone())));
                    out.push(a);
                }
            }
            return Ok(out);
        }
    }
    // an equation linear in some variable
    for (i, e) in eqs.iter().enumerate() {
        for v in &vars {
            if !e.depends(v) {
                continue;
            }
            if let Some(cs) = poly_in(e, v) {
                if cs.len() == 2 && !cs[1].is_zero() && !is_zero_expr(&cs[1]) {
                    let val = simplify(&div(neg(cs[0].clone()), cs[1].clone()));
                    let rest: Vec<Sx> = eqs.iter().enumerate().filter(|(j, _)| *j != i).map(|(_, q)| subs(q, v, &val)).collect();
                    let rv: Vec<Sx> = vars.iter().filter(|w| *w != v).cloned().collect();
                    let mut out = Vec::new();
                    for mut a in elim(rest, rv, note, depth + 1)? {
                        let full = subs_many(&val, &a);
                        a.push((v.clone(), simplify(&full)));
                        out.push(a);
                    }
                    return Ok(out);
                }
            }
        }
    }
    Err("не удалось решить систему в символьном виде".into())
}

/// Float polynomial roots helper for callers.
pub fn _unused(_: &Rat) -> bool {
    Rat::zero().is_one()
}
