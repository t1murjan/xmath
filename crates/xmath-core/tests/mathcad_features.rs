//! Mathcad 15 language features: definitions with a shown value, vectorize,
//! vector definitions, matrix columns, range sums, program `if` chains.

use xmath_core::eval::Evaluator;
use xmath_core::value::Value;
use xmath_core::xmcd::import_xmcd_file;
use xmath_core::{evaluate_worksheet, parse_expr, parse_statement, Output, RegionKind, Stmt, Worksheet};

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

fn ws_of(lines: &[&str]) -> Worksheet {
    Worksheet::from_lines(&lines.join("\n"))
}

fn outputs(lines: &[&str]) -> Vec<String> {
    let ws = ws_of(lines);
    let out = evaluate_worksheet(&ws);
    ws.evaluation_order()
        .into_iter()
        .map(|i| match &out[&ws.regions[i].id].output {
            Output::Value { view, unit } => match unit {
                Some(u) => format!("{view} {u}"),
                None => view.to_string(),
            },
            Output::Error(e) => format!("ERROR {e}"),
            _ => String::new(),
        })
        .collect()
}

#[test]
fn vectorize_elementwise() {
    let v = "v := [1; 2; 3]";
    let w = "w := [4; 5; 6]";
    assert_eq!(col(&[v, w, "vectorize(v*w)"]), vec![4.0, 10.0, 18.0]);
    assert_eq!(col(&[v, w, "vectorize(w/v)"]), vec![4.0, 2.5, 2.0]);
    assert_eq!(col(&[v, "vectorize(v^2 + 1)"]), vec![2.0, 5.0, 10.0]);
    assert_eq!(col(&[v, "vectorize(2^v)"]), vec![2.0, 4.0, 8.0]);
    assert_eq!(col(&[v, "vectorize(-v)"]), vec![-1.0, -2.0, -3.0]);
    // without the arrow, v/w of equal-size vectors is element-wise too; v*w stays a dot product
    assert_eq!(col(&[v, w, "v/w"]), vec![0.25, 0.4, 0.5]);
}

#[test]
fn vectorize_functions_and_matrices() {
    let v = "v := [1; 4; 9]";
    assert_eq!(col(&[v, "vectorize(sqrt(v))"]), vec![1.0, 2.0, 3.0]);
    assert_eq!(col(&[v, "f(x) := x^2 + 1", "vectorize(f(v))"]), vec![2.0, 17.0, 82.0]);
    // a user function with several arguments and a broadcast scalar
    assert_eq!(col(&[v, "g(x, y) := x*y", "vectorize(g(v, 2))"]), vec![2.0, 8.0, 18.0]);
    match run(&["A := [1, 2; 3, 4]", "B := [5, 6; 7, 8]", "vectorize(A*B)"]).unwrap() {
        Value::Matrix(m) => {
            assert_eq!((m.rows, m.cols), (2, 2));
            assert_eq!(m.data.iter().map(|n| n.v.re).collect::<Vec<_>>(), vec![5.0, 12.0, 21.0, 32.0]);
        }
        other => panic!("{other:?}"),
    }
    // all operands scalar: just the value
    match run(&["vectorize(2*3)"]).unwrap() {
        Value::Num(n) => assert_eq!(n.v.re, 6.0),
        other => panic!("{other:?}"),
    }
}

#[test]
fn vectorize_units_and_errors() {
    let out = outputs(&["F := [1; 2; 3]*N", "L := [2; 2; 4]*m", "vectorize(F*L) = "]);
    assert!(out[2].contains('J') || out[2].contains("N"), "{out:?}");
    let e = run(&["a := [1; 2]", "b := [1; 2; 3]", "vectorize(a*b)"]).unwrap_err();
    assert!(e.contains("размеры"), "{e}");
}

#[test]
fn vector_definition() {
    assert_eq!(col(&["M := [1, 2; 3, 4]", "v := [1; 1]", "[a; b] := M*v", "[a; b]"]), vec![3.0, 7.0]);
    let out = outputs(&["v := [1; 2; 3]*m", "[x; y; z] := v", "y ="]);
    assert!(out[2].contains('2'), "{out:?}");
    let e = run(&["v := [1; 2]", "[a; b; c] := v", "a"]).unwrap_err();
    assert!(e.contains("размер"), "{e}");
    let e = run(&["[a; b] := 5", "a"]).unwrap_err();
    assert!(e.contains("вектор"), "{e}");
}

