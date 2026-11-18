use xmath_core::{evaluate_worksheet, Output, Plot3dKind, PlotKind, Region, RegionKind, Worksheet};

fn sheet(lines: &[&str], kind: RegionKind) -> (Worksheet, u64) {
    let mut ws = Worksheet::from_lines(&lines.join("\n"));
    let id = ws.next_id();
    ws.regions.push(Region::new(id, 20.0, 1000.0, kind));
    (ws, id)
}

#[test]
fn polar_quickplot_sweeps_zero_to_two_pi() {
    let mut k = RegionKind::new_polar_plot();
    if let RegionKind::Plot { y, .. } = &mut k {
        y[0] = "1 + cos(t)".into();
    }
    let (ws, id) = sheet(&[], k);
    let out = evaluate_worksheet(&ws);
    match &out[&id].output {
        Output::Plot(d) => {
            assert_eq!(d.kind, PlotKind::Polar);
            let s = &d.series[0];
            assert!((s.points[0][0]).abs() < 1e-12);
            assert!((s.points.last().unwrap()[0] - std::f64::consts::TAU).abs() < 1e-9);
            assert!((s.points[0][1] - 2.0).abs() < 1e-12);
        }
        o => panic!("{o:?}"),
    }
}

#[test]
fn polar_with_range_variable() {
    let mut k = RegionKind::new_polar_plot();
    if let RegionKind::Plot { x, y, .. } = &mut k {
        *x = "th".into();
        y[0] = "cos(3*th)".into();
    }
    let (ws, id) = sheet(&["th := 0, 0.1 .. 6.3"], k);
    match &evaluate_worksheet(&ws)[&id].output {
        Output::Plot(d) => assert_eq!(d.series[0].points.len(), 64),
        o => panic!("{o:?}"),
    }
}

#[test]
fn surface_of_function_and_matrix() {
    let (ws, id) = sheet(&[], {
        let mut k = RegionKind::new_plot3d(Plot3dKind::Surface);
        if let RegionKind::Plot3d { expr, .. } = &mut k {
            *expr = "sin(x)*cos(y)".into();
        }
        k
    });
    match &evaluate_worksheet(&ws)[&id].output {
        Output::Plot3d(d) => {
            assert_eq!(d.z.len(), d.xs.len() * d.ys.len());
            assert!(d.error.is_none());
            let (lo, hi) = d.z_range().unwrap();
            assert!(lo < -0.9 && hi > 0.9);
        }
        o => panic!("{o:?}"),
    }
    let (ws, id) = sheet(&["M := [1, 2, 3; 4, 5, 6]"], {
        let mut k = RegionKind::new_plot3d(Plot3dKind::Contour);
        if let RegionKind::Plot3d { expr, .. } = &mut k {
            *expr = "M".into();
        }
        k
    });
    match &evaluate_worksheet(&ws)[&id].output {
        Output::Plot3d(d) => {
            assert_eq!((d.nx(), d.ny()), (2, 3));
            assert_eq!(d.at(1, 2), 6.0);
        }
        o => panic!("{o:?}"),
    }
}

#[test]
fn surface_of_named_function_and_scatter() {
    let (ws, id) = sheet(&["f(a, b) := a*b"], {
        let mut k = RegionKind::new_plot3d(Plot3dKind::Surface);
        if let RegionKind::Plot3d { expr, .. } = &mut k {
            *expr = "f".into();
        }
        k
    });
    match &evaluate_worksheet(&ws)[&id].output {
        Output::Plot3d(d) => {
            assert!(d.error.is_none(), "{:?}", d.error);
            assert_eq!(d.at(d.nx() - 1, d.ny() - 1), 25.0);
        }
        o => panic!("{o:?}"),
    }
    let (ws, id) = sheet(&["X := [1; 2; 3]", "Y := [4; 5; 6]", "Z := [7; 8; 9]"], {
        let mut k = RegionKind::new_plot3d(Plot3dKind::Scatter);
        if let RegionKind::Plot3d { expr, .. } = &mut k {
            *expr = "(X, Y, Z)".into();
        }
        k
    });
    match &evaluate_worksheet(&ws)[&id].output {
        Output::Plot3d(d) => assert_eq!(d.pts, vec![[1.0, 4.0, 7.0], [2.0, 5.0, 8.0], [3.0, 6.0, 9.0]]),
        o => panic!("{o:?}"),
    }
}

