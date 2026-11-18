//! PDF backend of [`Canvas`]: vector lines and polygons, text in embedded (subsetted) TrueType
//! fonts, and bitmaps. Built on `pdf-writer`; fonts are the very ones the application uses
//! (Arial / Liberation Sans, DejaVu Sans for symbols), so the PDF looks like the canvas and
//! the text can be selected and searched.

use crate::canvas::{Bitmap, Canvas, GlyphPos, Rgba, TextRun, P};
use egui::{FontData, FontDefinitions, FontFamily};
use pdf_writer::types::{CidFontType, FontFlags, SystemInfo, UnicodeCmap};
use pdf_writer::{Content, Filter, Finish, Name, Pdf, Rect, Ref, Str};
use std::collections::{BTreeMap, HashMap};
use std::io::Write;
use std::sync::Arc;

/// Page pixels (96 dpi) to PDF points.
const PX_TO_PT: f32 = 0.75;
/// Slant of synthetic italics, like egui draws them.
const ITALIC_SLANT: f32 = 0.25;

/// A font of the application that is embedded in the document.
struct Face {
    name: String,
    data: Arc<FontData>,
    /// Glyphs used so far (original id -> character), and the remapping to the subset ids.
    remap: subsetter::GlyphRemapper,
    used: BTreeMap<u16, (u16, char)>,
    cmap_cache: HashMap<char, Option<u16>>,
    units: f32,
}

struct ImageOut {
    width: u32,
    height: u32,
    rgb: Vec<u8>,
    alpha: Option<Vec<u8>>,
}

pub struct PdfDoc {
    defs: FontDefinitions,
    faces: Vec<Face>,
    face_index: HashMap<String, usize>,
    pages: Vec<(Vec<u8>, f32, f32)>,
    images: Vec<ImageOut>,
    alphas: Vec<u8>,
    title: String,
}

impl PdfDoc {
    pub fn new(defs: FontDefinitions, title: &str) -> PdfDoc {
        PdfDoc {
            defs,
            faces: Vec::new(),
            face_index: HashMap::new(),
            pages: Vec::new(),
            images: Vec::new(),
            alphas: Vec::new(),
            title: title.to_string(),
        }
    }

    pub fn page_count(&self) -> usize {
        self.pages.len()
    }

