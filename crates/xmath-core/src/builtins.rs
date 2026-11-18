//! Built-in functions operating on evaluated arguments.

use crate::numeric;
use crate::units::Dim;
use crate::value::*;
use num_complex::Complex64 as C64;

/// Names of all built-in functions (used for highlighting and completion).
pub const NAMES: &[&str] = &[
    "sin", "cos", "tan", "cot", "sec", "csc", "asin", "acos", "atan", "acot", "atan2", "sinh", "cosh", "tanh",
    "asinh", "acosh", "atanh", "exp", "ln", "log", "sqrt", "nthroot", "abs", "arg", "Re", "Im", "conj", "xor", "floor",
    "ceil", "round", "trunc", "mod", "sign", "signum", "max", "min", "mean", "median", "stdev", "Stdev", "var",
    "Var", "sum", "prod", "length", "rows", "cols", "last", "identity", "transpose", "det", "tr", "lsolve",
    "augment", "stack", "submatrix", "cross", "norme", "sort", "reverse", "linterp", "polyroots", "factorial",
    "gamma", "Γ", "erf", "erfc", "dnorm", "pnorm", "UnitsOf", "concat", "num2str", "strlen", "if", "integrate",
    "diff", "root", "matrix", "eigenvals", "eigenvecs", "eigenvec", "rank", "cholesky", "lu", "qr", "svd", "svds",
    "geninv", "rref", "norm1", "norm2", "normi", "cond2", "diag", "csort", "rsort", "lspline", "pspline", "cspline",
    "interp", "slope", "intercept", "line", "corr", "regress", "cvar", "skew", "kurt", "hist", "gmean", "hmean",
    "mode", "gcd", "lcm", "Φ", "heaviside", "δ", "kronecker", "sinc", "csgn", "qnorm", "dunif", "punif", "dexp",
    "pexp", "J0", "J1", "Jn", "Y0", "Y1", "Yn", "I0", "I1", "In", "K0", "K1", "Kn", "substr", "search", "str2num",
    "vec2str", "str2vec", "lookup", "match", "vlookup", "hlookup", "rkfixed", "Rkadapt", "expfit", "lgsfit",
    "logfit", "lnfit", "pwrfit", "sinfit", "linfit", "genfit", "medfit", "loess", "ksmooth", "medsmooth",
    "supsmooth", "predict", "rnd", "runif", "rnorm", "rexp", "rpois", "rbinom", "rgamma", "rchisq", "rbeta", "rt",
    "rlnorm", "rweibull", "rcauchy", "rlogis", "rgeom", "dbinom", "pbinom", "dpois", "ppois",
    "Find", "Minerr", "Maximize", "Minimize", "lim",
];

pub(crate) fn arity(name: &str, args: &[Value], n: usize) -> EResult<()> {
    if args.len() != n {
        return Err(format!("{name}: нужно аргументов: {n}"));
    }
    Ok(())
}

pub(crate) fn dimless(n: Num) -> EResult<C64> {
    if !n.dim.is_none() {
        return Err("аргумент должен быть безразмерным".into());
    }
    Ok(no_neg_zero(n.v))
}

/// `-4` is stored as `-4 - 0i`; drop the signed zero so complex functions
/// take the principal branch (sqrt(-4) = 2i, not -2i).
fn no_neg_zero(z: C64) -> C64 {
    if z.im == 0.0 {
        C64::new(z.re, 0.0)
    } else {
        z
    }
}

/// Applies a scalar function elementwise to scalars and matrices.
pub(crate) fn map1(v: &Value, f: impl Fn(Num) -> EResult<Num>) -> EResult<Value> {
    match v {
        Value::Num(n) => Ok(Value::Num(f(*n)?)),
        Value::Matrix(m) => Ok(Value::Matrix(m.map(f)?)),
        other => Err(format!("ожидалось число, найдено: {}", other.type_name())),
    }
}

fn complex_fn(v: &Value, f: impl Fn(C64) -> C64) -> EResult<Value> {
    map1(v, |n| {
        let r = f(dimless(n)?);
        Ok(Num::new(clean(r), Dim::NONE))
    })
}

/// Removes round-off imaginary parts produced by complex formulas on real input.
fn clean(z: C64) -> C64 {
    if z.im != 0.0 && z.im.abs() < 1e-15 * z.re.abs() {
        C64::new(z.re, 0.0)
    } else {
        z
    }
}

