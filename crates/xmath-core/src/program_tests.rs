//! Tests of Mathcad-style programs `{ ... }`.

use crate::ast::{Expr, PStmt, Stmt};
use crate::eval::Evaluator;
use crate::parser::{parse_expr, parse_statement};
use crate::value::Value;

/// Runs definitions line by line; the last line is evaluated.
fn run_with(ev: &mut Evaluator, lines: &[&str]) -> Result<Value, String> {
    let mut last = Err("пусто".to_string());
    for l in lines {
        match parse_statement(l).map_err(|e| e.msg)? {
            Stmt::Define { target, expr, .. } => ev.define(&target, &expr)?,
            Stmt::Eval { expr, .. } | Stmt::Show(expr) => {
                let sc = ev.top_scope();
                last = ev.eval(&expr, &sc);
            }
        }
    }
    last
}

fn run(lines: &[&str]) -> Result<Value, String> {
    run_with(&mut Evaluator::new(), lines)
}

fn f64_of(v: Value) -> f64 {
    v.as_num().unwrap().v.re
}

fn num(lines: &[&str]) -> f64 {
    match run(lines) {
        Ok(v) => f64_of(v),
        Err(e) => panic!("{lines:?}: {e}"),
    }
}

fn err(lines: &[&str]) -> String {
    run(lines).expect_err("expected an error")
}

fn roundtrip(src: &str) {
    let e = parse_expr(src).unwrap_or_else(|e| panic!("{src}: {}", e.msg));
    let printed = e.to_string();
    let e2 = parse_expr(&printed).unwrap_or_else(|e| panic!("{printed}: {}", e.msg));
    assert_eq!(e, e2, "{src} -> {printed}");
}

#[test]
fn parse_roundtrips() {
    for s in [
        "{ s ← 0; for i ∈ 1..n { s ← s + i^2 }; s }",
        "{ return -1 if x < 0; sqrt(x) }",
        r#"{ if x < 0 { "neg" } else if x == 0 { "zero" } otherwise { "pos" } }"#,
        "{ y ← 1/x on error 0; y }",
        "{ i ← 0; while i < a { i ← i + 1; if i == 5 { break } }; i }",
        "{ v[0] ← 1; v[1] ← 2; v }",
        "{ for k ∈ 0, 2..10 { if k == 4 { continue }; s ← s + k }; s }",
        "{ for i ∈ v { for j ∈ 1..3 { s ← s + i*j } }; s }",
        "f(1 on error 2)",
        "{ if a { if b { 1 } otherwise { 2 } } otherwise { 3 } }",
        "{ x ← { y ← 2; y^2 }; x + 1 }",
        "2*{ a ← 3; a }",
    ] {
        roundtrip(s);
    }
}

#[test]
fn parse_display_forms() {
    assert_eq!(
        parse_expr("{ s <- 0; for i in 1..n { s <- s + i }; s }").unwrap().to_string(),
        "{ s ← 0; for i ∈ 1..n { s ← s + i }; s }"
    );
    assert_eq!(
        parse_expr("{ if x < 0 { 1 } else { 2 } }").unwrap().to_string(),
        "{ if x < 0 { 1 } otherwise { 2 } }"
    );
    assert_eq!(parse_expr("{ x ← 1 if a; x }").unwrap().to_string(), "{ x ← 1 if a; x }");
    // postfix and block forms are the same tree
    assert_eq!(parse_expr("{ return 1 if a }").unwrap(), parse_expr("{ if a { return 1 } }").unwrap());
    assert_eq!(parse_statement("f(n) := { s ← 0; s }").unwrap().to_string(), "f(n) := { s ← 0; s }");
}

#[test]
fn parse_ast_shape() {
    let Expr::Program(p) = parse_expr("{ if a { 1 } else if b { 2 } otherwise { 3 } }").unwrap() else { panic!() };
    let PStmt::If { else_: Some(e), .. } = &p[0] else { panic!() };
    assert!(matches!(e[0], PStmt::If { else_: Some(_), .. }));
}

#[test]
fn parse_errors() {
    assert!(parse_expr("{ }").is_err());
    assert!(parse_expr("{ x ← }").is_err());
    assert!(parse_expr("{ 1 2 ").is_err());
    assert!(parse_expr("{ f(x) ← 1 }").is_err());
    assert!(parse_expr("{ for i 1..3 { i } }").is_err());
    assert!(parse_expr("{ if x 1 }").is_err());
    assert!(parse_expr("{ 1 + }").is_err());
}

