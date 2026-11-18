//! UI-independent plot geometry: tick numbers in Mathcad style, nice axis
//! steps, and *scenes* — lists of drawing primitives in a local pixel
//! coordinate system — for polar plots. The GUI paints a [`Scene`] with egui;
//! an exporter (PDF, SVG, ...) can paint the same primitives into any other
//! sink.

use crate::worksheet::{LineStyle, NumFormat, NumKind, PlotFormat, Series};

/// RGB colour.
pub type Rgb = [u8; 3];

/// Default trace colours of Mathcad 15 (red, blue, magenta, green, ...).
pub const TRACE_COLORS: [Rgb; 6] = [[255, 0, 0], [0, 0, 255], [255, 0, 255], [0, 128, 0], [0, 160, 160], [128, 64, 0]];
/// Default line styles: solid, dot, dash, dash-dot...
pub const TRACE_LINES: [LineStyle; 6] = [LineStyle::Solid, LineStyle::Dot, LineStyle::Dash, LineStyle::DashDot, LineStyle::Solid, LineStyle::Dot];
/// Grid colour of Mathcad plots.
pub const GRID_COLOR: Rgb = [0, 200, 0];

/// Colour, line style and width of trace `i`.
pub fn trace_style(i: usize, fmt: &PlotFormat) -> (Rgb, LineStyle, f32) {
    let o = fmt.traces.get(i).cloned().unwrap_or_default();
    (o.color.unwrap_or(TRACE_COLORS[i % TRACE_COLORS.len()]), o.line.unwrap_or(TRACE_LINES[i % TRACE_LINES.len()]), o.width.unwrap_or(2.0))
}

// ------------------------------------------------------------------ ticks

/// A tick number: mantissa text and an optional power of ten (`2×10⁻⁶`).
#[derive(Clone, Debug, PartialEq, Default)]
pub struct TickLabel {
    pub mant: String,
    pub exp: Option<i32>,
}

impl TickLabel {
    pub fn plain(s: impl Into<String>) -> TickLabel {
        TickLabel { mant: s.into(), exp: None }
    }
}

/// `-12` as `⁻¹²`.
pub fn superscript(e: i32) -> String {
    e.to_string()
        .chars()
        .map(|c| match c {
            '-' => '⁻',
            '0' => '⁰',
            '1' => '¹',
            '2' => '²',
            '3' => '³',
            '4' => '⁴',
            '5' => '⁵',
            '6' => '⁶',
            '7' => '⁷',
            '8' => '⁸',
            _ => '⁹',
        })
        .collect()
}

impl std::fmt::Display for TickLabel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.exp {
            Some(e) => write!(f, "{}×10{}", self.mant, superscript(e)),
            None => write!(f, "{}", self.mant),
        }
    }
}

fn trim_zeros(s: &str) -> String {
    if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        s.to_string()
    }
}

/// Formats a tick value like Mathcad does with the worksheet result format:
/// numbers with a decimal exponent `e >= threshold` or `e <= -threshold` are
/// shown as `m×10ᵉ` (`0.001` → `1×10⁻³` with the default threshold 3),
/// trailing zeros are trimmed, the minus is U+2212.
pub fn format_tick(v: f64, exp_threshold: i32) -> TickLabel {
    if !v.is_finite() {
        return TickLabel::plain("");
    }
    let sci = format!("{:.6e}", v.abs());
    let (m, e) = sci.split_once('e').unwrap_or((&sci, "0"));
    let e: i32 = e.parse().unwrap_or(0);
    let m_val: f64 = m.parse().unwrap_or(0.0);
    if m_val == 0.0 {
        return TickLabel::plain("0");
    }
    let sign = if v < 0.0 { "−" } else { "" };
    if e >= exp_threshold || e <= -exp_threshold {
        TickLabel { mant: format!("{sign}{}", trim_zeros(m)), exp: Some(e) }
    } else {
        let s = trim_zeros(&format!("{:.9}", v.abs()));
        TickLabel::plain(format!("{sign}{s}"))
    }
}

fn pad_decimals(s: String, p: usize) -> String {
    let have = s.split_once('.').map_or(0, |(_, d)| d.len());
    if have >= p {
        s
    } else if s.contains('.') {
        format!("{s}{}", "0".repeat(p - have))
    } else {
        format!("{s}.{}", "0".repeat(p))
    }
}

