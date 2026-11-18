//! Laying out a worksheet on pages for the print preview and the PDF export.
//!
//! [`Sheet::layout`] turns every region into something that can be painted (a typeset formula,
//! a text galley, a plot, a picture) in *device pixels* (worksheet pixels times `zoom`).
//! [`Sheet::paginate`] cuts the sheet into pages like Mathcad does: at the page-break lines
//! (multiples of the printable page height); a region that would be cut by a break moves to
//! the next page. [`Sheet::paint_page`] paints one page with an [`egui::Painter`]: the
//! preview window uses the painter of the application, the PDF export a headless context
//! whose shapes are translated by [`crate::canvas::paint_shapes`].

use crate::canvas::{self, Bitmap};
use crate::mathview::{Lay, Ts, RED};
use crate::pdf::PdfDoc;
use crate::plot;
use base64::Engine;
use egui::text::{LayoutJob, TextFormat};
use egui::{Align2, Color32, Context, FontFamily, FontId, Painter, Pos2, Rect, Stroke, TextureHandle, TextureOptions, Vec2};
use std::collections::HashMap;
use std::sync::Arc;
use xmath_core::worksheet::MM_TO_PX;
use xmath_core::{Output, PageSetup, Plot3dData, Plot3dFormat, Plot3dKind, PlotData, PlotFormat, PlotKind, RegionKind, RegionOutput, View3d, Worksheet};

pub const BASE_PT: f32 = 10.0;

/// Points to worksheet pixels at `zoom`.
pub fn px(pt: f32, zoom: f32) -> f32 {
    pt * 4.0 / 3.0 * zoom
}

pub enum Item {
    Math { lay: Lay },
    Text { galley: Arc<egui::Galley>, fake_bold: bool },
    Image { bitmap: Arc<Bitmap>, texture: Option<TextureHandle> },
    Plot(Box<PlotItem>),
    Plot3d(Box<Plot3dItem>),
    /// A small grey box (reference to another worksheet).
    Note { galley: Arc<egui::Galley> },
}

pub struct Plot3dItem {
    pub size: Vec2,
    pub data: Plot3dData,
    pub view: View3d,
    pub fmt: Plot3dFormat,
    pub expr: String,
    pub exp_threshold: i32,
    pub fs: f32,
}

pub struct PlotItem {
    pub polar: bool,
    pub plot_rect: Rect,
    pub data: PlotData,
    pub limits: [Option<f64>; 4],
    pub fmt: PlotFormat,
    pub y_labels: Vec<String>,
    pub x_label: String,
    pub x_grey: bool,
    pub legend_w: f32,
    pub fs: f32,
}

/// A laid-out region; `rect` is its place on the sheet.
pub struct Placed {
    pub rect: Rect,
    pub item: Item,
}

/// One page of the sheet: its rows (device pixels) and the left edge of its column.
#[derive(Clone, Copy, Debug)]
pub struct PageCell {
    pub y0: f32,
    pub y1: f32,
    pub x0: f32,
    pub x1: f32,
}

pub struct Sheet {
    pub zoom: f32,
    pub setup: PageSetup,
    pub has_bold: bool,
    pub items: Vec<Placed>,
    /// Start of every page on the sheet (device pixels); one more entry marks the end.
    pub pages: Vec<PageCell>,
    /// Number of page columns.
    pub cols: usize,
}

/// The font family of a text region.
pub fn text_family(style: &xmath_core::worksheet::TextStyle, has_bold: bool) -> FontFamily {
    use xmath_core::worksheet::TextFont;
    let (plain, bold) = match style.font {
        TextFont::Sans => return if style.bold && has_bold { FontFamily::Name("bold".into()) } else { FontFamily::Proportional },
        TextFont::Serif => ("serif", "serif-bold"),
        TextFont::Mono => ("mono", "mono-bold"),
    };
    FontFamily::Name((if style.bold { bold } else { plain }).into())
}