#[test]
fn existing_syntax_unaffected() {
    // `in` is still the inch unit, `;` still separates matrix rows, `:` still defines
    assert_eq!(parse_expr("2 in").unwrap().to_string(), "2*in");
    assert_eq!(parse_expr("[1, 2; 3, 4]").unwrap().to_string(), "[1, 2; 3, 4]");
    assert_eq!(parse_statement("x : 5").unwrap().to_string(), "x := 5");
    assert_eq!(parse_statement("x ← 5").unwrap().to_string(), "x := 5");
    assert_eq!(parse_expr("a < -1").unwrap().to_string(), "a < -1");
    assert_eq!(parse_expr("if(a, b, c)").unwrap().to_string(), "if(a, b, c)");
}

#[test]
fn sum_of_squares() {
    assert_eq!(num(&["f(n) := { s ← 0; for i ∈ 1..n { s ← s + i^2 }; s }", "f(10)"]), 385.0);
    assert_eq!(num(&["f(n) := { s <- 0; for i in 1..n { s <- s + i^2 }; s }", "f(3)"]), 14.0);
}

#[test]
fn return_and_postfix_if() {
    let g = "g(x) := { return -1 if x < 0; sqrt(x) }";
    assert_eq!(num(&[g, "g(-4)"]), -1.0);
    assert_eq!(num(&[g, "g(9)"]), 3.0);
}

#[test]
fn if_otherwise_chain() {
    let h = r#"h(x) := { if x < 0 { "neg" } else if x == 0 { "zero" } otherwise { "pos" } }"#;
    for (x, want) in [("-2", "neg"), ("0", "zero"), ("5", "pos")] {
        match run(&[h, &format!("h({x})")]).unwrap() {
            Value::Str(s) => assert_eq!(&*s, want),
            other => panic!("{other:?}"),
        }
    }
}

#[test]
fn on_error() {
    let k = "k(x) := { y ← 1/x on error 0; y }";
    assert_eq!(num(&[k, "k(4)"]), 0.25);
    assert_eq!(num(&[k, "k(0)"]), 0.0);
    assert_eq!(num(&["k(x) := { undefined_name on error x + 1 }", "k(1)"]), 2.0);
    assert!(err(&["undefined_name + 1"]).contains("не определена"));
    assert_eq!(num(&["{ return undefined_name on error 3 }"]), 3.0);
}

#[test]
fn while_break_continue() {
    let w = "w(a) := { i ← 0; while i < a { i ← i + 1; if i == 5 { break } }; i }";
    assert_eq!(num(&[w, "w(3)"]), 3.0);
    assert_eq!(num(&[w, "w(100)"]), 5.0);
    // continue skips odd numbers
    let c = "c(n) := { s ← 0; for i ∈ 1..n { continue if mod(i, 2) == 1; s ← s + i }; s }";
    assert_eq!(num(&[c, "c(10)"]), 30.0);
}

#[test]
fn nested_loops() {
    let p = "p(n) := { s ← 0; for i ∈ 1..n { for j ∈ 1..n { s ← s + i*j } }; s }";
    assert_eq!(num(&[p, "p(3)"]), 36.0);
    // break only leaves the inner loop
    let q = "q := { s ← 0; for i ∈ 1..3 { for j ∈ 1..10 { break if j > 2; s ← s + 1 } }; s }";
    assert_eq!(num(&[q, "q"]), 6.0);
    // return from inside nested loops
    let r = "r := { for i ∈ 1..10 { for j ∈ 1..10 { return i*100 + j if i*j == 12 } }; 0 }";
    assert_eq!(num(&[r, "r"]), 206.0);
}

#[test]
fn program_as_variable_value_and_inline() {
    assert_eq!(num(&["x := { a ← 2; a^3 }", "x + 1"]), 9.0);
    assert_eq!(num(&["2*{ a ← 3; a } + 1"]), 7.0);
    assert_eq!(num(&["{ a ← 3; a } + { a ← 4; a }"]), 7.0);
}

