//! Mathcad-style X-Y plot: green grid, crossed axes, coloured traces,
//! editable axis limits, per-trace line styles, log scales, legend.

use egui::{Align2, Color32, FontId, Painter, Pos2, Rect, Shape, Stroke, Vec2};
use xmath_core::{AxisStyle, LineStyle, PlotData, PlotFormat};

pub const TRACE_COLORS: [Color32; 6] = [
    Color32::from_rgb(255, 0, 0),
    Color32::from_rgb(0, 0, 255),
    Color32::from_rgb(255, 0, 255),
    Color32::from_rgb(0, 128, 0),
    Color32::from_rgb(0, 160, 160),
    Color32::from_rgb(128, 64, 0),
];

/// Mathcad 15 defaults: trace 1 red solid, 2 blue dotted, 3 magenta dashed, 4 green dash-dot...
pub const TRACE_LINES: [LineStyle; 6] =
    [LineStyle::Solid, LineStyle::Dot, LineStyle::Dash, LineStyle::DashDot, LineStyle::Solid, LineStyle::Dot];

/// Colours offered by the format dialog.
pub const PALETTE: [(&str, [u8; 3]); 8] = [
    ("Красный", [255, 0, 0]),
    ("Синий", [0, 0, 255]),
    ("Пурпурный", [255, 0, 255]),
    ("Зелёный", [0, 128, 0]),
    ("Бирюзовый", [0, 160, 160]),
    ("Коричневый", [128, 64, 0]),
    ("Чёрный", [0, 0, 0]),
    ("Серый", [128, 128, 128]),
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

/// Axis limits as drawn. Values are in "axis space": exponents for log axes.
#[derive(Clone, Copy, Debug)]
pub struct Axes {
    pub x0: f64,
    pub x1: f64,
    pub xs: f64,
    pub y0: f64,
    pub y1: f64,
    pub ys: f64,
    pub log_x: bool,
    pub log_y: bool,
}

impl Axes {
    /// The four limits as data values: x-min, x-max, y-min, y-max.
    pub fn limits(&self) -> [f64; 4] {
        [untr(self.x0, self.log_x), untr(self.x1, self.log_x), untr(self.y0, self.log_y), untr(self.y1, self.log_y)]
    }
}

/// Per-plot settings taken from the region.
#[derive(Clone, Copy)]
pub struct View<'a> {
    pub x_min: Option<f64>,
    pub x_max: Option<f64>,
    pub y_min: Option<f64>,
    pub y_max: Option<f64>,
    pub fmt: &'a PlotFormat,
}

/// Colour, line style and width of trace `i`.
pub fn trace_style(i: usize, fmt: &PlotFormat) -> (Color32, LineStyle, f32) {
    let o = fmt.traces.get(i).copied().unwrap_or_default();
    let c = o.color.map(|[r, g, b]| Color32::from_rgb(r, g, b)).unwrap_or(TRACE_COLORS[i % TRACE_COLORS.len()]);
    (c, o.line.unwrap_or(TRACE_LINES[i % TRACE_LINES.len()]), o.width.unwrap_or(2.0))
}

fn tr(v: f64, log: bool) -> f64 {
    if log {
        v.max(1e-300).log10()
    } else {
        v
    }
}

fn untr(t: f64, log: bool) -> f64 {
    if log {
        10f64.powf(t)
    } else {
        t
    }
}

/// One axis in axis space; `min`/`max` are data limits in axis space (infinite when no data).
fn range(min: f64, max: f64, lo: Option<f64>, hi: Option<f64>, log: bool) -> (f64, f64, f64) {
    let (a, b, st) = if log {
        if min.is_finite() && max.is_finite() {
            let (a, b) = (min.floor(), max.ceil());
            (a, if b > a { b } else { a + 1.0 }, 1.0)
        } else {
            (-1.0, 3.0, 1.0)
        }
    } else {
        axis_range(min, max)
    };
    let a2 = lo.filter(|v| v.is_finite() && (!log || *v > 0.0)).map(|v| tr(v, log)).unwrap_or(a);
    let b2 = hi.filter(|v| v.is_finite() && (!log || *v > 0.0)).map(|v| tr(v, log)).unwrap_or(b);
    let (a2, b2) = if b2 > a2 { (a2, b2) } else { (a2, a2 + 1.0) };
    let st = if (a2, b2) == (a, b) {
        st
    } else if log {
        ((b2 - a2) / 6.0).ceil().max(1.0)
    } else {
        nice_step(b2 - a2, 8.0)
    };
    (a2, b2, st)
}

