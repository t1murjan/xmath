//! Mathcad-style plots: X-Y (grid, crossed/boxed axes, secondary Y axis, trace
//! types and symbols, legend, titles, number formats), polar and 3D plots.
//! Geometry comes from `xmath_core::plot_geom` / `plot3d`; this module paints it.

use egui::epaint::TextShape;
use egui::{Align2, Color32, FontId, Painter, Pos2, Rect, Shape, Stroke, Vec2};
use xmath_core::plot_geom::{format_tick_with, label_stride, Prim, Scene, TickLabel};
use xmath_core::{AxisStyle, LegendPos, LineStyle, PlotData, PlotFormat, Symbol, TraceType};

pub use xmath_core::plot_geom::{axis_range, nice_step, ticks};

pub fn rgb(c: [u8; 3]) -> Color32 {
    Color32::from_rgb(c[0], c[1], c[2])
}

/// Colour, line style and width of trace `i`.
pub fn trace_style(i: usize, fmt: &PlotFormat) -> (Color32, LineStyle, f32) {
    let (c, l, w) = xmath_core::plot_geom::trace_style(i, fmt);
    (rgb(c), l, w)
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
    /// Secondary Y axis: (low, high, step), when enabled.
    pub y2: Option<(f64, f64, f64)>,
    pub log_y2: bool,
}

