//! Mathcad extension pack functions that ship with Mathcad 15 (`PolyLookup`, …).
//!
//! These are tables, not algorithms: Mathcad evaluates them by looking the arguments up.

use crate::value::{EResult, Matrix, Num, StrMatrix, Value};
use num_complex::Complex64 as C64;
use std::cell::Cell;
use std::rc::Rc;

/// One uniform polyhedron: its name, its dual and its Wythoff symbol.
struct Poly {
    name: &'static str,
    dual: &'static str,
    wythoff: &'static str,
}

/// The 80 uniform polyhedra in Mathcad's order (1 = pentagonal prism, 80 = great
/// dirhombicosidodecahedron). The entries are the values Mathcad 15 stores for
/// `PolyLookup(1) … PolyLookup(80)`.
static UNIFORM: [Poly; 80] = [
    Poly { name: "pentagonal prism", dual: "pentagonal dipyramid", wythoff: "2 5|2" },
    Poly { name: "pentagonal antiprism", dual: "pentagonal deltohedron", wythoff: "|2 2 5" },
    Poly { name: "pentagrammic prism", dual: "pentagrammic dipyramid", wythoff: "2 5/2|2" },
    Poly { name: "pentagrammic antiprism", dual: "pentagrammic deltohedron", wythoff: "|2 2 5/2" },
    Poly { name: "pentagrammic crossed antiprism", dual: "pentagrammic concave deltohedron", wythoff: "|2 2 5/3" },
    Poly { name: "tetrahedron", dual: "tetrahedron", wythoff: "3|2 3" },
    Poly { name: "truncated tetrahedron", dual: "triakistetrahedron", wythoff: "2 3|3" },
    Poly { name: "octahemioctahedron", dual: "octahemioctacron", wythoff: "3/2 3|3" },
    Poly { name: "tetrahemihexahedron", dual: "tetrahemihexacron", wythoff: "3/2 3|2" },
    Poly { name: "octahedron", dual: "cube", wythoff: "4|2 3" },
    Poly { name: "cube", dual: "octahedron", wythoff: "3|2 4" },
    Poly { name: "cuboctahedron", dual: "rhombic dodecahedron", wythoff: "2|3 4" },
    Poly { name: "truncated octahedron", dual: "tetrakishexahedron", wythoff: "2 4|3" },
    Poly { name: "truncated cube", dual: "triakisoctahedron", wythoff: "2 3|4" },
    Poly { name: "rhombicuboctahedron", dual: "deltoidal icositetrahedron", wythoff: "3 4|2" },
    Poly { name: "truncated cuboctahedron", dual: "disdyakisdodecahedron", wythoff: "2 3 4|" },
    Poly { name: "snub cube", dual: "pentagonal icositetrahedron", wythoff: "|2 3 4" },
    Poly { name: "small cubicuboctahedron", dual: "small hexacronic icositetrahedron", wythoff: "3/2 4|4" },
    Poly { name: "great cubicuboctahedron", dual: "great hexacronic icositetrahedron", wythoff: "3 4|4/3" },
    Poly { name: "cubohemioctahedron", dual: "hexahemioctacron", wythoff: "4/3 4|3" },
    Poly { name: "cubitruncated cuboctahedron", dual: "tetradyakishexahedron", wythoff: "4/3 3 4|" },
    Poly { name: "great rhombicuboctahedron", dual: "great deltoidal icositetrahedron", wythoff: "3/2 4|2" },
    Poly { name: "small rhombihexahedron", dual: "small rhombihexacron", wythoff: "3/2 2 4|" },
    Poly { name: "stellated truncated hexahedron", dual: "great triakisoctahedron", wythoff: "2 3|4/3" },
    Poly { name: "great truncated cuboctahedron", dual: "great disdyakisdodecahedron", wythoff: "4/3 2 3|" },
    Poly { name: "great rhombihexahedron", dual: "great rhombihexacron", wythoff: "4/3 3/2 2|" },
    Poly { name: "icosahedron", dual: "dodecahedron", wythoff: "5|2 3" },
    Poly { name: "dodecahedron", dual: "icosahedron", wythoff: "3|2 5" },
    Poly { name: "icosidodecahedron", dual: "rhombic triacontahedron", wythoff: "2|3 5" },
    Poly { name: "truncated icosahedron", dual: "pentakisdodecahedron", wythoff: "2 5|3" },
    Poly { name: "truncated dodecahedron", dual: "triakisicosahedron", wythoff: "2 3|5" },
    Poly { name: "rhombicosidodecahedron", dual: "deltoidal hexecontahedron", wythoff: "3 5|2" },
    Poly { name: "truncated icosidodecahedron", dual: "disdyakistriacontahedron", wythoff: "2 3 5|" },
    Poly { name: "snub dodecahedron", dual: "pentagonal hexecontahedron", wythoff: "|2 3 5" },
    Poly { name: "small ditrigonal icosidodecahedron", dual: "small triambic icosahedron", wythoff: "3|5/2 3" },
    Poly { name: "small icosicosidodecahedron", dual: "small icosacronic hexecontahedron", wythoff: "5/2 3|3" },
    Poly { name: "small snub icosicosidodecahedron", dual: "small hexagonal hexecontahedron", wythoff: "|5/2 3 3" },
    Poly { name: "small dodecicosidodecahedron", dual: "small dodecacronic hexecontahedron", wythoff: "3/2 5|5" },
    Poly { name: "small stellated dodecahedron", dual: "great dodecahedron", wythoff: "5|2 5/2" },
    Poly { name: "great dodecahedron", dual: "small stellated dodecahedron", wythoff: "5/2|2 5" },
    Poly { name: "great dodecadodecahedron", dual: "medial rhombic triacontahedron", wythoff: "2|5/2 5" },
    Poly { name: "truncated great dodecahedron", dual: "small stellapentakisdodecahedron", wythoff: "2 5/2|5" },
    Poly { name: "rhombidodecadodecahedron", dual: "medial deltoidal hexecontahedron", wythoff: "5/2 5|2" },
    Poly { name: "small rhombidodecahedron", dual: "small rhombidodecacron", wythoff: "2 5/2 5|" },
    Poly { name: "snub dodecadodecahedron", dual: "medial pentagonal hexecontahedron", wythoff: "|2 5/2 5" },
    Poly { name: "ditrigonal dodecadodecahedron", dual: "medial triambic icosahedron", wythoff: "3|5/3 5" },
    Poly { name: "great ditrigonal dodecicosidodecahedron", dual: "great ditrigonal dodecacronic hexecontahedron", wythoff: "3 5|5/3" },
    Poly { name: "small ditrigonal dodecicosidodecahedron", dual: "small ditrigonal dodecacronic hexecontahedron", wythoff: "5/3 3|5" },
    Poly { name: "icosidodecadodecahedron", dual: "medial icosacronic hexecontahedron", wythoff: "5/3 5|3" },
    Poly { name: "icositruncated dodecadodecahedron", dual: "tridyakisicosahedron", wythoff: "5/3 3 5|" },
    Poly { name: "snub icosidodecadodecahedron", dual: "medial hexagonal hexecontahedron", wythoff: "|5/3 3 5" },
    Poly { name: "great ditrigonal icosidodecahedron", dual: "great triambic icosahedron", wythoff: "3/2|3 5" },
    Poly { name: "great icosicosidodecahedron", dual: "great icosacronic hexecontahedron", wythoff: "3/2 5|3" },
    Poly { name: "small icosihemidodecahedron", dual: "small icosihemidodecacron", wythoff: "3/2 3|5" },
    Poly { name: "small dodecicosahedron", dual: "small dodecicosacron", wythoff: "3/2 3 5|" },
    Poly { name: "small dodecahemidodecahedron", dual: "small dodecahemidodecacron", wythoff: "5/4 5|5" },
    Poly { name: "great stellated dodecahedron", dual: "great icosahedron", wythoff: "3|2 5/2" },
    Poly { name: "great icosahedron", dual: "great stellated dodecahedron", wythoff: "5/2|2 3" },
    Poly { name: "great icosidodecahedron", dual: "great rhombic triacontahedron", wythoff: "2|5/2 3" },
    Poly { name: "great truncated icosahedron", dual: "great stellapentakisdodecahedron", wythoff: "2 5/2|3" },
    Poly { name: "rhombicosahedron", dual: "rhombicosacron", wythoff: "2 5/2 3|" },
    Poly { name: "great snub icosidodecahedron", dual: "great pentagonal hexecontahedron", wythoff: "|2 5/2 3" },
    Poly { name: "small stellated truncated dodecahedron", dual: "great pentakisdodekahedron", wythoff: "2 5|5/3" },
    Poly { name: "truncated dodecadodecahedron", dual: "medial disdyakistriacontahedron", wythoff: "5/3 2 5|" },
    Poly { name: "inverted snub dodecadodecahedron", dual: "medial inverted pentagonal hexecontahedron", wythoff: "|5/3 2 5" },
    Poly { name: "great dodecicosidodecahedron", dual: "great dodecacronic hexecontahedron", wythoff: "5/2 3|5/3" },
    Poly { name: "small dodecahemicosahedron", dual: "small dodecahemicosacron", wythoff: "5/3 5/2|3" },
    Poly { name: "great dodecicosahedron", dual: "great dodecicosacron", wythoff: "5/3 5/2 3|" },
    Poly { name: "great snub dodecicosidodecahedron", dual: "great hexagonal hexecontahedron", wythoff: "|5/3 5/2 3" },
    Poly { name: "great dodecahemicosahedron", dual: "great dodecahemicosacron", wythoff: "5/4 5|3" },
    Poly { name: "great stellated truncated dodecahedron", dual: "great triakisicosahedron", wythoff: "2 3|5/3" },
    Poly { name: "great rhombicosidodecahedron", dual: "great deltoidal hexecontahedron", wythoff: "5/3 3|2" },
    Poly { name: "great truncated icosidodecahedron", dual: "great disdyakistriacontahedron", wythoff: "5/3 2 3|" },
    Poly { name: "great inverted snub icosidodecahedron", dual: "great inverted pentagonal hexecontahedron", wythoff: "|5/3 2 3" },
    Poly { name: "great dodecahemidodecahedron", dual: "great dodecahemidodecacron", wythoff: "5/3 5/2|5/3" },
    Poly { name: "great icosihemidodecahedron", dual: "great icosihemidodecacron", wythoff: "3/2 3|5/3" },
    Poly { name: "small retrosnub icosicosidodecahedron", dual: "small hexagrammic hexecontahedron", wythoff: "|3/2 3/2 5/2" },
    Poly { name: "great rhombidodecahedron", dual: "great rhombidodecacron", wythoff: "3/2 5/3 2|" },
    Poly { name: "great retrosnub icosidodecahedron", dual: "great pentagrammic hexecontahedron", wythoff: "|3/2 5/3 2" },
    Poly { name: "great dirhombicosidodecahedron", dual: "great dirhombicosidodecacron", wythoff: "|3/2 5/3 3 5/2" },];

