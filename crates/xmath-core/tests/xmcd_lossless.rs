//! `.xmcd` is the default document format: a worksheet saved as `.xmcd` and opened again must be
//! the same worksheet (positions, kinds, styles, plot formats, page, defaults) and evaluate to
//! the same results, plots included.

use std::collections::HashMap;
use xmath_core::worksheet::{Region, RegionKind};
use xmath_core::xmcd::import_xmcd;
use xmath_core::xmcd_export::export_xmcd;
use xmath_core::{parse_statement, Worksheet};

fn near(a: f32, b: f32) -> bool {
    (a - b).abs() < 0.1
}

/// Math is compared by its canonical statement (the exporter writes the syntax tree, so spacing
/// and redundant parentheses are not part of the model); everything else exactly, apart from the
/// sizes and positions that Mathcad stores in points with two decimals.
fn same_region(a: &Region, b: &Region) -> Result<(), String> {
    if a.id != b.id {
        return Err(format!("id {} != {}", a.id, b.id));
    }
    if !near(a.x, b.x) || !near(a.y, b.y) {
        return Err(format!("position ({}, {}) != ({}, {})", a.x, a.y, b.x, b.y));
    }
    if !near(a.anchor[0], b.anchor[0]) || !near(a.anchor[1], b.anchor[1]) {
        return Err(format!("anchor {:?} != {:?}", a.anchor, b.anchor));
    }
    match (&a.kind, &b.kind) {
        (RegionKind::Math { source: s }, RegionKind::Math { source: t }) => {
            // a one-statement program is the statement itself: Mathcad's schema requires two
            // statements, so the exporter drops the braces
            let canon = |x: &str| {
                let x = single_statement(x);
                parse_statement(&x).map(|s| s.to_string()).unwrap_or(x)
            };
            if canon(s) != canon(t) {
                return Err(format!("math «{s}» != «{t}»"));
            }
        }
        (
            RegionKind::Text { text: t1, width: w1, style: s1, lock_width: l1 },
            RegionKind::Text { text: t2, width: w2, style: s2, lock_width: l2 },
        ) => {
            if t1 != t2 || !near(*w1, *w2) || s1 != s2 || l1 != l2 {
                return Err(format!("text {a:?} != {b:?}"));
            }
        }
        (RegionKind::Image { png_base64: p1, width: w1, height: h1 }, RegionKind::Image { png_base64: p2, width: w2, height: h2 }) => {
            if p1 != p2 || !near(*w1, *w2) || !near(*h1, *h2) {
                return Err("picture differs".into());
            }
        }
        (x, y) => {
            if x != y {
                return Err(format!("{x:?} != {y:?}"));
            }
        }
    }
    Ok(())
}

/// The differences between two worksheets (empty: equal).
pub fn differences(a: &Worksheet, b: &Worksheet) -> Vec<String> {
    let mut d = Vec::new();
    if a.format != b.format {
        d.push(format!("format {:?} != {:?}", a.format, b.format));
    }
    if a.plot_defaults != b.plot_defaults {
        d.push("plot_defaults".into());
    }
    if a.page != b.page {
        d.push(format!("page {:?} != {:?}", a.page, b.page));
    }
    if a.regions.len() != b.regions.len() {
        d.push(format!("{} regions != {}", a.regions.len(), b.regions.len()));
        return d;
    }
    let by_id: HashMap<u64, &Region> = b.regions.iter().map(|r| (r.id, r)).collect();
    for r in &a.regions {
        match by_id.get(&r.id) {
            None => d.push(format!("region {} lost", r.id)),
            Some(q) => {
                if let Err(e) = same_region(r, q) {
                    d.push(format!("region {}: {e}", r.id));
                }
            }
        }
    }
    // evaluation: the same values and plots (compared as text: plot data holds NaN). Worksheets
    // with random numbers or random solver starts evaluate differently every time: such regions
    // are not compared.
    let text = |o: &HashMap<u64, xmath_core::RegionOutput>, id: u64| format!("{:?} {:?}", o[&id].output, o[&id].value);
    let (oa, ob) = (a.evaluate_opts(true), b.evaluate_opts(true));
    let mut more: Option<(Vec<HashMap<u64, xmath_core::RegionOutput>>, Vec<HashMap<u64, xmath_core::RegionOutput>>)> = None;
    for r in &a.regions {
        let (sx, sy) = (text(&oa, r.id), text(&ob, r.id));
        if sx == sy {
            continue;
        }
        let (ma, mb) = more.get_or_insert_with(|| ((0..4).map(|_| a.evaluate_opts(true)).collect(), (0..4).map(|_| b.evaluate_opts(true)).collect()));
        let xs: Vec<String> = ma.iter().map(|o| text(o, r.id)).chain([sx.clone()]).collect();
        // random in the original: nothing to compare
        if xs.iter().any(|t| *t != xs[0]) || mb.iter().map(|o| text(o, r.id)).chain([sy.clone()]).any(|t| xs.contains(&t)) {
            continue;
        }
        let k = sx.chars().zip(sy.chars()).take_while(|(a, b)| a == b).count();
        let ctx = |s: &str| s.chars().skip(k.saturating_sub(60)).take(160).collect::<String>();
        d.push(format!("region {}: results differ near {k}: «{}» != «{}»", r.id, ctx(&sx), ctx(&sy)));
    }
    d
}

