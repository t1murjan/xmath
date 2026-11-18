//! System fonts: Arial (or a metric-compatible replacement) and a wide Unicode symbol font.

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

/// The font definitions of the application and whether a real bold face is available.
pub fn definitions() -> (egui::FontDefinitions, bool) {
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
    (fonts, has_bold)
}

/// Installs the fonts into `ctx`; returns whether a real bold face is available.
pub fn setup_fonts(ctx: &egui::Context) -> bool {
    let (fonts, has_bold) = definitions();
    ctx.set_fonts(fonts);
    has_bold
}