/// `PolyLookup(i)` — the uniform polyhedra table: a 3×1 matrix of strings
/// `[name; dual; Wythoff symbol]`, `i` from 1 to 80.
pub fn poly_lookup(args: &[Value]) -> EResult<Value> {
    if args.len() != 1 {
        return Err("PolyLookup принимает один аргумент — номер многогранника".into());
    }
    let i = args[0].as_int()?;
    let p = usize::try_from(i)
        .ok()
        .and_then(|i| i.checked_sub(1))
        .and_then(|i| UNIFORM.get(i))
        .ok_or_else(|| format!("PolyLookup: номер {i} вне таблицы (1…80)"))?;
    let data: Vec<Rc<str>> = vec![p.name.into(), p.dual.into(), p.wythoff.into()];
    Ok(Value::StrMat(StrMatrix::column(data)))
}

// ---------------------------------------------------------------------------
// T runcation and roundoff with a threshold (Mathcad's Floor / Ceil / Trunc / Round)

/// Element-wise `f(z / t) * t`: the number rounded down (up, toward zero, to nearest) to a
/// multiple of the threshold `t`, which must have the same dimension as `z`.
pub fn round_to(v: &Value, t: &Value, f: impl Fn(f64) -> f64) -> EResult<Value> {
    let t = t.as_num()?;
    if t.v.norm() == 0.0 {
        return Err("порог не может быть нулём".into());
    }
    map_nums(v, |n| {
        if n.dim != t.dim {
            return Err("аргумент и порог должны иметь одинаковые единицы измерения".into());
        }
        let q = C64::new(f(n.v.re / t.v.re), f(n.v.im / t.v.re));
        Ok(Num::new(C64::new(q.re * t.v.re, q.im * t.v.re), n.dim))
    })
}

