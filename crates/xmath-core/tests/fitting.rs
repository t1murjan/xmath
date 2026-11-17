//! Tests of curve fitting, smoothing, random numbers and numeric edge cases.

use xmath_core::eval::Evaluator;
use xmath_core::value::Value;
use xmath_core::{parse_expr, parse_statement, Stmt};

fn run(lines: &[&str]) -> Result<Value, String> {
    let mut ev = Evaluator::new();
    let (last, defs) = lines.split_last().unwrap();
    for l in defs {
        match parse_statement(l).map_err(|e| e.msg)? {
            Stmt::Define { target, expr, .. } => ev.define(&target, &expr)?,
            _ => panic!("not a definition: {l}"),
        }
    }
    let e = parse_expr(last).map_err(|e| e.msg)?;
    let sc = ev.top_scope();
    ev.eval(&e, &sc)
}

fn vec_of(lines: &[&str]) -> Vec<f64> {
    match run(lines).unwrap() {
        Value::Matrix(m) => m.data.iter().map(|n| n.v.re).collect(),
        other => panic!("expected a matrix, got {other:?}"),
    }
}

fn num(lines: &[&str]) -> f64 {
    run(lines).unwrap().as_num().unwrap().v.re
}

fn close(a: &[f64], b: &[f64]) {
    assert_eq!(a.len(), b.len(), "{a:?} vs {b:?}");
    for (x, y) in a.iter().zip(b) {
        assert!((x - y).abs() < 1e-5 * (1.0 + y.abs()), "{a:?} vs {b:?}");
    }
}

const VX: &str = "vx := stack(0, 1, 2, 3, 4, 5)";

#[test]
fn expfit_recovers_parameters() {
    close(&vec_of(&[VX, "f(k) := 2*exp(0.5*k) + 1", "expfit(vx, stack(f(0), f(1), f(2), f(3), f(4), f(5)))"]), &[2.0, 0.5, 1.0]);
    close(
        &vec_of(&[VX, "f(k) := 2*exp(0.5*k) + 1", "expfit(vx, stack(f(0), f(1), f(2), f(3), f(4), f(5)), stack(1, 0.4, 0))"]),
        &[2.0, 0.5, 1.0],
    );
}

#[test]
fn guess_based_fits() {
    close(
        &vec_of(&[VX, "f(k) := 2*ln(k+1) + 0.5", "logfit(vx, stack(f(0), f(1), f(2), f(3), f(4), f(5)), stack(1, 1, 0))"]),
        &[2.0, 1.0, 0.5],
    );
    close(
        &vec_of(&[VX, "f(k) := 3/(1+4*exp(-1.2*k))", "lgsfit(vx, stack(f(0), f(1), f(2), f(3), f(4), f(5)), stack(3, 3, 1))"]),
        &[3.0, 4.0, 1.2],
    );
    close(
        &vec_of(&[VX, "f(k) := 1.5*(k+1)^2 + 2", "pwrfit(vx + 1, stack(f(0), f(1), f(2), f(3), f(4), f(5)), stack(1, 1.5, 1))"]),
        &[1.5, 2.0, 2.0],
    );
    close(
        &vec_of(&[VX, "f(k) := 2*sin(k+0.3) + 1", "sinfit(vx, stack(f(0), f(1), f(2), f(3), f(4), f(5)), stack(1.5, 0, 0.5))"]),
        &[2.0, 0.3, 1.0],
    );
    close(
        &vec_of(&[VX, "f(k) := 2*ln(k+1) + 0.5", "lnfit(vx + 1, stack(f(0), f(1), f(2), f(3), f(4), f(5)))"]),
        &[2.0, 0.5],
    );
}

#[test]
fn linfit_and_genfit() {
    close(
        &vec_of(&[
            VX,
            "F(x) := stack(1, x, x^2)",
            "q(k) := 1 + 2*k + 3*k^2",
            "linfit(vx, stack(q(0), q(1), q(2), q(3), q(4), q(5)), F)",
        ]),
        &[1.0, 2.0, 3.0],
    );
    let y = "stack(g(0), g(1), g(2), g(3), g(4), g(5))";
    let g = "g(k) := 2*exp(0.3*k)";
    close(
        &vec_of(&[VX, g, "G(x, u) := u[0]*exp(u[1]*x)", &format!("genfit(vx, {y}, stack(1, 0.2), G)")]),
        &[2.0, 0.3],
    );
    close(
        &vec_of(&[
            VX,
            g,
            "H(x, u) := stack(u[0]*exp(u[1]*x), exp(u[1]*x), u[0]*x*exp(u[1]*x))",
            &format!("genfit(vx, {y}, stack(1, 0.2), H)"),
        ]),
        &[2.0, 0.3],
    );
}

#[test]
fn loess_interpolates_a_line() {
    let y = "stack(1, 3, 5, 7, 9, 11)";
    let v = num(&[VX, &format!("s := loess(vx, {y}, 0.8)"), &format!("interp(s, vx, {y}, 2.5)")]);
    assert!((v - 6.0).abs() < 1e-6, "{v}");
}

#[test]
fn smoothers() {
    close(&vec_of(&["medsmooth(stack(1, 9, 2, 8, 3, 7), 3)"]), &[1.0, 2.0, 8.0, 3.0, 7.0, 7.0]);
    let r = vec_of(&[VX, "ksmooth(vx, stack(1, 3, 5, 7, 9, 11), 2)"]);
    assert_eq!(r.len(), 6);
    assert!(r.windows(2).all(|w| w[1] > w[0]));
    let r = vec_of(&[VX, "supsmooth(vx, stack(1, 3, 5, 7, 9, 11))"]);
    for (k, v) in r.iter().enumerate() {
        assert!((v - (1.0 + 2.0 * k as f64)).abs() < 1e-6, "{r:?}");
    }
}