/// Wrapped text of a text region.
pub fn text_galley(ctx: &Context, text: &str, style: &xmath_core::worksheet::TextStyle, wrap: f32, zoom: f32, has_bold: bool) -> Arc<egui::Galley> {
    let size = px(style.size, zoom);
    let family = text_family(style, has_bold);
    let mut job = LayoutJob::default();
    job.wrap.max_width = wrap;
    job.append(
        if text.is_empty() { " " } else { text },
        0.0,
        TextFormat {
            font_id: FontId::new(size, family),
            color: Color32::BLACK,
            italics: style.italic,
            underline: if style.underline { Stroke::new(1.0_f32, Color32::BLACK) } else { Stroke::NONE },
            line_height: Some(size * TEXT_LINE),
            ..Default::default()
        },
    );
    ctx.fonts(|f| f.layout_job(job))
}

/// Wrap width of a text region of a fixed width. Mathcad measures glyphs at whole device pixels
/// (a 10 pt font is drawn at 13 px, not 13.33) so its text is a few percent narrower than ours;
/// a little slack keeps our lines from breaking where Mathcad's do not (which would make the
/// region taller and run into the one below).
pub fn locked_wrap(width: f32, zoom: f32) -> f32 {
    ((width * 1.08 + 8.0) * zoom).max(20.0)
}

/// Mathcad's line pitch of text: 1.2 times the font size.
pub const TEXT_LINE: f32 = 1.2;

/// The grey box shown for a reference region: its galley and size.
pub fn reference_layout(ctx: &Context, href: &str, zoom: f32) -> (Arc<egui::Galley>, Vec2) {
    let fs = px(BASE_PT, zoom);
    let galley = ctx.fonts(|f| f.layout_no_wrap(format!("Ссылка: {href}"), FontId::proportional(fs * 0.95), Color32::from_gray(70)));
    let size = galley.size() + Vec2::new(fs, fs * 0.5);
    (galley, size)
}

/// Paints the reference box with its top-left corner at `pos`.
pub fn paint_reference(painter: &Painter, pos: Pos2, galley: &Arc<egui::Galley>, size: Vec2) {
    let r = Rect::from_min_size(pos, size);
    painter.rect_filled(r, 0.0, Color32::from_gray(238));
    painter.rect_stroke(r, 0.0, Stroke::new(1.0_f32, Color32::from_gray(150)), egui::StrokeKind::Inside);
    painter.galley(pos + Vec2::new(size.x - galley.size().x, size.y - galley.size().y) * 0.5, galley.clone(), Color32::from_gray(70));
}

/// Draws the region of a plot like the canvas does.
fn paint_plot(painter: &Painter, pos: Pos2, p: &PlotItem) {
    let view = plot::View { x_min: p.limits[0], x_max: p.limits[1], y_min: p.limits[2], y_max: p.limits[3], fmt: &p.fmt, aspect: p.plot_rect.width() / p.plot_rect.height() };
    let plot_rect = p.plot_rect.translate(pos.to_vec2());
    let fs = p.fs;
    let m = if p.polar { plot::Margins::default() } else { plot::margins(&p.fmt, &p.data, fs) };
    if p.polar {
        plot::paint_polar(painter, plot_rect, &p.data, &view, fs);
    } else {
        plot::paint(painter, plot_rect, &p.data, &view, fs);
    }
    let label_row = plot_rect.bottom() + m.bottom + fs * 0.4;
    if !p.fmt.hide_args {
        let n = p.y_labels.len().max(1) as f32;
        let mut ly = plot_rect.center().y - n * fs * 1.2;
        for (i, label) in p.y_labels.iter().enumerate() {
            let l = if label.is_empty() { "■" } else { label.as_str() };
            let r = painter.text(Pos2::new(pos.x + p.legend_w - 8.0, ly), Align2::RIGHT_TOP, l, FontId::proportional(fs), Color32::BLACK);
            let (lc, ls, lw) = plot::trace_style(i, &p.fmt);
            plot::sample(painter, Pos2::new(r.left(), r.bottom() + 3.0), Pos2::new(r.right().max(r.left() + 24.0), r.bottom() + 3.0), lc, ls, lw);
            ly += fs * 2.4;
        }
        let color = if p.x_grey { Color32::GRAY } else { Color32::BLACK };
        painter.text(Pos2::new(plot_rect.center().x, label_row), Align2::CENTER_TOP, &p.x_label, FontId::proportional(fs), color);
    }
    let mut y = label_row + fs * 1.6;
    for e in &p.data.errors {
        painter.text(Pos2::new(plot_rect.left(), y), Align2::LEFT_TOP, e, FontId::proportional(fs * 0.85), RED);
        y += fs * 1.2;
    }
}