    /// Starts a page of `w` x `h` page pixels.
    pub fn page(&mut self, w: f32, h: f32) -> PdfPage<'_> {
        let mut c = Content::new();
        // draw in page pixels, y down
        c.transform([PX_TO_PT, 0.0, 0.0, -PX_TO_PT, 0.0, h * PX_TO_PT]);
        PdfPage { doc: self, content: c, w, h, clip_open: false }
    }

    fn face_for(&mut self, family: &FontFamily, ch: char) -> Option<(usize, u16)> {
        let names = self.defs.families.get(family).or_else(|| self.defs.families.get(&FontFamily::Proportional))?.clone();
        for n in names {
            let idx = match self.face_index.get(&n) {
                Some(i) => *i,
                None => {
                    let data = self.defs.font_data.get(&n)?.clone();
                    let units = ttf_parser::Face::parse(&data.font, data.index).map(|f| f.units_per_em() as f32).unwrap_or(1000.0);
                    self.faces.push(Face { name: n.clone(), data, remap: subsetter::GlyphRemapper::new(), used: BTreeMap::new(), cmap_cache: HashMap::new(), units });
                    self.face_index.insert(n.clone(), self.faces.len() - 1);
                    self.faces.len() - 1
                }
            };
            let face = &mut self.faces[idx];
            let gid = match face.cmap_cache.get(&ch) {
                Some(g) => *g,
                None => {
                    let g = ttf_parser::Face::parse(&face.data.font, face.data.index)
                        .ok()
                        .and_then(|f| f.glyph_index(ch))
                        .map(|g| g.0)
                        .filter(|g| *g != 0);
                    face.cmap_cache.insert(ch, g);
                    g
                }
            };
            if let Some(g) = gid {
                return Some((idx, g));
            }
        }
        None
    }

    /// Serialises the document.
    pub fn finish(self) -> Vec<u8> {
        let mut pdf = Pdf::new();
        let mut next = 1;
        let mut alloc = || {
            let r = Ref::new(next);
            next += 1;
            r
        };
        let catalog = alloc();
        let pages_id = alloc();
        let info = alloc();
        let page_refs: Vec<(Ref, Ref)> = self.pages.iter().map(|_| (alloc(), alloc())).collect();
        pdf.catalog(catalog).pages(pages_id);
        pdf.document_info(info).title(pdf_writer::TextStr(&self.title)).creator(pdf_writer::TextStr("xmath")).producer(pdf_writer::TextStr("xmath"));

        // fonts
        let mut font_refs = Vec::new();
        for face in &self.faces {
            let type0 = alloc();
            let cid = alloc();
            let desc = alloc();
            let file = alloc();
            let tounicode = alloc();
            font_refs.push(type0);
            let data = &face.data.font;
            let sub = subsetter::subset(data, face.data.index, &face.remap).unwrap_or_else(|_| data.to_vec());
            let parsed = ttf_parser::Face::parse(data, face.data.index).ok();
            let base: String = face.name.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
            let base = format!("XMATH+{}", if base.is_empty() { "Font".to_string() } else { base });
            let scale = 1000.0 / face.units;
            let (bbox, ascent, descent, cap, italic) = match &parsed {
                Some(f) => {
                    let b = f.global_bounding_box();
                    (
                        Rect::new(b.x_min as f32 * scale, b.y_min as f32 * scale, b.x_max as f32 * scale, b.y_max as f32 * scale),
                        f.ascender() as f32 * scale,
                        f.descender() as f32 * scale,
                        f.capital_height().unwrap_or(f.ascender()) as f32 * scale,
                        f.italic_angle(),
                    )
                }
                None => (Rect::new(0.0, -200.0, 1000.0, 900.0), 900.0, -200.0, 700.0, 0.0),
            };
            let mut flate = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
            let _ = flate.write_all(&sub);
            let sub_z = flate.finish().unwrap_or_default();
            pdf.stream(file, &sub_z).filter(Filter::FlateDecode).pair(Name(b"Length1"), sub.len() as i32);
            pdf.font_descriptor(desc)
                .name(Name(base.as_bytes()))
                .flags(FontFlags::NON_SYMBOLIC)
                .bbox(bbox)
                .italic_angle(italic)
                .ascent(ascent)
                .descent(descent)
                .cap_height(cap)
                .stem_v(80.0)
                .font_file2(file);
            {
                let mut c = pdf.cid_font(cid);
                c.subtype(CidFontType::Type2)
                    .base_font(Name(base.as_bytes()))
                    .system_info(SystemInfo { registry: Str(b"Adobe"), ordering: Str(b"Identity"), supplement: 0 })
                    .font_descriptor(desc)
                    .default_width(0.0)
                    .cid_to_gid_map_predefined(Name(b"Identity"));
                let mut w = c.widths();
                for (orig, (new, _)) in &face.used {
                    let adv = parsed.as_ref().and_then(|f| f.glyph_hor_advance(ttf_parser::GlyphId(*orig))).unwrap_or(0) as f32 * scale;
                    w.consecutive(*new, [adv]);
                }
                w.finish();
                c.finish();
            }
            let mut cmap = UnicodeCmap::new(Name(b"Custom"), SystemInfo { registry: Str(b"Adobe"), ordering: Str(b"UCS"), supplement: 0 });
            for (_, (new, ch)) in &face.used {
                cmap.pair(*new, *ch);
            }
            let cmap_bytes = cmap.finish();
            pdf.cmap(tounicode, &cmap_bytes);
            pdf.type0_font(type0)
                .base_font(Name(base.as_bytes()))
                .encoding_predefined(Name(b"Identity-H"))
                .descendant_font(cid)
                .to_unicode(tounicode);
        }

        // images
        let mut image_refs = Vec::new();
        for im in &self.images {
            let id = alloc();
            let smask = im.alpha.as_ref().map(|_| alloc());
            image_refs.push(id);
            let z = deflate(&im.rgb);
            {
                let mut x = pdf.image_xobject(id, &z);
                x.width(im.width as i32).height(im.height as i32).bits_per_component(8);
                x.filter(Filter::FlateDecode);
                x.color_space().device_rgb();
                if let Some(s) = smask {
                    x.s_mask(s);
                }
            }
            if let (Some(s), Some(a)) = (smask, &im.alpha) {
                let z = deflate(a);
                let mut x = pdf.image_xobject(s, &z);
                x.width(im.width as i32).height(im.height as i32).bits_per_component(8);
                x.filter(Filter::FlateDecode);
                x.color_space().device_gray();
            }
        }

        // transparency states
        let mut gs_refs = Vec::new();
        for a in &self.alphas {
            let id = alloc();
            gs_refs.push(id);
            let v = *a as f32 / 255.0;
            pdf.ext_graphics(id).non_stroking_alpha(v).stroking_alpha(v);
        }

        // pages
        for (i, (content, w, h)) in self.pages.iter().enumerate() {
            let (page_id, content_id) = page_refs[i];
            let z = deflate(content);
            pdf.stream(content_id, &z).filter(Filter::FlateDecode);
            let mut page = pdf.page(page_id);
            page.parent(pages_id).media_box(Rect::new(0.0, 0.0, w * PX_TO_PT, h * PX_TO_PT)).contents(content_id);
            let mut res = page.resources();
            {
                let mut f = res.fonts();
                for (k, r) in font_refs.iter().enumerate() {
                    f.pair(Name(format!("F{k}").as_bytes()), *r);
                }
            }
            {
                let mut x = res.x_objects();
                for (k, r) in image_refs.iter().enumerate() {
                    x.pair(Name(format!("I{k}").as_bytes()), *r);
                }
            }
            {
                let mut g = res.ext_g_states();
                for (k, r) in gs_refs.iter().enumerate() {
                    g.pair(Name(format!("G{k}").as_bytes()), *r);
                }
            }
            res.finish();
            page.finish();
        }
        pdf.pages(pages_id).kids(page_refs.iter().map(|p| p.0)).count(page_refs.len() as i32);
        pdf.finish()
    }
}