fn map_nums(v: &Value, f: impl Fn(&Num) -> EResult<Num>) -> EResult<Value> {
    match v {
        Value::Num(n) => Ok(Value::Num(f(n)?)),
        Value::Matrix(m) => {
            let data = m.data.iter().map(&f).collect::<EResult<Vec<Num>>>()?;
            Ok(Value::Matrix(Matrix::new(m.rows, m.cols, data)))
        }
        Value::Range(r) => {
            let d: Vec<Num> = r.values().collect();
            let data = d.iter().map(&f).collect::<EResult<Vec<Num>>>()?;
            Ok(Value::Matrix(Matrix::new(data.len(), 1, data)))
        }
        other => Err(format!("ожидались числа, найдено: {}", other.type_name())),
    }
}

/// `mag(x)` — the magnitude of every element.
fn mag(args: &[Value]) -> EResult<Value> {
    one_arg("mag", args)?;
    map_nums(&args[0], |n| Ok(Num::new(C64::new(n.v.norm(), 0.0), n.dim)))
}

/// `phase(x)` — the argument of every element, in radians.
fn phase(args: &[Value]) -> EResult<Value> {
    one_arg("phase", args)?;
    map_nums(&args[0], |n| Ok(Num::real(n.v.arg())))
}

fn one_arg(name: &str, args: &[Value]) -> EResult<()> {
    if args.len() != 1 {
        return Err(format!("{name} принимает один аргумент"));
    }
    Ok(())
}

