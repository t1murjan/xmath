//! Mathcad-style X-Y plot: green grid, crossed axes, coloured traces.

use egui::{Align2, Color32, FontId, Painter, Pos2, Rect, Shape, Stroke};
use xmath_core::PlotData;

pub const TRACE_COLORS: [Color32; 6] = [
    Color32::from_rgb(255, 0, 0),
    Color32::from_rgb(0, 0, 255),
    Color32::from_rgb(255, 0, 255),
    Color32::from_rgb(0, 128, 0),
    Color32::from_rgb(0, 160, 160),
    Color32::from_rgb(128, 64, 0),
];

pub const GRID: Color32 = Color32::from_rgb(0, 200, 0);

fn nice_step(span: f64, target: f64) -> f64 {
    let raw = span / target;
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
fn axis_range(min: f64, max: f64) -> (f64, f64, f64) {
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

fn fmt_tick(v: f64, step: f64) -> String {
    let v = if v.abs() < step * 1e-9 { 0.0 } else { v };
    if v.abs() >= 1e5 || (v != 0.0 && v.abs() < 1e-3) {
        format!("{v:.1e}")
    } else {
        let decimals = (-(step.log10().floor())).max(0.0) as usize;
        format!("{v:.decimals$}").replace('-', "−")
    }
}

pub fn paint(painter: &Painter, rect: Rect, data: &PlotData, font: f32) {
    painter.rect_filled(rect, 0.0, Color32::WHITE);
    let pts = data.series.iter().flat_map(|s| s.points.iter()).filter(|p| p[0].is_finite() && p[1].is_finite());
    let (mut xmin, mut xmax, mut ymin, mut ymax) = (f64::INFINITY, f64::NEG_INFINITY, f64::INFINITY, f64::NEG_INFINITY);
    for p in pts {
        xmin = xmin.min(p[0]);
        xmax = xmax.max(p[0]);
        ymin = ymin.min(p[1]);
        ymax = ymax.max(p[1]);
    }
    let (x0, x1, xs) = axis_range(xmin, xmax);
    let (y0, y1, ys) = axis_range(ymin, ymax);
    let sx = |x: f64| rect.left() + ((x - x0) / (x1 - x0)) as f32 * rect.width();
    let sy = |y: f64| rect.bottom() - ((y - y0) / (y1 - y0)) as f32 * rect.height();

    let grid = Stroke::new(1.0, GRID);
    let mut k = 0;
    loop {
        let x = x0 + k as f64 * xs;
        if x > x1 + xs * 1e-9 || k > 200 {
            break;
        }
        painter.line_segment([Pos2::new(sx(x), rect.top()), Pos2::new(sx(x), rect.bottom())], grid);
        k += 1;
    }
    let mut k = 0;
    loop {
        let y = y0 + k as f64 * ys;
        if y > y1 + ys * 1e-9 || k > 200 {
            break;
        }
        painter.line_segment([Pos2::new(rect.left(), sy(y)), Pos2::new(rect.right(), sy(y))], grid);
        k += 1;
    }

    // crossed axes through zero (or along the border when zero is outside)
    let ax_y = if y0 <= 0.0 && y1 >= 0.0 { sy(0.0) } else { rect.bottom() };
    let ax_x = if x0 <= 0.0 && x1 >= 0.0 { sx(0.0) } else { rect.left() };
    let black = Stroke::new(1.0, Color32::BLACK);
    painter.line_segment([Pos2::new(rect.left(), ax_y), Pos2::new(rect.right(), ax_y)], black);
    painter.line_segment([Pos2::new(ax_x, rect.top()), Pos2::new(ax_x, rect.bottom())], black);

    let fid = FontId::proportional(font * 0.85);
    let mut k = 0;
    loop {
        let x = x0 + k as f64 * xs;
        if x > x1 + xs * 1e-9 || k > 200 {
            break;
        }
        painter.text(Pos2::new(sx(x), ax_y - 1.0), Align2::CENTER_BOTTOM, fmt_tick(x, xs), fid.clone(), Color32::BLACK);
        k += 1;
    }
    let mut k = 0;
    loop {
        let y = y0 + k as f64 * ys;
        if y > y1 + ys * 1e-9 || k > 200 {
            break;
        }
        let anchor = if (sx(0.0) - rect.left()).abs() < 1.0 || ax_x == rect.left() {
            Align2::RIGHT_CENTER
        } else {
            Align2::LEFT_CENTER
        };
        let px = if anchor == Align2::RIGHT_CENTER { ax_x - 3.0 } else { ax_x + 3.0 };
        painter.text(Pos2::new(px, sy(y)), anchor, fmt_tick(y, ys), fid.clone(), Color32::BLACK);
        k += 1;
    }

    let clip = painter.with_clip_rect(rect.expand(1.0));
    for (i, s) in data.series.iter().enumerate() {
        let stroke = Stroke::new(2.0, TRACE_COLORS[i % TRACE_COLORS.len()]);
        let mut run: Vec<Pos2> = Vec::new();
        let flush = |run: &mut Vec<Pos2>| {
            if run.len() > 1 {
                clip.add(Shape::line(std::mem::take(run), stroke));
            } else {
                run.clear();
            }
        };
        for p in &s.points {
            if p[0].is_finite() && p[1].is_finite() {
                run.push(Pos2::new(sx(p[0]), sy(p[1])));
            } else {
                flush(&mut run);
            }
        }
        flush(&mut run);
    }
    // frame
    let r = rect;
    painter.add(Shape::closed_line(
        vec![r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom()],
        Stroke::new(1.0, Color32::from_gray(90)),
    ));
}

/// Splits "f(x), g(a, b)" into top-level comma separated parts.
pub fn split_traces(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut cur = String::new();
    for c in s.chars() {
        match c {
            '(' | '[' => depth += 1,
            ')' | ']' => depth -= 1,
            ',' if depth == 0 => {
                out.push(cur.trim().to_string());
                cur.clear();
                continue;
            }
            _ => {}
        }
        cur.push(c);
    }
    out.push(cur.trim().to_string());
    out
}
