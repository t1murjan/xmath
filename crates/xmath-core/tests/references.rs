//! Worksheet references: the definitions of the referenced worksheet are visible below.

use std::path::PathBuf;
use xmath_core::worksheet::Region;
use xmath_core::xmcd_export::export_xmcd;
use xmath_core::{evaluate_worksheet_in, Output, RegionKind, Worksheet};

fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("xmath-ref-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn value(ws: &Worksheet, dir: &std::path::Path, id: u64) -> String {
    let out = evaluate_worksheet_in(ws, dir);
    match &out[&id].output {
        Output::Value { view, .. } => view.to_string(),
        Output::Error(e) => format!("ERR {e}"),
        _ => String::new(),
    }
}

#[test]
fn reference_to_xmath_and_xmcd() {
    let d = tmp("a");
    std::fs::create_dir_all(d.join("lib")).unwrap();
    let lib = Worksheet::from_lines("k := 6\nsq(x) := x^2");
    std::fs::write(d.join("lib/consts.xmath"), lib.to_json()).unwrap();
    std::fs::write(d.join("other.xmcd"), export_xmcd(&Worksheet::from_lines("m := 7")).xml).unwrap();

    let mut ws = Worksheet::default();
    ws.regions.push(Region::new(1, 20.0, 20.0, RegionKind::Reference { href: "lib/consts.xmath".into() }));
    ws.regions.push(Region::new(2, 20.0, 60.0, RegionKind::Reference { href: "other.xmcd".into() }));
    ws.regions.push(Region::new(3, 20.0, 100.0, RegionKind::Math { source: "sq(k) + m =".into() }));
    // below the references only
    ws.regions.push(Region::new(4, 20.0, 5.0, RegionKind::Math { source: "k =".into() }));
    assert_eq!(value(&ws, &d, 3), "43");
    assert!(value(&ws, &d, 4).starts_with("ERR"), "{}", value(&ws, &d, 4));
}

#[test]
fn missing_and_cyclic_references() {
    let d = tmp("b");
    let mut a = Worksheet::default();
    a.regions.push(Region::new(1, 0.0, 0.0, RegionKind::Reference { href: "b.xmath".into() }));
    let mut b = Worksheet::default();
    b.regions.push(Region::new(1, 0.0, 0.0, RegionKind::Reference { href: "a.xmath".into() }));
    b.regions.push(Region::new(2, 0.0, 30.0, RegionKind::Math { source: "z := 1".into() }));
    std::fs::write(d.join("a.xmath"), a.to_json()).unwrap();
    std::fs::write(d.join("b.xmath"), b.to_json()).unwrap();
    // a -> b -> a: the cycle is cut, nothing hangs
    let out = evaluate_worksheet_in(&a, &d);
    assert!(matches!(out[&1].output, Output::None));

    let mut c = Worksheet::default();
    c.regions.push(Region::new(1, 0.0, 0.0, RegionKind::Reference { href: "nope.xmath".into() }));
    let out = evaluate_worksheet_in(&c, &d);
    assert!(matches!(out[&1].output, Output::Error(_)));
}

#[test]
fn imported_reference_region() {
    let xml = r#"<?xml version="1.0"?>
<worksheet xmlns="http://schemas.mathsoft.com/worksheet30" xmlns:ml="http://schemas.mathsoft.com/math30">
<regions>
<region region-id="1" left="24" top="132" width="100" height="10" align-x="24" align-y="132">
<reference><originRef xmlns="http://schemas.mathsoft.com/provenance10" href="C:\Docs\Lib\Consts.xmcd"/></reference>
<link href="./Consts.xmcd"/>
<rendering item-idref="1"/>
</region>
</regions>
<binaryContent><item item-id="1">iVBORw0KGgo=</item></binaryContent>
</worksheet>"#;
    let ws = xmath_core::xmcd::import_xmcd(xml).unwrap().worksheet;
    assert!(matches!(&ws.regions[0].kind, RegionKind::Reference { href } if href == "Consts.xmcd"));
}
