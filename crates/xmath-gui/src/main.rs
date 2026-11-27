//! xmath — a free Mathcad 15 style worksheet application.

#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use xmath_gui::{app, fonts};

use std::path::PathBuf;

fn main() -> eframe::Result<()> {
    let mut paths = Vec::new();
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        if a == "--ui-scale" {
            if let Some(v) = args.next() {
                std::env::set_var("XMATH_UI_SCALE", v);
            }
        } else if let Some(v) = a.strip_prefix("--ui-scale=") {
            std::env::set_var("XMATH_UI_SCALE", v);
        } else {
            paths.push(PathBuf::from(a));
        }
    }
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("xmath")
            .with_app_id("xmath")
            .with_inner_size([1180.0, 820.0])
            .with_min_inner_size([640.0, 420.0]),
        ..Default::default()
    };
    eframe::run_native(
        "xmath",
        options,
        Box::new(move |cc| {
            let bold = fonts::setup_fonts(&cc.egui_ctx);
            app::setup_style(&cc.egui_ctx);
            Ok(Box::new(app::App::new(cc, paths, bold)))
        }),
    )
}
