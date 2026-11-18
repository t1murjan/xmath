//! xmath — a free Mathcad 15 style worksheet application.

#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod editor;
mod image;
mod import;
mod mathview;
mod palettes;
mod plot;

use std::path::PathBuf;
use std::sync::Arc;

/// Arial (or a metric-compatible replacement) for the Mathcad look.
const REGULAR_FONTS: &[&str] = &[
    "C:\\Windows\\Fonts\\arial.ttf",
    "/System/Library/Fonts/Supplemental/Arial.ttf",
    "/Library/Fonts/Arial.ttf",
    "/usr/share/fonts/truetype/msttcorefonts/Arial.ttf",
    "/usr/share/fonts/truetype/msttcorefonts/arial.ttf",
    "/usr/share/fonts/TTF/arial.ttf",
    "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
    "/usr/share/fonts/truetype/liberation2/LiberationSans-Regular.ttf",
    "/usr/share/fonts/liberation-sans/LiberationSans-Regular.ttf",
    "/usr/share/fonts/liberation-sans-fonts/LiberationSans-Regular.ttf",
    "/usr/share/fonts/TTF/LiberationSans-Regular.ttf",
];

const BOLD_FONTS: &[&str] = &[
    "C:\\Windows\\Fonts\\arialbd.ttf",
    "/System/Library/Fonts/Supplemental/Arial Bold.ttf",
    "/Library/Fonts/Arial Bold.ttf",
    "/usr/share/fonts/truetype/msttcorefonts/Arial_Bold.ttf",
    "/usr/share/fonts/truetype/msttcorefonts/arialbd.ttf",
    "/usr/share/fonts/TTF/arialbd.ttf",
    "/usr/share/fonts/truetype/liberation/LiberationSans-Bold.ttf",
    "/usr/share/fonts/truetype/liberation2/LiberationSans-Bold.ttf",
    "/usr/share/fonts/liberation-sans/LiberationSans-Bold.ttf",
    "/usr/share/fonts/liberation-sans-fonts/LiberationSans-Bold.ttf",
    "/usr/share/fonts/TTF/LiberationSans-Bold.ttf",
];

/// Wide Unicode coverage for math symbols (∫, Σ, ■, Greek, arrows).
const SYMBOL_FONTS: &[&str] = &[
    "C:\\Windows\\Fonts\\seguisym.ttf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    "/usr/share/fonts/dejavu/DejaVuSans.ttf",
    "/usr/share/fonts/dejavu-sans-fonts/DejaVuSans.ttf",
    "/usr/share/fonts/TTF/DejaVuSans.ttf",
];

fn first_font(paths: &[&str]) -> Option<Vec<u8>> {
    paths.iter().find_map(|p| std::fs::read(p).ok())
}

/// Installs system fonts; returns whether a real bold face is available.
fn setup_fonts(ctx: &egui::Context) -> bool {
    let mut fonts = egui::FontDefinitions::default();
    let mut front = Vec::new();
    if let Some(data) = first_font(REGULAR_FONTS) {
        fonts.font_data.insert("arial".into(), Arc::new(egui::FontData::from_owned(data)));
        front.push("arial".to_string());
    }
    if let Some(data) = first_font(SYMBOL_FONTS) {
        fonts.font_data.insert("symbols".into(), Arc::new(egui::FontData::from_owned(data)));
        front.push("symbols".to_string());
    }
    let prop = fonts.families.entry(egui::FontFamily::Proportional).or_default();
    for (i, f) in front.iter().enumerate() {
        prop.insert(i, f.clone());
    }
    let fallback = prop.clone();
    let mut has_bold = false;
    if let Some(data) = first_font(BOLD_FONTS) {
        fonts.font_data.insert("arial-bold".into(), Arc::new(egui::FontData::from_owned(data)));
        let mut fam = vec!["arial-bold".to_string()];
        fam.extend(fallback);
        fonts.families.insert(egui::FontFamily::Name("bold".into()), fam);
        has_bold = true;
    }
    ctx.set_fonts(fonts);
    has_bold
}

/// Windows XP / Mathcad 15 colour scheme.
fn setup_style(ctx: &egui::Context) {
    let mut v = egui::Visuals::light();
    v.panel_fill = app::PANEL;
    v.window_fill = app::PANEL;
    v.faint_bg_color = egui::Color32::from_rgb(245, 244, 236);
    v.extreme_bg_color = egui::Color32::WHITE;
    v.override_text_color = Some(egui::Color32::BLACK);
    v.selection.bg_fill = egui::Color32::from_rgb(49, 106, 197);
    v.selection.stroke = egui::Stroke::new(1.0_f32, egui::Color32::WHITE);
    v.window_stroke = egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(0, 60, 160));
    let w = &mut v.widgets;
    w.inactive.weak_bg_fill = app::PANEL;
    w.inactive.bg_fill = egui::Color32::WHITE;
    w.inactive.bg_stroke = egui::Stroke::NONE;
    w.hovered.weak_bg_fill = egui::Color32::from_rgb(252, 252, 248);
    w.hovered.bg_stroke = egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(120, 140, 190));
    w.active.weak_bg_fill = egui::Color32::from_rgb(220, 218, 205);
    w.active.bg_stroke = egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(90, 110, 170));
    for s in [&mut w.noninteractive, &mut w.inactive, &mut w.hovered, &mut w.active, &mut w.open] {
        s.corner_radius = egui::CornerRadius::same(2);
        s.fg_stroke.color = egui::Color32::BLACK;
    }
    ctx.set_visuals(v);
    ctx.style_mut(|s| {
        s.spacing.button_padding = egui::vec2(5.0, 2.0);
        s.spacing.menu_margin = egui::Margin::same(3);
        s.text_styles.insert(egui::TextStyle::Body, egui::FontId::proportional(13.0));
        s.text_styles.insert(egui::TextStyle::Button, egui::FontId::proportional(13.0));
    });
}

fn main() -> eframe::Result<()> {
    let path = std::env::args().nth(1).map(PathBuf::from);
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
            let bold = setup_fonts(&cc.egui_ctx);
            setup_style(&cc.egui_ctx);
            Ok(Box::new(app::App::new(cc, path, bold)))
        }),
    )
}