/// `phasecor(x)` — removes the 2π jumps of a phase vector.
fn phasecor(args: &[Value]) -> EResult<Value> {
    one_arg("phasecor", args)?;
    let m = match &args[0] {
        Value::Matrix(m) if m.is_vector() => m.clone(),
        Value::Num(n) => return Ok(Value::Num(*n)),
        other => return Err(format!("phasecor: ожидался вектор, найдено: {}", other.type_name())),
    };
    let two_pi = 2.0 * std::f64::consts::PI;
    let mut out = Vec::with_capacity(m.data.len());
    let mut prev = 0.0f64;
    for (k, n) in m.data.iter().enumerate() {
        let mut a = n.v.re;
        if k > 0 {
            while a - prev > std::f64::consts::PI {
                a -= two_pi;
            }
            while prev - a > std::f64::consts::PI {
                a += two_pi;
            }
        }
        prev = a;
        out.push(Num::real(a));
    }
    Ok(Value::Matrix(Matrix::new(out.len(), 1, out)))
}

/// `makeri(mag, phase)` — the complex vector with these magnitudes and phases.
fn makeri(args: &[Value]) -> EResult<Value> {
    if args.len() != 2 {
        return Err("makeri принимает два аргумента — амплитуды и фазы".into());
    }
    let (m, p) = (as_vec(&args[0], "makeri")?, as_vec(&args[1], "makeri")?);
    if m.len() != p.len() {
        return Err("makeri: векторы амплитуд и фаз должны быть одной длины".into());
    }
    let data: Vec<Num> = m
        .iter()
        .zip(&p)
        .map(|(a, f)| Num::new(C64::from_polar(a.v.norm(), f.v.re), a.dim))
        .collect();
    Ok(Value::Matrix(Matrix::new(data.len(), 1, data)))
}

fn as_vec<'a>(v: &'a Value, name: &str) -> EResult<Vec<&'a Num>> {
    match v {
        Value::Matrix(m) if m.is_vector() => Ok(m.data.iter().collect()),
        other => Err(format!("{name}: ожидался вектор, найдено: {}", other.type_name())),
    }
}

