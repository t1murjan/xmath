//! xmath — a free Mathcad 15 style worksheet application.

#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use xmath_gui::{app, fonts};

use std::path::PathBuf;

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
    let mut path = None;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        if a == "--ui-scale" {
            if let Some(v) = args.next() {
                std::env::set_var("XMATH_UI_SCALE", v);
            }
        } else if let Some(v) = a.strip_prefix("--ui-scale=") {
            std::env::set_var("XMATH_UI_SCALE", v);
        } else if path.is_none() {
            path = Some(PathBuf::from(a));
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
            setup_style(&cc.egui_ctx);
            Ok(Box::new(app::App::new(cc, path, bold)))
        }),
    )
}
