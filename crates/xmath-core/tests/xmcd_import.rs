use xmath_core::xmcd::{import_xmcd, import_xmcd_file};
use xmath_core::{evaluate_worksheet, Output, RegionKind, Worksheet};

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
        RegionKind::Plot3d { expr, .. } => format!("PLOT3D:{expr}"),
        RegionKind::Reference { href } => format!("REF:{href}"),
    }
}

fn shown(ws: &Worksheet, id: u64) -> String {
    let out = evaluate_worksheet(ws);
    match &out[&id].output {
        Output::Value { view, .. } => view.to_string(),
        Output::Error(e) => format!("ERROR {e}"),
        _ => String::new(),
    }
}

#[test]
fn units_and_functions() {
    let (ws, warns) = load("units.xmcd");
    assert!(warns.is_empty(), "{warns:?}");
    assert_eq!(source(&ws, 1), "mass := 5*kg");
    assert_eq!(source(&ws, 2), "a := 2*(m/s^2)");
    assert_eq!(source(&ws, 4), "F = N");
    assert_eq!(source(&ws, 5), "f(x, y) := x^2 + y");
    assert_eq!(source(&ws, 8), "x.max ≡ -3");
    assert_eq!(ws.format.precision, 4);
    assert!(ws.regions.iter().any(|r| matches!(&r.kind, RegionKind::Math { source } if source == "ORIGIN ≡ 1")));
    // coordinates are scaled from points to 96 dpi
    assert!((ws.regions[0].x - 48.0).abs() < 1e-3);
    assert_eq!(shown(&ws, 6), "10");
    assert_eq!(shown(&ws, 9), "3");
    assert_eq!(shown(&ws, 4), "10");
}

#[test]
fn sqrt_of_negative_is_imaginary() {
    let (ws, _) = load("units.xmcd");
    assert_eq!(source(&ws, 7), "sqrt(-16) =");
    assert!(shown(&ws, 7).contains('i'), "{}", shown(&ws, 7));
}

#[test]
fn matrix_is_column_major() {
    let (ws, warns) = load("matrix_range.xmcd");
    assert_eq!(source(&ws, 1), "M := [1, 2, 3; 4, 5, 6]");
    assert_eq!(source(&ws, 3), "M[0, 1] =");
    assert_eq!(shown(&ws, 3), "2");
    assert_eq!(source(&ws, 4), "transpose(M) =");
    assert_eq!(shown(&ws, 9), "2");
    assert!(warns.is_empty(), "{warns:?}");
    assert_eq!(source(&ws, 10), "vectorize(sin(x)) =");
}

#[test]
fn ranges_and_indexed() {
    let (ws, _) = load("matrix_range.xmcd");
    assert_eq!(source(&ws, 5), "i := 0..4");
    assert_eq!(source(&ws, 6), "v[i] := i^2");
    assert_eq!(source(&ws, 8), "x := 0, 0.5..2");
    assert_eq!(shown(&ws, 7), "9");
}

#[test]
fn calculus() {
    let (ws, warns) = load("calculus.xmcd");
    assert!(warns.is_empty(), "{warns:?}");
    assert_eq!(source(&ws, 1), "integrate(x^2, x, 0, 3) =");
    assert_eq!(source(&ws, 2), "diff(x^3, x) =");
    assert_eq!(source(&ws, 4), "sum(k, 1, 10, k) =");
    assert_eq!(source(&ws, 5), "prod(k, 1, 5, k) =");
    assert_eq!(source(&ws, 7), "sum(j, j^2) =");
    assert_eq!(shown(&ws, 1), "9");
    assert_eq!(shown(&ws, 2), "12");
    assert_eq!(shown(&ws, 4), "55");
    assert_eq!(shown(&ws, 5), "120");
    assert_eq!(shown(&ws, 7), "30");
    assert_eq!(shown(&ws, 8), "3");
    assert_eq!(shown(&ws, 9), "120");
}

#[test]
fn text_plot_program_and_fallbacks() {
    let (ws, warns) = load("text_plot_program.xmcd");
    match &ws.regions.iter().find(|r| r.id == 1).unwrap().kind {
        RegionKind::Text { text, style, .. } => {
            assert_eq!(text, "Расчёт балки\nВторой абзац\nс переносом");
            assert!(style.bold);
            assert_eq!(style.size, 14.0);
        }
        _ => panic!("text expected"),
    }
    assert_eq!(source(&ws, 2), "TEXT:Площадь A_1 и x^2 заданы ниже.");
    assert_eq!(source(&ws, 4), "PLOT:t|t^2|t", "{warns:?}");
    assert!(source(&ws, 5).starts_with("TEXT:[График не импортирован]"));
    assert_eq!(source(&ws, 6), "g(n) := { s ← 0; return s }");
    assert!(source(&ws, 7).starts_with("TEXT:[Рисунок"));
    assert_eq!(source(&ws, 8), "foo(1) =");
    assert!(source(&ws, 9).starts_with("TEXT:"));
    assert!(source(&ws, 10).starts_with("TEXT:"));
    assert!(warns.len() >= 5, "{warns:?}");
    let out = evaluate_worksheet(&ws);
    match &out[&4].output {
        Output::Plot(p) => assert_eq!(p.series.len(), 2),
        o => panic!("{o:?}"),
    }
}

#[test]
fn malformed_input_does_not_panic() {
    assert!(import_xmcd("").is_err());
    assert!(import_xmcd("<a><b></a>").is_err());
    assert!(import_xmcd("<foo/>").is_err());
    let r = import_xmcd(
        "<worksheet><regions><region region-id='x'><math><ml:eval xmlns:ml='m'/></math></region><region/></regions></worksheet>",
    )
    .unwrap();
    assert!(!r.warnings.is_empty());
    let r = import_xmcd("<worksheet/>").unwrap();
    assert!(r.worksheet.regions.is_empty());
}