pub fn axes(data: &PlotData, v: &View) -> Axes {
    let (lx, ly) = (v.fmt.log_x, v.fmt.log_y);
    let (mut xmin, mut xmax, mut ymin, mut ymax) = (f64::INFINITY, f64::NEG_INFINITY, f64::INFINITY, f64::NEG_INFINITY);
    for p in data.series.iter().flat_map(|s| s.points.iter()).filter(|p| p[0].is_finite() && p[1].is_finite()) {
        if (lx && p[0] <= 0.0) || (ly && p[1] <= 0.0) {
            continue;
        }
        let (a, b) = (tr(p[0], lx), tr(p[1], ly));
        xmin = xmin.min(a);
        xmax = xmax.max(a);
        ymin = ymin.min(b);
        ymax = ymax.max(b);
    }
    let (x0, x1, xs) = range(xmin, xmax, v.x_min, v.x_max, lx);
    let (y0, y1, ys) = range(ymin, ymax, v.y_min, v.y_max, ly);
    Axes { x0, x1, xs, y0, y1, ys, log_x: lx, log_y: ly }
}

/// Text of an axis limit in the editing boxes.
pub fn fmt_limit(v: f64) -> String {
    let s = format!("{v:.6}");
    let s = if s.contains('.') { s.trim_end_matches('0').trim_end_matches('.').to_string() } else { s };
    if v != 0.0 && (v.abs() >= 1e6 || v.abs() < 1e-4) {
        format!("{v:e}")
    } else {
        s
    }
}

fn ticks(a: f64, b: f64, st: f64) -> Vec<f64> {
    let mut out = Vec::new();
    let mut k = (a / st - 1e-9).ceil();
    while k * st <= b + st * 1e-9 && out.len() < 400 {
        out.push(k * st);
        k += 1.0;
    }
    out
}

fn line_shapes(pts: Vec<Pos2>, stroke: Stroke, style: LineStyle) -> Vec<Shape> {
    let w = stroke.width;
    match style {
        LineStyle::Solid | LineStyle::Points => vec![Shape::line(pts, stroke)],
        LineStyle::Dot => Shape::dashed_line(&pts, stroke, w, w * 2.0 + 1.0),
        LineStyle::Dash => Shape::dashed_line(&pts, stroke, w * 4.0 + 2.0, w * 2.0 + 1.0),
        LineStyle::DashDot => Shape::dashed_line_with_offset(&pts, stroke, &[w * 4.0 + 2.0, w], &[w * 2.0, w * 2.0], 0.0),
    }
}

/// Draws a short sample of a trace (for legends and the format dialog).
pub fn sample(painter: &Painter, a: Pos2, b: Pos2, color: Color32, style: LineStyle, width: f32) {
    if style == LineStyle::Points {
        painter.circle_filled(a.lerp(b, 0.5), width * 0.5 + 1.0, color);
    } else {
        for sh in line_shapes(vec![a, b], Stroke::new(width, color), style) {
            painter.add(sh);
        }
    }
}

