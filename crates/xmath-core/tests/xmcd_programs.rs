//! Mathcad programs in `.xmcd`: import of every construct Mathcad 15 writes, the evaluation
//! rules of Mathcad 13 and later, and export back to the format.

use xmath_core::xmcd::import_xmcd;
use xmath_core::xmcd_export::export_xmcd;
use xmath_core::{evaluate_worksheet, Output, RegionKind, Worksheet};

/// A worksheet with the given `<region>` elements, one region per statement.
fn sheet(regions: &[&str]) -> String {
    let mut out = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <worksheet xmlns=\"http://schemas.mathsoft.com/worksheet30\" \
         xmlns:ml=\"http://schemas.mathsoft.com/math30\">\n<regions>\n",
    );
    for (i, r) in regions.iter().enumerate() {
        out.push_str(&format!(
            "<region region-id=\"{}\" left=\"36\" top=\"{}\" width=\"200\" height=\"21\"><math>{r}</math></region>\n",
            i + 1,
            30 * (i + 1)
        ));
    }
    out.push_str("</regions>\n</worksheet>\n");
    out
}

fn load(regions: &[&str]) -> Worksheet {
    let xml = sheet(regions);
    import_xmcd(&xml).unwrap_or_else(|e| panic!("import: {e}\n{xml}")).worksheet
}

fn source(ws: &Worksheet, id: u64) -> String {
    match &ws.regions.iter().find(|r| r.id == id).expect("region").kind {
        RegionKind::Math { source } => source.clone(),
        RegionKind::Text { text, .. } => format!("TEXT:{text}"),
        _ => "OTHER".into(),
    }
}

fn value(ws: &Worksheet, id: u64) -> String {
    let out = evaluate_worksheet(ws);
    match &out[&id].output {
        Output::Value { view, unit } => match unit {
            Some(u) => format!("{view} {u}"),
            None => view.to_string(),
        },
        Output::Error(e) => format!("ERROR {e}"),
        Output::Symbolic { result, .. } => format!("SYM {result}"),
        _ => String::new(),
    }
}

fn last_value(ws: &Worksheet) -> String {
    let id = ws.regions.iter().map(|r| r.id).max().unwrap();
    value(ws, id)
}

// ---------------------------------------------------------------- import

#[test]
fn bare_loop_in_a_region_is_a_program() {
    // Mathcad writes `for i ∈ 0..3 { x[i] ← i^2 }` directly under <eval>, no <program>
    let ws = load(&[
        "<ml:eval><ml:for><ml:id>i</ml:id><ml:range><ml:real>0</ml:real><ml:real>3</ml:real></ml:range>\
         <ml:localDefine><ml:apply><ml:indexer/><ml:id>x</ml:id><ml:id>i</ml:id></ml:apply>\
         <ml:apply><ml:pow/><ml:id>i</ml:id><ml:real>2</ml:real></ml:apply></ml:localDefine>\
         </ml:for></ml:eval>",
    ]);
    assert_eq!(source(&ws, 1), "{ for i ∈ 0..3 { x[i] ← i^2 } } =");
    // the value of a local assignment is its right side, so the loop ends with 3^2
    assert_eq!(value(&ws, 1), "9");
}

#[test]
fn matrix_at_the_end_of_a_program_is_its_value() {
    // Mathcad stacks the last statements in a <matrix>; the program then returns that vector
    let ws = load(&[
        "<ml:eval><ml:program><ml:localDefine><ml:id>j</ml:id><ml:real>11</ml:real></ml:localDefine>\
         <ml:matrix rows=\"2\" cols=\"1\"><ml:id>j</ml:id><ml:real>5</ml:real></ml:matrix>\
         </ml:program></ml:eval>",
    ]);
    assert_eq!(source(&ws, 1), "{ j ← 11; [j; 5] } =");
    assert_eq!(value(&ws, 1), "[11; 5]");
}

#[test]
fn local_assignment_used_as_an_expression() {
    // `X[k] := (k ← 2)`: the right side runs first, then the index is evaluated
    let ws = load(&[
        "<ml:define><ml:id>k</ml:id><ml:real>0</ml:real></ml:define>",
        "<ml:define><ml:apply><ml:indexer/><ml:id>X</ml:id><ml:id>k</ml:id></ml:apply>\
         <ml:localDefine><ml:id>k</ml:id><ml:real>2</ml:real></ml:localDefine></ml:define>",
        "<ml:eval><ml:id>X</ml:id></ml:eval>",
    ]);
    assert_eq!(source(&ws, 2), "X[k] := k ← 2");
    // a worksheet definition evaluates the index first (Mathcad's own stored value is [2])
    assert_eq!(value(&ws, 3), "[2]");
}

