//! Solve blocks: Given / Find / Minerr / Maximize / Minimize.

use xmath_core::{evaluate_worksheet, Output, ResultView, Worksheet};

/// Evaluates one region per line; returns the output of every region.
fn run(src: &str) -> Vec<Output> {
    let ws = sheet(src);
    let res = evaluate_worksheet(&ws);
    ws.regions.iter().map(|r| res[&r.id].output.clone()).collect()
}

fn sheet(src: &str) -> Worksheet {
    let mut ws = Worksheet::from_lines(src);
    ws.format.precision = 12;
    ws
}

fn parse_num(t: &str) -> f64 {
    let t = t.replace('−', "-").replace(' ', "");
    match t.split_once("·10^") {
        Some((m, e)) => m.parse::<f64>().unwrap() * 10f64.powi(e.parse().unwrap()),
        None => t.parse().unwrap_or_else(|_| panic!("cannot parse {t}")),
    }
}

fn numbers(v: &ResultView) -> Vec<f64> {
    match v {
        ResultView::Scalar { number, .. } => vec![parse_num(&number.to_string())],
        ResultView::Matrix { cells, .. } => cells.iter().map(|c| parse_num(&c.to_string())).collect(),
        other => panic!("unexpected view {other:?}"),
    }
}

fn err_of(o: &Output) -> Option<&str> {
    match o {
        Output::Error(e) => Some(e),
        _ => None,
    }
}

/// Value of the last region as a flat list of SI numbers.
fn values(src: &str) -> Vec<f64> {
    let outs = run(src);
    for o in &outs {
        if let Some(e) = err_of(o) {
            panic!("unexpected error: {e}");
        }
    }
    let ws = sheet(src);
    let res = evaluate_worksheet(&ws);
    let last = ws.regions.last().unwrap();
    match &res[&last.id].output {
        Output::Value { view, .. } => numbers(view),
        o => panic!("no value: {o:?}"),
    }
}

fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-4 * (1.0 + b.abs()), "{a} != {b}");
}

#[test]
fn one_unknown() {
    let v = values("x := 1\nGiven\nx^2 == 2\nFind(x) =");
    close(v[0], 2f64.sqrt());
    let v = values("x := -1\ngiven\nx^2 == 2\nFind(x) =");
    close(v[0], -(2f64.sqrt()));
}

#[test]
fn two_unknowns_circle_line() {
    let v = values("x := 1\ny := 1\nGiven\nx^2 + y^2 == 5\nx + y == 3\nFind(x, y) =");
    assert!((v[0] - 1.0).abs() < 1e-4 || (v[0] - 2.0).abs() < 1e-4);
    close(v[0] + v[1], 3.0);
}

#[test]
fn three_unknowns() {
    let v = values("x := 0\ny := 0\nz := 0\nGiven\nx + y + z == 6\nx - y == -1\nx + 2*z == 4\nFind(x, y, z) =");
    close(v[0], 2.0);
    close(v[1], 3.0);
    close(v[2], 1.0);
}

#[test]
fn inequality_constraints() {
    let v = values("x := 0\nGiven\nx > 3\nx < 10\nFind(x) =");
    assert!(v[0] >= 3.0 - 1e-6 && v[0] <= 10.0);
    let v = values("x := 5\nGiven\nx^2 == 4\nx > 0\nFind(x) =");
    close(v[0], 2.0);
}

#[test]
fn minerr_inconsistent() {
    let v = values("x := 0\nGiven\nx == 1\nx == 3\nMinerr(x) =");
    close(v[0], 2.0);
    // Find reports an error for the same system
    let outs = run("x := 0\nGiven\nx == 1\nx == 3\nFind(x) =");
    assert!(err_of(outs.last().unwrap()).unwrap().contains("Решение не найдено"));
}

#[test]
fn function_of_solve_block() {
    let v = values("x := 1\nGiven\nx^2 == 4\nr(a) := Find(x)\nr(1) =");
    close(v[0], 2.0);
    let v = values("x := 1\na := 0\nGiven\nx^2 == a\nr(a) := Find(x)\nr(9) =");
    close(v[0], 3.0);
    let v = values("x := 1\na := 0\nGiven\nx^2 == a\nr(a) := Find(x)\nr(16) + r(25) =");
    close(v[0], 9.0);
}

#[test]
fn definition_assigned_find() {
    let v = values("x := 1\ny := 1\nGiven\nx + y == 3\nx - y == 1\nsol := Find(x, y)\nsol[1] =");
    close(v[0], 1.0);
}

#[test]
fn units() {
    // beam: deflection 5 w L^4 / (384 E I) = L / 250  ->  L^3 = 384 E I / (1250 w)
    let src = "w := 10 kN/m\nE := 200 GPa\nI := 8000 cm^4\nL := 5 m\nGiven\n5*w*L^4/(384*E*I) == L/250\nFind(L) =";
    let v = values(src);
    let expect = (384.0_f64 * 200e9 * 8000e-8 / (1250.0 * 10e3)).powf(1.0 / 3.0);
    close(v[0], expect);
    // dimension mismatch in a constraint
    let outs = run("x := 1 m\nGiven\nx == 2 s\nFind(x) =");
    assert!(err_of(outs.last().unwrap()).unwrap().contains("единиц"));
}

#[test]
fn minimize_in_block() {
    let v = values("f(x, y) := (x - 1)^2 + (y + 2)^2\nx := 0\ny := 0\nGiven\nx + y == 0\nMinimize(f, x, y) =");
    close(v[0], 1.5);
    close(v[1], -1.5);
    let v = values("f(x) := -(x - 3)^2 + 5\nx := 0\nGiven\nx < 2\nMaximize(f, x) =");
    close(v[0], 2.0);
}

#[test]
fn errors() {
    // no guess
    let outs = run("Given\nx^2 == 4\nFind(x) =");
    assert!(err_of(outs.last().unwrap()).unwrap().contains("начальное приближение"));
    // Given without Find
    let outs = run("x := 1\nGiven\nx^2 == 4");
    assert!(err_of(&outs[1]).unwrap().contains("Given"));
    // definition inside the block
    let outs = run("x := 1\nGiven\nz := 3\nx == z\nFind(x) =");
    assert!(err_of(&outs[2]).unwrap().contains("Определения внутри блока"));
    // undefined name in a constraint is reported on its own region
    let outs = run("x := 1\nGiven\nx == q\nFind(x) =");
    assert!(err_of(&outs[2]).unwrap().contains("'q'"));
    // no convergence
    let outs = run("x := 1\nGiven\nx^2 == -1\nFind(x) =");
    assert!(err_of(outs.last().unwrap()).unwrap().contains("Решение не найдено"));
    // Find outside a block
    let outs = run("x := 1\nFind(x) =");
    assert!(err_of(outs.last().unwrap()).unwrap().contains("Given"));
}