/// Draws a surface / contour / 3D bar / 3D scatter region.
fn paint_plot3d(painter: &Painter, pos: Pos2, p: &Plot3dItem) {
    let rect = Rect::from_min_size(pos, p.size);
    painter.rect_filled(rect, 0.0, Color32::WHITE);
    let size = [p.size.x, p.size.y];
    let scene = if p.data.kind == Plot3dKind::Contour {
        xmath_core::plot3d::contour_scene(&p.data, &p.fmt, size, p.fs, p.exp_threshold)
    } else {
        xmath_core::plot3d::scene3d(&p.data, &p.view, &p.fmt, size, p.fs, p.exp_threshold)
    };
    plot::paint_scene(&painter.with_clip_rect(rect), rect.min, &scene);
    painter.rect_stroke(rect, 0.0, Stroke::new(1.0_f32, Color32::from_gray(90)), egui::StrokeKind::Middle);
    let txt = if p.expr.is_empty() { "■" } else { p.expr.as_str() };
    let r = painter.text(Pos2::new(rect.left() + 4.0, rect.bottom() + p.fs * 0.4), Align2::LEFT_TOP, txt, FontId::proportional(p.fs), Color32::BLACK);
    if let Some(e) = &p.data.error {
        painter.text(Pos2::new(rect.left(), r.bottom() + p.fs * 0.3), Align2::LEFT_TOP, e, FontId::proportional(p.fs * 0.85), RED);
    }
}