#[test]
fn matrix_column_assignment() {
    // `B⟨1⟩ := 2*A⟨1⟩` with ORIGIN 0 grows B to three rows and two columns
    let ws = load(&[
        "<ml:eval><ml:program><ml:localDefine><ml:id>A</ml:id><ml:apply><ml:id>identity</ml:id>\
         <ml:real>3</ml:real></ml:apply></ml:localDefine>\
         <ml:localDefine><ml:apply><ml:matcol/><ml:id>B</ml:id><ml:real>1</ml:real></ml:apply>\
         <ml:apply><ml:mult/><ml:real>2</ml:real><ml:apply><ml:matcol/><ml:id>A</ml:id>\
         <ml:real>1</ml:real></ml:apply></ml:apply></ml:localDefine>\
         <ml:id>B</ml:id></ml:program></ml:eval>",
    ]);
    assert_eq!(source(&ws, 1), "{ A ← identity(3); B^<1> ← 2*A^<1>; B } =");
    assert_eq!(value(&ws, 1), "[0, 0; 0, 2; 0, 0]");
}

#[test]
fn local_function_definition() {
    // `sq(x) ← x^2` inside a program is a local function, not a worksheet definition
    let ws = load(&[
        "<ml:eval><ml:program><ml:localDefine><ml:function><ml:id>sq</ml:id><ml:boundVars>\
         <ml:id>x</ml:id></ml:boundVars></ml:function>\
         <ml:apply><ml:pow/><ml:id>x</ml:id><ml:real>2</ml:real></ml:apply></ml:localDefine>\
         <ml:apply><ml:id>sq</ml:id><ml:real>3</ml:real></ml:apply></ml:program></ml:eval>",
    ]);
    assert_eq!(source(&ws, 1), "{ sq(x) ← x^2; sq(3) } =");
    assert_eq!(value(&ws, 1), "9");
}

#[test]
fn control_flow_inside_an_expression() {
    // `a on error continue` / `… break` / `… (return b)`
    let ws = load(&[
        "<ml:define><ml:function><ml:id>f</ml:id><ml:boundVars><ml:id>x</ml:id></ml:boundVars>\
         </ml:function><ml:tryCatch><ml:apply><ml:div/><ml:real>1</ml:real><ml:id>x</ml:id></ml:apply>\
         <ml:return><ml:real>0</ml:real></ml:return></ml:tryCatch></ml:define>",
        "<ml:eval><ml:apply><ml:id>f</ml:id><ml:real>0</ml:real></ml:apply></ml:eval>",
        // the second iteration divides by zero and continues; the third adds 10
        "<ml:eval><ml:program><ml:localDefine><ml:id>s</ml:id><ml:real>0</ml:real></ml:localDefine>\
         <ml:for><ml:id>i</ml:id><ml:range><ml:real>1</ml:real><ml:real>3</ml:real></ml:range>\
         <ml:program><ml:tryCatch><ml:apply><ml:div/><ml:real>1</ml:real>\
         <ml:apply><ml:minus/><ml:id>i</ml:id><ml:real>2</ml:real></ml:apply></ml:apply>\
         <ml:continue/></ml:tryCatch>\
         <ml:localDefine><ml:id>s</ml:id><ml:apply><ml:plus/><ml:id>s</ml:id>\
         <ml:real>10</ml:real></ml:apply></ml:localDefine></ml:program></ml:for>\
         <ml:id>s</ml:id></ml:program></ml:eval>",
    ]);
    assert_eq!(value(&ws, 2), "0");
    assert_eq!(value(&ws, 3), "20");
}

// ------------------------------------------------- Mathcad's program scope

#[test]
fn an_unassigned_local_is_zero() {
    // Mathcad 13+: a name assigned anywhere in a program is its local, and reading it before
    // the assignment gives 0 — even when the worksheet defines it
    let ws = load(&[
        "<ml:define><ml:id>p</ml:id><ml:real>2</ml:real></ml:define>",
        "<ml:eval><ml:program><ml:ifThen><ml:real>0</ml:real>\
         <ml:localDefine><ml:id>p</ml:id><ml:real>1</ml:real></ml:localDefine></ml:ifThen>\
         <ml:id>p</ml:id></ml:program></ml:eval>",
    ]);
    assert_eq!(value(&ws, 2), "0");
}