#[test]
fn plot3d_errors_are_reported() {
    let (ws, id) = sheet(&["a := 5"], {
        let mut k = RegionKind::new_plot3d(Plot3dKind::Surface);
        if let RegionKind::Plot3d { expr, .. } = &mut k {
            *expr = "a".into();
        }
        k
    });
    match &evaluate_worksheet(&ws)[&id].output {
        Output::Plot3d(d) => assert!(d.error.is_some()),
        o => panic!("{o:?}"),
    }
}

#[test]
fn old_files_load_and_new_kinds_roundtrip() {
    // a file written before polar/3D plots existed
    let ws = Worksheet::from_json(r#"{"regions":[{"id":1,"x":0,"y":0,"type":"plot","x_expr":"t","y_exprs":["sin(t)"],"fmt":{"grid":false,"log_y":true,"traces":[{"color":[1,2,3]}]}}]}"#).unwrap();
    match &ws.regions[0].kind {
        RegionKind::Plot { kind, fmt, .. } => {
            assert_eq!(*kind, PlotKind::Xy);
            assert!(!fmt.grid && fmt.log_y && !fmt.y2_axis && fmt.show_title);
            assert_eq!(fmt.traces[0].color, Some([1, 2, 3]));
        }
        _ => panic!(),
    }
    assert!(ws.plot_defaults.is_none());
    let mut w = Worksheet::default();
    w.regions.push(Region::new(1, 0.0, 0.0, RegionKind::new_polar_plot()));
    w.regions.push(Region::new(2, 0.0, 0.0, RegionKind::new_plot3d(Plot3dKind::Bar)));
    let json = w.to_json();
    assert!(json.contains("\"polar\"") && json.contains("\"plot3d\"") && json.contains("\"bar\""));
    assert_eq!(Worksheet::from_json(&json).unwrap(), w);
    // a minimal 3D region loads with defaults
    let w = Worksheet::from_json(r#"{"regions":[{"id":1,"x":0,"y":0,"type":"plot3d","expr":"M"}]}"#).unwrap();
    assert!(matches!(w.regions[0].kind, RegionKind::Plot3d { kind: Plot3dKind::Surface, .. }));
}

#[test]
fn axis_units_are_reported() {
    let mut k = RegionKind::new_plot();
    if let RegionKind::Plot { x, y, .. } = &mut k {
        *x = "t".into();
        y[0] = "t*3*m/s".into();
    }
    let (ws, id) = sheet(&["t := 0*s, 1*s .. 5*s"], k);
    match &evaluate_worksheet(&ws)[&id].output {
        Output::Plot(d) => {
            assert_eq!(d.x_unit, "s");
            assert_eq!(d.y_unit, "m");
        }
        o => panic!("{o:?}"),
    }
}

#[test]
fn example_plots_worksheet_evaluates_without_errors() {
    let ws = Worksheet::from_json(include_str!("../../../examples/plots.xmath")).unwrap();
    let out = evaluate_worksheet(&ws);
    let mut kinds = (0, 0, 0, 0);
    for r in &ws.regions {
        let o = &out[&r.id];
        assert!(o.error().is_none(), "{:?}: {:?}", r.kind, o.error());
        match &o.output {
            Output::Plot(d) => {
                assert!(d.errors.is_empty(), "{:?}", d.errors);
                assert!(!d.series.is_empty());
                if d.kind == PlotKind::Polar {
                    kinds.0 += 1;
                } else {
                    kinds.1 += 1;
                }
            }
            Output::Plot3d(d) => {
                assert!(d.error.is_none(), "{:?}", d.error);
                assert!(!d.is_empty());
                kinds.2 += 1;
                if d.kind == Plot3dKind::Scatter {
                    kinds.3 += 1;
                }
            }
            _ => {}
        }
    }
    assert_eq!(kinds, (1, 3, 4, 1));
}