/// Real-only function; real input outside the domain falls back to complex.
fn real_or_complex(v: &Value, real: impl Fn(f64) -> f64, complex: impl Fn(C64) -> C64) -> EResult<Value> {
    map1(v, |n| {
        let z = dimless(n)?;
        if z.im == 0.0 {
            let r = real(z.re);
            if !r.is_nan() || z.re.is_nan() {
                return Ok(Num::real(r));
            }
        }
        Ok(Num::new(clean(complex(z)), Dim::NONE))
    })
}

pub(crate) fn real_fn(v: &Value, f: impl Fn(f64) -> f64) -> EResult<Value> {
    map1(v, |n| {
        let z = dimless(n)?;
        if z.im != 0.0 {
            return Err("аргумент должен быть вещественным".into());
        }
        Ok(Num::real(f(z.re)))
    })
}

/// Applies `f` to the real SI value, keeping the units.
fn real_keep_dim(v: &Value, f: impl Fn(f64) -> f64) -> EResult<Value> {
    map1(v, |n| Ok(Num::new(C64::new(f(n.v.re), f(n.v.im)), n.dim)))
}

/// All elements of scalars / matrices / ranges given as arguments.
pub(crate) fn flatten(args: &[Value]) -> EResult<Vec<Num>> {
    let mut out = Vec::new();
    for a in args {
        match a {
            Value::Num(n) => out.push(*n),
            Value::Matrix(m) => out.extend_from_slice(&m.data),
            Value::Range(r) => out.extend(r.values()),
            other => return Err(format!("ожидались числа, найдено: {}", other.type_name())),
        }
    }
    if out.is_empty() {
        return Err("нет значений".into());
    }
    Ok(out)
}

pub(crate) fn same_dim(v: &[Num]) -> EResult<Dim> {
    let d = v[0].dim;
    if v.iter().any(|n| n.dim != d) {
        return Err("значения имеют разные единицы измерения".into());
    }
    Ok(d)
}

fn extremum(args: &[Value], want_max: bool) -> EResult<Value> {
    let mut v = flatten(args)?;
    // a plain 0 is compatible with any units
    if let Some(d) = v.iter().map(|n| n.dim).find(|d| !d.is_none()) {
        for n in v.iter_mut() {
            if n.dim.is_none() && n.v == C64::new(0.0, 0.0) {
                n.dim = d;
            }
        }
    }
    let d = same_dim(&v)?;
    let mut best = v[0].v.re;
    for n in &v {
        if !n.is_real() {
            return Err("max и min требуют вещественных значений".into());
        }
        if (want_max && n.v.re > best) || (!want_max && n.v.re < best) {
            best = n.v.re;
        }
    }
    Ok(Value::Num(Num::with_dim(best, d)))
}

fn variance(args: &[Value], sample: bool) -> EResult<Num> {
    let v = flatten(args)?;
    let d = same_dim(&v)?;
    let n = v.len() as f64;
    if sample && v.len() < 2 {
        return Err("нужно не менее двух значений".into());
    }
    let mean: C64 = v.iter().map(|x| x.v).sum::<C64>() / n;
    let ss: f64 = v.iter().map(|x| (x.v - mean).norm_sqr()).sum();
    Ok(Num::with_dim(ss / if sample { n - 1.0 } else { n }, d.mul(d)))
}

fn sqrt_num(n: Num) -> EResult<Num> {
    let dim = n.dim.powf(0.5).ok_or("корень из этих единиц измерения представить нельзя")?;
    let z = no_neg_zero(n.v);
    let v = if z.im == 0.0 && z.re >= 0.0 { C64::new(z.re.sqrt(), 0.0) } else { z.sqrt() };
    Ok(Num::new(v, dim))
}

pub(crate) fn int_arg(v: &Value) -> EResult<i64> {
    v.as_int()
}

fn factorial(x: f64) -> EResult<f64> {
    if x < 0.0 && x.fract() == 0.0 {
        return Err("факториал отрицательного целого".into());
    }
    if x.fract() == 0.0 && x <= 170.0 {
        return Ok((1..=x as u64).map(|k| k as f64).product());
    }
    Ok(libm::tgamma(x + 1.0))
}

/// A vector argument (row vectors are accepted and returned as columns).
pub(crate) fn vector_arg(v: &Value, what: &str) -> EResult<Matrix> {
    match v {
        Value::Matrix(m) if m.cols == 1 => Ok(m.clone()),
        Value::Matrix(m) if m.rows == 1 => Ok(m.transpose()),
        Value::Range(r) => Ok(Matrix::column(r.values().collect())),
        _ => Err(format!("{what} должен быть вектором")),
    }
}