pub fn paint(painter: &Painter, rect: Rect, data: &PlotData, v: &View, font: f32) -> Axes {
    painter.rect_filled(rect, 0.0, Color32::WHITE);
    let ax = axes(data, v);
    let (x0, x1, y0, y1) = (ax.x0, ax.x1, ax.y0, ax.y1);
    let fx = |t: f64| rect.left() + ((t - x0) / (x1 - x0)) as f32 * rect.width();
    let fy = |t: f64| rect.bottom() - ((t - y0) / (y1 - y0)) as f32 * rect.height();
    let sx = |x: f64| fx(tr(x, ax.log_x));
    let sy = |y: f64| fy(tr(y, ax.log_y));
    let xt = ticks(x0, x1, ax.xs);
    let yt = ticks(y0, y1, ax.ys);

    if v.fmt.grid {
        let grid = Stroke::new(1.0_f32, GRID);
        for &t in &xt {
            painter.line_segment([Pos2::new(fx(t), rect.top()), Pos2::new(fx(t), rect.bottom())], grid);
        }
        for &t in &yt {
            painter.line_segment([Pos2::new(rect.left(), fy(t)), Pos2::new(rect.right(), fy(t))], grid);
        }
    }

    // axes: crossed through zero (or along the border when zero is outside), boxed, none
    let crossed = v.fmt.axes == AxisStyle::Crossed;
    let zero_x = !ax.log_x && x0 <= 0.0 && x1 >= 0.0;
    let zero_y = !ax.log_y && y0 <= 0.0 && y1 >= 0.0;
    let ax_y = if crossed && zero_y { fy(0.0) } else { rect.bottom() };
    let ax_x = if crossed && zero_x { fx(0.0) } else { rect.left() };
    if v.fmt.axes != AxisStyle::None {
        let black = Stroke::new(1.0_f32, Color32::BLACK);
        painter.line_segment([Pos2::new(rect.left(), ax_y), Pos2::new(rect.right(), ax_y)], black);
        painter.line_segment([Pos2::new(ax_x, rect.top()), Pos2::new(ax_x, rect.bottom())], black);

        let fid = FontId::proportional(font * 0.85);
        for &t in &xt {
            painter.text(Pos2::new(fx(t), ax_y - 1.0), Align2::CENTER_BOTTOM, fmt_tick(untr(t, ax.log_x), ax.xs), fid.clone(), Color32::BLACK);
        }
        let left = ax_x <= rect.left() + 1.0;
        for &t in &yt {
            let px = if left { rect.left() + 3.0 } else { ax_x + 3.0 };
            painter.text(Pos2::new(px, fy(t)), Align2::LEFT_CENTER, fmt_tick(untr(t, ax.log_y), ax.ys), fid.clone(), Color32::BLACK);
        }
    }

    let clip = painter.with_clip_rect(rect.expand(1.0));
    for (i, s) in data.series.iter().enumerate() {
        let (color, style, width) = trace_style(i, v.fmt);
        let stroke = Stroke::new(width, color);
        let ok = |p: &[f64; 2]| p[0].is_finite() && p[1].is_finite() && (!ax.log_x || p[0] > 0.0) && (!ax.log_y || p[1] > 0.0);
        if style == LineStyle::Points {
            for p in s.points.iter().filter(|p| ok(p)).step_by((s.points.len() / 2000).max(1)) {
                clip.circle_filled(Pos2::new(sx(p[0]), sy(p[1])), width * 0.5 + 1.0, color);
            }
            continue;
        }
        let mut run: Vec<Pos2> = Vec::new();
        let flush = |run: &mut Vec<Pos2>| {
            if run.len() > 1 {
                for sh in line_shapes(std::mem::take(run), stroke, style) {
                    clip.add(sh);
                }
            } else {
                run.clear();
            }
        };
        for p in &s.points {
            if ok(p) {
                run.push(Pos2::new(sx(p[0]), sy(p[1])));
            } else {
                flush(&mut run);
            }
        }
        flush(&mut run);
    }
    if v.fmt.legend {
        let lf = FontId::proportional(font * 0.85);
        let mut y = rect.top() + 6.0;
        for (i, s) in data.series.iter().enumerate() {
            let (c, st, w) = trace_style(i, v.fmt);
            let label = if s.label.is_empty() { format!("trace {}", i + 1) } else { s.label.clone() };
            let g = painter.layout_no_wrap(label, lf.clone(), Color32::BLACK);
            let h = g.size().y;
            let gx = rect.right() - 6.0 - g.size().x;
            painter.rect_filled(Rect::from_min_size(Pos2::new(gx - 36.0, y - 2.0), Vec2::new(g.size().x + 40.0, h + 4.0)), 0.0, Color32::from_white_alpha(220));
            sample(painter, Pos2::new(gx - 32.0, y + h / 2.0), Pos2::new(gx - 6.0, y + h / 2.0), c, st, w);
            painter.galley(Pos2::new(gx, y), g, Color32::BLACK);
            y += h + 4.0;
        }
    }
    // frame
    let r = rect;
    painter.add(Shape::closed_line(
        vec![r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom()],
        Stroke::new(1.0_f32, Color32::from_gray(90)),
    ));
    ax
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