/// `∠(r, θ)` — the complex number with magnitude `r` and phase `θ` (Mathcad's polar form;
/// the corpus sheet sets degrees as the angular unit, and its stored results are degrees).
fn polar(args: &[Value]) -> EResult<Value> {
    if args.len() != 2 {
        return Err("∠(r, θ) принимает модуль и угол".into());
    }
    let r = args[0].as_num()?;
    let theta = args[1].as_num()?;
    let radians = theta.v.re * std::f64::consts::PI / 180.0;
    Ok(Value::Num(Num::new(
        C64::new(r.v.re * radians.cos(), r.v.re * radians.sin()),
        r.dim,
    )))
}

/// `DMS([degrees; minutes; seconds])` — the angle in radians.
fn dms(args: &[Value]) -> EResult<Value> {
    one_arg("DMS", args)?;
    let v = as_vec(&args[0], "DMS")?;
    if v.len() != 3 {
        return Err("DMS принимает вектор из трёх чисел: градусы, минуты, секунды".into());
    }
    let deg = v[0].v.re + v[1].v.re / 60.0 + v[2].v.re / 3600.0;
    Ok(Value::real(deg * std::f64::consts::PI / 180.0))
}

/// `Seed(n)` — sets the seed of the random number generator.
fn seed(args: &[Value]) -> EResult<Value> {
    one_arg("Seed", args)?;
    let n = args[0].as_int()?;
    crate::fitting::set_seed(n as u64);
    Ok(Value::real(n as f64))
}

thread_local! {
    /// The default FIR window (`setwindow`), 1 = rectangular.
    static WINDOW: Cell<i64> = const { Cell::new(1) };
}

/// `setwindow(w)` — sets and returns the default FIR window; `0` asks for the current one.
fn setwindow(args: &[Value]) -> EResult<Value> {
    one_arg("setwindow", args)?;
    let w = args[0].as_int()?;
    if w == 0 {
        return Ok(Value::real(WINDOW.with(|c| c.get()) as f64));
    }
    WINDOW.with(|c| c.set(w));
    Ok(Value::real(w as f64))
}

/// `SIUnitsOf(z)` — the SI base units of `z`, with the value 1 (or 1 itself when `z` is
/// dimensionless, as Mathcad documents).
fn si_units_of(args: &[Value]) -> EResult<Value> {
    one_arg("SIUnitsOf", args)?;
    let n = args[0].as_num()?;
    Ok(Value::Num(Num::new(C64::new(1.0, 0.0), n.dim)))
}

/// `stderr(X, Y)` — the standard error of the estimate of the linear regression of `Y` on `X`.
fn stderr(args: &[Value]) -> EResult<Value> {
    if args.len() != 2 {
        return Err("stderr принимает два аргумента: X и Y".into());
    }
    let x = as_vec(&args[0], "stderr")?;
    let y = as_vec(&args[1], "stderr")?;
    let n = x.len();
    if n != y.len() || n < 3 {
        return Err("stderr: векторы X и Y должны быть одной длины и содержать не меньше трёх точек".into());
    }
    let (mx, my) = (mean(x.iter().map(|n| n.v.re)), mean(y.iter().map(|n| n.v.re)));
    let sxx: f64 = x.iter().map(|n| (n.v.re - mx).powi(2)).sum();
    if sxx == 0.0 {
        return Err("stderr: все значения X одинаковы".into());
    }
    let sxy: f64 = x.iter().zip(&y).map(|(a, b)| (a.v.re - mx) * (b.v.re - my)).sum();
    let b = sxy / sxx;
    let a = my - b * mx;
    let sse: f64 = x.iter().zip(&y).map(|(p, q)| (q.v.re - (a + b * p.v.re)).powi(2)).sum();
    // the standard error is a residual in Y, so it carries Y's dimension
    let dim = y[0].dim;
    Ok(Value::Num(Num::new(C64::new((sse / (n - 2) as f64).sqrt(), 0.0), dim)))
}

fn mean(v: impl Iterator<Item = f64>) -> f64 {
    let (s, n) = v.fold((0.0, 0usize), |(s, n), x| (s + x, n + 1));
    if n == 0 {
        0.0
    } else {
        s / n as f64
    }
}

