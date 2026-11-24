//! Export to Mathcad 15 `.xmcd` and back: the evaluation results must not change.

use std::collections::HashMap;
use xmath_core::xmcd::import_xmcd;
use xmath_core::xmcd_export::export_xmcd;
use xmath_core::{evaluate_worksheet, Output, RegionKind, Worksheet};

fn show(ws: &Worksheet) -> HashMap<u64, String> {
    let out = evaluate_worksheet(ws);
    ws.regions
        .iter()
        .map(|r| {
            let o = &out[&r.id];
            let s = match (&r.kind, &o.output) {
                (RegionKind::Math { source }, Output::Value { view, unit }) => {
                    let stmt = o.stmt.as_ref().and_then(|s| s.as_ref().ok()).map(|s| s.to_string()).unwrap_or(source.clone());
                    format!("{stmt} -> {view} {}", unit.as_ref().map(|u| u.to_string()).unwrap_or_default())
                }
                (RegionKind::Math { source }, Output::Symbolic { result, .. }) => {
                    let stmt = o.stmt.as_ref().and_then(|s| s.as_ref().ok()).map(|s| s.to_string()).unwrap_or(source.clone());
                    format!("{stmt} => {result}")
                }
                (RegionKind::Math { source }, Output::Error(e)) => format!("{source} ERR {e}"),
                (RegionKind::Math { source }, _) => {
                    o.stmt.as_ref().and_then(|s| s.as_ref().ok()).map(|s| s.to_string()).unwrap_or(source.clone())
                }
                (RegionKind::Text { text, .. }, _) => text.split_whitespace().collect::<Vec<_>>().join(" "),
                (RegionKind::Plot { .. }, _) => "PLOT".into(),
                (RegionKind::Plot3d { .. }, _) => "PLOT3D".into(),
                (RegionKind::Image { .. }, _) => "IMAGE".into(),
                (RegionKind::Reference { href }, _) => format!("REF {href}"),
            };
            (r.id, s)
        })
        .collect()
}

fn roundtrip(ws: &Worksheet) -> (Worksheet, Vec<String>) {
    let ex = export_xmcd(ws);
    let imp = import_xmcd(&ex.xml).unwrap_or_else(|e| panic!("import of the exported file failed: {e}\n{}", &ex.xml[..ex.xml.len().min(3000)]));
    (imp.worksheet, imp.warnings)
}

fn check_example(path: &str) {
    let src = std::fs::read_to_string(path).unwrap();
    let ws = Worksheet::from_json(&src).unwrap();
    let (back, warnings) = roundtrip(&ws);
    assert!(warnings.is_empty(), "{path}: import warnings {warnings:?}");
    let (a, b) = (show(&ws), show(&back));
    let mut bad = Vec::new();
    for r in &ws.regions {
        if matches!(r.kind, RegionKind::Plot { .. } | RegionKind::Plot3d { .. }) {
            continue; // plots are not exported
        }
        if a.get(&r.id) != b.get(&r.id) {
            bad.push(format!("#{}: {:?}  !=  {:?}", r.id, a.get(&r.id), b.get(&r.id)));
        }
    }
    assert!(bad.is_empty(), "{path}: {} regions differ after the round trip:\n{}", bad.len(), bad.join("\n"));
    // positions survive (to the precision of Mathcad's points)
    for r in &ws.regions {
        let q = back.regions.iter().find(|q| q.id == r.id).unwrap();
        assert!((q.x - r.x).abs() < 0.1 && (q.y - r.y).abs() < 0.1, "{path}: region {} moved", r.id);
    }
}

#[test]
fn examples_roundtrip() {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../examples");
    let mut n = 0;
    for e in std::fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.extension().is_some_and(|x| x == "xmath") {
            check_example(p.to_str().unwrap());
            n += 1;
        }
    }
    assert!(n >= 5, "examples not found");
}