#[test]
fn locals_shadow_worksheet_variables() {
    // the worksheet `a` is untouched, the program works on its own copy
    let mut ev = Evaluator::new();
    assert_eq!(f64_of(run_with(&mut ev, &["a := 10", "f := { a ← a + 1; a }", "f"]).unwrap()), 11.0);
    assert_eq!(f64_of(run_with(&mut ev, &["a"]).unwrap()), 10.0);
    // parameters can be reassigned locally
    assert_eq!(num(&["f(x) := { x ← x*2; x + 1 }", "f(5)"]), 11.0);
    // assignments inside blocks update the same local
    assert_eq!(num(&["{ s ← 1; if 1 == 1 { s ← 5 }; s }"]), 5.0);
    // a nested program has its own locals
    assert_eq!(num(&["{ s ← 1; t ← { s ← 7; s }; s + t }"]), 8.0);
}

#[test]
fn vectors_in_programs() {
    let sq = "sq(n) := { for i ∈ 0..n-1 { v[i] ← i^2 }; v }";
    assert_eq!(num(&[sq, "sq(5)[4]"]), 16.0);
    assert_eq!(num(&[sq, "rows(sq(5))"]), 5.0);
    // iterate over a vector
    assert_eq!(num(&["{ s ← 0; for x ∈ [1; 2; 3] { s ← s + x }; s }"]), 6.0);
    // a worksheet vector with the same name is not modified
    let mut ev = Evaluator::new();
    run_with(&mut ev, &["v := [1; 2; 3]"]).ok();
    assert_eq!(f64_of(run_with(&mut ev, &["{ v[0] ← 9; v[0] }"]).unwrap()), 9.0);
    assert_eq!(f64_of(run_with(&mut ev, &["v[0]"]).unwrap()), 1.0);
}

#[test]
fn range_variables_are_iterated() {
    assert_eq!(num(&["r := 1..4", "{ s ← 0; for i ∈ r { s ← s + i }; s }"]), 10.0);
    assert_eq!(num(&["{ s ← 0; for i ∈ 10, 8..0 { s ← s + i }; s }"]), 30.0);
}

#[test]
fn recursion_with_programs() {
    let fact = "fact(n) := { return 1 if n <= 1; n*fact(n - 1) }";
    assert_eq!(num(&[fact, "fact(10)"]), 3628800.0);
    let fib = "fib(n) := { a ← 0; b ← 1; for i ∈ 1..n { t ← a + b; a ← b; b ← t }; a }";
    assert_eq!(num(&[fib, "fib(20)"]), 6765.0);
    let newton = "nt(a) := { x ← a; while abs(x*x - a) > 1e-12 { x ← (x + a/x)/2 }; x }";
    assert!((num(&[newton, "nt(2)"]) - 2f64.sqrt()).abs() < 1e-9);
}

#[test]
fn units_in_programs() {
    let v = run(&["{ d ← 3 m; d ← d + 2 m; d }"]).unwrap();
    let n = v.as_num().unwrap();
    assert_eq!(n.v.re, 5.0);
    assert!(!n.dim.is_none());
}

#[test]
fn iteration_limit() {
    let mut ev = Evaluator::new();
    ev.max_iterations = 1000;
    let e = run_with(&mut ev, &["{ i ← 0; while 1 == 1 { i ← i + 1 }; i }"]).unwrap_err();
    assert!(e.contains("итераций"), "{e}");
    // within the limit
    assert_eq!(f64_of(run_with(&mut ev, &["{ i ← 0; while i < 500 { i ← i + 1 }; i }"]).unwrap()), 500.0);
    // a handler catches the limit error like any other
    assert_eq!(f64_of(run_with(&mut ev, &["x := { i ← 0; while 1 == 1 { i ← i + 1 }; i } on error 42", "x"]).unwrap()), 42.0);
}

#[test]
fn errors() {
    assert!(err(&["{ y + 1 }"]).contains("'y' не определена"));
    assert!(err(&["{ if 1 == 2 { 1 } }"]).contains("не вернула"));
    assert!(err(&["{ break }"]).contains("break"));
    assert!(err(&["{ continue }"]).contains("continue"));
    assert!(err(&["{ for i ∈ \"abc\" { i } }"]).contains("for"));
    assert!(err(&["{ s ← 1; s[0] ← 2 }"]).contains("'s'"));
}