impl Axes {
    /// The four limits as data values: x-min, x-max, y-min, y-max.
    pub fn limits(&self) -> [f64; 4] {
        [untr(self.x0, self.log_x), untr(self.x1, self.log_x), untr(self.y0, self.log_y), untr(self.y1, self.log_y)]
    }
    /// Secondary axis limits as data values.
    pub fn limits2(&self) -> [f64; 2] {
        self.y2.map_or([0.0, 1.0], |(a, b, _)| [untr(a, self.log_y2), untr(b, self.log_y2)])
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
    /// Width / height of the plot box (for equal scales).
    pub aspect: f32,
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
fn range(min: f64, max: f64, lo: Option<f64>, hi: Option<f64>, log: bool, grids: Option<u32>) -> (f64, f64, f64) {
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
    let mut st = if (a2, b2) == (a, b) {
        st
    } else if log {
        ((b2 - a2) / 6.0).ceil().max(1.0)
    } else {
        nice_step(b2 - a2, 8.0)
    };
    if let (Some(n), false) = (grids, log) {
        // about `n` grid intervals, with a round step
        st = nice_step(b2 - a2, n.clamp(1, 100) as f64);
    }
    (a2, b2, st)
}

fn series_extent(data: &PlotData, v: &View, want_y2: bool) -> (f64, f64, f64, f64, f64, f64) {
    let f = v.fmt;
    let (lx, ly) = (f.log_x, if want_y2 { f.log_y2 } else { f.log_y });
    let (mut xmin, mut xmax, mut ymin, mut ymax) = (f64::INFINITY, f64::NEG_INFINITY, f64::INFINITY, f64::NEG_INFINITY);
    for (i, s) in data.series.iter().enumerate() {
        let on2 = f.y2_axis && f.traces.get(i).is_some_and(|t| t.y2);
        let base = matches!(f.traces.get(i).map(|t| t.kind), Some(TraceType::Bars | TraceType::Stem));
        for p in s.points.iter().filter(|p| p[0].is_finite() && p[1].is_finite()) {
            if (lx && p[0] <= 0.0) || (ly && p[1] <= 0.0) {
                continue;
            }
            let (a, b) = (tr(p[0], lx), tr(p[1], ly));
            xmin = xmin.min(a);
            xmax = xmax.max(a);
            if on2 == want_y2 {
                ymin = ymin.min(b);
                ymax = ymax.max(b);
                if base && !ly {
                    ymin = ymin.min(0.0);
                    ymax = ymax.max(0.0);
                }
            }
        }
    }
    (xmin, xmax, ymin, ymax, 0.0, 0.0)
}

pub fn axes(data: &PlotData, v: &View) -> Axes {
    let f = v.fmt;
    let (xmin, xmax, ymin, ymax, ..) = series_extent(data, v, false);
    let g = |k: usize| (!f.axis[k].autogrid).then_some(f.axis[k].grids);
    let (mut x0, mut x1, mut xs) = range(xmin, xmax, v.x_min, v.x_max, f.log_x, g(0));
    let (mut y0, mut y1, mut ys) = range(ymin, ymax, v.y_min, v.y_max, f.log_y, g(1));
    if f.equal_scale && !f.log_x && !f.log_y && v.aspect > 0.0 {
        // the same data units per pixel on both axes: widen the axis that is tighter
        let (px, py) = ((x1 - x0) / v.aspect as f64, y1 - y0);
        if px > py {
            let c = (y0 + y1) / 2.0;
            let h = px / 2.0;
            (y0, y1) = (c - h, c + h);
            ys = nice_step(y1 - y0, 8.0);
        } else {
            let c = (x0 + x1) / 2.0;
            let w = py * v.aspect as f64 / 2.0;
            (x0, x1) = (c - w, c + w);
            xs = nice_step(x1 - x0, 8.0);
        }
    }
    let y2 = f.y2_axis.then(|| {
        let (_, _, a, b, ..) = series_extent(data, v, true);
        range(a, b, f.y2_min, f.y2_max, f.log_y2, g(2))
    });
    Axes { x0, x1, xs, y0, y1, ys, log_x: f.log_x, log_y: f.log_y, y2, log_y2: f.log_y2 }
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

// ----------------------------------------------------------------- text

/// Lays out a tick label (`2×10⁻⁶` as base text plus a raised small exponent).
struct Label {
    base: std::sync::Arc<egui::Galley>,
    exp: Option<std::sync::Arc<egui::Galley>>,
}

impl Label {
    fn new(painter: &Painter, l: &TickLabel, size: f32, color: Color32) -> Label {
        let font = FontId::proportional(size);
        match l.exp {
            None => Label { base: painter.layout_no_wrap(l.mant.clone(), font, color), exp: None },
            Some(e) => Label {
                base: painter.layout_no_wrap(format!("{}×10", l.mant), font, color),
                exp: Some(painter.layout_no_wrap(e.to_string().replace('-', "−"), FontId::proportional(size * 0.68), color)),
            },
        }
    }
    fn size(&self) -> Vec2 {
        let b = self.base.size();
        match &self.exp {
            Some(e) => Vec2::new(b.x + e.size().x, b.y + e.size().y * 0.35),
            None => b,
        }
    }
    /// Draws the label so that the point `align` (0..1) of its box is at `pos`.
    fn draw(&self, painter: &Painter, pos: Pos2, align: [f32; 2], bg: Option<Color32>) -> Rect {
        let s = self.size();
        let tl = Pos2::new(pos.x - s.x * align[0], pos.y - s.y * align[1]);
        let rect = Rect::from_min_size(tl, s);
        if let Some(bg) = bg {
            painter.rect_filled(rect.expand(1.0), 0.0, bg);
        }
        let top = tl + Vec2::new(0.0, self.exp.as_ref().map_or(0.0, |e| e.size().y * 0.35));
        painter.galley(top, self.base.clone(), Color32::BLACK);
        if let Some(e) = &self.exp {
            painter.galley(Pos2::new(top.x + self.base.size().x, tl.y), e.clone(), Color32::BLACK);
        }
        rect
    }
}

fn rotated_text(painter: &Painter, center: Pos2, text: &str, size: f32, color: Color32, up: bool) {
    let g = painter.layout_no_wrap(text.to_string(), FontId::proportional(size), color);
    let (w, h) = (g.size().x, g.size().y);
    // up: text reads bottom-to-top, its box is centred on `center`
    let (pos, angle) = if up {
        (Pos2::new(center.x - h / 2.0, center.y + w / 2.0), -std::f32::consts::FRAC_PI_2)
    } else {
        (Pos2::new(center.x + h / 2.0, center.y - w / 2.0), std::f32::consts::FRAC_PI_2)
    };
    painter.add(Shape::Text(TextShape::new(pos, g, color).with_angle(angle)));
}

// -------------------------------------------------------------- layout

/// Space an X-Y plot needs around its box.
#[derive(Clone, Copy, Debug, Default)]
pub struct Margins {
    pub left: f32,
    pub right: f32,
    pub top: f32,
    pub bottom: f32,
    /// Height of the row of x numbers below a boxed plot (included in `bottom`).
    pub num_bottom: f32,
    /// Width of the y numbers left of a boxed plot (included in `left`).
    pub num_left: f32,
}

/// Axis titles to draw: the user's text, or the unit of the swept variable, in grey.
pub fn axis_titles(fmt: &PlotFormat, data: &PlotData) -> [Option<(String, bool)>; 3] {
    let mut out: [Option<(String, bool)>; 3] = [None, None, None];
    let units = [&data.x_unit, &data.y_unit, &String::new()];
    for k in 0..3 {
        let l = &fmt.labels[k];
        if l.show && !l.text.trim().is_empty() {
            out[k] = Some((l.text.clone(), false));
        } else if !units[k].is_empty() && (k < 2 || fmt.y2_axis) {
            out[k] = Some((format!("[{}]", units[k]), true));
        }
    }
    if k_none(fmt) {
        out[2] = None;
    }
    out
}

fn k_none(fmt: &PlotFormat) -> bool {
    !fmt.y2_axis
}

pub fn margins(fmt: &PlotFormat, data: &PlotData, font: f32) -> Margins {
    let mut m = Margins::default();
    let titles = axis_titles(fmt, data);
    let boxed = fmt.axes == AxisStyle::Boxed;
    let fs = font * 0.85;
    if boxed {
        if fmt.axis[0].numbering {
            m.num_bottom = fs * 1.35;
        }
        if fmt.axis[1].numbering {
            m.num_left = fs * 3.4;
        }
    }
    m.left = m.num_left;
    m.bottom = m.num_bottom;
    if titles[1].is_some() {
        m.left += font * 1.3;
    }
    if titles[0].is_some() {
        m.bottom += font * 1.4;
    }
    if fmt.y2_axis {
        m.right = if fmt.axis[2].numbering && fmt.axes != AxisStyle::None { fs * 3.4 } else { 4.0 };
        if titles[2].is_some() {
            m.right += font * 1.3;
        }
    }
    if fmt.legend_visible() && fmt.legend_pos == LegendPos::Bottom {
        m.bottom += font * 1.5;
    }
    if fmt.show_title && !fmt.title.trim().is_empty() {
        if fmt.title_bottom {
            m.bottom += font * 1.6;
        } else {
            m.top += font * 1.6;
        }
    }
    m
}

// ------------------------------------------------------------- symbols

/// Draws a marker symbol centred at `c`; `w` is the symbol width (Mathcad's «Ширина символа»).
pub fn draw_symbol(painter: &Painter, c: Pos2, sym: Symbol, w: f32, color: Color32) {
    let r = 2.0 + w * 1.5;
    let st = Stroke::new(1.3_f32, color);
    match sym {
        Symbol::None => {}
        Symbol::Triangle => {
            painter.add(Shape::convex_polygon(vec![c + Vec2::new(0.0, -r), c + Vec2::new(r, r * 0.8), c + Vec2::new(-r, r * 0.8)], color, Stroke::NONE));
        }
        Symbol::Square => {
            painter.rect_filled(Rect::from_center_size(c, Vec2::splat(r * 1.6)), 0.0, color);
        }
        Symbol::Circle => {
            painter.circle_filled(c, r * 0.85, color);
        }
        Symbol::Diamond => {
            painter.add(Shape::convex_polygon(vec![c + Vec2::new(0.0, -r * 1.2), c + Vec2::new(r * 1.2, 0.0), c + Vec2::new(0.0, r * 1.2), c + Vec2::new(-r * 1.2, 0.0)], color, Stroke::NONE));
        }
        Symbol::Cross => {
            painter.line_segment([c + Vec2::new(-r, -r), c + Vec2::new(r, r)], st);
            painter.line_segment([c + Vec2::new(-r, r), c + Vec2::new(r, -r)], st);
        }
        Symbol::Plus => {
            painter.line_segment([c + Vec2::new(-r, 0.0), c + Vec2::new(r, 0.0)], st);
            painter.line_segment([c + Vec2::new(0.0, -r), c + Vec2::new(0.0, r)], st);
        }
        Symbol::OCircle => {
            painter.circle_stroke(c, r * 0.85, st);
        }
        Symbol::OSquare => {
            painter.rect_stroke(Rect::from_center_size(c, Vec2::splat(r * 1.6)), 0.0, st, egui::StrokeKind::Middle);
        }
    }
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

fn parse_markers(s: &str) -> Vec<f64> {
    s.split(|c: char| c == ',' || c == ';' || c.is_whitespace()).filter_map(|t| t.replace(',', ".").parse::<f64>().ok()).filter(|v| v.is_finite()).collect()
}

fn trace_label(i: usize, fmt: &PlotFormat, s_label: &str) -> String {
    match fmt.traces.get(i) {
        Some(t) if !t.legend.is_empty() => t.legend.clone(),
        _ => {
            let _ = s_label;
            format!("кривая {}", i + 1)
        }
    }
}

/// Paints the plot into `rect` (the plot box); numbers, titles and legend that
/// live outside the box use the space reserved by [`margins`].
pub fn paint(painter: &Painter, rect: Rect, data: &PlotData, v: &View, font: f32) -> Axes {
    let fmt = v.fmt;
    painter.rect_filled(rect.expand2(Vec2::new(0.0, 0.0)), 0.0, Color32::WHITE);
    let ax = axes(data, v);
    let (x0, x1, y0, y1) = (ax.x0, ax.x1, ax.y0, ax.y1);
    let fx = |t: f64| rect.left() + ((t - x0) / (x1 - x0)) as f32 * rect.width();
    let fy = |t: f64| rect.bottom() - ((t - y0) / (y1 - y0)) as f32 * rect.height();
    let fy2 = |t: f64| match ax.y2 {
        Some((a, b, _)) => rect.bottom() - ((t - a) / (b - a)) as f32 * rect.height(),
        None => fy(t),
    };
    let sx = |x: f64| fx(tr(x, ax.log_x));
    let sy = |y: f64| fy(tr(y, ax.log_y));
    let sy2 = |y: f64| fy2(tr(y, ax.log_y2));
    let xt = ticks(x0, x1, ax.xs);
    let yt = ticks(y0, y1, ax.ys);
    let y2t = ax.y2.map(|(a, b, s)| ticks(a, b, s)).unwrap_or_default();
    let m = margins(fmt, data, font);
    let fs = font * 0.85;

    // grid
    let grid_line = |k: usize| Stroke::new(1.0_f32, rgb(fmt.axis[k].grid_color));
    if fmt.grid_of(0) {
        for &t in &xt {
            painter.line_segment([Pos2::new(fx(t), rect.top()), Pos2::new(fx(t), rect.bottom())], grid_line(0));
        }
    }
    if fmt.grid_of(1) {
        for &t in &yt {
            painter.line_segment([Pos2::new(rect.left(), fy(t)), Pos2::new(rect.right(), fy(t))], grid_line(1));
        }
    }
    if fmt.grid_of(2) && ax.y2.is_some() {
        for &t in &y2t {
            painter.line_segment([Pos2::new(rect.left(), fy2(t)), Pos2::new(rect.right(), fy2(t))], grid_line(2));
        }
    }
    // marker lines
    for (k, axis) in [(0usize, 0), (1, 1), (2, 2)] {
        let o = &fmt.axis[k];
        if !o.markers || (k == 2 && ax.y2.is_none()) {
            continue;
        }
        let st = Stroke::new(1.0_f32, rgb(o.marker_color));
        for val in parse_markers(&o.marker_values) {
            if (axis == 0 && ax.log_x || axis == 1 && ax.log_y || axis == 2 && ax.log_y2) && val <= 0.0 {
                continue;
            }
            let (a, b) = match axis {
                0 => (Pos2::new(sx(val), rect.top()), Pos2::new(sx(val), rect.bottom())),
                1 => (Pos2::new(rect.left(), sy(val)), Pos2::new(rect.right(), sy(val))),
                _ => (Pos2::new(rect.left(), sy2(val)), Pos2::new(rect.right(), sy2(val))),
            };
            for sh in Shape::dashed_line(&[a, b], st, 5.0, 3.0) {
                painter.with_clip_rect(rect).add(sh);
            }
        }
    }

    // axes: crossed through zero (or along the border when zero is outside), boxed, none
    let crossed = fmt.axes == AxisStyle::Crossed;
    let boxed = fmt.axes == AxisStyle::Boxed;
    let zero_x = !ax.log_x && x0 <= 0.0 && x1 >= 0.0;
    let zero_y = !ax.log_y && y0 <= 0.0 && y1 >= 0.0;
    let ax_y = if crossed && zero_y { fy(0.0) } else { rect.bottom() };
    let ax_x = if crossed && zero_x { fx(0.0) } else { rect.left() };
    let fid = FontId::proportional(fs);
    let _ = &fid;
    if fmt.axes != AxisStyle::None {
        let black = Stroke::new(1.0_f32, Color32::BLACK);
        painter.line_segment([Pos2::new(rect.left(), ax_y), Pos2::new(rect.right(), ax_y)], black);
        painter.line_segment([Pos2::new(ax_x, rect.top()), Pos2::new(ax_x, rect.bottom())], black);
        let nf = &fmt.number;
        // x numbers: outside below the box (boxed), else beside the axis line
        if fmt.axis[0].numbering {
            let x_inside_bottom = ax_y >= rect.bottom() - 1.0;
            let (py, ay) = if boxed {
                (rect.bottom() + 3.0, 0.0)
            } else if x_inside_bottom {
                (ax_y - 2.0, 1.0)
            } else {
                (ax_y + 3.0, 0.0)
            };
            let skip_zero = crossed && zero_x && zero_y;
            let labels: Vec<(f64, Label)> = xt
                .iter()
                .filter(|t| !(skip_zero && t.abs() < ax.xs * 1e-9))
                .map(|&t| (t, Label::new(painter, &format_tick_with(untr(t, ax.log_x), nf), fs, Color32::BLACK)))
                .collect();
            let centers: Vec<f32> = labels.iter().map(|(t, _)| fx(*t)).collect();
            let widths: Vec<f32> = labels.iter().map(|(_, l)| l.size().x).collect();
            let stride = label_stride(&centers, &widths, 4.0);
            for (i, (t, l)) in labels.iter().enumerate().step_by(stride) {
                l.draw(painter, Pos2::new(fx(*t), py), [0.5, ay], None);
                let _ = i;
            }
        }
        if fmt.axis[1].numbering {
            let at_left = ax_x <= rect.left() + 1.0;
            let (px, ax_al) = if boxed {
                (rect.left() - 4.0, 1.0)
            } else if at_left {
                (rect.left() + 3.0, 0.0)
            } else {
                (ax_x - 3.0, 1.0)
            };
            let skip_zero = crossed && zero_x && zero_y;
            let labels: Vec<(f64, Label)> = yt
                .iter()
                .filter(|t| !(skip_zero && t.abs() < ax.ys * 1e-9))
                .map(|&t| (t, Label::new(painter, &format_tick_with(untr(t, ax.log_y), nf), fs, Color32::BLACK)))
                .collect();
            let centers: Vec<f32> = labels.iter().map(|(t, _)| fy(*t)).collect();
            let heights: Vec<f32> = labels.iter().map(|(_, l)| l.size().y).collect();
            let stride = label_stride(&centers, &heights, 1.0);
            for (t, l) in labels.iter().step_by(stride) {
                l.draw(painter, Pos2::new(px, fy(*t)), [ax_al, 0.5], None);
            }
        }
        if let (Some(_), true) = (ax.y2, fmt.axis[2].numbering) {
            painter.line_segment([Pos2::new(rect.right(), rect.top()), Pos2::new(rect.right(), rect.bottom())], black);
            let labels: Vec<(f64, Label)> = y2t.iter().map(|&t| (t, Label::new(painter, &format_tick_with(untr(t, ax.log_y2), nf), fs, Color32::BLACK))).collect();
            let centers: Vec<f32> = labels.iter().map(|(t, _)| fy2(*t)).collect();
            let heights: Vec<f32> = labels.iter().map(|(_, l)| l.size().y).collect();
            let stride = label_stride(&centers, &heights, 1.0);
            for (t, l) in labels.iter().step_by(stride) {
                l.draw(painter, Pos2::new(rect.right() + 4.0, fy2(*t)), [0.0, 0.5], None);
            }
        }
    }

    // traces
    let clip = painter.with_clip_rect(rect.expand(1.0));
    for (i, s) in data.series.iter().enumerate() {
        let (color, style, width) = trace_style(i, fmt);
        let tf = fmt.traces.get(i).cloned().unwrap_or_default();
        let on2 = fmt.y2_axis && tf.y2 && ax.y2.is_some();
        let (log_y, syy): (bool, &dyn Fn(f64) -> f32) = if on2 { (ax.log_y2, &sy2) } else { (ax.log_y, &sy) };
        let ok = |p: &[f64; 2]| p[0].is_finite() && p[1].is_finite() && (!ax.log_x || p[0] > 0.0) && (!log_y || p[1] > 0.0);
        let screen: Vec<Option<Pos2>> = s.points.iter().map(|p| ok(p).then(|| Pos2::new(sx(p[0]), syy(p[1])))).collect();
        let base_y = {
            let zero = if log_y { rect.bottom() } else if on2 { sy2(0.0) } else { sy(0.0) };
            zero.clamp(rect.top(), rect.bottom())
        };
        let kind = if style == LineStyle::Points && tf.kind == TraceType::Lines { TraceType::Points } else { tf.kind };
        let stroke = Stroke::new(width, color);
        let draw_runs = |pts: &dyn Fn() -> Vec<Vec<Pos2>>| {
            for run in pts() {
                if run.len() > 1 {
                    for sh in line_shapes(run, stroke, style) {
                        clip.add(sh);
                    }
                }
            }
        };
        let runs = |step: bool| -> Vec<Vec<Pos2>> {
            let mut out: Vec<Vec<Pos2>> = Vec::new();
            let mut cur: Vec<Pos2> = Vec::new();
            for p in &screen {
                match p {
                    Some(q) => {
                        if step {
                            if let Some(l) = cur.last().copied() {
                                cur.push(Pos2::new(q.x, l.y));
                            }
                        }
                        cur.push(*q);
                    }
                    None => out.push(std::mem::take(&mut cur)),
                }
            }
            out.push(cur);
            out
        };
        let pts_ok: Vec<Pos2> = screen.iter().flatten().copied().collect();
        match kind {
            TraceType::Lines => draw_runs(&|| runs(false)),
            TraceType::Step => draw_runs(&|| runs(true)),
            TraceType::Points => {
                for q in pts_ok.iter().step_by((pts_ok.len() / 2000).max(1)) {
                    clip.circle_filled(*q, width * 0.5 + 1.0, color);
                }
            }
            TraceType::Bars => {
                let spacing = pts_ok.windows(2).map(|w| (w[1].x - w[0].x).abs()).fold(f32::MAX, f32::min);
                let bw = if spacing.is_finite() && spacing < f32::MAX { (spacing * 0.8).clamp(2.0, 80.0) } else { 10.0 };
                for q in &pts_ok {
                    let r = Rect::from_x_y_ranges(q.x - bw / 2.0..=q.x + bw / 2.0, q.y.min(base_y)..=q.y.max(base_y));
                    clip.rect_filled(r, 0.0, color.gamma_multiply(0.55));
                    clip.rect_stroke(r, 0.0, Stroke::new(1.0_f32, color), egui::StrokeKind::Middle);
                }
            }
            TraceType::Stem => {
                for q in &pts_ok {
                    clip.line_segment([Pos2::new(q.x, base_y), *q], stroke);
                    clip.circle_filled(*q, width + 1.5, color);
                }
            }
            TraceType::Error => {
                for q in &pts_ok {
                    let e = 5.0 + width * 2.0;
                    clip.line_segment([Pos2::new(q.x, q.y - e), Pos2::new(q.x, q.y + e)], stroke);
                    clip.line_segment([Pos2::new(q.x - 3.0, q.y - e), Pos2::new(q.x + 3.0, q.y - e)], stroke);
                    clip.line_segment([Pos2::new(q.x - 3.0, q.y + e), Pos2::new(q.x + 3.0, q.y + e)], stroke);
                    clip.circle_filled(*q, 2.0, color);
                }
            }
        }
        if tf.symbol != Symbol::None {
            let freq = tf.symbol_freq.unwrap_or(1).max(1) as usize;
            let sw = tf.symbol_width.unwrap_or(1.0);
            for q in pts_ok.iter().step_by(freq) {
                if rect.expand(4.0).contains(*q) {
                    draw_symbol(&clip, *q, tf.symbol, sw, color);
                }
            }
        }
    }

    // frame
    painter.add(Shape::closed_line(vec![rect.left_top(), rect.right_top(), rect.right_bottom(), rect.left_bottom()], Stroke::new(1.0_f32, Color32::from_gray(90))));

    // legend
    if fmt.legend_visible() && !data.series.is_empty() {
        let lf = FontId::proportional(fs);
        let entries: Vec<(usize, std::sync::Arc<egui::Galley>)> =
            data.series.iter().enumerate().map(|(i, s)| (i, painter.layout_no_wrap(trace_label(i, fmt, &s.label), lf.clone(), Color32::BLACK))).collect();
        let draw_sample = |i: usize, a: Pos2, b: Pos2| {
            let (c, st, w) = trace_style(i, fmt);
            sample(painter, a, b, c, st, w);
            if let Some(t) = fmt.traces.get(i) {
                if t.symbol != Symbol::None {
                    draw_symbol(painter, a.lerp(b, 0.5), t.symbol, t.symbol_width.unwrap_or(1.0), c);
                }
            }
        };
        if fmt.legend_pos == LegendPos::Bottom {
            let total: f32 = entries.iter().map(|(_, g)| g.size().x + 40.0 + 12.0).sum::<f32>() - 12.0;
            let mut x = rect.center().x - total / 2.0;
            let y = rect.bottom() + m.num_bottom + (m.bottom - m.num_bottom).min(font * 1.5) - font * 1.5 + font * 0.2
                + if fmt.labels[0].show && axis_titles(fmt, data)[0].is_some() { font * 1.4 } else { 0.0 };
            for (i, g) in &entries {
                draw_sample(*i, Pos2::new(x, y + g.size().y / 2.0), Pos2::new(x + 28.0, y + g.size().y / 2.0));
                painter.galley(Pos2::new(x + 34.0, y), g.clone(), Color32::BLACK);
                x += g.size().x + 40.0 + 12.0;
            }
        } else {
            let w = entries.iter().map(|(_, g)| g.size().x).fold(0.0, f32::max) + 46.0;
            let h = entries.iter().map(|(_, g)| g.size().y + 3.0).sum::<f32>() + 4.0;
            let left = matches!(fmt.legend_pos, LegendPos::TopLeft | LegendPos::BottomLeft);
            let top = matches!(fmt.legend_pos, LegendPos::TopLeft | LegendPos::TopRight);
            let ox = if left { rect.left() + 6.0 } else { rect.right() - 6.0 - w };
            let oy = if top { rect.top() + 6.0 } else { rect.bottom() - 6.0 - h };
            painter.rect_filled(Rect::from_min_size(Pos2::new(ox, oy), Vec2::new(w, h)), 0.0, Color32::from_white_alpha(225));
            let mut y = oy + 3.0;
            for (i, g) in &entries {
                draw_sample(*i, Pos2::new(ox + 4.0, y + g.size().y / 2.0), Pos2::new(ox + 32.0, y + g.size().y / 2.0));
                painter.galley(Pos2::new(ox + 38.0, y), g.clone(), Color32::BLACK);
                y += g.size().y + 3.0;
            }
        }
    }

    // titles
    let titles = axis_titles(fmt, data);
    let tcol = |grey: bool| if grey { Color32::GRAY } else { Color32::BLACK };
    if let Some((t, grey)) = &titles[0] {
        let y = rect.bottom() + m.num_bottom + font * 0.15;
        painter.text(Pos2::new(rect.center().x, y), Align2::CENTER_TOP, t, FontId::proportional(font), tcol(*grey));
    }
    if let Some((t, grey)) = &titles[1] {
        rotated_text(painter, Pos2::new(rect.left() - m.num_left - font * 0.7, rect.center().y), t, font, tcol(*grey), true);
    }
    if let Some((t, grey)) = &titles[2] {
        rotated_text(painter, Pos2::new(rect.right() + m.right - font * 0.65, rect.center().y), t, font, tcol(*grey), false);
    }
    if fmt.show_title && !fmt.title.trim().is_empty() {
        let (y, al) = if fmt.title_bottom { (rect.bottom() + m.bottom - font * 0.2, Align2::CENTER_BOTTOM) } else { (rect.top() - font * 0.35, Align2::CENTER_BOTTOM) };
        painter.text(Pos2::new(rect.center().x, y), al, &fmt.title, FontId::proportional(font * 1.1), Color32::BLACK);
    }
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

// ---------------------------------------------------------------- scenes

fn p2(origin: Pos2, p: [f32; 2]) -> Pos2 {
    Pos2::new(origin.x + p[0], origin.y + p[1])
}

/// Paints a core [`Scene`] with its top-left corner at `origin`.
pub fn paint_scene(painter: &Painter, origin: Pos2, scene: &Scene) {
    for prim in &scene.prims {
        match prim {
            Prim::Poly { pts, fill, stroke } => {
                let pts: Vec<Pos2> = pts.iter().map(|p| p2(origin, *p)).collect();
                if let Some(f) = fill {
                    // a plain triangle fan: convex_polygon's anti-aliasing fringe spikes on thin slivers
                    let mut mesh = egui::epaint::Mesh::default();
                    let c = rgb(*f);
                    for q in &pts {
                        mesh.colored_vertex(*q, c);
                    }
                    for k in 1..pts.len().saturating_sub(1) as u32 {
                        mesh.add_triangle(0, k, k + 1);
                    }
                    painter.add(Shape::mesh(mesh));
                }
                // edges as separate segments: a closed stroke of thin slivers produces miter spikes
                if let Some((c, w)) = stroke {
                    let st = Stroke::new(*w, rgb(*c));
                    for k in 0..pts.len() {
                        painter.line_segment([pts[k], pts[(k + 1) % pts.len()]], st);
                    }
                }
            }
            Prim::Line { pts, color, width, style } => {
                let pts: Vec<Pos2> = pts.iter().map(|p| p2(origin, *p)).collect();
                for sh in line_shapes(pts, Stroke::new(*width, rgb(*color)), *style) {
                    painter.add(sh);
                }
            }
            Prim::Dot { c, r, color } => {
                painter.circle_filled(p2(origin, *c), *r, rgb(*color));
            }
            Prim::Text { pos, label, align, size, color, bg } => {
                let l = Label::new(painter, label, *size, rgb(*color));
                l.draw(painter, p2(origin, *pos), *align, bg.map(rgb));
            }
        }
    }
}

/// Paints a polar plot into `rect` and returns its geometry.
pub fn paint_polar(painter: &Painter, rect: Rect, data: &PlotData, v: &View, font: f32) -> xmath_core::plot_geom::PolarGeom {
    painter.rect_filled(rect, 0.0, Color32::WHITE);
    let (scene, geom) = xmath_core::plot_geom::polar_scene(&data.series, [rect.width(), rect.height()], v.fmt, v.y_min, v.y_max, font);
    paint_scene(&painter.with_clip_rect(rect.expand(font * 2.0)), rect.min, &scene);
    geom
}

// ------------------------------------------------------------ layout

/// Width of the column left of an X-Y plot that holds the ordinate expressions, at zoom 1.
pub const LEGEND_W: f32 = 80.0;

/// Positions inside a plot region, relative to the region's top-left corner.
#[derive(Clone, Copy, Debug)]
pub struct PlotGeom {
    pub plot_rect: Rect,
    pub margins: Margins,
    /// Width of the column of ordinate expressions (grown when an expression is wider).
    pub legend_w: f32,
    /// Top of the row with the abscissa expression.
    pub label_row: f32,
    /// Bottom of the region without error lines.
    pub bottom: f32,
    pub right: f32,
}

/// Lays out a plot region of `w` x `h` pixels (already scaled by `zoom`); `y_text_w` is the
/// widest ordinate expression (or edit field), which pushes the plot to the right instead of being clipped.
#[allow(clippy::too_many_arguments)]
pub fn plot_geom(fmt: &PlotFormat, data: &PlotData, polar: bool, w: f32, h: f32, fs: f32, zoom: f32, y_text_w: f32) -> PlotGeom {
    let base = LEGEND_W * zoom;
    let legend_w = base.max(y_text_w + 14.0);
    let m = if polar { Margins::default() } else { margins(fmt, data, fs) };
    let plot_rect = Rect::from_min_size(
        Pos2::new(legend_w + m.left, m.top),
        Vec2::new((w - base - m.left - m.right).max(60.0), (h - fs * 2.0 - m.top - m.bottom).max(40.0)),
    );
    let label_row = plot_rect.bottom() + m.bottom + fs * 0.4;
    PlotGeom { plot_rect, margins: m, legend_w, label_row, bottom: label_row + fs * 1.6, right: plot_rect.right() + m.right }
}
