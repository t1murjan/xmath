//! The symbolic example worksheet evaluates without errors.

use xmath_core::{evaluate_worksheet, Output, RegionKind, Worksheet};

#[test]
fn example_worksheet_has_no_errors() {
    let ws = Worksheet::from_json(include_str!("../../../examples/symbolic.xmath")).unwrap();
    let res = evaluate_worksheet(&ws);
    let mut symbolic = 0;
    for r in &ws.regions {
        if let RegionKind::Math { source } = &r.kind {
            let o = &res[&r.id];
            assert!(o.error().is_none(), "{source}: {:?}", o.error());
            if matches!(o.output, Output::Symbolic { .. }) {
                symbolic += 1;
            }
        }
    }
    assert!(symbolic >= 40, "{symbolic}");
}

#[test]
fn imported_sym_eval_becomes_symbolic_statement() {
    let xml = r#"<worksheet xmlns="http://schemas.mathsoft.com/worksheet30" xmlns:ml="http://schemas.mathsoft.com/math30"><regions><region region-id="1" left="10" top="10" width="100" height="20" align-x="20" align-y="19"><math><ml:symEval><ml:apply><ml:plus/><ml:real>1</ml:real><ml:real>2</ml:real></ml:apply><ml:symResult><ml:real>3</ml:real></ml:symResult></ml:symEval></math></region></regions></worksheet>"#;
    let r = xmath_core::xmcd::import_xmcd(xml).unwrap();
    assert!(r.warnings.iter().all(|w| !w.contains("заменено численным")), "{:?}", r.warnings);
    match &r.worksheet.regions[0].kind {
        RegionKind::Math { source } => assert_eq!(source, "1 + 2 →"),
        _ => panic!(),
    }
    let rep = xmath_core::xmcd_check::check_import(&r);
    assert_eq!(rep.ok(), 1, "{rep:?}");
}