/// `Spear(x, y)` — Spearman's rank correlation: a 5×1 vector `[D; z; p_z; r_s; p_r]` with
/// the sum of squared rank differences, its standard score, the two-sided probability of that
/// score, the rank correlation and the two-sided probability of the correlation.
fn spear(args: &[Value]) -> EResult<Value> {
    if args.len() != 2 {
        return Err("Spear принимает два вектора".into());
    }
    let x = as_vec(&args[0], "Spear")?;
    let y = as_vec(&args[1], "Spear")?;
    if x.len() != y.len() || x.is_empty() {
        return Err("Spear: векторы должны быть одной длины".into());
    }
    let rx = ranks(x.iter().map(|n| n.v.re).collect());
    let ry = ranks(y.iter().map(|n| n.v.re).collect());
    let n = rx.len() as f64;
    let d: f64 = rx.iter().zip(&ry).map(|(a, b)| (a - b).powi(2)).sum();
    let (mx, my) = (mean(rx.iter().copied()), mean(ry.iter().copied()));
    let sxy: f64 = rx.iter().zip(&ry).map(|(a, b)| (a - mx) * (b - my)).sum();
    let sxx: f64 = rx.iter().map(|a| (a - mx).powi(2)).sum();
    let syy: f64 = ry.iter().map(|b| (b - my).powi(2)).sum();
    if sxx == 0.0 || syy == 0.0 {
        return Err("Spear: все значения в одном из векторов одинаковы".into());
    }
    let rho = sxy / (sxx * syy).sqrt();
    // Mathcad reports the standard score of D with a negative sign (both samples it ships
    // give -r_s*sqrt(n-1))
    let z = -rho * (n - 1.0).sqrt();
    let p_z = 2.0 * 0.5 * libm::erfc(z.abs() / std::f64::consts::SQRT_2);
    let p_r = if rho.abs() >= 1.0 {
        0.0
    } else {
        let t = rho * ((n - 2.0) / (1.0 - rho * rho)).sqrt();
        2.0 * crate::special::t_cdf(-t.abs(), n - 2.0)
    };
    let data = vec![d, z, p_z, rho, p_r];
    Ok(Value::Matrix(Matrix::new(5, 1, data.into_iter().map(Num::real).collect())))
}

/// `kendltau(x, y)` — Kendall's tau: `[t; z; p]` (the coefficient, its standard score and
/// the two-sided probability under no association).
fn kendltau(args: &[Value]) -> EResult<Value> {
    if args.len() != 2 {
        return Err("kendltau принимает два вектора".into());
    }
    let x = as_vec(&args[0], "kendltau")?;
    let y = as_vec(&args[1], "kendltau")?;
    let n = x.len();
    if n != y.len() || n < 3 {
        return Err("kendltau: векторы должны быть одной длины (не меньше трёх точек)".into());
    }
    let (mut conc, mut disc, mut tx, mut ty) = (0.0f64, 0.0f64, 0.0f64, 0.0f64);
    for i in 0..n {
        for j in (i + 1)..n {
            let dx = x[i].v.re - x[j].v.re;
            let dy = y[i].v.re - y[j].v.re;
            if dx == 0.0 && dy == 0.0 {
                continue;
            } else if dx == 0.0 {
                tx += 1.0;
            } else if dy == 0.0 {
                ty += 1.0;
            } else if (dx > 0.0) == (dy > 0.0) {
                conc += 1.0;
            } else {
                disc += 1.0;
            }
        }
    }
    let denom = ((conc + disc + ty) * (conc + disc + tx)).sqrt();
    if denom == 0.0 {
        return Err("kendltau: все пары связаны".into());
    }
    let tau = (conc - disc) / denom;
    let n = n as f64;
    let sd = (2.0 * (2.0 * n + 5.0) / (9.0 * n * (n - 1.0))).sqrt();
    let z = tau / sd;
    let p = libm::erfc(z.abs() / std::f64::consts::SQRT_2);
    Ok(Value::Matrix(Matrix::column(vec![Num::real(tau), Num::real(z), Num::real(p)])))
}