#[test]
fn constructs_roundtrip() {
    let lines = [
        "a := 3",
        "b := a^2 + -a",
        "(a + b)*2 =",
        "x.max ≡ 10",
        "x.max/4 =",
        "f(x, y) := x^2 - 3*y",
        "f(2, 1) =",
        "v := [1; 2; 3]",
        "M := [1, 2, 3; 4, 5, 6]",
        "M[1, 2] =",
        "v[1] =",
        "|v| =",
        "i := 0..3",
        "w[i] := i^2",
        "k := 0, 0.5..2",
        "integrate(x^2, x, 0, 3) =",
        "diff(x^3, x) =",
        "diff(x^3, x, 2) =",
        "sum(n, 1, 10, n^2) =",
        "prod(n, 1, 5, n) =",
        "sum(i, i^2) =",
        "g(x) := {if x > 0 {1} otherwise {-1}}",
        "g(-4) =",
        "h(n) := { s ← 0; for i ∈ 1..n { s ← s + i }; s }",
        "h(5) =",
        "p := 5 m/s",
        "p = km/hr",
        "d := 2 m = cm",
        "sqrt(16) + nthroot(27, 3) =",
        "n! =",
        "T := transpose(M)",
        "vectorize(v^2) =",
        "\"текст\" =",
        "(1 + 2i) * (3 - i) =",
        "1/0 on error 7 =",
        "a > 2 =",
        "a ≥ 5 and a < 6 =",
    ];
    let ws = Worksheet::from_lines(&lines.join("\n"));
    let (back, warnings) = roundtrip(&ws);
    assert!(warnings.is_empty(), "{warnings:?}");
    let (a, b) = (show(&ws), show(&back));
    for r in &ws.regions {
        assert_eq!(a[&r.id], b[&r.id], "region {}", r.id);
    }
}

#[test]
fn text_picture_and_reference_roundtrip() {
    use xmath_core::worksheet::{Region, TextStyle};
    let mut ws = Worksheet::default();
    ws.format.precision = 5;
    ws.page.landscape = true;
    ws.page.paper = xmath_core::Paper::Letter;
    ws.regions.push(Region::new(
        1,
        30.0,
        20.0,
        RegionKind::Text { text: "Заголовок\nвторая строка & <теги>".into(), width: 300.0, style: TextStyle { size: 14.0, bold: true, italic: true, underline: false, ..Default::default() }, lock_width: true },
    ));
    ws.regions.push(Region::new(2, 40.0, 100.0, RegionKind::Reference { href: "lib/other.xmcd".into() }));
    // a 1x1 PNG
    let png = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==";
    ws.regions.push(Region::new(3, 40.0, 150.0, RegionKind::Image { png_base64: png.into(), width: 50.0, height: 40.0 }));
    ws.regions.push(Region::new(4, 40.0, 220.0, RegionKind::Plot {
        x: "t".into(),
        y: vec!["sin(t)".into()],
        width: 300.0,
        height: 200.0,
        x_min: None,
        x_max: None,
        y_min: None,
        y_max: None,
        xs: vec![],
        fmt: Default::default(),
        kind: Default::default(),
    }));
    ws.regions.push(Region::new(5, 40.0, 440.0, RegionKind::new_plot3d(xmath_core::Plot3dKind::Surface)));
    let ex = export_xmcd(&ws);
    assert_eq!(ex.warnings.len(), 1, "{:?}", ex.warnings); // the 3D plot is a picture for Mathcad
    let back = import_xmcd(&ex.xml).unwrap().worksheet;
    assert_eq!(back.format.precision, 5);
    assert!(back.page.landscape);
    assert_eq!(back.page.paper, xmath_core::Paper::Letter);
    match &back.regions.iter().find(|r| r.id == 1).unwrap().kind {
        RegionKind::Text { text, style, .. } => {
            assert_eq!(text, "Заголовок\nвторая строка & <теги>");
            assert!(style.bold && style.italic && style.size == 14.0);
        }
        k => panic!("{k:?}"),
    }
    assert!(matches!(&back.regions.iter().find(|r| r.id == 2).unwrap().kind, RegionKind::Reference { href } if href == "lib/other.xmcd"));
    assert!(matches!(&back.regions.iter().find(|r| r.id == 3).unwrap().kind, RegionKind::Image { png_base64, .. } if png_base64 == png));
    assert!(matches!(&back.regions.iter().find(|r| r.id == 4).unwrap().kind, RegionKind::Plot { y, .. } if y == &vec!["sin(t)".to_string()]));
    assert!(matches!(&back.regions.iter().find(|r| r.id == 5).unwrap().kind, RegionKind::Plot3d { .. }));
}

/// The exporter output has the structure of real Mathcad files (checked in a lenient way):
/// well-formed XML in the worksheet30 / math30 namespaces.
#[test]
fn well_formed() {
    let ws = Worksheet::from_lines("a := 1\nf(x) := x^2\nf(a) =");
    let ex = export_xmcd(&ws);
    let doc = roxmltree::Document::parse(&ex.xml).unwrap();
    assert_eq!(doc.root_element().tag_name().name(), "worksheet");
    assert_eq!(doc.root_element().tag_name().namespace(), Some("http://schemas.mathsoft.com/worksheet30"));
    let n = doc.descendants().filter(|n| n.tag_name().namespace() == Some("http://schemas.mathsoft.com/math30")).count();
    assert!(n > 10);
}