fn deflate(data: &[u8]) -> Vec<u8> {
    let mut e = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    let _ = e.write_all(data);
    e.finish().unwrap_or_default()
}

pub struct PdfPage<'a> {
    doc: &'a mut PdfDoc,
    content: Content,
    w: f32,
    h: f32,
    clip_open: bool,
}

impl PdfPage<'_> {
    /// Ends the page and adds it to the document.
    pub fn finish(mut self) {
        if self.clip_open {
            self.content.restore_state();
        }
        let bytes = self.content.finish().to_vec();
        self.doc.pages.push((bytes, self.w, self.h));
    }

    fn set_alpha(&mut self, a: u8) -> bool {
        if a == 255 {
            return false;
        }
        let k = match self.doc.alphas.iter().position(|x| *x == a) {
            Some(k) => k,
            None => {
                self.doc.alphas.push(a);
                self.doc.alphas.len() - 1
            }
        };
        self.content.set_parameters(Name(format!("G{k}").as_bytes()));
        true
    }

    fn colors(&mut self, c: Rgba, stroke: bool) {
        let (r, g, b) = (c.0 as f32 / 255.0, c.1 as f32 / 255.0, c.2 as f32 / 255.0);
        if stroke {
            self.content.set_stroke_rgb(r, g, b);
        } else {
            self.content.set_fill_rgb(r, g, b);
        }
    }

    fn path(&mut self, pts: &[P], closed: bool) {
        let mut it = pts.iter();
        if let Some(p) = it.next() {
            self.content.move_to(p.0, p.1);
        }
        for p in it {
            self.content.line_to(p.0, p.1);
        }
        if closed {
            self.content.close_path();
        }
    }

    fn glyph_run(&mut self, run: &TextRun, dx: f32) {
        // resolve glyphs to (face, gid), grouped by face
        let mut groups: Vec<(usize, Vec<(u16, GlyphPos)>)> = Vec::new();
        for g in &run.glyphs {
            let Some((fi, gid)) = self.doc.face_for(&run.family, g.ch) else { continue };
            let face = &mut self.doc.faces[fi];
            let new = match face.used.get(&gid) {
                Some((n, _)) => *n,
                None => {
                    let n = face.remap.remap(gid);
                    face.used.insert(gid, (n, g.ch));
                    n
                }
            };
            match groups.last_mut() {
                Some((f, v)) if *f == fi => v.push((gid, GlyphPos { x: g.x + dx, ..*g })),
                _ => groups.push((fi, vec![(gid, GlyphPos { x: g.x + dx, ..*g })])),
            }
            let _ = new;
        }
        let alpha = self.set_alpha(run.color.3);
        self.colors(run.color, false);
        for (fi, glyphs) in groups {
            let units = self.doc.faces[fi].units;
            let data = self.doc.faces[fi].data.clone();
            let parsed = ttf_parser::Face::parse(&data.font, data.index).ok();
            let s = run.size;
            let first = glyphs[0].1;
            let slant = if run.italic { ITALIC_SLANT } else { 0.0 };
            self.content.begin_text();
            self.content.set_font(Name(format!("F{fi}").as_bytes()), s);
            // y is flipped because the page is drawn y-down; the slant imitates an italic face
            let (sn, cs) = run.angle.sin_cos();
            self.content.set_text_matrix([cs, sn, sn + slant * cs, -cs + slant * sn, first.x, first.y]);
            let mut items = self.content.show_positioned();
            let mut ops = items.items();
            for (k, (gid, g)) in glyphs.iter().enumerate() {
                let new = self.doc.faces[fi].used[gid].0;
                ops.show(Str(&new.to_be_bytes()));
                if let Some((_, next)) = glyphs.get(k + 1) {
                    let adv = parsed.as_ref().and_then(|f| f.glyph_hor_advance(ttf_parser::GlyphId(*gid))).unwrap_or(0) as f32 / units;
                    // the pen should be at `next.x`; it is at g.x + adv * s
                    let err = (g.x + adv * s * run.angle.cos() - next.x) * run.angle.cos() + (g.y + adv * s * run.angle.sin() - next.y) * run.angle.sin();
                    if err.abs() > 0.002 * s {
                        ops.adjust(err / s * 1000.0);
                    }
                }
            }
            ops.finish();
            items.finish();
            self.content.end_text();
        }
        let _ = alpha;
    }
}