/// Best rational approximation `n/d` with `d <= max_den`.
fn rational(v: f64, max_den: i64) -> (i64, i64) {
    let (mut h0, mut h1, mut k0, mut k1) = (0i64, 1i64, 1i64, 0i64);
    let mut x = v;
    for _ in 0..20 {
        let a = x.floor();
        let ai = a as i64;
        let (h2, k2) = (ai * h1 + h0, ai * k1 + k0);
        if k2 > max_den {
            break;
        }
        (h0, h1, k0, k1) = (h1, h2, k1, k2);
        let f = x - a;
        if f.abs() < 1e-9 || (h1 as f64 / k1 as f64 - v).abs() < 1e-9 {
            break;
        }
        x = 1.0 / f;
    }
    if k1 == 0 {
        (v.round() as i64, 1)
    } else {
        (h1, k1)
    }
}

/// Formats an axis number with the plot's number format («Формат числа»).
pub fn format_tick_with(v: f64, nf: &NumFormat) -> TickLabel {
    if !v.is_finite() {
        return TickLabel::plain("");
    }
    let p = nf.precision.min(15) as usize;
    let sign = if v < 0.0 { "−" } else { "" };
    let a = v.abs();
    let sci = |eng: bool| -> TickLabel {
        if a == 0.0 {
            return TickLabel::plain("0");
        }
        let s = format!("{:.*e}", if eng { 9 } else { p }, a);
        let (m, e) = s.split_once('e').unwrap_or((&s, "0"));
        let mut e: i32 = e.parse().unwrap_or(0);
        let mut m: f64 = m.parse().unwrap_or(0.0);
        if eng {
            let e3 = e.div_euclid(3) * 3;
            m *= 10f64.powi(e - e3);
            e = e3;
        }
        let ms = format!("{:.*}", p, m);
        let ms = if nf.trailing_zeros { ms } else { trim_zeros(&ms) };
        TickLabel { mant: format!("{sign}{ms}"), exp: Some(e) }
    };
    match nf.kind {
        NumKind::General => {
            let mut t = if nf.eng_exponents && a != 0.0 && {
                let e = format!("{:.6e}", a).split_once('e').and_then(|(_, e)| e.parse::<i32>().ok()).unwrap_or(0);
                e >= nf.exp_threshold || e <= -nf.exp_threshold
            } {
                sci(true)
            } else {
                format_tick(v, nf.exp_threshold)
            };
            if nf.trailing_zeros && t.exp.is_none() && !t.mant.is_empty() {
                t.mant = pad_decimals(t.mant, p);
            }
            t
        }
        NumKind::Decimal => {
            let s = format!("{:.*}", p, a);
            let s = if nf.trailing_zeros { s } else { trim_zeros(&s) };
            let neg = v < 0.0 && s.chars().any(|c| c.is_ascii_digit() && c != '0');
            TickLabel::plain(format!("{}{s}", if neg { "−" } else { "" }))
        }
        NumKind::Scientific => sci(false),
        NumKind::Engineering => sci(true),
        NumKind::Fraction => {
            let (n, d) = rational(a, 100);
            let s = if d == 1 { n.to_string() } else { format!("{n}/{d}") };
            TickLabel::plain(if n == 0 { "0".to_string() } else { format!("{sign}{s}") })
        }
    }
}

/// A "nice" step (1, 2 or 5 times a power of ten) giving about `target` intervals.
pub fn nice_step(span: f64, target: f64) -> f64 {
    let raw = span / target;
    if !(raw.is_finite() && raw > 0.0) {
        return 1.0;
    }
    let mag = 10f64.powf(raw.log10().floor());
    let f = raw / mag;
    let nice = if f < 1.5 {
        1.0
    } else if f < 3.0 {
        2.0
    } else if f < 7.0 {
        5.0
    } else {
        10.0
    };
    nice * mag
}

/// Autoscaled axis: (low, high, step).
pub fn axis_range(min: f64, max: f64) -> (f64, f64, f64) {
    let (mut lo, mut hi) = (min, max);
    if !(lo.is_finite() && hi.is_finite()) {
        return (-10.0, 10.0, 2.0);
    }
    if hi - lo < 1e-300 {
        let d = if lo.abs() > 0.0 { lo.abs() * 0.1 } else { 1.0 };
        lo -= d;
        hi += d;
    }
    let step = nice_step(hi - lo, 8.0);
    ((lo / step).floor() * step, (hi / step).ceil() * step, step)
}