#[test]
fn programs_are_imported() {
    let (ws, warns) = load("programs.xmcd");
    assert_eq!(source(&ws, 1), "f(n) := { s ← 0; for i ∈ 1..n { s ← s + i^2 }; return s }", "{warns:?}");
    assert_eq!(shown(&ws, 2), "385");
    assert_eq!(source(&ws, 3), r#"h(x) := { if x < 0 { "neg" } otherwise { "pos" } }"#);
    assert_eq!(shown(&ws, 4), "\"neg\"");
    assert_eq!(source(&ws, 5), "w(a) := { i ← 0; while i < a { i ← i + 1; break if i == 5 }; i }");
    assert_eq!(shown(&ws, 6), "5");
    assert_eq!(source(&ws, 7), "k(x) := { y ← 1/x on error 0; y }");
    assert_eq!(shown(&ws, 8), "0");
    // unknown statements fall back to the text placeholder
    assert!(source(&ws, 9).starts_with("TEXT:[Программа Mathcad не поддерживается]"));
    assert!(warns.iter().any(|w| w.contains("weirdStatement")), "{warns:?}");
}

#[test]
fn solve_block() {
    let (ws, warns) = load("solve.xmcd");
    assert!(warns.is_empty(), "{warns:?}");
    assert_eq!(source(&ws, 3), "Given");
    assert_eq!(source(&ws, 4), "x + y == 3");
    assert_eq!(source(&ws, 6), "Find(x, y) =");
    let v = shown(&ws, 6);
    assert!(v.contains('2') && v.contains('1'), "{v}");
}

#[test]
fn determinant() {
    let (ws, warns) = load("determinant.xmcd");
    assert!(warns.is_empty(), "{warns:?}");
    assert_eq!(source(&ws, 2), "|M| =");
    assert!(shown(&ws, 2).contains("-2") || shown(&ws, 2).contains("−2"), "{}", shown(&ws, 2));
}

#[test]
fn json_roundtrip() {
    let (ws, _) = load("units.xmcd");
    assert_eq!(Worksheet::from_json(&ws.to_json()).unwrap(), ws);
}

#[test]
fn text_region_size_font_and_width() {
    use xmath_core::worksheet::TextFont;
    let xml = r#"<?xml version="1.0"?>
<worksheet xmlns="http://schemas.mathsoft.com/worksheet30" xmlns:ml="http://schemas.mathsoft.com/math30">
<settings><presentation><textStyles>
<textStyle name="Normal"><inlineAttr font-family="Times New Roman" font-size="12" font-weight="normal" font-style="normal" underline="false"/></textStyle>
</textStyles></presentation></settings>
<regions>
<region region-id="1" left="30" top="6" width="150" height="30">
<text use-page-width="false" push-down="false" lock-width="false"><p style="Normal">Первая строка</p></text>
</region>
<region region-id="2" left="30" top="60" width="90" height="20">
<text use-page-width="false" push-down="false" lock-width="false"><p style="Normal"><f family="Arial" size="16">Заголовок</f></p></text>
</region>
</regions>
</worksheet>"#;
    let ws = import_xmcd(xml).unwrap().worksheet;
    let get = |id: u64| match &ws.regions.iter().find(|r| r.id == id).unwrap().kind {
        RegionKind::Text { style, width, lock_width, .. } => (style.clone(), *width, *lock_width),
        _ => panic!("not text"),
    };
    let (st, w, lock) = get(1);
    assert_eq!(st.size, 12.0);
    assert_eq!(st.font, TextFont::Serif);
    assert!((w - 200.0).abs() < 0.01, "{w}");
    assert!(lock, "the stored width must be kept");
    let (st, w, _) = get(2);
    assert_eq!(st.size, 16.0, "inline size beats the style");
    assert_eq!(st.font, TextFont::Sans);
    assert!((w - 120.0).abs() < 0.01, "{w}");
}

/// A multi-index `M[J, k]` arrives as an `<ml:sequence>` of the indices inside the indexer.
#[test]
fn multi_index_from_a_sequence() {
    let xml = r#"<worksheet xmlns="http://schemas.mathsoft.com/worksheet30" xmlns:ml="http://schemas.mathsoft.com/math30"><regions>
<region region-id="1" left="0" top="0" width="100" height="20"><math><ml:define><ml:id>M</ml:id><ml:matrix rows="2" cols="2"><ml:real>1</ml:real><ml:real>2</ml:real><ml:real>3</ml:real><ml:real>4</ml:real></ml:matrix></ml:define></math></region>
<region region-id="2" left="0" top="30" width="100" height="20"><math><ml:define><ml:id>J</ml:id><ml:real>0</ml:real></ml:define></math></region>
<region region-id="3" left="0" top="60" width="100" height="20"><math><ml:define><ml:id>k</ml:id><ml:real>1</ml:real></ml:define></math></region>
<region region-id="4" left="0" top="90" width="100" height="20"><math><ml:eval><ml:apply><ml:indexer/><ml:id>M</ml:id><ml:sequence><ml:id>J</ml:id><ml:id>k</ml:id></ml:sequence></ml:apply></ml:eval></math></region>
</regions></worksheet>"#;
    let r = import_xmcd(xml).expect("import");
    assert_eq!(source(&r.worksheet, 4), "M[J, k] =");
    // Mathcad stores matrix elements column by column: [[1, 3], [2, 4]], so M[0, 1] is 3
    assert_eq!(shown(&r.worksheet, 4), "3");
}