#[test]
fn the_right_side_reads_the_worksheet_until_the_program_assigns() {
    // `{ s ← s + 1 }` starts from the worksheet `s` (Mathcad: worksheet names are referenced
    // on the right of a local assignment until the program reassigns them)
    let ws = load(&[
        "<ml:define><ml:id>s</ml:id><ml:real>10</ml:real></ml:define>",
        "<ml:eval><ml:program><ml:for><ml:id>i</ml:id><ml:range><ml:real>0</ml:real>\
         <ml:real>2</ml:real></ml:range><ml:localDefine><ml:id>s</ml:id>\
         <ml:apply><ml:plus/><ml:id>s</ml:id><ml:real>1</ml:real></ml:apply>\
         </ml:localDefine></ml:for><ml:id>s</ml:id></ml:program></ml:eval>",
    ]);
    assert_eq!(value(&ws, 2), "13");
}

#[test]
fn a_function_parameter_is_not_shadowed_by_an_assignment_of_the_same_name() {
    // Mathcad's own example: `sqroot(a, ε, maxiter) := { for i ∈ 0..maxiter { … maxiter ← i } }`
    let ws = load(&[
        "<ml:define><ml:function><ml:id>loop</ml:id><ml:boundVars><ml:id>n</ml:id></ml:boundVars>\
         </ml:function><ml:program><ml:localDefine><ml:id>m</ml:id><ml:real>0</ml:real></ml:localDefine>\
         <ml:for><ml:id>i</ml:id><ml:range><ml:real>0</ml:real><ml:id>n</ml:id></ml:range>\
         <ml:localDefine><ml:id>m</ml:id><ml:id>i</ml:id></ml:localDefine></ml:for>\
         <ml:matrix rows=\"2\" cols=\"1\"><ml:id>m</ml:id><ml:id>n</ml:id></ml:matrix></ml:program></ml:define>",
        "<ml:eval><ml:apply><ml:id>loop</ml:id><ml:real>3</ml:real></ml:apply></ml:eval>",
    ]);
    assert_eq!(value(&ws, 2), "[3; 3]");
}

// ---------------------------------------------------------------- export

fn exported(ws: &Worksheet) -> String {
    export_xmcd(ws).xml
}

#[test]
fn export_unwraps_a_one_statement_program() {
    // <ml:program> needs two children, so a single statement stands for itself (as Mathcad
    // writes it: <ml:eval><ml:for>…)
    let ws = load(&[
        "<ml:eval><ml:for><ml:id>i</ml:id><ml:range><ml:real>0</ml:real><ml:real>3</ml:real></ml:range>\
         <ml:localDefine><ml:apply><ml:indexer/><ml:id>x</ml:id><ml:id>i</ml:id></ml:apply>\
         <ml:apply><ml:pow/><ml:id>i</ml:id><ml:real>2</ml:real></ml:apply></ml:localDefine>\
         </ml:for></ml:eval>",
    ]);
    let xml = exported(&ws);
    assert!(xml.contains("<ml:for>"), "{xml}");
    assert!(!xml.contains("<ml:program>"), "{xml}");
}

#[test]
fn export_wraps_two_statements_and_keeps_column_targets() {
    let ws = load(&[
        "<ml:eval><ml:program><ml:localDefine><ml:id>A</ml:id><ml:apply><ml:id>identity</ml:id>\
         <ml:real>3</ml:real></ml:apply></ml:localDefine>\
         <ml:localDefine><ml:apply><ml:matcol/><ml:id>B</ml:id><ml:real>1</ml:real></ml:apply>\
         <ml:apply><ml:mult/><ml:real>2</ml:real><ml:apply><ml:matcol/><ml:id>A</ml:id>\
         <ml:real>1</ml:real></ml:apply></ml:apply></ml:localDefine></ml:program></ml:eval>",
    ]);
    let xml = exported(&ws);
    assert!(xml.contains("<ml:program>"), "{xml}");
    assert!(xml.contains("<ml:matcol/>"), "{xml}");
    // and the exported file imports again with the same source
    let back = import_xmcd(&xml).expect("re-import").worksheet;
    assert_eq!(source(&back, 1), source(&ws, 1));
}