impl Sheet {
    /// Lays out all regions of `ws` (with the results in `outputs`) at `zoom`. `ctx` must have
    /// its fonts available (inside `Context::run`, or in the application's update).
    pub fn layout(ctx: &Context, ws: &Worksheet, outputs: &HashMap<u64, RegionOutput>, zoom: f32, has_bold: bool) -> Sheet {
        let setup = ws.page.clone();
        let (page_w, _) = setup.printable_px();
        let fs = px(BASE_PT, zoom);
        let mut items = Vec::new();
        for region in &ws.regions {
            let pos = Pos2::new(region.x * zoom, region.y * zoom);
            let out = outputs.get(&region.id);
            match &region.kind {
                RegionKind::Math { source } => {
                    let (lay, _) = math_layout(ctx, out, source, fs);
                    let rect = Rect::from_min_size(pos, Vec2::new(lay.w + 4.0, lay.h() + 6.0));
                    // the formula is drawn with its baseline at `asc + 3`
                    let mut lay = lay;
                    lay.boxes.clear();
                    items.push(Placed { rect, item: Item::Math { lay } });
                }
                RegionKind::Text { text, style, width, lock_width } => {
                    let wrap = if *lock_width { locked_wrap(*width, zoom) } else { (page_w * zoom - pos.x).max(120.0 * zoom) };
                    let galley = text_galley(ctx, text, style, wrap, zoom, has_bold);
                    let rect = Rect::from_min_size(pos, galley.size() + Vec2::splat(4.0));
                    items.push(Placed { rect, item: Item::Text { galley, fake_bold: style.bold && !has_bold && style.font == xmath_core::worksheet::TextFont::Sans } });
                }
                RegionKind::Image { png_base64, width, height } => {
                    let rect = Rect::from_min_size(pos, Vec2::new(width * zoom, height * zoom));
                    if let Some(bitmap) = decode_bitmap(png_base64) {
                        let image = egui::ColorImage::from_rgba_unmultiplied([bitmap.width as usize, bitmap.height as usize], &bitmap.rgba);
                        let texture = Some(ctx.load_texture(format!("print-{}", region.id), image, TextureOptions::LINEAR));
                        items.push(Placed { rect, item: Item::Image { bitmap: Arc::new(bitmap), texture } });
                    } else {
                        items.push(Placed { rect, item: Item::Image { bitmap: Arc::new(Bitmap { width: 0, height: 0, rgba: Vec::new() }), texture: None } });
                    }
                }
                RegionKind::Plot { x, y, width, height, x_min, x_max, y_min, y_max, fmt, kind, .. } => {
                    let polar = *kind == PlotKind::Polar;
                    let data = match out.map(|o| &o.output) {
                        Some(Output::Plot(d)) => d.clone(),
                        _ => Default::default(),
                    };
                    let y_text_w = if fmt.hide_args {
                        0.0
                    } else {
                        y.iter()
                            .map(|l| ctx.fonts(|f| f.layout_no_wrap(if l.is_empty() { "■".to_string() } else { l.clone() }, FontId::proportional(fs), Color32::BLACK).size().x))
                            .fold(0.0, f32::max)
                    };
                    let geom = plot::plot_geom(fmt, &data, polar, width * zoom, height * zoom, fs, zoom, y_text_w);
                    let (x_label, x_grey) = match (x.is_empty(), &data.x_var) {
                        (false, _) => (x.clone(), false),
                        (true, Some(v)) => (v.clone(), true),
                        (true, None) if polar => ("θ".to_string(), true),
                        (true, None) => ("■".to_string(), false),
                    };
                    let bottom = geom.bottom + data.errors.len() as f32 * fs * 1.2;
                    let rect = Rect::from_min_max(pos, pos + Vec2::new(geom.right, bottom));
                    items.push(Placed {
                        rect,
                        item: Item::Plot(Box::new(PlotItem {
                            polar,
                            plot_rect: geom.plot_rect,
                            data,
                            limits: [*x_min, *x_max, *y_min, *y_max],
                            fmt: fmt.clone(),
                            y_labels: y.clone(),
                            x_label,
                            x_grey,
                            legend_w: geom.legend_w,
                            fs,
                        })),
                    });
                }
                RegionKind::Plot3d { kind, expr, width, height, view, fmt } => {
                    let data = match out.map(|o| &o.output) {
                        Some(Output::Plot3d(d)) => d.clone(),
                        _ => Plot3dData { kind: *kind, ..Default::default() },
                    };
                    let size = Vec2::new((width * zoom).max(80.0), (height * zoom - fs * 2.0).max(60.0));
                    let mut bottom = size.y + fs * 2.0;
                    if data.error.is_some() {
                        bottom += fs * 1.2;
                    }
                    items.push(Placed {
                        rect: Rect::from_min_size(pos, Vec2::new(size.x, bottom)),
                        item: Item::Plot3d(Box::new(Plot3dItem { size, data, view: *view, fmt: fmt.clone(), expr: expr.clone(), exp_threshold: ws.format.exp_threshold, fs })),
                    });
                }
                RegionKind::Reference { href } => {
                    let (galley, size) = reference_layout(ctx, href, zoom);
                    items.push(Placed { rect: Rect::from_min_size(pos, size), item: Item::Note { galley } });
                }
            }
        }
        let mut sheet = Sheet { zoom, setup, has_bold, items, pages: Vec::new(), cols: 1 };
        sheet.paginate();
        sheet
    }

    /// Height of the printable area of a page in device pixels.
    pub fn page_h(&self) -> f32 {
        self.setup.printable_px().1 * self.zoom
    }

    pub fn page_w(&self) -> f32 {
        self.setup.printable_px().0 * self.zoom
    }

