//! A small drawing interface for printing: everything on a page (formulas, text, plots,
//! pictures) is reduced to lines, polygons, text runs and bitmaps. The PDF writer implements
//! [`Canvas`]; new kinds of plots can draw to it directly, or simply paint with an
//! [`egui::Painter`] and let [`paint_shapes`] translate the shapes.
//!
//! Coordinates are page pixels (96 dpi, y down, origin at the top-left corner of the page).

use egui::epaint::{ClippedShape, ColorMode, Shape, StrokeKind, TextShape};
use egui::{Color32, FontFamily, Pos2, Rect, TextureId, Vec2};

pub type P = (f32, f32);

/// Straight (non-premultiplied) RGBA colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Rgba(pub u8, pub u8, pub u8, pub u8);

impl Rgba {
    pub const BLACK: Rgba = Rgba(0, 0, 0, 255);

    pub fn from_color32(c: Color32) -> Rgba {
        let [r, g, b, a] = c.to_srgba_unmultiplied();
        Rgba(r, g, b, a)
    }

    pub fn is_transparent(self) -> bool {
        self.3 == 0
    }
}

/// One positioned glyph: `x` is the pen position, `y` the baseline.
#[derive(Clone, Copy, Debug)]
pub struct GlyphPos {
    pub ch: char,
    pub x: f32,
    pub y: f32,
}

/// A run of glyphs in one font, size and colour.
#[derive(Clone, Debug)]
pub struct TextRun {
    pub family: FontFamily,
    /// Font size in page pixels.
    pub size: f32,
    pub color: Rgba,
    pub italic: bool,
    /// Draw twice, shifted by a fraction of the size (a bold face that the system lacks).
    pub fake_bold: bool,
    /// Rotation of the baseline in radians (clockwise on the y-down page; 0 is horizontal).
    pub angle: f32,
    pub glyphs: Vec<GlyphPos>,
}

/// An RGBA bitmap (straight alpha).
#[derive(Clone, Debug)]
pub struct Bitmap {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

pub trait Canvas {
    /// Restricts drawing to `rect` (min x, min y, max x, max y); `None` removes the restriction.
    fn clip(&mut self, rect: Option<[f32; 4]>);
    /// A straight line, optionally dashed `(dash, gap)`.
    fn line(&mut self, a: P, b: P, width: f32, color: Rgba, dash: Option<(f32, f32)>);
    fn polyline(&mut self, pts: &[P], width: f32, color: Rgba, closed: bool);
    fn fill_poly(&mut self, pts: &[P], color: Rgba);
    fn text(&mut self, run: &TextRun);
    /// A picture stretched over `rect` (min x, min y, max x, max y).
    fn image(&mut self, rect: [f32; 4], bitmap: &Bitmap);

    /// A circle (approximated by a polygon).
    fn circle(&mut self, c: P, r: f32, fill: Option<Rgba>, stroke: Option<(f32, Rgba)>) {
        let n = ((r * 3.0).ceil() as usize).clamp(16, 96);
        let pts: Vec<P> = (0..n)
            .map(|i| {
                let a = i as f32 / n as f32 * std::f32::consts::TAU;
                (c.0 + r * a.cos(), c.1 + r * a.sin())
            })
            .collect();
        if let Some(f) = fill {
            self.fill_poly(&pts, f);
        }
        if let Some((w, col)) = stroke {
            self.polyline(&pts, w, col, true);
        }
    }