impl Canvas for PdfPage<'_> {
    fn clip(&mut self, rect: Option<[f32; 4]>) {
        if self.clip_open {
            self.content.restore_state();
            self.clip_open = false;
        }
        if let Some(r) = rect {
            self.content.save_state();
            self.content.rect(r[0], r[1], (r[2] - r[0]).max(0.0), (r[3] - r[1]).max(0.0));
            self.content.clip_nonzero();
            self.content.end_path();
            self.clip_open = true;
        }
    }

    fn line(&mut self, a: P, b: P, width: f32, color: Rgba, dash: Option<(f32, f32)>) {
        self.content.save_state();
        let alpha = self.set_alpha(color.3);
        self.colors(color, true);
        self.content.set_line_width(width.max(0.05));
        if let Some((d, g)) = dash {
            self.content.set_dash_pattern([d, g], 0.0);
        }
        self.content.move_to(a.0, a.1).line_to(b.0, b.1).stroke();
        let _ = alpha;
        self.content.restore_state();
    }

    fn polyline(&mut self, pts: &[P], width: f32, color: Rgba, closed: bool) {
        if pts.len() < 2 {
            return;
        }
        self.content.save_state();
        self.set_alpha(color.3);
        self.colors(color, true);
        self.content.set_line_width(width.max(0.05));
        self.content.set_line_join(pdf_writer::types::LineJoinStyle::RoundJoin);
        self.path(pts, closed);
        self.content.stroke();
        self.content.restore_state();
    }

    fn fill_poly(&mut self, pts: &[P], color: Rgba) {
        if pts.len() < 3 {
            return;
        }
        self.content.save_state();
        self.set_alpha(color.3);
        self.colors(color, false);
        self.path(pts, true);
        self.content.fill_nonzero();
        self.content.restore_state();
    }

    fn text(&mut self, run: &TextRun) {
        if run.glyphs.is_empty() {
            return;
        }
        self.content.save_state();
        self.glyph_run(run, 0.0);
        if run.fake_bold {
            self.glyph_run(run, run.size * 0.05);
        }
        self.content.restore_state();
    }

    fn image(&mut self, r: [f32; 4], bm: &Bitmap) {
        if bm.width == 0 || bm.height == 0 || bm.rgba.len() < (bm.width * bm.height * 4) as usize {
            return;
        }
        let n = (bm.width * bm.height) as usize;
        let mut rgb = Vec::with_capacity(n * 3);
        let mut alpha = Vec::with_capacity(n);
        for px in bm.rgba.chunks_exact(4).take(n) {
            rgb.extend_from_slice(&px[..3]);
            alpha.push(px[3]);
        }
        let alpha = alpha.iter().any(|a| *a != 255).then_some(alpha);
        self.doc.images.push(ImageOut { width: bm.width, height: bm.height, rgb, alpha });
        let k = self.doc.images.len() - 1;
        self.content.save_state();
        // the unit square, flipped back (the page is drawn y-down)
        self.content.transform([r[2] - r[0], 0.0, 0.0, -(r[3] - r[1]), r[0], r[3]]);
        self.content.x_object(Name(format!("I{k}").as_bytes()));
        self.content.restore_state();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_document_is_pdf() {
        let (defs, _) = crate::fonts::definitions();
        let mut doc = PdfDoc::new(defs, "t");
        {
            let mut p = doc.page(794.0, 1123.0);
            p.line((10.0, 10.0), (100.0, 100.0), 1.0, Rgba::BLACK, None);
            p.fill_poly(&[(0.0, 0.0), (50.0, 0.0), (50.0, 50.0)], Rgba(255, 0, 0, 128));
            p.finish();
        }
        let b = doc.finish();
        assert!(b.starts_with(b"%PDF"));
    }
}
