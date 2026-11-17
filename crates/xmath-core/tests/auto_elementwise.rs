//! Mathcad 15 applies some array operations element by element without the vectorize arrow.

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

fn col(lines: &[&str]) -> Vec<f64> {
    match run(lines).unwrap() {
        Value::Matrix(m) => m.data.iter().map(|n| n.v.re).collect(),
        other => panic!("expected a matrix, got {other:?}"),
    }
}

#[test]
fn automatic_elementwise_rules() {
    let v = "v := [1; 2; 4]";
    assert_eq!(col(&[v, "v^2"]), vec![1.0, 4.0, 16.0]);
    assert_eq!(col(&[v, "v^0.5 + 0"]).len(), 3);
    assert_eq!(col(&[v, "w := [2; 2; 2]", "v/w"]), vec![0.5, 1.0, 2.0]);
    assert_eq!(col(&[v, "8/v"]), vec![8.0, 4.0, 2.0]);
    assert_eq!(col(&[v, "v + 1"]), vec![2.0, 3.0, 5.0]);
    assert_eq!(col(&[v, "2^v"]), vec![2.0, 4.0, 16.0]);
    // vector * vector stays the dot product, square matrices keep matrix algebra
    match run(&[v, "v*v"]).unwrap() {
        Value::Num(n) => assert_eq!(n.v.re, 21.0),
        o => panic!("{o:?}"),
    }
    assert_eq!(col(&["A := [1, 1; 0, 1]", "A^2"]), vec![1.0, 2.0, 0.0, 1.0]);
    assert!(run(&[v, "[1; 2]/v"]).is_err());
}