/// Tick positions `k·st` within `[a, b]`.
pub fn ticks(a: f64, b: f64, st: f64) -> Vec<f64> {
    let mut out = Vec::new();
    if !(st.is_finite() && st > 0.0) {
        return out;
    }
    let mut k = (a / st - 1e-9).ceil();
    while k * st <= b + st * 1e-9 && out.len() < 400 {
        let v = k * st;
        out.push(if v.abs() < st * 1e-9 { 0.0 } else { v });
        k += 1.0;
    }
    out
}

/// How many labels to skip so that no two overlap: the smallest `k` such that
/// labels `0, k, 2k, ...` (centres `centers`, widths `widths`) keep `gap` between them.
pub fn label_stride(centers: &[f32], widths: &[f32], gap: f32) -> usize {
    let n = centers.len().min(widths.len());
    'k: for k in 1..=n.max(1) {
        let mut last: Option<usize> = None;
        for i in (0..n).step_by(k) {
            if let Some(j) = last {
                let need = (widths[i] + widths[j]) / 2.0 + gap;
                if (centers[i] - centers[j]).abs() < need {
                    continue 'k;
                }
            }
            last = Some(i);
        }
        return k;
    }
    n.max(1)
}

// ------------------------------------------------------------------ scenes

/// A drawing primitive in local pixel coordinates (y down).
#[derive(Clone, Debug, PartialEq)]
pub enum Prim {
    Poly { pts: Vec<[f32; 2]>, fill: Option<Rgb>, stroke: Option<(Rgb, f32)> },
    Line { pts: Vec<[f32; 2]>, color: Rgb, width: f32, style: LineStyle },
    Dot { c: [f32; 2], r: f32, color: Rgb },
    /// `align` is the point of the text box placed at `pos` (0..1 each: `[0.5, 0.0]` is top centre).
    Text { pos: [f32; 2], label: TickLabel, align: [f32; 2], size: f32, color: Rgb, bg: Option<Rgb> },
}

/// A set of primitives plus the size of the area they were computed for.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Scene {
    pub size: [f32; 2],
    pub prims: Vec<Prim>,
}

impl Scene {
    pub fn line(&mut self, a: [f32; 2], b: [f32; 2], color: Rgb, width: f32) {
        self.prims.push(Prim::Line { pts: vec![a, b], color, width, style: LineStyle::Solid });
    }
    pub fn text(&mut self, pos: [f32; 2], s: TickLabel, align: [f32; 2], size: f32, color: Rgb) {
        self.prims.push(Prim::Text { pos, label: s, align, size, color, bg: None });
    }
}

/// Geometry of a drawn polar plot, for the limit boxes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PolarGeom {
    pub center: [f32; 2],
    pub radius: f32,
    pub r_min: f64,
    pub r_max: f64,
    pub r_step: f64,
}

/// Radial limits as drawn: (min, max, step). `max_abs` is the largest |r| of the data.
pub fn polar_range(max_abs: f64, lo: Option<f64>, hi: Option<f64>) -> (f64, f64, f64) {
    let rmin = lo.filter(|v| v.is_finite() && *v >= 0.0).unwrap_or(0.0);
    let (a, b, st) = if max_abs.is_finite() && max_abs > 0.0 {
        let st = nice_step(max_abs - rmin, 5.0);
        let b = (max_abs / st - 1e-9).ceil() * st;
        (rmin, b.max(rmin + st), st)
    } else {
        (rmin, rmin + 1.0, 0.2)
    };
    match hi.filter(|v| v.is_finite() && *v > rmin) {
        Some(h) => (a, h, nice_step(h - a, 5.0)),
        None => (a, b, st),
    }
}