fn concat_matrices(args: &[Value], horizontal: bool) -> EResult<Value> {
    let mats = args.iter().map(|a| a.to_matrix()).collect::<EResult<Vec<_>>>()?;
    if mats.is_empty() {
        return Err("нужен хотя бы один аргумент".into());
    }
    if horizontal {
        let rows = mats[0].rows;
        if mats.iter().any(|m| m.rows != rows) {
            return Err("все аргументы должны иметь одинаковое число строк".into());
        }
        let cols = mats.iter().map(|m| m.cols).sum();
        let mut out = Matrix::zeros(rows, cols);
        let mut c0 = 0;
        for m in &mats {
            for i in 0..rows {
                for j in 0..m.cols {
                    out.set(i, c0 + j, m.get(i, j));
                }
            }
            c0 += m.cols;
        }
        Ok(Value::Matrix(out))
    } else {
        let cols = mats[0].cols;
        if mats.iter().any(|m| m.cols != cols) {
            return Err("все аргументы должны иметь одинаковое число столбцов".into());
        }
        let rows = mats.iter().map(|m| m.rows).sum();
        let mut data = Vec::new();
        for m in &mats {
            data.extend_from_slice(&m.data);
        }
        Ok(Value::Matrix(Matrix::new(rows, cols, data)))
    }
}

pub fn format_plain(n: Num) -> String {
    crate::format::format_num(n, &crate::format::FormatOptions::default()).to_string()
}

