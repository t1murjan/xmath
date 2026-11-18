//! Raster backend of [`Canvas`] (software rendering with `tiny-skia`): the pictures of plots that
//! are stored in `.xmcd` files next to Mathcad's own plot data, so that the plot is visible
//! even where the file is read by something that cannot interpret the plot item.
//!
//! Text is drawn from the glyph outlines of the application's fonts.

use crate::canvas::{Bitmap, Canvas, Rgba, TextRun, P};
use egui::{FontDefinitions, FontFamily};
use std::collections::HashMap;
use tiny_skia::{FillRule, Mask, Paint, PathBuilder, Pixmap, PixmapPaint, Stroke, StrokeDash, Transform};

/// Italic slant, as egui draws synthetic italics.
const ITALIC_SLANT: f32 = 0.25;

pub struct RasterCanvas {
    pub pixmap: Pixmap,
    /// Device pixels per canvas pixel.
    scale: f32,
    defs: FontDefinitions,
    clip: Option<Mask>,
    glyph_cache: HashMap<(String, char), Option<tiny_skia::Path>>,
}

/// Collects a glyph outline into a path (font units, y up).
struct Outline(PathBuilder);

impl ttf_parser::OutlineBuilder for Outline {
    fn move_to(&mut self, x: f32, y: f32) {
        self.0.move_to(x, y);
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.0.line_to(x, y);
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        self.0.quad_to(x1, y1, x, y);
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.0.cubic_to(x1, y1, x2, y2, x, y);
    }
    fn close(&mut self) {
        self.0.close();
    }
}

fn paint_of(c: Rgba) -> Paint<'static> {
    let mut p = Paint::default();
    p.set_color_rgba8(c.0, c.1, c.2, c.3);
    p.anti_alias = true;
    p
}

impl RasterCanvas {
    /// A white canvas of `w` x `h` canvas pixels drawn at `scale` device pixels per pixel.
    pub fn new(w: f32, h: f32, scale: f32, defs: FontDefinitions) -> Option<RasterCanvas> {
        let mut pixmap = Pixmap::new((w * scale).ceil().max(1.0) as u32, (h * scale).ceil().max(1.0) as u32)?;
        pixmap.fill(tiny_skia::Color::WHITE);
        Some(RasterCanvas { pixmap, scale, defs, clip: None, glyph_cache: HashMap::new() })
    }

    pub fn png(&self) -> Vec<u8> {
        // the canvas starts opaque white, so premultiplied and straight alpha agree
        let (w, h) = (self.pixmap.width(), self.pixmap.height());
        let Some(img) = ::image::RgbaImage::from_raw(w, h, self.pixmap.data().to_vec()) else { return Vec::new() };
        let mut out = std::io::Cursor::new(Vec::new());
        match img.write_to(&mut out, ::image::ImageFormat::Png) {
            Ok(()) => out.into_inner(),
            Err(_) => Vec::new(),
        }
    }

    fn base(&self) -> Transform {
        Transform::from_scale(self.scale, self.scale)
    }

    fn path_of(pts: &[P], closed: bool) -> Option<tiny_skia::Path> {
        let mut pb = PathBuilder::new();
        for (i, p) in pts.iter().enumerate() {
            if !(p.0.is_finite() && p.1.is_finite()) {
                return None;
            }
            if i == 0 {
                pb.move_to(p.0, p.1);
            } else {
                pb.line_to(p.0, p.1);
            }
        }
        if closed {
            pb.close();
        }
        pb.finish()
    }

    fn stroke_path(&mut self, path: &tiny_skia::Path, width: f32, color: Rgba, dash: Option<(f32, f32)>) {
        let mut stroke = Stroke { width: width.max(0.1), ..Default::default() };
        stroke.line_join = tiny_skia::LineJoin::Round;
        if let Some((d, g)) = dash {
            stroke.dash = StrokeDash::new(vec![d.max(0.1), g.max(0.1)], 0.0);
        }
        let t = self.base();
        self.pixmap.stroke_path(path, &paint_of(color), &stroke, t, self.clip.as_ref());
    }