fn roundtrip(ws: &Worksheet) -> Worksheet {
    let ex = export_xmcd(ws);
    import_xmcd(&ex.xml).unwrap_or_else(|e| panic!("import of the exported file failed: {e}")).worksheet
}

#[test]
fn examples_survive_xmcd() {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../examples");
    let mut n = 0;
    for e in std::fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.extension().is_some_and(|x| x == "xmath") {
            let ws = Worksheet::from_json(&std::fs::read_to_string(&p).unwrap()).unwrap();
            let back = roundtrip(&ws);
            let d = differences(&ws, &back);
            assert!(d.is_empty(), "{}: {} differences after the round trip:\n{}", p.display(), d.len(), d.join("\n"));
            // and once more: saving what was opened is stable
            let again = roundtrip(&back);
            assert!(differences(&back, &again).is_empty(), "{}: second round trip changed the worksheet", p.display());
            n += 1;
        }
    }
    assert!(n >= 7, "examples not found");
}

/// `XMCD_CORPUS=dir cargo test -p xmath-core --test xmcd_lossless -- --ignored --nocapture`:
/// real Mathcad files (private) are imported, exported and imported again, plots included.
#[test]
#[ignore]
fn corpus_survives_xmcd() {
    let Ok(dir) = std::env::var("XMCD_CORPUS") else { return };
    let mut entries: Vec<_> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().path()).collect();
    entries.sort();
    let (mut files, mut bad_files, mut plots) = (0, 0, 0);
    for p in entries {
        let Ok(bytes) = std::fs::read(&p) else { continue };
        let Ok(imp) = import_xmcd(&String::from_utf8_lossy(&bytes)) else { continue };
        files += 1;
        plots += imp.worksheet.regions.iter().filter(|r| matches!(r.kind, RegionKind::Plot { .. })).count();
        let back = roundtrip(&imp.worksheet);
        let d = differences(&imp.worksheet, &back);
        if !d.is_empty() {
            bad_files += 1;
            println!("{}: {} differences, first: {}", p.display(), d.len(), d.iter().take(2).cloned().collect::<Vec<_>>().join(" ;; "));
        }
    }
    println!("{files} files, {plots} imported X-Y plots, {bad_files} files changed by the round trip");
    assert_eq!(bad_files, 0);
}

