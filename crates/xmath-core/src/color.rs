//! Mathcad's colour conversions, used by the image-processing worksheets: `rgb_to_hsv`,
//! `hsv_to_rgb`, `rgb_to_hls`, `hls_to_rgb`.
//!
//! Components are in Mathcad's ranges: RGB and the S/V/L components are 0..255, the hue is in
//! degrees 0..360. A three-element vector goes in and comes out; three numbers are accepted too.

use crate::value::{Matrix, Num, Value};
use crate::units::Dim;

fn triple(v: &Value) -> Result<[f64; 3], String> {
    match v {
        Value::Matrix(m) if m.data.len() == 3 => {
            let mut out = [0.0; 3];
            for (i, n) in m.data.iter().enumerate() {
                if n.dim != Dim::NONE {
                    return Err("компоненты цвета должны быть безразмерными".into());
                }
                out[i] = n.v.re;
            }
            Ok(out)
        }
        other => Err(format!("ожидался вектор из трёх чисел, найдено: {}", other.type_name())),
    }
}

fn column(v: [f64; 3]) -> Value {
    Value::Matrix(Matrix::column(v.iter().map(|x| Num::real(*x)).collect()))
}

/// RGB (0..255) to hue (degrees), saturation and value (both 0..255).
pub fn rgb_to_hsv(rgb: [f64; 3]) -> [f64; 3] {
    let [r, g, b] = rgb.map(|c| c / 255.0);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = max - min;
    let h = hue(r, g, b, max, d);
    let s = if max == 0.0 { 0.0 } else { d / max };
    [h, s * 255.0, max * 255.0]
}

/// RGB (0..255) to hue (degrees), lightness and saturation (both 0..255).
pub fn rgb_to_hls(rgb: [f64; 3]) -> [f64; 3] {
    let [r, g, b] = rgb.map(|c| c / 255.0);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = max - min;
    let l = (max + min) / 2.0;
    let h = hue(r, g, b, max, d);
    let s = if d == 0.0 { 0.0 } else { d / (1.0 - (2.0 * l - 1.0).abs()) };
    [h, l * 255.0, s * 255.0]
}

fn hue(r: f64, g: f64, b: f64, max: f64, d: f64) -> f64 {
    if d == 0.0 {
        return 0.0;
    }
    let h = if max == r {
        60.0 * (((g - b) / d) % 6.0)
    } else if max == g {
        60.0 * ((b - r) / d + 2.0)
    } else {
        60.0 * ((r - g) / d + 4.0)
    };
    if h < 0.0 {
        h + 360.0
    } else {
        h
    }
}

/// Hue (degrees), saturation and value (0..255) back to RGB.
pub fn hsv_to_rgb(hsv: [f64; 3]) -> [f64; 3] {
    let [h, s, v] = hsv;
    let (h, s, v) = (h.rem_euclid(360.0), (s / 255.0).clamp(0.0, 1.0), (v / 255.0).clamp(0.0, 1.0));
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = v - c;
    let (r, g, b) = sector(h, c, x);
    [(r + m) * 255.0, (g + m) * 255.0, (b + m) * 255.0]
}

/// Hue (degrees), lightness and saturation (0..255) back to RGB.
pub fn hls_to_rgb(hls: [f64; 3]) -> [f64; 3] {
    let [h, l, s] = hls;
    let (h, l, s) = (h.rem_euclid(360.0), (l / 255.0).clamp(0.0, 1.0), (s / 255.0).clamp(0.0, 1.0));
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = l - c / 2.0;
    let (r, g, b) = sector(h, c, x);
    [(r + m) * 255.0, (g + m) * 255.0, (b + m) * 255.0]
}

fn sector(h: f64, c: f64, x: f64) -> (f64, f64, f64) {
    match (h / 60.0) as i32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    }
}

/// The single-vector argument of the four builtins.
pub fn arg(v: &Value) -> Result<[f64; 3], String> {
    triple(v)
}

pub fn out(v: [f64; 3]) -> Value {
    column(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_mathcad() {
        // the values Mathcad 15 itself stored in Hlsv.xmcd of the shipped samples
        assert_eq!(rgb_to_hsv([255.0, 0.0, 0.0]), [0.0, 255.0, 255.0]);
        assert_eq!(rgb_to_hsv([0.0, 255.0, 0.0]), [120.0, 255.0, 255.0]);
        assert_eq!(rgb_to_hsv([0.0, 0.0, 255.0]), [240.0, 255.0, 255.0]);
        assert_eq!(rgb_to_hls([255.0, 0.0, 0.0]), [0.0, 127.5, 255.0]);
        assert_eq!(rgb_to_hls([0.0, 255.0, 0.0]), [120.0, 127.5, 255.0]);
        assert_eq!(rgb_to_hls([0.0, 0.0, 255.0]), [240.0, 127.5, 255.0]);
    }

    #[test]
    fn round_trips() {
        for rgb in [[255.0, 0.0, 0.0], [0.0, 255.0, 0.0], [0.0, 0.0, 255.0], [17.0, 200.0, 99.0], [128.0, 128.0, 128.0]] {
            let back = hsv_to_rgb(rgb_to_hsv(rgb));
            let back_hls = hls_to_rgb(rgb_to_hls(rgb));
            for i in 0..3 {
                assert!((back[i] - rgb[i]).abs() < 1e-9, "hsv {rgb:?} -> {back:?}");
                assert!((back_hls[i] - rgb[i]).abs() < 1e-9, "hls {rgb:?} -> {back_hls:?}");
            }
        }
    }
}