    /// Outline of `ch` in the first font of `family` that has it, with the font's units per em.
    fn glyph(&mut self, family: &FontFamily, ch: char) -> Option<(tiny_skia::Path, f32)> {
        let names = self.defs.families.get(family).or_else(|| self.defs.families.get(&FontFamily::Proportional))?.clone();
        for n in names {
            let key = (n.clone(), ch);
            let data = self.defs.font_data.get(&n)?.clone();
            let face = ttf_parser::Face::parse(&data.font, data.index).ok()?;
            let units = face.units_per_em() as f32;
            if let Some(cached) = self.glyph_cache.get(&key) {
                match cached {
                    Some(p) => return Some((p.clone(), units)),
                    None => continue,
                }
            }
            let path = face.glyph_index(ch).filter(|g| g.0 != 0).and_then(|g| {
                let mut o = Outline(PathBuilder::new());
                face.outline_glyph(g, &mut o)?;
                o.0.finish()
            });
            self.glyph_cache.insert(key, path.clone());
            if let Some(p) = path {
                return Some((p, units));
            }
        }
        None
    }

    fn glyph_run(&mut self, run: &TextRun, dx: f32) {
        let (sin, cos) = run.angle.sin_cos();
        let paint = paint_of(run.color);
        for g in &run.glyphs {
            let Some((path, units)) = self.glyph(&run.family, g.ch) else { continue };
            let s = run.size / units;
            // font units (y up) -> canvas pixels (y down), slanted for italics, rotated, moved to the pen
            let slant = if run.italic { ITALIC_SLANT } else { 0.0 };
            let local = Transform::from_row(s, 0.0, s * slant, -s, 0.0, 0.0);
            let rot = Transform::from_row(cos, sin, -sin, cos, g.x + dx, g.y);
            let t = self.base().pre_concat(rot).pre_concat(local);
            self.pixmap.fill_path(&path, &paint, FillRule::Winding, t, self.clip.as_ref());
        }
    }
}

impl Canvas for RasterCanvas {
    fn clip(&mut self, rect: Option<[f32; 4]>) {
        self.clip = rect.and_then(|r| {
            let mut mask = Mask::new(self.pixmap.width(), self.pixmap.height())?;
            let rr = tiny_skia::Rect::from_ltrb(r[0], r[1], r[2].max(r[0] + 0.01), r[3].max(r[1] + 0.01))?;
            let path = PathBuilder::from_rect(rr);
            mask.fill_path(&path, FillRule::Winding, false, self.base());
            Some(mask)
        });
    }

    fn line(&mut self, a: P, b: P, width: f32, color: Rgba, dash: Option<(f32, f32)>) {
        if let Some(p) = Self::path_of(&[a, b], false) {
            self.stroke_path(&p, width, color, dash);
        }
    }

    fn polyline(&mut self, pts: &[P], width: f32, color: Rgba, closed: bool) {
        if pts.len() < 2 {
            return;
        }
        if let Some(p) = Self::path_of(pts, closed) {
            self.stroke_path(&p, width, color, None);
        }
    }

    fn fill_poly(&mut self, pts: &[P], color: Rgba) {
        if pts.len() < 3 {
            return;
        }
        if let Some(p) = Self::path_of(pts, true) {
            let t = self.base();
            self.pixmap.fill_path(&p, &paint_of(color), FillRule::Winding, t, self.clip.as_ref());
        }
    }

    fn text(&mut self, run: &TextRun) {
        self.glyph_run(run, 0.0);
        if run.fake_bold {
            self.glyph_run(run, run.size * 0.05);
        }
    }

    fn image(&mut self, r: [f32; 4], bm: &Bitmap) {
        let n = (bm.width * bm.height) as usize;
        if bm.width == 0 || bm.height == 0 || bm.rgba.len() < n * 4 {
            return;
        }
        let Some(mut src) = Pixmap::new(bm.width, bm.height) else { return };
        for (dst, px) in src.pixels_mut().iter_mut().zip(bm.rgba.chunks_exact(4)) {
            *dst = tiny_skia::ColorU8::from_rgba(px[0], px[1], px[2], px[3]).premultiply();
        }
        let sx = (r[2] - r[0]) / bm.width as f32;
        let sy = (r[3] - r[1]) / bm.height as f32;
        let t = self.base().pre_translate(r[0], r[1]).pre_scale(sx, sy);
        let paint = PixmapPaint { quality: tiny_skia::FilterQuality::Bilinear, ..Default::default() };
        self.pixmap.draw_pixmap(0, 0, src.as_ref(), &paint, t, self.clip.as_ref());
    }
}