/// Every field of the plot model, set to something other than the default.
#[test]
fn every_plot_field_survives() {
    use xmath_core::worksheet::{AxisLabel, AxisOpts, LegendPos, NumFormat, TextFont, TextStyle, TraceFormat};
    use xmath_core::{AxisStyle, Colormap, LineStyle, NumKind, Paper, PlotFormat, Plot3dFormat, Plot3dKind, PlotKind, Symbol, TraceType, UnitSystem, View3d};
    let mut ws = Worksheet::default();
    ws.format = xmath_core::FormatOptions { precision: 7, exp_threshold: 5, zero_threshold: 12, unit_system: UnitSystem::Cgs };
    ws.page.paper = Paper::A3;
    ws.page.landscape = true;
    ws.page.margins = [10.0, 12.0, 14.0, 16.0];
    ws.page.within_margins = true;
    let mut pd = PlotFormat::new_plot_default();
    pd.grid = false;
    pd.title = "по умолчанию".into();
    ws.plot_defaults = Some(pd);
    let mut fmt = PlotFormat {
        grid: false,
        axes: AxisStyle::Boxed,
        log_x: true,
        log_y: true,
        log_y2: true,
        legend: true,
        hide_legend: false,
        legend_pos: LegendPos::TopLeft,
        hide_args: true,
        y2_axis: true,
        y2_min: Some(-2.5),
        y2_max: Some(40.0),
        equal_scale: true,
        number: NumFormat { kind: NumKind::Scientific, precision: 5, trailing_zeros: true, eng_exponents: true, exp_threshold: 4 },
        title: "Заголовок -- с двойным дефисом <&>".into(),
        title_bottom: true,
        show_title: false,
        labels: [AxisLabel { show: false, text: "x, мм".into() }, AxisLabel { show: true, text: "y--y".into() }, AxisLabel { show: true, text: "y2".into() }],
        ..Default::default()
    };
    fmt.axis[0] = AxisOpts { grid: Some(true), grid_color: [1, 2, 3], numbering: false, markers: true, marker_color: [9, 8, 7], autogrid: false, grids: 4, marker_values: "0.5, 2".into() };
    fmt.axis[1].grid = Some(false);
    for (i, line) in [LineStyle::Solid, LineStyle::Dot, LineStyle::Dash, LineStyle::DashDot, LineStyle::Points].into_iter().enumerate() {
        fmt.traces.push(TraceFormat {
            color: Some([10 * i as u8, 200, 255 - 10 * i as u8]),
            line: Some(line),
            width: Some(1.0 + i as f32),
            legend: format!("кривая {i}"),
            symbol: Symbol::ALL[i % Symbol::ALL.len()],
            symbol_freq: Some(i as u32 + 1),
            symbol_width: Some(4.0),
            kind: TraceType::ALL[i % TraceType::ALL.len()],
            y2: i % 2 == 1,
        });
    }
    let plot = |x: &str, y: &[&str], fmt: PlotFormat, kind| RegionKind::Plot {
        x: x.into(),
        y: y.iter().map(|s| s.to_string()).collect(),
        width: 333.5,
        height: 211.25,
        x_min: Some(-1.5),
        x_max: Some(12.0),
        y_min: Some(0.0),
        y_max: Some(3e-7),
        xs: vec![],
        fmt,
        kind,
    };
    ws.regions.push(Region::new(1, 20.0, 20.0, plot("t", &["sin(t)", "cos(t)^2", "f(t)/2", "a[1]", "sqrt(t) + 1", "-t", "exp(-t)"], fmt, PlotKind::Xy)));
    ws.regions.push(Region::new(2, 20.0, 300.0, plot("", &["x^2"], PlotFormat::default(), PlotKind::Xy)));
    let mut multi = plot("x1", &["y1", "y2"], PlotFormat::default(), PlotKind::Xy);
    if let RegionKind::Plot { xs, .. } = &mut multi {
        *xs = vec!["x1".into(), "x2".into()];
    }
    ws.regions.push(Region::new(3, 20.0, 600.0, multi));
    ws.regions.push(Region::new(4, 400.0, 20.0, plot("theta", &["1 + cos(theta)"], PlotFormat::default(), PlotKind::Polar)));
    for (i, kind) in [Plot3dKind::Surface, Plot3dKind::Contour, Plot3dKind::Scatter, Plot3dKind::Bar].into_iter().enumerate() {
        ws.regions.push(Region::new(
            10 + i as u64,
            400.0,
            300.0 + 300.0 * i as f32,
            RegionKind::Plot3d {
                kind,
                expr: "M".into(),
                width: 301.0,
                height: 222.5,
                view: View3d { azimuth: 77.0, elevation: -12.0, zoom: 1.5 },
                fmt: Plot3dFormat { colormap: Colormap::Hot, fill: false, wireframe: false, levels: 17, labels: false, box_axes: false },
            },
        ));
    }
    ws.regions.push(Region::new(20, 30.0, 900.0, RegionKind::Math { source: "M := [1, 2; 3, 4]".into() }));
    ws.regions.push(Region::new(21, 30.0, 930.0, RegionKind::Math { source: "a := [1; 2]".into() }));
    for (i, font) in [TextFont::Sans, TextFont::Serif, TextFont::Mono].into_iter().enumerate() {
        ws.regions.push(Region::new(
            30 + i as u64,
            30.0,
            1000.0 + 40.0 * i as f32,
            RegionKind::Text {
                text: format!("Текст {i}\nвторая строка"),
                width: 250.0 + i as f32,
                style: TextStyle { size: 9.0 + i as f32, bold: i == 0, italic: i == 1, underline: i == 2, font },
                lock_width: i != 1,
            },
        ));
    }
    ws.regions.push(Region { id: 40, x: 500.0, y: 40.0, anchor: [12.0, 14.0], kind: RegionKind::Math { source: "b := 2".into() } });
    let back = roundtrip(&ws);
    let d = differences(&ws, &back);
    assert!(d.is_empty(), "{}", d.join("\n"));
}

/// An X-Y plot saved by xmath and edited in Mathcad (its expressions changed in the plot item)
/// is read from the plot item, not from the stale xmath comment.
#[test]
fn plot_edited_in_mathcad_wins() {
    let mut ws = Worksheet::default();
    ws.regions.push(Region::new(
        1,
        10.0,
        10.0,
        RegionKind::Plot {
            x: "t".into(),
            y: vec!["sin(t)".into()],
            width: 300.0,
            height: 200.0,
            x_min: None,
            x_max: None,
            y_min: None,
            y_max: None,
            xs: vec![],
            fmt: xmath_core::PlotFormat { title: "keep".into(), ..Default::default() },
            kind: Default::default(),
        },
    ));
    let xml = export_xmcd(&ws).xml;
    // a Mathcad user edits the plot item: simulate it by making the comment stale
    let stale = xml.replace("\"sin(t)\"", "\"cos(t)\"");
    assert_ne!(stale, xml);
    let back = import_xmcd(&stale).unwrap().worksheet;
    match &back.regions[0].kind {
        RegionKind::Plot { y, .. } => assert_eq!(y, &vec!["sin(t)".to_string()]),
        k => panic!("{k:?}"),
    }
}