#[test]
fn predict_extrapolates_sine() {
    let data = "stack(0, 0.479426, 0.841471, 0.997495, 0.909297, 0.598472, 0.14112, -0.350783, -0.756802, -0.977530, -0.958924, -0.705540, -0.279415, 0.215120, 0.656987, 0.938000, 0.989358, 0.798487, 0.412118, -0.075180)";
    let r = vec_of(&[&format!("predict({data}, 4, 3)")]);
    for (a, b) in r.iter().zip([-0.5440, -0.8797, -1.0000]) {
        assert!((a - b).abs() < 2e-3, "{r:?}");
    }
}

#[test]
fn random_numbers() {
    let r = vec_of(&["rnorm(2000, 5, 2)"]);
    let mean = r.iter().sum::<f64>() / r.len() as f64;
    assert!((mean - 5.0).abs() < 0.2);
    let r = vec_of(&["runif(500, 1, 3)"]);
    assert!(r.iter().all(|&v| (1.0..3.0).contains(&v)));
    assert_eq!(vec_of(&["rbinom(4, 10, 0.5)"]).len(), 4);
    assert!(vec_of(&["rpois(50, 4)"]).iter().all(|v| v.fract() == 0.0 && *v >= 0.0));
    assert!(vec_of(&["rexp(50, 2)"]).iter().all(|&v| v > 0.0));
    assert!((num(&["pbinom(3, 10, 0.5)"]) - 0.171875).abs() < 1e-9);
    assert!((num(&["dpois(2, 3)"]) - 0.22404180765538775).abs() < 1e-9);
}

#[test]
fn principal_roots_of_negative_numbers() {
    let z = run(&["x := -8", "x^(1/3)"]).unwrap().as_num().unwrap();
    assert!((z.v.re - 1.0).abs() < 1e-12 && (z.v.im - 3f64.sqrt()).abs() < 1e-12, "{z:?}");
    let z = run(&["(-8)^(1/3)"]).unwrap().as_num().unwrap();
    assert!(z.v.im > 0.0);
    let z = run(&["(-1)^0.5"]).unwrap().as_num().unwrap();
    assert!((z.v.im - 1.0).abs() < 1e-12);
    assert!((num(&["arg(-8)"]) - std::f64::consts::PI).abs() < 1e-12);
    assert!((num(&["nthroot(-8, 3)"]) + 2.0).abs() < 1e-12);
    let z = run(&["ln(-8)"]).unwrap().as_num().unwrap();
    assert!(z.v.im > 0.0);
}

#[test]
fn zero_is_compatible_with_any_units() {
    assert!((num(&["z := 1 m", "integrate(t^2, t, 0, z) / m^3"]) - 1.0 / 3.0).abs() < 1e-9);
    assert!((num(&["max(0, 3 m) / m"]) - 3.0).abs() < 1e-12);
    assert!((num(&["root(t^2 - 4 m^2, t, 0, 3 m) / m"]) - 2.0).abs() < 1e-9);
    assert_eq!(num(&["0 m + 0"]), 0.0);
}

#[test]
fn rounding_and_mod() {
    assert_eq!(num(&["mod(-7, 3)"]), -1.0);
    assert_eq!(num(&["round(-2.5)"]), -3.0);
    assert_eq!(num(&["floor(-2.5)"]), -3.0);
    assert_eq!(num(&["ceil(-2.5)"]), -2.0);
}

#[test]
fn loess_regression_small_span() {
    let defs = ["vx := stack(1, 2, 3, 4, 5)", "vy := stack(2.7, 7.4, 20.1, 54.6, 148.4)", "vs := loess(vx, vy, 0.75)"];
    for (x, lo, hi) in [(2.5, 7.4, 54.6), (1.0, 2.0, 10.0), (5.0, 54.6, 160.0), (3.0, 7.4, 54.6)] {
        let mut l = defs.to_vec();
        let e = format!("interp(vs, vx, vy, {x})");
        l.push(&e);
        let v = num(&l);
        assert!(v.is_finite() && v >= lo && v <= hi, "x={x}: {v}");
    }
    // outside the range and duplicated x values stay finite
    let v = num(&[defs[0], defs[1], defs[2], "interp(vs, vx, vy, 6)"]);
    assert!(v.is_finite() && v.abs() < 1e4, "{v}");
    let v = num(&[
        "vx := stack(1, 1, 2, 2, 3, 3)",
        "vy := stack(1, 1, 4, 4, 9, 9)",
        "vs := loess(vx, vy, 0.5)",
        "interp(vs, vx, vy, 1.5)",
    ]);
    assert!(v.is_finite() && (1.0..=9.0).contains(&v), "{v}");
}

#[test]
fn loess_follows_smooth_function() {
    let v = num(&[
        "f(k) := sin(k/5)",
        "vx := stack(0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19)",
        "vy := stack(f(0), f(1), f(2), f(3), f(4), f(5), f(6), f(7), f(8), f(9), f(10), f(11), f(12), f(13), f(14), f(15), f(16), f(17), f(18), f(19))",
        "vs := loess(vx, vy, 0.5)",
        "interp(vs, vx, vy, 7.5) - sin(1.5)",
    ]);
    assert!(v.abs() < 5e-3, "{v}");
}
