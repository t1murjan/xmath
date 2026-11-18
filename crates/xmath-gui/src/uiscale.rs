//! Interface scale ("Вид → Масштаб интерфейса"). Applied through egui's zoom factor on top of the
//! native pixels-per-point, independently of the worksheet zoom.
//!
//! Priority: `--ui-scale X` / `XMATH_UI_SCALE=X` (session override) > saved choice > auto.
//! Auto: the native scale factor from the OS when it is not 1.0; otherwise the first of
//! `GDK_SCALE`, `QT_SCALE_FACTOR`, `Xft.dpi / 96` (via `xrdb -query`) that is above 1; otherwise
//! 1.5 for 4K-class monitors (at least 3200x1800 points), else 1.0.

use egui::Context;

pub const LEVELS: [u32; 7] = [100, 125, 150, 175, 200, 250, 300];

/// `None` is "Авто".
pub type Choice = Option<u32>;

pub fn label(c: Choice) -> String {
    match c {
        None => "Авто".into(),
        Some(p) => format!("{p} %"),
    }
}

pub fn serialize(c: Choice) -> String {
    c.map_or("auto".into(), |p| p.to_string())
}

pub fn parse(s: &str) -> Choice {
    s.trim().parse::<u32>().ok().map(|p| p.clamp(50, 400))
}

/// Value of `XMATH_UI_SCALE` (a factor such as `1.5`, or a percentage such as `150`).
pub fn env_override() -> Option<f32> {
    let v: f32 = std::env::var("XMATH_UI_SCALE").ok()?.trim().replace(',', ".").trim_end_matches('%').parse().ok()?;
    let v = if v > 10.0 { v / 100.0 } else { v };
    (v.is_finite() && v > 0.0).then_some(v.clamp(0.5, 4.0))
}

/// The next/previous level for «Увеличить / Уменьшить интерфейс».
pub fn step(cur: Choice, effective: f32, up: bool) -> Choice {
    let pct = cur.unwrap_or((effective * 100.0).round() as u32);
    let next = if up { LEVELS.iter().copied().find(|l| *l > pct) } else { LEVELS.iter().rev().copied().find(|l| *l < pct) };
    next.or(Some(if up { 300 } else { 100 }))
}

fn env_factor(name: &str) -> Option<f32> {
    std::env::var(name).ok()?.trim().parse::<f32>().ok().filter(|v| *v > 1.0)
}

fn xft_dpi() -> Option<f32> {
    let out = std::process::Command::new("xrdb").arg("-query").output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let dpi: f32 = text.lines().find_map(|l| l.strip_prefix("Xft.dpi:"))?.trim().parse().ok()?;
    Some(dpi / 96.0).filter(|v| *v > 1.0)
}

/// Extra zoom for «Авто» from plain inputs (testable). `env`/`xft` are only consulted when
/// the native scale is 1.0.
fn decide(native: f32, env: impl Fn() -> Option<f32>, xft: impl Fn() -> Option<f32>, monitor: Option<(f32, f32)>) -> f32 {
    if (native - 1.0).abs() > 0.01 {
        return 1.0; // the OS already scales
    }
    if let Some(f) = env().or_else(xft) {
        return snap(f);
    }
    match monitor {
        Some((w, h)) if w >= 3200.0 && h >= 1800.0 => 1.5,
        _ => 1.0,
    }
}

fn auto_factor(ctx: &Context) -> f32 {
    let native = ctx.native_pixels_per_point().unwrap_or(1.0);
    let mon = ctx.input(|i| i.viewport().monitor_size).map(|m| (m.x, m.y));
    decide(native, || env_factor("GDK_SCALE").or_else(|| env_factor("QT_SCALE_FACTOR")), xft_dpi, mon)
}

fn snap(f: f32) -> f32 {
    let pct = (f * 100.0 / 25.0).round() * 25.0;
    (pct / 100.0).clamp(1.0, 3.0)
}

/// Effective zoom factor for the current settings.
pub fn effective(ctx: &Context, choice: Choice, auto_cache: &mut Option<f32>) -> f32 {
    if let Some(v) = env_override() {
        return v;
    }
    match choice {
        Some(p) => p as f32 / 100.0,
        None => {
            if auto_cache.is_none() {
                // monitor size arrives with the first frames; wait for it before deciding
                if ctx.input(|i| i.viewport().monitor_size).is_some() || ctx.cumulative_pass_nr() > 5 {
                    *auto_cache = Some(auto_factor(ctx));
                } else {
                    ctx.request_repaint();
                    return 1.0;
                }
            }
            auto_cache.unwrap_or(1.0)
        }
    }
}

pub fn apply(ctx: &Context, factor: f32) {
    ctx.options_mut(|o| o.zoom_with_keyboard = false);
    if (ctx.zoom_factor() - factor).abs() > 1e-4 {
        ctx.set_zoom_factor(factor);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_decision() {
        let none = || None;
        assert_eq!(decide(1.0, none, none, Some((2560.0, 1440.0))), 1.0);
        assert_eq!(decide(1.0, none, none, Some((3440.0, 1440.0))), 1.0);
        assert_eq!(decide(1.0, none, none, Some((3840.0, 2160.0))), 1.5);
        assert_eq!(decide(1.0, || Some(2.0), none, Some((1920.0, 1080.0))), 2.0);
        assert_eq!(decide(1.0, none, || Some(144.0 / 96.0), None), 1.5);
        assert_eq!(decide(2.0, || Some(2.0), none, Some((3840.0, 2160.0))), 1.0);
    }
}
