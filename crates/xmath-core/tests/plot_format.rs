use xmath_core::{AxisStyle, LineStyle, RegionKind, TraceFormat, Worksheet};

#[test]
fn old_plot_json_loads_with_defaults() {
    let ws = Worksheet::from_json(r#"{"regions":[{"id":1,"x":0,"y":0,"type":"plot","x_expr":"t","y_exprs":["sin(t)"]}]}"#).unwrap();
    match &ws.regions[0].kind {
        RegionKind::Plot { x_min, y_max, fmt, .. } => {
            assert!(x_min.is_none() && y_max.is_none());
            assert!(fmt.is_default());
        }
        _ => panic!("not a plot"),
    }
    // defaults are not written out
    let json = ws.to_json();
    assert!(!json.contains("x_min") && !json.contains("fmt"));
}

#[test]
fn plot_format_roundtrip() {
    let mut ws = Worksheet::default();
    let mut k = RegionKind::new_plot();
    if let RegionKind::Plot { x_min, fmt, .. } = &mut k {
        *x_min = Some(-5.0);
        fmt.grid = false;
        fmt.axes = AxisStyle::Boxed;
        fmt.log_y = true;
        fmt.traces.push(TraceFormat { color: Some([1, 2, 3]), line: Some(LineStyle::Dash), width: Some(3.0) });
    }
    ws.regions.push(xmath_core::Region { id: 1, x: 0.0, y: 0.0, kind: k });
    let back = Worksheet::from_json(&ws.to_json()).unwrap();
    assert_eq!(back, ws);
}
