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

/// Times New Roman or Liberation Serif.
const SERIF_FONTS: &[&str] = &[
    "C:\\Windows\\Fonts\\times.ttf",
    "/System/Library/Fonts/Supplemental/Times New Roman.ttf",
    "/Library/Fonts/Times New Roman.ttf",
    "/usr/share/fonts/truetype/msttcorefonts/Times_New_Roman.ttf",
    "/usr/share/fonts/truetype/msttcorefonts/times.ttf",
    "/usr/share/fonts/TTF/times.ttf",
    "/usr/share/fonts/truetype/liberation/LiberationSerif-Regular.ttf",
    "/usr/share/fonts/truetype/liberation2/LiberationSerif-Regular.ttf",
    "/usr/share/fonts/liberation-serif/LiberationSerif-Regular.ttf",
    "/usr/share/fonts/liberation-serif-fonts/LiberationSerif-Regular.ttf",
    "/usr/share/fonts/TTF/LiberationSerif-Regular.ttf",
];

const SERIF_BOLD_FONTS: &[&str] = &[
    "C:\\Windows\\Fonts\\timesbd.ttf",
    "/System/Library/Fonts/Supplemental/Times New Roman Bold.ttf",
    "/Library/Fonts/Times New Roman Bold.ttf",
    "/usr/share/fonts/truetype/msttcorefonts/Times_New_Roman_Bold.ttf",
    "/usr/share/fonts/truetype/msttcorefonts/timesbd.ttf",
    "/usr/share/fonts/TTF/timesbd.ttf",
    "/usr/share/fonts/truetype/liberation/LiberationSerif-Bold.ttf",
    "/usr/share/fonts/truetype/liberation2/LiberationSerif-Bold.ttf",
    "/usr/share/fonts/liberation-serif/LiberationSerif-Bold.ttf",
    "/usr/share/fonts/liberation-serif-fonts/LiberationSerif-Bold.ttf",
    "/usr/share/fonts/TTF/LiberationSerif-Bold.ttf",
];

/// Courier New or Liberation Mono.
const MONO_FONTS: &[&str] = &[
    "C:\\Windows\\Fonts\\cour.ttf",
    "/System/Library/Fonts/Supplemental/Courier New.ttf",
    "/Library/Fonts/Courier New.ttf",
    "/usr/share/fonts/truetype/msttcorefonts/Courier_New.ttf",
    "/usr/share/fonts/truetype/msttcorefonts/cour.ttf",
    "/usr/share/fonts/TTF/cour.ttf",
    "/usr/share/fonts/truetype/liberation/LiberationMono-Regular.ttf",
    "/usr/share/fonts/truetype/liberation2/LiberationMono-Regular.ttf",
    "/usr/share/fonts/liberation-mono/LiberationMono-Regular.ttf",
    "/usr/share/fonts/liberation-mono-fonts/LiberationMono-Regular.ttf",
    "/usr/share/fonts/TTF/LiberationMono-Regular.ttf",
];

const MONO_BOLD_FONTS: &[&str] = &[
    "C:\\Windows\\Fonts\\courbd.ttf",
    "/System/Library/Fonts/Supplemental/Courier New Bold.ttf",
    "/Library/Fonts/Courier New Bold.ttf",
    "/usr/share/fonts/truetype/msttcorefonts/Courier_New_Bold.ttf",
    "/usr/share/fonts/truetype/msttcorefonts/courbd.ttf",
    "/usr/share/fonts/TTF/courbd.ttf",
    "/usr/share/fonts/truetype/liberation/LiberationMono-Bold.ttf",
    "/usr/share/fonts/truetype/liberation2/LiberationMono-Bold.ttf",
    "/usr/share/fonts/liberation-mono/LiberationMono-Bold.ttf",
    "/usr/share/fonts/liberation-mono-fonts/LiberationMono-Bold.ttf",
    "/usr/share/fonts/TTF/LiberationMono-Bold.ttf",
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
    // Times New Roman and Courier New classes of text regions (always defined; without the face
    // they fall back to the Arial chain)
    let chain = fonts.families.get(&egui::FontFamily::Proportional).cloned().unwrap_or_default();
    for (name, paths) in [("serif", SERIF_FONTS), ("serif-bold", SERIF_BOLD_FONTS), ("mono", MONO_FONTS), ("mono-bold", MONO_BOLD_FONTS)] {
        let mut fam = Vec::new();
        if let Some(data) = first_font(paths) {
            fonts.font_data.insert(name.into(), Arc::new(egui::FontData::from_owned(data)));
            fam.push(name.to_string());
        }
        fam.extend(chain.iter().cloned());
        fonts.families.insert(egui::FontFamily::Name(name.into()), fam);
    }
    (fonts, has_bold)
}

/// Installs the fonts into `ctx`; returns whether a real bold face is available.
pub fn setup_fonts(ctx: &egui::Context) -> bool {
    let (fonts, has_bold) = definitions();
    ctx.set_fonts(fonts);
    has_bold
}