    /// Decides where the pages begin. Rows: a page is at most one printable height; a region
    /// cut by a break moves to the next page only if the page keeps at least a third of its
    /// height (otherwise it is cut and continues on the next page). Columns: content right of
    /// the printable width goes to extra pages to the right, as in Mathcad (unless the setup
    /// says to print within the margins). Pages without any content are dropped; pages are
    /// numbered row by row.
    fn paginate(&mut self) {
        let ph = self.page_h();
        let pw = self.page_w();
        let bottom = self.items.iter().map(|i| i.rect.bottom()).fold(0.0f32, f32::max);
        let right = self.items.iter().map(|i| i.rect.right()).fold(0.0f32, f32::max);
        let mut rows: Vec<(f32, f32)> = Vec::new();
        let mut start = 0.0f32;
        let mut guard = 0;
        loop {
            guard += 1;
            let full = start + ph;
            let mut limit = full;
            if full < bottom - 0.5 && guard < 10_000 {
                // the break goes to the full height or to the top of a region (at least a third
                // of a page down), whichever cuts the fewest regions; ties: the lower break
                let cuts = |l: f32| self.items.iter().filter(|i| i.rect.top() < l - 0.5 && i.rect.bottom() > l + 0.5).count();
                let mut best = cuts(full);
                let mut cands: Vec<f32> = self.items.iter().map(|i| i.rect.top()).filter(|t| *t >= start + ph / 3.0 && *t < full - 0.5).collect();
                cands.sort_by(|a, b| b.total_cmp(a));
                cands.dedup();
                for c in cands {
                    let n = cuts(c);
                    if n < best {
                        best = n;
                        limit = c;
                    }
                }
            }
            rows.push((start, limit));
            if full >= bottom - 0.5 || guard >= 10_000 {
                break;
            }
            start = limit;
        }
        // column starts: fixed steps of the printable width (a region on a boundary continues
        // on the next page, as in Mathcad)
        let mut col_starts = vec![0.0f32];
        if !self.setup.within_margins {
            let mut cx = 0.0f32;
            let mut g = 0;
            // content may reach into the right margin without needing another page
            let tol = self.setup.margins[2] * MM_TO_PX * self.zoom + 12.0;
            while cx + pw + tol < right - 0.5 && g < 10_000 {
                g += 1;
                cx += pw;
                col_starts.push(cx);
            }
        }
        let ncols = col_starts.len();
        let mut pages = Vec::new();
        for &(y0, y1) in &rows {
            for (c, &x0) in col_starts.iter().enumerate() {
                let x1 = col_starts.get(c + 1).copied().unwrap_or(f32::INFINITY).min(x0 + pw);
                let any = self.items.iter().any(|i| i.rect.bottom() > y0 && i.rect.top() < y1 && (ncols == 1 || (i.rect.right() > x0 + 12.0 && i.rect.left() < x1 - 12.0)));
                if any {
                    pages.push(PageCell { y0, y1, x0, x1 });
                }
            }
        }
        if pages.is_empty() {
            pages.push(PageCell { y0: 0.0, y1: ph, x0: 0.0, x1: pw });
        }
        self.cols = ncols;
        self.pages = pages;
    }

    pub fn page_count(&self) -> usize {
        self.pages.len().max(1)
    }

    /// Vertical extent of page `k` on the sheet (device pixels).
    pub fn page_range(&self, k: usize) -> (f32, f32) {
        let c = &self.pages[k.min(self.pages.len() - 1)];
        (c.y0, c.y1)
    }

    fn page_x(&self, k: usize) -> f32 {
        self.pages[k.min(self.pages.len() - 1)].x0
    }

    /// The area of page `k` that may be drawn on, in page coordinates when the printable area
    /// starts at `origin`; `paper_w` is the paper width in the same unit. Content reaching into
    /// the side margins is kept only when the sheet is a single page column.
    pub fn page_clip(&self, k: usize, origin: Pos2, page_left: f32, paper_w: f32) -> Rect {
        let (y0, y1) = self.page_range(k);
        let h = (y1 - y0).min(self.page_h());
        if self.cols == 1 && !self.setup.within_margins {
            Rect::from_min_size(Pos2::new(page_left, origin.y), Vec2::new(paper_w, h))
        } else {
            Rect::from_min_size(origin, Vec2::new(self.page_w(), h))
        }
    }

    /// Items that appear on page `k` (those of a taller region continue on the next page).
    fn on_page(&self, k: usize) -> impl Iterator<Item = &Placed> {
        let (y0, end) = self.page_range(k);
        let (x0, x1) = (self.page_x(k), self.pages[k.min(self.pages.len() - 1)].x1);
        let cols = self.cols;
        self.items.iter().filter(move |i| i.rect.bottom() > y0 && i.rect.top() < end && (cols == 1 || (i.rect.right() > x0 && i.rect.left() < x1)))
    }

