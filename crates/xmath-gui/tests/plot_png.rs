//! The pictures stored next to plots in `.xmcd` files.

use xmath_core::{RegionKind, Worksheet};
use xmath_gui::print::{render_plot_pngs, PNG_SCALE};

fn examples_plots() -> Worksheet {
    let src = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../examples/plots.xmath")).unwrap();
    Worksheet::from_json(&src).unwrap()
}

#[test]
fn every_plot_gets_a_picture() {
    let ws = examples_plots();
    let outputs = ws.evaluate();
    let pngs = render_plot_pngs(&ws, &outputs);
    let plots = ws.regions.iter().filter(|r| matches!(r.kind, RegionKind::Plot { .. } | RegionKind::Plot3d { .. })).count();
    assert!(plots >= 5, "{plots} plots");
    assert_eq!(pngs.len(), plots);
    for (id, png) in &pngs {
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n", "region {id}");
        let img = image::load_from_memory(png).unwrap().to_rgba8();
        let dark = img.pixels().filter(|p| p.0[0] < 200 || p.0[1] < 200 || p.0[2] < 200).count();
        assert!(dark > 200, "region {id}: the picture is blank ({dark} dark pixels)");
        assert!(img.width() as f32 / PNG_SCALE > 80.0, "region {id}");
        if let Ok(dir) = std::env::var("XMATH_PNG_DIR") {
            std::fs::write(format!("{dir}/plot_{id}.png"), png).unwrap();
        }
    }
}

#[test]
fn saved_xmcd_carries_pictures() {
    let ws = examples_plots();
    let outputs = ws.evaluate_opts(true);
    let renderings = render_plot_pngs(&ws, &outputs);
    let opts = xmath_core::xmcd_export::ExportOptions { renderings, rendering_scale: PNG_SCALE, outputs: Some(outputs), ..Default::default() };
    let r = xmath_core::xmcd_export::export_xmcd_with(&ws, &opts);
    assert!(r.xml.contains("<rendering item-idref="));
    let back = xmath_core::xmcd::import_xmcd(&r.xml).unwrap().worksheet;
    assert_eq!(back.regions.len(), ws.regions.len());
    // the model is the same although the polar and 3D plots are pictures in the file
    for a in ws.regions.iter().filter(|r| matches!(r.kind, RegionKind::Plot { .. } | RegionKind::Plot3d { .. })) {
        let b = back.regions.iter().find(|r| r.id == a.id).unwrap();
        assert_eq!(a.kind, b.kind, "region {}", a.id);
    }
}