/// Calls built-in `name`. Returns `None` if there is no such built-in.
pub fn call(name: &str, args: &[Value], origin: i64) -> Option<EResult<Value>> {
    let one = |f: &dyn Fn(&Value) -> EResult<Value>| -> EResult<Value> {
        arity(name, args, 1)?;
        f(&args[0])
    };
    let r = match name {
        "sin" => one(&|v| complex_fn(v, |z| if z.im == 0.0 { C64::new(z.re.sin(), 0.0) } else { z.sin() })),
        "cos" => one(&|v| complex_fn(v, |z| if z.im == 0.0 { C64::new(z.re.cos(), 0.0) } else { z.cos() })),
        "tan" => one(&|v| complex_fn(v, |z| if z.im == 0.0 { C64::new(z.re.tan(), 0.0) } else { z.tan() })),
        "cot" => one(&|v| complex_fn(v, |z| 1.0 / z.tan())),
        "sec" => one(&|v| complex_fn(v, |z| 1.0 / z.cos())),
        "csc" => one(&|v| complex_fn(v, |z| 1.0 / z.sin())),
        "asin" => one(&|v| real_or_complex(v, f64::asin, |z| z.asin())),
        "acos" => one(&|v| real_or_complex(v, f64::acos, |z| z.acos())),
        "atan" => one(&|v| real_or_complex(v, f64::atan, |z| z.atan())),
        "acot" => one(&|v| real_or_complex(v, |x| (1.0 / x).atan(), |z| (1.0 / z).atan())),
        "sinh" => one(&|v| complex_fn(v, |z| z.sinh())),
        "cosh" => one(&|v| complex_fn(v, |z| z.cosh())),
        "tanh" => one(&|v| complex_fn(v, |z| z.tanh())),
        "asinh" => one(&|v| complex_fn(v, |z| z.asinh())),
        "acosh" => one(&|v| real_or_complex(v, f64::acosh, |z| z.acosh())),
        "atanh" => one(&|v| real_or_complex(v, f64::atanh, |z| z.atanh())),
        "exp" => one(&|v| complex_fn(v, |z| if z.im == 0.0 { C64::new(z.re.exp(), 0.0) } else { z.exp() })),
        "ln" => one(&|v| real_or_complex(v, |x| if x > 0.0 { x.ln() } else { f64::NAN }, |z| z.ln())),
        "log" => match args.len() {
            1 => real_or_complex(&args[0], |x| if x > 0.0 { x.log10() } else { f64::NAN }, |z| z.ln() / 10f64.ln()),
            2 => (|| {
                let b = dimless(args[1].as_num()?)?;
                complex_fn(&args[0], |z| z.ln() / b.ln())
            })(),
            _ => Err("log принимает 1 или 2 аргумента".into()),
        },
        "sqrt" => one(&|v| map1(v, sqrt_num)),
        "nthroot" => (|| {
            arity(name, args, 2)?;
            let n = int_arg(&args[1])?;
            if n == 0 {
                return Err("показатель корня не может быть нулём".into());
            }
            map1(&args[0], |x| {
                let dim = x.dim.powf(1.0 / n as f64).ok_or("такие единицы измерения представить нельзя")?;
                let v = if x.v.im == 0.0 && x.v.re < 0.0 && n % 2 != 0 {
                    C64::new(-(-x.v.re).powf(1.0 / n as f64), 0.0)
                } else {
                    cpow(x.v, C64::new(1.0 / n as f64, 0.0))
                };
                Ok(Num::new(v, dim))
            })
        })(),
        "abs" => one(&|v| map1(v, |n| Ok(Num::new(C64::new(n.v.norm(), 0.0), n.dim)))),
        "arg" => one(&|v| map1(v, |n| Ok(Num::real(n.v.arg())))),
        "Re" => one(&|v| map1(v, |n| Ok(Num::with_dim(n.v.re, n.dim)))),
        "Im" => one(&|v| map1(v, |n| Ok(Num::with_dim(n.v.im, n.dim)))),
        "conj" => one(&|v| map1(v, |n| Ok(Num::new(n.v.conj(), n.dim)))),
        "xor" => (|| {
            arity(name, args, 2)?;
            let f = |a: &Num, b: &Num| Num::real(((a.v != C64::new(0.0, 0.0)) != (b.v != C64::new(0.0, 0.0))) as u8 as f64);
            match (&args[0], &args[1]) {
                (Value::Num(a), Value::Num(b)) => Ok(Value::Num(f(a, b))),
                (Value::Matrix(m), Value::Num(b)) => Ok(Value::Matrix(Matrix::new(m.rows, m.cols, m.data.iter().map(|a| f(a, b)).collect()))),
                (Value::Num(a), Value::Matrix(m)) => Ok(Value::Matrix(Matrix::new(m.rows, m.cols, m.data.iter().map(|b| f(a, b)).collect()))),
                (Value::Matrix(a), Value::Matrix(b)) if a.rows == b.rows && a.cols == b.cols => {
                    Ok(Value::Matrix(Matrix::new(a.rows, a.cols, a.data.iter().zip(&b.data).map(|(x, y)| f(x, y)).collect())))
                }
                _ => Err("xor: аргументы должны быть числами или матрицами одного размера".into()),
            }
        })(),
        "atan2" => (|| {
            // Mathcad order: atan2(x, y) is the angle of the point (x, y)
            arity(name, args, 2)?;
            let x = args[0].as_num()?;
            let y = args[1].as_num()?;
            if x.dim != y.dim {
                return Err("Единицы измерения не совпадают".into());
            }
            Ok(Value::real(y.v.re.atan2(x.v.re)))
        })(),
        "floor" => one(&|v| real_keep_dim(v, f64::floor)),
        "ceil" => one(&|v| real_keep_dim(v, f64::ceil)),
        "trunc" => one(&|v| real_keep_dim(v, f64::trunc)),
        "round" => match args.len() {
            1 => real_keep_dim(&args[0], f64::round),
            2 => (|| {
                let n = int_arg(&args[1])? as i32;
                let p = 10f64.powi(n);
                real_keep_dim(&args[0], |x| (x * p).round() / p)
            })(),
            _ => Err("round принимает 1 или 2 аргумента".into()),
        },
        "mod" => (|| {
            arity(name, args, 2)?;
            let x = args[0].as_num()?;
            let y = args[1].as_num()?;
            if x.dim != y.dim {
                return Err("Единицы измерения не совпадают".into());
            }
            if y.v.re == 0.0 {
                return Err("Деление на ноль".into());
            }
            Ok(Value::Num(Num::with_dim(x.v.re % y.v.re, x.dim)))
        })(),
        "sign" => one(&|v| map1(v, |n| Ok(Num::real(if n.v.re > 0.0 { 1.0 } else if n.v.re < 0.0 { -1.0 } else { 0.0 })))),
        // signum(z) = z/|z| (1 for zero), as in Mathcad
        "signum" => one(&|v| {
            map1(v, |n| {
                let r = n.v.norm();
                Ok(if r == 0.0 { Num::real(1.0) } else { Num::new(n.v / r, Dim::NONE) })
            })
        }),
        "max" => extremum(args, true),
        "min" => extremum(args, false),
        "mean" => (|| {
            let v = flatten(args)?;
            let d = same_dim(&v)?;
            Ok(Value::Num(Num::new(v.iter().map(|x| x.v).sum::<C64>() / v.len() as f64, d)))
        })(),
        "median" => (|| {
            let v = flatten(args)?;
            let d = same_dim(&v)?;
            let mut xs: Vec<f64> = v.iter().map(|x| x.v.re).collect();
            xs.sort_by(f64::total_cmp);
            let n = xs.len();
            let m = if n % 2 == 1 { xs[n / 2] } else { 0.5 * (xs[n / 2 - 1] + xs[n / 2]) };
            Ok(Value::Num(Num::with_dim(m, d)))
        })(),
        "var" => variance(args, false).map(Value::Num),
        "Var" => variance(args, true).map(Value::Num),
        "stdev" => variance(args, false).and_then(sqrt_num).map(Value::Num),
        "Stdev" => variance(args, true).and_then(sqrt_num).map(Value::Num),
        "sum" | "prod" => (|| {
            let v = flatten(args)?;
            let mut acc = v[0];
            for n in &v[1..] {
                acc = if name == "sum" { num_add(acc, *n)? } else { num_mul(acc, *n) };
            }
            Ok(Value::Num(acc))
        })(),
        "length" => one(&|v| match v {
            Value::Matrix(m) if m.is_vector() => Ok(Value::real(m.data.len() as f64)),
            Value::Range(r) => Ok(Value::real(r.len() as f64)),
            Value::Str(s) => Ok(Value::real(s.chars().count() as f64)),
            _ => Err("ожидался вектор".into()),
        }),
        "rows" => one(&|v| Ok(Value::real(v.to_matrix()?.rows as f64))),
        "cols" => one(&|v| Ok(Value::real(v.to_matrix()?.cols as f64))),
        "last" => one(&|v| {
            let m = v.to_matrix()?;
            Ok(Value::real((m.data.len() as i64 - 1 + origin) as f64))
        }),
        "identity" => one(&|v| {
            let n = int_arg(v)?;
            if n <= 0 || n > 10_000 {
                return Err("недопустимый размер".into());
            }
            Ok(Value::Matrix(identity(n as usize)))
        }),
        "transpose" => one(&|v| Ok(Value::Matrix(v.to_matrix()?.transpose()))),
        "det" => one(&|v| Ok(Value::Num(det(&v.to_matrix()?)?))),
        "tr" => one(&|v| {
            let m = v.to_matrix()?;
            if !m.is_square() {
                return Err("нужна квадратная матрица".into());
            }
            let mut acc = m.get(0, 0);
            for i in 1..m.rows {
                acc = num_add(acc, m.get(i, i))?;
            }
            Ok(Value::Num(acc))
        }),
        "lsolve" => (|| {
            arity(name, args, 2)?;
            Ok(Value::Matrix(lsolve(args[0].as_matrix()?, &args[1].to_matrix()?)?))
        })(),
        "augment" => concat_matrices(args, true),
        "stack" => concat_matrices(args, false),
        "submatrix" => (|| {
            arity(name, args, 5)?;
            let m = args[0].as_matrix()?;
            let ix = args[1..].iter().map(|a| int_arg(a).map(|k| k - origin)).collect::<EResult<Vec<_>>>()?;
            let (ir, jr, ic, jc) = (ix[0], ix[1], ix[2], ix[3]);
            if ir < 0 || ic < 0 || jr < ir || jc < ic || jr as usize >= m.rows || jc as usize >= m.cols {
                return Err("индекс вне допустимых границ".into());
            }
            let mut out = Matrix::zeros((jr - ir + 1) as usize, (jc - ic + 1) as usize);
            for i in 0..out.rows {
                for j in 0..out.cols {
                    out.set(i, j, m.get(ir as usize + i, ic as usize + j));
                }
            }
            Ok(Value::Matrix(out))
        })(),
        "cross" => (|| {
            arity(name, args, 2)?;
            let a = vector_arg(&args[0], "cross: первый аргумент")?;
            let b = vector_arg(&args[1], "cross: второй аргумент")?;
            if a.rows != 3 || b.rows != 3 {
                return Err("нужны два вектора из 3 элементов".into());
            }
            let c = |i: usize, j: usize| num_sub(num_mul(a.data[i], b.data[j]), num_mul(a.data[j], b.data[i]));
            Ok(Value::Matrix(Matrix::column(vec![c(1, 2)?, c(2, 0)?, c(0, 1)?])))
        })(),
        "norme" => one(&|v| {
            let m = v.to_matrix()?;
            let d = m.uniform_dim()?;
            Ok(Value::Num(Num::with_dim(m.data.iter().map(|n| n.v.norm_sqr()).sum::<f64>().sqrt(), d)))
        }),
        "sort" => one(&|v| {
            let m = vector_arg(v, "sort: аргумент")?;
            same_dim(&m.data)?;
            let mut data = m.data.clone();
            data.sort_by(|a, b| a.v.re.total_cmp(&b.v.re));
            Ok(Value::Matrix(Matrix::column(data)))
        }),
        "reverse" => one(&|v| {
            let m = v.to_matrix()?;
            let mut out = Matrix::zeros(m.rows, m.cols);
            for i in 0..m.rows {
                for j in 0..m.cols {
                    out.set(i, j, m.get(m.rows - 1 - i, j));
                }
            }
            Ok(Value::Matrix(out))
        }),
        "linterp" => (|| {
            arity(name, args, 3)?;
            let vx = vector_arg(&args[0], "linterp: vx")?;
            let vy = vector_arg(&args[1], "linterp: vy")?;
            if vx.rows != vy.rows || vx.rows < 2 {
                return Err("vx и vy должны иметь одинаковую длину (не менее 2)".into());
            }
            let dx = same_dim(&vx.data)?;
            let dy = same_dim(&vy.data)?;
            map1(&args[2], |x| {
                if x.dim != dx {
                    return Err("x и vx имеют разные единицы измерения".into());
                }
                let n = vx.rows;
                let xs: Vec<f64> = vx.data.iter().map(|v| v.v.re).collect();
                let mut k = 0;
                while k < n - 2 && x.v.re > xs[k + 1] {
                    k += 1;
                }
                let t = (x.v.re - xs[k]) / (xs[k + 1] - xs[k]);
                let y = vy.data[k].v + (vy.data[k + 1].v - vy.data[k].v) * t;
                Ok(Num::new(y, dy))
            })
        })(),
        "polyroots" => one(&|v| {
            let m = vector_arg(v, "polyroots: коэффициенты")?;
            let c: Vec<C64> = m.data.iter().map(|n| dimless(*n)).collect::<EResult<_>>()?;
            let roots = numeric::polyroots(&c)?;
            Ok(Value::Matrix(Matrix::column(roots.into_iter().map(|z| Num::new(z, Dim::NONE)).collect())))
        }),
        "factorial" => one(&|v| {
            map1(v, |n| {
                let x = dimless(n)?;
                Ok(Num::real(factorial(x.re)?))
            })
        }),
        "gamma" | "Γ" => one(&|v| real_fn(v, libm::tgamma)),
        "erf" => one(&|v| real_fn(v, libm::erf)),
        "erfc" => one(&|v| real_fn(v, libm::erfc)),
        "dnorm" => (|| {
            arity(name, args, 3)?;
            let (mu, sigma) = (args[1].as_real()?, args[2].as_real()?);
            if sigma <= 0.0 {
                return Err("σ должна быть положительной".into());
            }
            real_fn(&args[0], |x| {
                (-(x - mu).powi(2) / (2.0 * sigma * sigma)).exp() / (sigma * (2.0 * std::f64::consts::PI).sqrt())
            })
        })(),
        "pnorm" => (|| {
            arity(name, args, 3)?;
            let (mu, sigma) = (args[1].as_real()?, args[2].as_real()?);
            if sigma <= 0.0 {
                return Err("σ должна быть положительной".into());
            }
            real_fn(&args[0], |x| 0.5 * libm::erfc(-(x - mu) / (sigma * std::f64::consts::SQRT_2)))
        })(),
        "UnitsOf" => one(&|v| map1(v, |n| Ok(Num::with_dim(1.0, n.dim)))),
        "concat" => (|| {
            let mut s = String::new();
            for a in args {
                match a {
                    Value::Str(x) => s.push_str(x),
                    other => return Err(format!("ожидались строки, найдено: {}", other.type_name())),
                }
            }
            Ok(Value::Str(s.into()))
        })(),
        "num2str" => one(&|v| Ok(Value::Str(format_plain(v.as_num()?).into()))),
        "strlen" => one(&|v| match v {
            Value::Str(s) => Ok(Value::real(s.chars().count() as f64)),
            _ => Err("ожидалась строка".into()),
        }),
        _ => return crate::builtins_ext::call(name, args, origin),
    };
    Some(r)
}