    /// Paints page `k`; `origin` is where the top-left corner of the printable area goes and
    /// the painter's clip rectangle should already limit drawing to that area.
    pub fn paint_page(&self, painter: &Painter, k: usize, origin: Pos2) {
        let (y0, _) = self.page_range(k);
        let o = origin - Vec2::new(self.page_x(k), y0);
        for p in self.on_page(k) {
            let pos = p.rect.min + o.to_vec2();
            match &p.item {
                Item::Math { lay } => {
                    lay.paint(painter, pos + Vec2::new(2.0, 3.0 + lay.asc));
                }
                Item::Text { galley, fake_bold } => {
                    let at = pos + Vec2::splat(2.0);
                    painter.galley(at, galley.clone(), Color32::BLACK);
                    if *fake_bold {
                        painter.galley(at + Vec2::X * 0.7 * self.zoom, galley.clone(), Color32::BLACK);
                    }
                }
                Item::Image { bitmap, texture } => match texture {
                    Some(t) if bitmap.width > 0 => {
                        painter.image(t.id(), Rect::from_min_size(pos, p.rect.size()), Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
                    }
                    _ => {
                        let r = Rect::from_min_size(pos, p.rect.size());
                        painter.rect_stroke(r, 0.0, Stroke::new(1.0_f32, Color32::GRAY), egui::StrokeKind::Inside);
                    }
                },
                Item::Plot(pl) => paint_plot(painter, pos, pl),
                Item::Plot3d(pl) => paint_plot3d(painter, pos, pl),
                Item::Note { galley } => paint_reference(painter, pos, galley, p.rect.size()),
            }
        }
    }

    /// Pictures on page `k` with their place on the paper (for [`canvas::Canvas::image`]),
    /// `origin` as in [`Sheet::paint_page`].
    pub fn page_images(&self, k: usize, origin: Pos2) -> Vec<([f32; 4], Arc<Bitmap>)> {
        let (y0, _) = self.page_range(k);
        let o = origin - Vec2::new(self.page_x(k), y0);
        self.on_page(k)
            .filter_map(|p| match &p.item {
                Item::Image { bitmap, .. } if bitmap.width > 0 => {
                    let r = p.rect.translate(o.to_vec2());
                    Some(([r.min.x, r.min.y, r.max.x, r.max.y], bitmap.clone()))
                }
                _ => None,
            })
            .collect()
    }
}

fn decode_bitmap(b64: &str) -> Option<Bitmap> {
    let bytes = base64::engine::general_purpose::STANDARD.decode(b64.as_bytes()).ok()?;
    let img = ::image::load_from_memory(&bytes).ok()?.to_rgba8();
    Some(Bitmap { width: img.width(), height: img.height(), rgba: img.into_raw() })
}

fn math_layout(ctx: &Context, out: Option<&RegionOutput>, source: &str, fs: f32) -> (Lay, Option<String>) {
    ctx.fonts(|f| {
        let mut ts = Ts::new(f);
        let err = out.and_then(|o| o.error());
        match out.and_then(|o| o.stmt.clone()) {
            Some(Ok(stmt)) => {
                if err.is_some() {
                    ts.color = RED;
                }
                let result = match out.map(|o| &o.output) {
                    Some(Output::Value { view, unit }) => Some((view, unit.as_ref())),
                    _ => None,
                };
                if let Some(Output::Symbolic { result, .. }) = out.map(|o| &o.output) {
                    ts.sym = Some(result.clone());
                }
                (ts.statement(&stmt, result, fs), err)
            }
            _ => match xmath_core::parse_statement(source) {
                Ok(stmt) => (ts.statement(&stmt, None, fs), err),
                Err(e) => (ts.text_c(source, fs, RED), Some(e.msg)),
            },
        }
    })
}

// ------------------------------------------------------------------ PDF export

/// Options of [`export_pdf`].
pub struct PdfOptions {
    pub title: String,
}

/// Renders the worksheet to PDF bytes: every page of the sheet becomes a PDF page of the
/// paper size from the page setup. Formulas, text and plots are vector graphics; text is real
/// (embedded fonts). `outputs` are the evaluation results of `ws`.
/// Width and height of every region as the worksheet draws it at 100 % (with its results),
/// laid out without a window: used to keep regions from overlapping (MCP, «Разделить области»).
pub fn region_sizes(ws: &Worksheet, outputs: &HashMap<u64, RegionOutput>) -> HashMap<u64, [f32; 2]> {
    let (defs, has_bold) = crate::fonts::definitions();
    let ctx = Context::default();
    ctx.set_fonts(defs);
    let _ = ctx.run(Default::default(), |_| {});
    let mut sheet: Option<Sheet> = None;
    let _ = ctx.run(Default::default(), |ctx| {
        sheet = Some(Sheet::layout(ctx, ws, outputs, 1.0, has_bold));
    });
    let sheet = sheet.expect("layout pass ran");
    ws.regions.iter().zip(sheet.items.iter()).map(|(r, p)| (r.id, [p.rect.width(), p.rect.height()])).collect()
}

pub fn export_pdf(ws: &Worksheet, outputs: &HashMap<u64, RegionOutput>, opts: &PdfOptions) -> Vec<u8> {
    let (defs, has_bold) = crate::fonts::definitions();
    let ctx = Context::default();
    ctx.set_fonts(defs.clone());
    // a fine pixel grid keeps the glyph positions precise
    ctx.set_pixels_per_point(4.0);
    // the first pass makes the fonts available
    let _ = ctx.run(Default::default(), |_| {});

    let setup = ws.page.clone();
    let (pw_mm, ph_mm) = setup.paper_mm();
    let (paper_w, paper_h) = (pw_mm * MM_TO_PX, ph_mm * MM_TO_PX);
    let (ml, mt) = (setup.margins[0] * MM_TO_PX, setup.margins[1] * MM_TO_PX);

    let mut doc = PdfDoc::new(defs, &opts.title);
    let mut sheet: Option<Sheet> = None;
    let _ = ctx.run(Default::default(), |ctx| {
        sheet = Some(Sheet::layout(ctx, ws, outputs, 1.0, has_bold));
    });
    let sheet = sheet.expect("layout pass ran");
    for k in 0..sheet.page_count() {
        let out = ctx.run(Default::default(), |ctx| {
            let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new(("print-page", k))));
            let clip = sheet.page_clip(k, Pos2::new(ml, mt), 0.0, paper_w);
            sheet.paint_page(&painter.with_clip_rect(clip), k, Pos2::new(ml, mt));
        });
        let mut page = doc.page(paper_w, paper_h);
        use canvas::Canvas;
        canvas::paint_shapes(&out.shapes, &mut page, Vec2::ZERO, has_bold);
        let c = sheet.page_clip(k, Pos2::new(ml, mt), 0.0, paper_w);
        page.clip(Some([c.min.x, c.min.y, c.max.x, c.max.y]));
        for (r, bm) in sheet.page_images(k, Pos2::new(ml, mt)) {
            page.image(r, &bm);
        }
        page.clip(None);
        page.finish();
    }
    doc.finish()
}