/// `kendltau2(M)` — Kendall's tau-b of a contingency table: `[tau; z; p]`.
fn kendltau2(args: &[Value]) -> EResult<Value> {
    let m = contingency(args, "kendltau2")?;
    let n: f64 = m.data.iter().map(|x| x.v.re).sum();
    if n < 3.0 {
        return Err("kendltau2: в таблице слишком мало наблюдений".into());
    }
    let row_tot: Vec<f64> = (0..m.rows).map(|r| (0..m.cols).map(|c| m.get(r, c).v.re).sum()).collect();
    let col_tot: Vec<f64> = (0..m.cols).map(|c| (0..m.rows).map(|r| m.get(r, c).v.re).sum()).collect();
    let (mut conc, mut disc) = (0.0, 0.0);
    for i in 0..m.rows {
        for j in 0..m.cols {
            for k in i + 1..m.rows {
                for l in 0..m.cols {
                    let pairs = m.get(i, j).v.re * m.get(k, l).v.re;
                    if l > j {
                        conc += pairs;
                    } else if l < j {
                        disc += pairs;
                    }
                }
            }
        }
    }
    let choose2 = |x: f64| x * (x - 1.0) / 2.0;
    let tx: f64 = row_tot.iter().map(|r| choose2(*r)).sum();
    let ty: f64 = col_tot.iter().map(|c| choose2(*c)).sum();
    let n0 = choose2(n);
    let denom = ((n0 - tx) * (n0 - ty)).sqrt();
    if denom == 0.0 {
        return Err("kendltau2: таблица вырождена".into());
    }
    let tau = (conc - disc) / denom;
    let sd = (2.0 * (2.0 * n + 5.0) / (9.0 * n * (n - 1.0))).sqrt();
    let z = tau / sd;
    let p = libm::erfc(z.abs() / std::f64::consts::SQRT_2);
    Ok(Value::Matrix(Matrix::column(vec![Num::real(tau), Num::real(z), Num::real(p)])))
}

fn contingency(args: &[Value], name: &str) -> EResult<Matrix> {
    if args.len() != 1 {
        return Err(format!("{name} принимает таблицу сопряжённости"));
    }
    let m = args[0].as_matrix()?.clone();
    if m.rows < 2 || m.cols < 2 {
        return Err(format!("{name}: таблица должна быть хотя бы 2×2"));
    }
    Ok(m)
}

/// `contingtbl(M)` — the statistics of a contingency table: `[χ²; df; p; V; C]`.
fn contin_tbl(args: &[Value]) -> EResult<Value> {
    let m = contingency(args, "contingtbl")?;
    let total: f64 = m.data.iter().map(|n| n.v.re).sum();
    if total == 0.0 {
        return Err("contingtbl: пустая таблица".into());
    }
    let row_tot: Vec<f64> = (0..m.rows).map(|r| (0..m.cols).map(|c| m.get(r, c).v.re).sum()).collect();
    let col_tot: Vec<f64> = (0..m.cols).map(|c| (0..m.rows).map(|r| m.get(r, c).v.re).sum()).collect();
    let mut chi2 = 0.0;
    for r in 0..m.rows {
        for c in 0..m.cols {
            let e = row_tot[r] * col_tot[c] / total;
            if e > 0.0 {
                let d = m.get(r, c).v.re - e;
                chi2 += d * d / e;
            }
        }
    }
    let df = ((m.rows - 1) * (m.cols - 1)) as f64;
    let p = 1.0 - crate::special::chisq_cdf(chi2, df);
    let k = (m.rows.min(m.cols) - 1) as f64;
    let v = (chi2 / (total * k)).sqrt();
    let c = (chi2 / (chi2 + total)).sqrt();
    Ok(Value::Matrix(Matrix::column(vec![
        Num::real(chi2),
        Num::real(df),
        Num::real(p),
        Num::real(v),
        Num::real(c),
    ])))
}

/// `Rank(x)` — the ranks of the elements (1 is the smallest, ties share their average rank).
fn rank(args: &[Value]) -> EResult<Value> {
    one_arg("Rank", args)?;
    let v = as_vec(&args[0], "Rank")?;
    let r = ranks(v.iter().map(|n| n.v.re).collect());
    Ok(Value::Matrix(Matrix::column(r.into_iter().map(Num::real).collect())))
}