/// The results stored in the exported file agree with what is computed (`--check` on our own files).
#[test]
fn exported_results_are_checked_ok() {
    let ws = Worksheet::from_lines("a := 3\nb := a^2 + 1/2\nb =\nv := [1; 2; 3]\nv =\nM := [1, 2; 3, 4]\nM =\nc := (1 + 2i)*(3 - i)\nc =\nd := 5 m/s\nd =\ns := \"текст\"\ns =\nf(x) := x^2\nf(4) =");
    let xml = export_xmcd(&ws).xml;
    assert!(xml.contains("<result "), "results are written");
    let imp = import_xmcd(&xml).unwrap();
    let report = xmath_core::xmcd_check::check_import(&imp);
    assert!(report.ok() >= 6, "{} ok of {}", report.ok(), report.regions.len());
    assert_eq!(report.mismatch(), 0, "{:?}", report.regions);
    assert_eq!(report.not_evaluated(), 0, "{:?}", report.regions);
}

#[test]
#[ignore]
fn debug_order() {
    let Ok(f) = std::env::var("XMCD_ONE") else { return };
    let imp = import_xmcd(&String::from_utf8_lossy(&std::fs::read(f).unwrap())).unwrap();
    let a = imp.worksheet;
    let b = roundtrip(&a);
    let ia: Vec<u64> = a.evaluation_order().iter().map(|&i| a.regions[i].id).collect();
    let ib: Vec<u64> = b.evaluation_order().iter().map(|&i| b.regions[i].id).collect();
    let k = ia.iter().zip(&ib).position(|(x, y)| x != y);
    println!("first order difference at {k:?}");
    if let Some(k) = k {
        for id in [ia[k], ib[k]] {
            let ra = a.regions.iter().find(|r| r.id == id).unwrap();
            let rb = b.regions.iter().find(|r| r.id == id).unwrap();
            println!("{id}: a {:?} {:?}  b {:?} {:?}", ra.anchor_pos(), ra.kind, rb.anchor_pos(), rb.kind);
        }
    }
}

#[test]
#[ignore]
fn debug_sources() {
    let Ok(f) = std::env::var("XMCD_ONE") else { return };
    let imp = import_xmcd(&String::from_utf8_lossy(&std::fs::read(f).unwrap())).unwrap();
    let a = imp.worksheet;
    let b = roundtrip(&a);
    for r in &a.regions {
        let q = b.regions.iter().find(|q| q.id == r.id).unwrap();
        if let (RegionKind::Math { source: s }, RegionKind::Math { source: t }) = (&r.kind, &q.kind) {
            if s != t {
                println!("{}: «{s}» -> «{t}»", r.id);
            }
        }
    }
}

#[test]
#[ignore]
fn debug_eval() {
    let Ok(f) = std::env::var("XMCD_ONE") else { return };
    let imp = import_xmcd(&String::from_utf8_lossy(&std::fs::read(f).unwrap())).unwrap();
    let a = imp.worksheet;
    let b = roundtrip(&a);
    let id = 797;
    let show = |w: &Worksheet| format!("{:?}", w.evaluate_opts(true)[&id].output).chars().take(80).collect::<String>();
    println!("a {} / {}", show(&a), show(&a));
    println!("b {} / {}", show(&b), show(&b));
    // a with the regions in the order of b
    let mut c = a.clone();
    c.regions = b.regions.iter().map(|r| a.regions.iter().find(|q| q.id == r.id).unwrap().clone()).collect();
    println!("c {}", show(&c));
    let ia: Vec<u64> = a.evaluation_order().iter().map(|&i| a.regions[i].id).collect();
    let ic: Vec<u64> = c.evaluation_order().iter().map(|&i| c.regions[i].id).collect();
    println!("order same: {}", ia == ic);
}

/// `name(x) := { stmt } -> name(x) := stmt`, and the same for a bare `{ stmt }`.
fn single_statement(s: &str) -> String {
    let Some(open) = s.find("{ ") else { return s.to_string() };
    let Some(close) = s.rfind(" }") else { return s.to_string() };
    if close + 2 != s.len() || close <= open {
        return s.to_string();
    }
    let inner = &s[open + 2..close];
    if inner.contains(';') || inner.contains('{') {
        return s.to_string();
    }
    format!("{}{}", &s[..open], inner)
}