/// Builds the scene of a polar plot into an area of `size` pixels. Series
/// points are `[θ (radians), r]`; a negative `r` is drawn on the opposite side.
pub fn polar_scene(series: &[Series], size: [f32; 2], fmt: &PlotFormat, r_lo: Option<f64>, r_hi: Option<f64>, font: f32) -> (Scene, PolarGeom) {
    let mut sc = Scene { size, prims: Vec::new() };
    let max_abs = series
        .iter()
        .flat_map(|s| s.points.iter())
        .filter(|p| p[0].is_finite() && p[1].is_finite())
        .map(|p| p[1].abs())
        .fold(0.0, f64::max);
    let (r0, r1, rs) = polar_range(max_abs, r_lo, r_hi);
    let margin = font * 1.7;
    let radius = ((size[0].min(size[1]) / 2.0) - margin).max(10.0);
    let c = [size[0] / 2.0, size[1] / 2.0];
    let geom = PolarGeom { center: c, radius, r_min: r0, r_max: r1, r_step: rs };
    let rpx = |r: f64| ((r.abs() - r0) / (r1 - r0)).clamp(0.0, 10.0) as f32 * radius;
    let pt = |th: f64, r: f64| {
        let d = rpx(r);
        let th = if r < 0.0 { th + std::f64::consts::PI } else { th };
        [c[0] + d * th.cos() as f32, c[1] - d * th.sin() as f32]
    };
    let black = [0, 0, 0];
    let circle = |rad: f32| -> Vec<[f32; 2]> { (0..=120).map(|k| { let a = k as f32 / 120.0 * std::f32::consts::TAU; [c[0] + rad * a.cos(), c[1] - rad * a.sin()] }).collect() };
    // grid: radial lines every 30 degrees and rings at the radial ticks
    let ring_ticks = ticks(r0, r1, rs);
    if fmt.grid {
        for k in 0..12 {
            let a = (k as f32 * 30.0).to_radians();
            sc.line(c, [c[0] + radius * a.cos(), c[1] - radius * a.sin()], GRID_COLOR, 1.0);
        }
        for &t in &ring_ticks {
            let d = rpx(t);
            if d > 0.5 && d < radius - 0.5 {
                sc.prims.push(Prim::Line { pts: circle(d), color: GRID_COLOR, width: 1.0, style: LineStyle::Solid });
            }
        }
    }
    // frame, angle labels (degrees), radial numbers along the 0 degree ray
    sc.prims.push(Prim::Line { pts: circle(radius), color: [90, 90, 90], width: 1.0, style: LineStyle::Solid });
    if fmt.axes != crate::worksheet::AxisStyle::None {
        sc.line([c[0] - radius, c[1]], [c[0] + radius, c[1]], black, 1.0);
        sc.line([c[0], c[1] - radius], [c[0], c[1] + radius], black, 1.0);
        for k in 0..12 {
            let deg = k * 30;
            let a = (deg as f32).to_radians();
            let off = radius + font * 0.35;
            let (cx, cy) = (a.cos(), -a.sin());
            sc.text(
                [c[0] + off * cx, c[1] + off * cy],
                TickLabel::plain(deg.to_string()),
                [0.5 - cx * 0.5, 0.5 - cy * 0.5],
                font * 0.85,
                black,
            );
        }
        let widths: Vec<f32> = ring_ticks.iter().map(|&t| format_tick_with(t, &fmt.number).to_string().chars().count() as f32 * font * 0.5).collect();
        let centers: Vec<f32> = ring_ticks.iter().map(|&t| rpx(t)).collect();
        let stride = label_stride(&centers, &widths, 3.0);
        for (i, &t) in ring_ticks.iter().enumerate().step_by(stride) {
            if i == 0 && r0 == 0.0 {
                continue;
            }
            sc.text([c[0] + rpx(t), c[1] + 2.0], format_tick_with(t, &fmt.number), [0.5, 0.0], font * 0.85, black);
        }
    }
    for (i, s) in series.iter().enumerate() {
        let (color, style, width) = trace_style(i, fmt);
        let mut run: Vec<[f32; 2]> = Vec::new();
        let flush = |run: &mut Vec<[f32; 2]>, sc: &mut Scene| {
            if run.len() > 1 {
                sc.prims.push(Prim::Line { pts: std::mem::take(run), color, width, style });
            } else if run.len() == 1 && style == LineStyle::Points {
                sc.prims.push(Prim::Dot { c: run[0], r: width * 0.5 + 1.0, color });
                run.clear();
            } else {
                run.clear();
            }
        };
        for p in &s.points {
            if p[0].is_finite() && p[1].is_finite() {
                let q = pt(p[0], p[1]);
                if style == LineStyle::Points {
                    sc.prims.push(Prim::Dot { c: q, r: width * 0.5 + 1.0, color });
                } else {
                    run.push(q);
                }
            } else {
                flush(&mut run, &mut sc);
            }
        }
        flush(&mut run, &mut sc);
    }
    (sc, geom)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tick_exponential() {
        assert_eq!(format_tick(2.0e-6, 3).to_string(), "2×10⁻⁶");
        assert_eq!(format_tick(0.001, 3).to_string(), "1×10⁻³");
        assert_eq!(format_tick(-1.5e-4, 3).to_string(), "−1.5×10⁻⁴");
        assert_eq!(format_tick(1000.0, 3).to_string(), "1×10³");
        assert_eq!(format_tick(250000.0, 3).to_string(), "2.5×10⁵");
        assert_eq!(format_tick(1e-12, 3).to_string(), "1×10⁻¹²");
    }

    #[test]
    fn tick_plain() {
        assert_eq!(format_tick(0.0, 3).to_string(), "0");
        assert_eq!(format_tick(0.5, 3).to_string(), "0.5");
        assert_eq!(format_tick(1.50, 3).to_string(), "1.5");
        assert_eq!(format_tick(-2.0, 3).to_string(), "−2");
        assert_eq!(format_tick(0.30000000000000004, 3).to_string(), "0.3");
        assert_eq!(format_tick(0.01, 3).to_string(), "0.01");
        assert_eq!(format_tick(999.0, 3).to_string(), "999");
        // a bigger threshold keeps the plain form
        assert_eq!(format_tick(0.001, 5).to_string(), "0.001");
        assert_eq!(format_tick(1000.0, 5).to_string(), "1000");
    }

    #[test]
    fn number_formats() {
        let mut nf = NumFormat::default();
        assert_eq!(format_tick_with(0.002, &nf).to_string(), "2×10⁻³");
        nf.kind = NumKind::Decimal;
        assert_eq!(format_tick_with(0.5, &nf).to_string(), "0.5");
        nf.trailing_zeros = true;
        assert_eq!(format_tick_with(0.5, &nf).to_string(), "0.500");
        assert_eq!(format_tick_with(-2.0, &nf).to_string(), "−2.000");
        nf.kind = NumKind::Scientific;
        nf.trailing_zeros = false;
        assert_eq!(format_tick_with(1500.0, &nf).to_string(), "1.5×10³");
        nf.kind = NumKind::Engineering;
        assert_eq!(format_tick_with(0.0025, &nf).to_string(), "2.5×10⁻³");
        assert_eq!(format_tick_with(45000.0, &nf).to_string(), "45×10³");
        nf.kind = NumKind::Fraction;
        assert_eq!(format_tick_with(0.75, &nf).to_string(), "3/4");
        assert_eq!(format_tick_with(-1.5, &nf).to_string(), "−3/2");
        assert_eq!(format_tick_with(2.0, &nf).to_string(), "2");
    }

    #[test]
    fn steps_and_ticks() {
        assert_eq!(nice_step(10.0, 8.0), 1.0);
        assert_eq!(nice_step(1.0, 5.0), 0.2);
        assert_eq!(nice_step(100.0, 4.0), 20.0);
        let (lo, hi, st) = axis_range(0.1, 9.7);
        assert!(lo <= 0.1 && hi >= 9.7 && st > 0.0);
        let t = ticks(0.0, 1.0, 0.25);
        assert_eq!(t, vec![0.0, 0.25, 0.5, 0.75, 1.0]);
        assert_eq!(ticks(-1.0, 1.0, 0.5)[2], 0.0);
    }

    #[test]
    fn stride_avoids_overlap() {
        // 10 labels 30 px wide at 20 px spacing overlap: every 2nd (40 px) is still too tight with gap 15
        let c: Vec<f32> = (0..10).map(|i| i as f32 * 20.0).collect();
        let w = vec![30.0; 10];
        assert_eq!(label_stride(&c, &w, 0.0), 2);
        assert_eq!(label_stride(&c, &w, 15.0), 3);
        assert_eq!(label_stride(&c, &vec![10.0; 10], 5.0), 1);
    }

    #[test]
    fn polar_scene_geometry() {
        let pts: Vec<[f64; 2]> = (0..=100).map(|k| { let t = k as f64 / 100.0 * std::f64::consts::TAU; [t, 2.0 * (2.0 * t).cos()] }).collect();
        let s = Series { label: "r".into(), points: pts };
        let (sc, g) = polar_scene(&[s], [300.0, 300.0], &PlotFormat::default(), None, None, 12.0);
        assert!(g.r_max >= 2.0 && g.r_max <= 3.0);
        assert!(sc.prims.iter().any(|p| matches!(p, Prim::Text { label, .. } if label.mant == "30")));
        assert!(sc.prims.iter().any(|p| matches!(p, Prim::Line { color: [255, 0, 0], .. })));
    }
}