/// `XMCD_CORPUS=/path/to/dir cargo test --test xmcd_export -- --ignored`: real Mathcad files are
/// imported, exported and imported again; the results must be the same.
#[test]
#[ignore]
fn corpus_roundtrip() {
    let Ok(dir) = std::env::var("XMCD_CORPUS") else { return };
    let (mut files, mut bad_files, mut regions, mut bad) = (0, 0, 0, 0);
    let mut entries: Vec<_> = std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().path()).collect();
    entries.sort();
    for p in entries {
        let Ok(bytes) = std::fs::read(&p) else { continue };
        let Ok(imp) = import_xmcd(&String::from_utf8_lossy(&bytes)) else { continue };
        files += 1;
        let ws = imp.worksheet;
        let ex = export_xmcd(&ws);
        let back = match import_xmcd(&ex.xml) {
            Ok(b) => b.worksheet,
            Err(e) => {
                println!("{}: re-import failed: {e}", p.display());
                bad_files += 1;
                continue;
            }
        };
        let (a, b) = (show(&ws), show(&back));
        let mut file_bad = 0;
        for r in &ws.regions {
            if matches!(r.kind, RegionKind::Plot { .. } | RegionKind::Plot3d { .. } | RegionKind::Image { .. }) {
                continue;
            }
            regions += 1;
            if a.get(&r.id) != b.get(&r.id) {
                file_bad += 1;
                if file_bad <= 3 {
                    println!("{} #{}: {:?} != {:?}", p.file_name().unwrap().to_string_lossy(), r.id, a.get(&r.id), b.get(&r.id));
                }
            }
        }
        bad += file_bad;
    }
    println!("{files} files, {bad_files} failed to re-import, {regions} regions, {bad} differ");
    assert_eq!(bad_files, 0);
    assert_eq!(bad, 0);
}

/// The XML a worksheet is exported to has to be what Mathcad 15 writes, or Mathcad will show an
/// error instead of the formula. A range sum is the `summation` operator with a lambda and no
/// bounds there, not a private `rangeSum` element.
#[test]
fn exported_xml_uses_mathcad_spellings() {
    let mut ws = Worksheet::default();
    for (source, y) in [
        ("v := 0..5", 10.0),
        ("s := sum(k, 1, 10, k^2)", 40.0),
        ("t := sum(x, x^2)", 70.0),
        ("p := prod(x, x)", 100.0),
    ] {
        let id = ws.next_id();
        ws.regions.push(xmath_core::worksheet::Region::new(id, 20.0, y, RegionKind::Math { source: source.to_string() }));
    }
    let xml = export_xmcd(&ws).xml;
    assert!(!xml.contains("rangeSum"), "a private element would not open in Mathcad");
    assert!(!xml.contains("rangeProduct"), "a private element would not open in Mathcad");
    assert!(xml.contains("<ml:summation"), "range sums are summations with a lambda");
    assert!(xml.contains("<ml:product"), "range products are products with a lambda");

    // and the file reads back to the same worksheet
    let back = import_xmcd(&xml).expect("import");
    let sources: Vec<String> = back
        .worksheet
        .regions
        .iter()
        .filter_map(|r| match &r.kind {
            RegionKind::Math { source } => Some(source.clone()),
            _ => None,
        })
        .collect();
    assert!(sources.iter().any(|s| s.contains("sum(k, 1, 10")), "{sources:?}");
    assert!(sources.iter().any(|s| s.contains("sum(x")), "{sources:?}");
}

/// The sections Mathcad 15 writes have to be there, in its order.
#[test]
fn exported_structure_matches_mathcad() {
    let ws = Worksheet::default();
    let xml = export_xmcd(&ws).xml;
    let order = ["<pointReleaseData", "<metadata", "<settings", "<regions", "<binaryContent"];
    let mut at = 0;
    for tag in order {
        let pos = xml.find(tag).unwrap_or_else(|| panic!("no {tag}"));
        assert!(pos >= at, "{tag} is out of order");
        at = pos;
    }
    assert!(xml.starts_with("<?xml"));
    assert!(xml.contains("xmlns=\"http://schemas.mathsoft.com/worksheet30\""));
    assert!(xml.contains("<presentation"));
    assert!(xml.contains("<pageModel"));
    assert!(xml.contains("<calculation"));
}