/// Ranks (1 is the smallest), ties get the average rank.
fn ranks(v: Vec<f64>) -> Vec<f64> {
    let mut order: Vec<usize> = (0..v.len()).collect();
    order.sort_by(|a, b| v[*a].partial_cmp(&v[*b]).unwrap_or(std::cmp::Ordering::Equal));
    let mut r = vec![0.0; v.len()];
    let mut i = 0;
    while i < order.len() {
        let mut j = i;
        while j + 1 < order.len() && v[order[j + 1]] == v[order[i]] {
            j += 1;
        }
        let avg = (i + j) as f64 / 2.0 + 1.0;
        for k in i..=j {
            r[order[k]] = avg;
        }
        i = j + 1;
    }
    r
}

// ---------------------------------------------------------------------------
// Image Processing pack: thresholding and connected components

/// `binarize(M, t)` — an image with 1 where the pixel is at least `t` and 0 elsewhere.
fn binarize(args: &[Value]) -> EResult<Value> {
    if args.len() != 2 {
        return Err("binarize(M, порог) принимает изображение и порог".into());
    }
    let m = args[0].as_matrix()?;
    let t = args[1].as_num()?.v.re;
    let data = m.data.iter().map(|n| Num::real(if n.v.re >= t { 1.0 } else { 0.0 })).collect();
    Ok(Value::Matrix(Matrix::new(m.rows, m.cols, data)))
}

/// `concomp(M, con, fg)` — labels the connected components of the foreground value `fg`
/// (`con` = 4 or 8 neighbours); background becomes 0 and the components are numbered from 1.
fn concomp(args: &[Value]) -> EResult<Value> {
    if args.len() != 3 {
        return Err("concomp(M, связность, значение) принимает изображение, 4 или 8 и значение".into());
    }
    let m = args[0].as_matrix()?;
    let con = args[1].as_int()?;
    let fg = args[2].as_num()?.v.re;
    if con != 4 && con != 8 {
        return Err("concomp: связность бывает 4 или 8".into());
    }
    let (rows, cols) = (m.rows, m.cols);
    let mut labels = vec![0.0f64; rows * cols];
    let mut next = 0.0;
    for start in 0..rows * cols {
        if labels[start] != 0.0 || m.data[start].v.re != fg {
            continue;
        }
        next += 1.0;
        let mut stack = vec![start];
        labels[start] = next;
        while let Some(k) = stack.pop() {
            let (r, c) = ((k / cols) as i64, (k % cols) as i64);
            let mut step = |dr: i64, dc: i64| {
                let (nr, nc) = (r + dr, c + dc);
                if nr < 0 || nc < 0 || nr as usize >= rows || nc as usize >= cols {
                    return;
                }
                let n = nr as usize * cols + nc as usize;
                if labels[n] == 0.0 && m.data[n].v.re == fg {
                    labels[n] = next;
                    stack.push(n);
                }
            };
            step(-1, 0);
            step(1, 0);
            step(0, -1);
            step(0, 1);
            if con == 8 {
                step(-1, -1);
                step(-1, 1);
                step(1, -1);
                step(1, 1);
            }
        }
    }
    Ok(Value::Matrix(Matrix::new(rows, cols, labels.into_iter().map(Num::real).collect())))
}

/// Functions of the extension pack, called from [`crate::builtins::call`].
pub fn call(name: &str, args: &[Value]) -> Option<EResult<Value>> {
    match name {
        "PolyLookup" => Some(poly_lookup(args)),
        "mag" => Some(mag(args)),
        "phase" => Some(phase(args)),
        "phasecor" => Some(phasecor(args)),
        "makeri" => Some(makeri(args)),
        "DMS" => Some(dms(args)),
        "∠" => Some(polar(args)),
        "binarize" => Some(binarize(args)),
        "concomp" => Some(concomp(args)),
        "Seed" => Some(seed(args)),
        "setwindow" => Some(setwindow(args)),
        "SIUnitsOf" => Some(si_units_of(args)),
        "stderr" => Some(stderr(args)),
        "Spear" => Some(spear(args)),
        "Rank" => Some(rank(args)),
        "kendltau" => Some(kendltau(args)),
        "kendltau2" => Some(kendltau2(args)),
        "contingtbl" => Some(contin_tbl(args)),
        _ => None,
    }
}