    /// A rectangle (min x, min y, max x, max y).
    fn rect(&mut self, r: [f32; 4], fill: Option<Rgba>, stroke: Option<(f32, Rgba)>) {
        let pts = [(r[0], r[1]), (r[2], r[1]), (r[2], r[3]), (r[0], r[3])];
        if let Some(f) = fill {
            self.fill_poly(&pts, f);
        }
        if let Some((w, col)) = stroke {
            self.polyline(&pts, w, col, true);
        }
    }
}

fn pt(p: Pos2, o: Vec2) -> P {
    (p.x + o.x, p.y + o.y)
}

/// Draws the shapes an [`egui::Painter`] produced (after `Context::run`) onto `canvas`,
/// moved by `origin`. Text, lines, polygons, circles and rectangles are supported; textured
/// meshes (images) are skipped, untextured ones are drawn as filled triangles.
pub fn paint_shapes(shapes: &[ClippedShape], canvas: &mut dyn Canvas, origin: Vec2, has_bold_font: bool) {
    let mut last_clip: Option<Rect> = None;
    for cs in shapes {
        if last_clip != Some(cs.clip_rect) {
            let c = cs.clip_rect;
            let huge = c.width() > 1.0e5 || c.height() > 1.0e5;
            canvas.clip(if huge { None } else { Some([c.min.x + origin.x, c.min.y + origin.y, c.max.x + origin.x, c.max.y + origin.y]) });
            last_clip = Some(cs.clip_rect);
        }
        paint_shape(&cs.shape, canvas, origin, has_bold_font);
    }
    canvas.clip(None);
}

fn stroke_of(w: f32, c: Color32) -> Option<(f32, Rgba)> {
    (w > 0.0 && c.a() > 0).then(|| (w, Rgba::from_color32(c)))
}

fn paint_shape(shape: &Shape, canvas: &mut dyn Canvas, o: Vec2, has_bold: bool) {
    match shape {
        Shape::Noop => {}
        Shape::Vec(v) => v.iter().for_each(|s| paint_shape(s, canvas, o, has_bold)),
        Shape::LineSegment { points, stroke } => {
            if let Some((w, c)) = stroke_of(stroke.width, stroke.color) {
                canvas.line(pt(points[0], o), pt(points[1], o), w, c, None);
            }
        }
        Shape::Path(p) => {
            let pts: Vec<P> = p.points.iter().map(|q| pt(*q, o)).collect();
            if pts.len() < 2 {
                return;
            }
            if p.fill.a() > 0 && pts.len() >= 3 {
                canvas.fill_poly(&pts, Rgba::from_color32(p.fill));
            }
            if let ColorMode::Solid(c) = p.stroke.color {
                if let Some((w, c)) = stroke_of(p.stroke.width, c) {
                    canvas.polyline(&pts, w, c, p.closed);
                }
            }
        }
        Shape::Rect(r) => {
            let mut rect = r.rect;
            let w = r.stroke.width;
            match r.stroke_kind {
                StrokeKind::Inside => rect = rect.shrink(w / 2.0),
                StrokeKind::Outside => rect = rect.expand(w / 2.0),
                StrokeKind::Middle => {}
            }
            let fill = (r.fill.a() > 0).then(|| Rgba::from_color32(r.fill));
            let min = pt(rect.min, o);
            let max = pt(rect.max, o);
            canvas.rect([min.0, min.1, max.0, max.1], fill, stroke_of(w, r.stroke.color));
        }
        Shape::Circle(c) => {
            let fill = (c.fill.a() > 0).then(|| Rgba::from_color32(c.fill));
            canvas.circle(pt(c.center, o), c.radius, fill, stroke_of(c.stroke.width, c.stroke.color));
        }
        Shape::Ellipse(e) => {
            let n = 48;
            let pts: Vec<P> = (0..n)
                .map(|i| {
                    let a = i as f32 / n as f32 * std::f32::consts::TAU;
                    (e.center.x + o.x + e.radius.x * a.cos(), e.center.y + o.y + e.radius.y * a.sin())
                })
                .collect();
            if e.fill.a() > 0 {
                canvas.fill_poly(&pts, Rgba::from_color32(e.fill));
            }
            if let Some((w, c)) = stroke_of(e.stroke.width, e.stroke.color) {
                canvas.polyline(&pts, w, c, true);
            }
        }
        Shape::QuadraticBezier(b) => {
            let pts: Vec<P> = b.flatten(Some(0.2)).iter().map(|q| pt(*q, o)).collect();
            if let ColorMode::Solid(c) = b.stroke.color {
                if let Some((w, c)) = stroke_of(b.stroke.width, c) {
                    canvas.polyline(&pts, w, c, false);
                }
            }
        }
        Shape::CubicBezier(b) => {
            let pts: Vec<P> = b.flatten(Some(0.2)).iter().map(|q| pt(*q, o)).collect();
            if let ColorMode::Solid(c) = b.stroke.color {
                if let Some((w, c)) = stroke_of(b.stroke.width, c) {
                    canvas.polyline(&pts, w, c, false);
                }
            }
        }
        Shape::Mesh(m) => {
            // untextured (white pixel) triangles, e.g. the filled surface of a plot
            if m.texture_id != TextureId::default() {
                return;
            }
            for t in m.indices.chunks_exact(3) {
                let v: Vec<_> = t.iter().filter_map(|i| m.vertices.get(*i as usize)).collect();
                if v.len() != 3 {
                    continue;
                }
                let avg = |f: fn(&Color32) -> u8| ((f(&v[0].color) as u32 + f(&v[1].color) as u32 + f(&v[2].color) as u32) / 3) as u8;
                let c = Color32::from_rgba_unmultiplied(avg(|c| c.r()), avg(|c| c.g()), avg(|c| c.b()), avg(|c| c.a()));
                if c.a() > 0 {
                    let pts = [pt(v[0].pos, o), pt(v[1].pos, o), pt(v[2].pos, o)];
                    canvas.fill_poly(&pts, Rgba::from_color32(c));
                }
            }
        }
        Shape::Text(t) => paint_text(t, canvas, o, has_bold),
        Shape::Callback(_) => {}
    }
}

fn paint_text(t: &TextShape, canvas: &mut dyn Canvas, o: Vec2, has_bold: bool) {
    let job = &t.galley.job;
    let org = t.pos + o;
    let (sin, cos) = t.angle.sin_cos();
    // glyph positions are laid out horizontally from `org` and rotated about it
    let place = |gx: f32, gy: f32| -> (f32, f32) { (org.x + gx * cos - gy * sin, org.y + gx * sin + gy * cos) };
    let mut formats = job.sections.iter().flat_map(|section| {
        job.text[section.byte_range.clone()].chars()
            .filter(|&ch| !job.break_on_newline || ch != '\n')
            .map(|_| &section.format)
    }).peekable();
    for row in &t.galley.rows {
        let place = |x, y| place(row.pos.x + x, row.pos.y + y);
        // consecutive glyphs of one section form a run
        let mut i = 0;
        while i < row.glyphs.len() {
            let Some(f) = formats.next() else { break };
            let mut j = i + 1;
            while j < row.glyphs.len() && formats.next_if(|next| std::ptr::eq(*next, f)).is_some() {
                j += 1;
            }
            let color = t.override_text_color.unwrap_or(if f.color == Color32::PLACEHOLDER { t.fallback_color } else { f.color });
            let glyphs: Vec<GlyphPos> = row.glyphs[i..j]
                .iter()
                .filter(|g| !g.chr.is_control())
                .map(|g| {
                    let (x, y) = place(g.pos.x, g.pos.y);
                    GlyphPos { ch: g.chr, x, y }
                })
                .collect();
            let bold_fam = matches!(&f.font_id.family, FontFamily::Name(n) if &**n == "bold");
            if !glyphs.is_empty() && color.a() > 0 {
                canvas.text(&TextRun {
                    family: f.font_id.family.clone(),
                    size: f.font_id.size,
                    color: Rgba::from_color32(color),
                    italic: f.italics,
                    fake_bold: bold_fam && !has_bold,
                    angle: t.angle,
                    glyphs,
                });
            }
            for (stroke, kind) in [(f.underline, 0), (f.strikethrough, 1), (t.underline, 0)] {
                if stroke.width > 0.0 && stroke.color.a() > 0 {
                    let first = &row.glyphs[i];
                    let last = &row.glyphs[j - 1];
                    let y = match kind {
                        0 => first.pos.y - first.font_ascent + first.line_height,
                        _ => first.pos.y - first.font_ascent + first.line_height / 2.0,
                    };
                    canvas.line(place(first.pos.x, y), place(last.pos.x + last.advance_width, y), stroke.width, Rgba::from_color32(stroke.color), None);
                }
            }
            i = j;
        }
    }
}