#[test]
fn matrix_column() {
    assert_eq!(col(&["M := [1, 2; 3, 4]", "M^<1>"]), vec![2.0, 4.0]);
    assert_eq!(col(&["M := [1, 2; 3, 4]", "M^<0>"]), vec![1.0, 3.0]);
    assert_eq!(col(&["ORIGIN := 1", "M := [1, 2; 3, 4]", "M^<1>"]), vec![1.0, 3.0]);
    assert_eq!(col(&["M := [1, 2; 3, 4]", "transpose(M)^<0>"]), vec![1.0, 2.0]);
    let e = run(&["M := [1, 2; 3, 4]", "M^<2>"]).unwrap_err();
    assert!(e.contains("столбец"), "{e}");
    assert!(run(&["x := 3", "x^<0>"]).unwrap_err().contains("матриц"));
}

#[test]
fn definition_with_shown_value() {
    let out = outputs(&["x := 2 m", "d := 3*x =", "e := 3*x = cm", "d + e ="]);
    assert!(out[1].contains('6'), "{out:?}");
    assert!(out[2].contains("600"), "{out:?}");
    // the variable is defined, too
    assert!(out[3].contains("12"), "{out:?}");
    // errors show up on the region
    let out = outputs(&["d := q ="]);
    assert!(out[0].starts_with("ERROR"), "{out:?}");
}

#[test]
fn range_sum_with_single_bound() {
    let out = outputs(&["v := [1; 2; 3]", "i := 0..2", "sum(i, v[i]) ="]);
    assert_eq!(out[2], "6");
}

#[test]
fn program_if_chain_otherwise() {
    let f = "f(x) := { 1 if x < 0; if x >= 0 and x < 5 { 2 } otherwise { 3 } }";
    assert_eq!(outputs(&[f, "f(-1) =", "f(1) =", "f(9) ="]), vec!["", "1", "2", "3"]);
}

fn load(name: &str) -> (Worksheet, Vec<String>) {
    let p = format!("{}/tests/data/{name}", env!("CARGO_MANIFEST_DIR"));
    let r = import_xmcd_file(p).expect("import");
    (r.worksheet, r.warnings)
}

fn source(ws: &Worksheet, id: u64) -> String {
    match &ws.regions.iter().find(|r| r.id == id).unwrap().kind {
        RegionKind::Math { source } => source.clone(),
        RegionKind::Text { text, .. } => format!("TEXT:{text}"),
        RegionKind::Plot { x, y, .. } => format!("PLOT:{x}|{}", y.join("|")),
        RegionKind::Image { .. } => "IMAGE".to_string(),
    }
}

fn shown(ws: &Worksheet, id: u64) -> String {
    let out = evaluate_worksheet(ws);
    match &out[&id].output {
        Output::Value { view, unit } => match unit {
            Some(u) => format!("{view} {u}"),
            None => view.to_string(),
        },
        Output::Error(e) => format!("ERROR {e}"),
        _ => String::new(),
    }
}

#[test]
fn import_mathcad_features() {
    let (ws, warns) = load("mathcad_features.xmcd");
    assert!(warns.is_empty(), "{warns:?}");
    assert_eq!(source(&ws, 3), "p := vectorize(v*w)");
    assert_eq!(source(&ws, 5), "d := 2*m = cm");
    assert_eq!(source(&ws, 6), "k := 7 =");
    assert_eq!(source(&ws, 7), "[a; b; c] := v + w =");
    assert_eq!(source(&ws, 10), "M^<1> =");
    assert_eq!(source(&ws, 12), "S := sum(i, v[i])");
    assert_eq!(source(&ws, 14), "TEXT:t, mks");
    for id in [3, 5, 6, 7] {
        assert!(parse_statement(&source(&ws, id)).is_ok());
    }
    let p = shown(&ws, 4);
    assert!(p.contains("18") && p.contains("10"), "{p}");
    assert!(shown(&ws, 5).contains("200"), "{}", shown(&ws, 5));
    assert_eq!(shown(&ws, 6), "7");
    assert_eq!(shown(&ws, 8), "45");
    assert!(shown(&ws, 10).contains('2') && shown(&ws, 10).contains('4'));
    assert_eq!(shown(&ws, 13), "6");
}