/// Pixels per worksheet pixel of the pictures [`render_plot_pngs`] draws.
pub const PNG_SCALE: f32 = 2.0;

/// Draws every plot of `ws` (X-Y, polar, 3D) as the application does and returns PNG pictures by
/// region id; they are stored in `.xmcd` files as `<rendering>`. `outputs` are the evaluation
/// results of `ws`.
pub fn render_plot_pngs(ws: &Worksheet, outputs: &HashMap<u64, RegionOutput>) -> HashMap<u64, Vec<u8>> {
    let (defs, has_bold) = crate::fonts::definitions();
    let ctx = Context::default();
    ctx.set_fonts(defs.clone());
    ctx.set_pixels_per_point(4.0);
    let _ = ctx.run(Default::default(), |_| {});
    let mut result = HashMap::new();
    for region in ws.regions.iter().filter(|r| matches!(r.kind, RegionKind::Plot { .. } | RegionKind::Plot3d { .. })) {
        // a worksheet of this region alone, placed at the origin of a page without margins
        let mut one = Worksheet { regions: vec![region.clone()], format: ws.format, ..Worksheet::default() };
        one.regions[0].x = 0.0;
        one.regions[0].y = 0.0;
        one.regions[0].anchor = [0.0, 0.0];
        one.page = ws.page.clone();
        let mut sheet: Option<Sheet> = None;
        let _ = ctx.run(Default::default(), |ctx| {
            sheet = Some(Sheet::layout(ctx, &one, outputs, 1.0, has_bold));
        });
        let Some(sheet) = sheet else { continue };
        let Some(item) = sheet.items.first() else { continue };
        // axis numbers and labels reach beyond the nominal rectangle of the region
        let (pad_lo, pad_hi) = (18.0, 24.0);
        let size = item.rect.size() + Vec2::splat(pad_lo + pad_hi);
        let out = ctx.run(Default::default(), |ctx| {
            let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new(("plot-png", region.id))));
            let clip = Rect::from_min_size(Pos2::ZERO, size);
            sheet.paint_page(&painter.with_clip_rect(clip), 0, Pos2::new(pad_lo, pad_lo));
        });
        let Some(mut canvas) = crate::raster::RasterCanvas::new(size.x, size.y, PNG_SCALE, defs.clone()) else { continue };
        canvas::paint_shapes(&out.shapes, &mut canvas, Vec2::ZERO, has_bold);
        let png = canvas.png();
        if !png.is_empty() {
            result.insert(region.id, png);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page_count(s: &str) -> usize {
        s.matches("/Type /Page").count() - s.matches("/Type /Pages").count()
    }

    fn pdf_of(lines: &str) -> Vec<u8> {
        let ws = Worksheet::from_lines(lines);
        let out = xmath_core::evaluate_worksheet(&ws);
        export_pdf(&ws, &out, &PdfOptions { title: "test".into() })
    }


    fn blank(rect: Rect) -> Placed {
        Placed { rect, item: Item::Image { bitmap: Arc::new(Bitmap { width: 0, height: 0, rgba: Vec::new() }), texture: None } }
    }

    fn sheet_of(items: Vec<Placed>) -> Sheet {
        let mut s = Sheet { zoom: 1.0, setup: PageSetup::default(), has_bold: true, items, pages: Vec::new(), cols: 1 };
        s.paginate();
        s
    }

    #[test]
    fn dense_regions_near_a_break_do_not_make_strips() {
        let ph = PageSetup::default().printable_px().1;
        // many overlapping regions stacked right above and across the first break
        let mut items = Vec::new();
        for i in 0..40 {
            let top = 20.0 + i as f32 * 5.0 + ph * 0.55;
            items.push(blank(Rect::from_min_size(Pos2::new(10.0, top), Vec2::new(100.0, 400.0))));
        }
        items.push(blank(Rect::from_min_size(Pos2::new(10.0, 5.0), Vec2::new(100.0, 20.0))));
        let s = sheet_of(items);
        let n = s.page_count();
        for k in 0..n - 1 {
            let (a, b) = s.page_range(k);
            assert!(b - a >= ph / 3.0 - 0.5, "page {k} is a strip: {}", b - a);
        }
    }

    #[test]
    fn wide_content_goes_to_pages_on_the_right() {
        let pw = PageSetup::default().printable_px().0;
        let items = vec![blank(Rect::from_min_size(Pos2::new(10.0, 10.0), Vec2::new(50.0, 20.0))), blank(Rect::from_min_size(Pos2::new(pw + 20.0, 10.0), Vec2::new(50.0, 20.0)))];
        assert_eq!(sheet_of(items).page_count(), 2);
        let mut s = Sheet { zoom: 1.0, setup: PageSetup { within_margins: true, ..PageSetup::default() }, has_bold: true, items: vec![blank(Rect::from_min_size(Pos2::new(pw + 20.0, 10.0), Vec2::new(50.0, 20.0)))], pages: Vec::new(), cols: 1 };
        s.paginate();
        assert_eq!(s.page_count(), 1);
    }

    #[test]
    fn pdf_has_header_and_one_page() {
        let b = pdf_of("# Заголовок\na := 3\nb := a^2 + 1/2\nb =");
        assert!(b.starts_with(b"%PDF"));
        let s = String::from_utf8_lossy(&b);
        assert_eq!(page_count(&s), 1);
    }

    #[test]
    fn long_worksheet_has_several_pages() {
        let mut lines = String::new();
        for i in 0..80 {
            lines.push_str(&format!("x{i} := {i}\n"));
        }
        let ws = Worksheet::from_lines(&lines);
        let out = xmath_core::evaluate_worksheet(&ws);
        let b = export_pdf(&ws, &out, &PdfOptions { title: "t".into() });
        let n = page_count(&String::from_utf8_lossy(&b));
        assert!(n >= 3, "pages: {n}"); // 80 rows * 30 px = 2400 px: three printable pages
    }
}
