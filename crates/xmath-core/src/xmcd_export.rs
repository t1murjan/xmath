//! Export of a worksheet to a Mathcad 15 `.xmcd` file (XML formats `worksheet30` / `math30`).
//!
//! The inverse of [`crate::xmcd`]: math regions become `ml:` trees (definitions, evaluations,
//! function definitions with `boundVars`, matrices stored column by column, ranges, integrals,
//! derivatives, sums and products as `lambda` + `bounds`, programs, `vectorize`, display units
//! as `unitOverride`), text regions become `<text>` paragraphs, pictures become embedded PNGs
//! and references become `<reference>` regions.
//!
//! X-Y plots are written as Mathcad's own plot item (expression tree, axis limits, trace
//! colours and line types; see [`crate::xmcd_plot_write`]). Everything xmath knows about a plot
//! that the item cannot hold, the polar and 3D plots, and the worksheet-wide xmath settings
//! (default plot format, number format, page) are also stored as `<!--xmath:… {json}-->`
//! comments, which Mathcad ignores and [`crate::xmcd`] reads back, so that a worksheet
//! saved by xmath and opened again in xmath is unchanged. [`ExportOptions`] carries the
//! pictures of the plots as the program draws them (`<rendering>`); polar and 3D plots are
//! written as pictures (or as a text note without one) because Mathcad's format for them
//! is not known. Results of evaluations are written as `<result>` like Mathcad does.

use crate::ast::{BinOp, Expr, PStmt, Stmt, Target, UnOp};
use crate::parser::parse_statement;
use crate::worksheet::{LineStyle, Paper, PlotKind, Region, RegionKind, RegionOutput, Snapshot, TextStyle, Worksheet};
use crate::xmcd_plot_write::{encode_plot, PlotSpec, TraceSpec};
use std::collections::HashMap;
use std::fmt::Write;

/// Mathcad coordinates are in points; the worksheet uses pixels of 96 dpi.
const SCALE: f32 = 96.0 / 72.0;

/// The exported file and what could not be carried over.
#[derive(Debug, Clone)]
pub struct ExportResult {
    pub xml: String,
    pub warnings: Vec<String>,
}

/// What the caller can add to an export.
#[derive(Default)]
pub struct ExportOptions {
    /// PNG pictures of regions (plots), by region id, as the program draws them.
    pub renderings: HashMap<u64, Vec<u8>>,
    /// Device pixels per worksheet pixel of the pictures in `renderings` (0: 1).
    pub rendering_scale: f32,
    /// Where files are read from during the evaluation that produces the `<result>`s.
    pub env: Option<crate::fileio::EvalEnv>,
    /// Evaluation results with snapshots (`Worksheet::evaluate_with(true, …)`); computed here
    /// when `None`.
    pub outputs: Option<HashMap<u64, RegionOutput>>,
}

/// Exports `ws` as `.xmcd` XML text (results are computed here, no pictures).
pub fn export_xmcd(ws: &Worksheet) -> ExportResult {
    export_xmcd_with(ws, &ExportOptions::default())
}

/// Exports `ws` as `.xmcd` XML text.
pub fn export_xmcd_with(ws: &Worksheet, opts: &ExportOptions) -> ExportResult {
    let outputs = match &opts.outputs {
        Some(o) if o.values().any(|r| r.value.is_some()) => o.clone(),
        _ => ws.evaluate_with(true, &opts.env.clone().unwrap_or_default()),
    };
    let mut x = Exporter { warnings: Vec::new(), items: Vec::new(), outputs, renderings: &opts.renderings, rendering_scale: if opts.rendering_scale > 0.0 { opts.rendering_scale } else { 1.0 }, pending_result: None, pending_sym: None };
    let mut out = String::new();
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"no\"?>\n");
    out.push_str(
        "<worksheet version=\"3.0.3\" xmlns=\"http://schemas.mathsoft.com/worksheet30\" \
         xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\" xmlns:ws=\"http://schemas.mathsoft.com/worksheet30\" \
         xmlns:ml=\"http://schemas.mathsoft.com/math30\" xmlns:u=\"http://schemas.mathsoft.com/units10\" \
         xmlns:p=\"http://schemas.mathsoft.com/provenance10\">\n",
    );
    out.push_str("\t<pointReleaseData/>\n\t<metadata>\n\t\t<generator>xmath</generator>\n\t\t<userData>\n\t\t\t<title/>\n\t\t\t<description/>\n\t\t\t<author/>\n\t\t\t<company/>\n\t\t\t<keywords/>\n\t\t\t<revisedBy/>\n\t\t</userData>\n");
    // Mathcad's schema requires the document identity
    let zero = "00000000-0000-0000-0000-000000000000";
    let _ = writeln!(
        out,
        "\t\t<identityInfo>\n\t\t\t<revision>1</revision>\n\t\t\t<documentID>{}</documentID>\n\t\t\t<versionID>{}</versionID>\n\t\t\t<parentVersionID>{zero}</parentVersionID>\n\t\t\t<branchID>{zero}</branchID>\n\t\t</identityInfo>\n\t</metadata>",
        guid(ws, 1),
        guid(ws, 2)
    );
    out.push_str(&xmath_comment("worksheet", &worksheet_json(ws)));
    x.settings(ws, &mut out);
    out.push_str("\t<regions>\n");
    // regions are written in evaluation order (the order Mathcad evaluates by position anyway)
    for i in ws.evaluation_order() {
        x.region(&ws.regions[i], &mut out);
    }
    out.push_str("\t</regions>\n");
    if x.items.is_empty() {
        // Mathcad files always have binary content: a 1×1 picture keeps the element non-empty
        x.items.push((ONE_PIXEL_PNG.to_string(), false));
    }
    {
        out.push_str("\t<binaryContent>\n");
        for (i, (b64, gz)) in x.items.iter().enumerate() {
            let _ = write!(out, "\t\t<item item-id=\"{}\"{}>", i + 1, if *gz { " content-encoding=\"gzip\"" } else { "" });
            // base64 in lines of 72 characters like Mathcad does
            for (k, ch) in b64.as_bytes().chunks(72).enumerate() {
                if k > 0 {
                    out.push('\n');
                }
                out.push_str(std::str::from_utf8(ch).unwrap_or(""));
            }
            out.push_str("</item>\n");
        }
        out.push_str("\t</binaryContent>\n");
    }
    out.push_str("</worksheet>\n");
    ExportResult { xml: out, warnings: x.warnings }
}

fn esc(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '"' => o.push_str("&quot;"),
            c if (c as u32) < 0x20 && c != '\n' && c != '\t' => {}
            c => o.push(c),
        }
    }
    o
}

fn pt(v: f32) -> String {
    let r = (v / SCALE * 1000.0).round() / 1000.0;
    format!("{r}")
}

struct Exporter<'a> {
    warnings: Vec<String>,
    /// Base64 payloads of `binaryContent` items (item id = index + 1) and whether gzip-compressed.
    items: Vec<(String, bool)>,
    outputs: HashMap<u64, RegionOutput>,
    renderings: &'a HashMap<u64, Vec<u8>>,
    rendering_scale: f32,
    /// `<result>` of the math region being written, consumed by its first `ml:eval`.
    pending_result: Option<Snapshot>,
    /// Symbolic result of the region being written (`<ml:symResult>`).
    pending_sym: Option<Expr>,
}

impl Exporter<'_> {
    fn settings(&mut self, ws: &Worksheet, out: &mut String) {
        out.push_str("\t<settings>\n\t\t<presentation>\n\t\t\t<textRendering>\n\t\t\t\t<textStyles>\n");
        for (name, size, bold, italic, family, align) in [
            ("Normal", 10, false, false, "Arial", "left"),
            ("Heading 1", 14, true, false, "Arial", "left"),
            ("Heading 2", 12, true, true, "Arial", "left"),
            ("Heading 3", 12, false, false, "Arial", "left"),
            ("Title", 24, true, false, "Times New Roman", "center"),
        ] {
            let _ = writeln!(
                out,
                "\t\t\t\t\t<textStyle name=\"{name}\">\n\t\t\t\t\t\t<blockAttr margin-left=\"0\" margin-right=\"0\" text-indent=\"inherit\" text-align=\"{align}\" list-style-type=\"inherit\" tabs=\"inherit\"/>\n\t\t\t\t\t\t<inlineAttr font-family=\"{family}\" font-charset=\"0\" font-size=\"{size}\" font-weight=\"{}\" font-style=\"{}\" underline=\"false\" line-through=\"false\" vertical-align=\"baseline\"/>\n\t\t\t\t\t</textStyle>",
                if bold { "bold" } else { "normal" },
                if italic { "italic" } else { "normal" }
            );
        }
        out.push_str("\t\t\t\t</textStyles>\n\t\t\t</textRendering>\n");
        out.push_str("\t\t\t<mathRendering equation-color=\"#000\">\n");
        out.push_str("\t\t\t\t<operators multiplication=\"narrow-dot\" derivative=\"derivative\" literal-subscript=\"large\" definition=\"colon-equal\" global-definition=\"triple-equal\" local-definition=\"left-arrow\" equality=\"bold-equal\" symbolic-evaluation=\"right-arrow\"/>\n");
        out.push_str("\t\t\t\t<mathStyles>\n");
        // the ten styles Mathcad 15 always writes (its schema requires all of them)
        for (n, f, sz) in [
            ("Variables", "Times New Roman", 10),
            ("Constants", "Times New Roman", 10),
            ("User 1", "Arial", 10),
            ("User 2", "Courier New", 10),
            ("User 3", "Arial", 10),
            ("User 4", "Times New Roman", 10),
            ("User 5", "Times New Roman", 10),
            ("User 6", "Arial", 10),
            ("User 7", "Times New Roman", 10),
            ("Math Text Font", "Times New Roman", 14),
        ] {
            let _ = writeln!(out, "\t\t\t\t\t<mathStyle name=\"{n}\" font-family=\"{f}\" font-charset=\"0\" font-size=\"{sz}\" font-weight=\"normal\" font-style=\"normal\" underline=\"false\"/>");
        }
        out.push_str("\t\t\t\t</mathStyles>\n");
        out.push_str("\t\t\t\t<dimensionNames mass=\"mass\" length=\"length\" time=\"time\" current=\"current\" thermodynamic-temperature=\"temperature\" luminous-intensity=\"luminosity\" amount-of-substance=\"substance\" display=\"false\"/>\n");
        out.push_str("\t\t\t\t<symbolics derivation-steps-style=\"vertical-insert\" show-comments=\"false\" evaluate-in-place=\"false\"/>\n");
        let f = &ws.format;
        let _ = writeln!(
            out,
            "\t\t\t\t<results numeric-only=\"true\">\n\t\t\t\t\t<general precision=\"{}\" show-trailing-zeros=\"false\" radix=\"dec\" complex-threshold=\"10\" zero-threshold=\"{}\" imaginary-value=\"i\" exponential-threshold=\"{}\"/>\n\t\t\t\t\t<matrix display-style=\"auto\" expand-nested-arrays=\"false\"/>\n\t\t\t\t\t<unit format-units=\"true\" simplify-units=\"true\" fractional-unit-exponent=\"false\"/>\n\t\t\t\t</results>\n\t\t\t</mathRendering>",
            f.precision, f.zero_threshold, f.exp_threshold
        );
        let p = &ws.page;
        let (w_mm, h_mm) = p.paper.size_mm();
        let code = match p.paper {
            Paper::Letter => 1,
            Paper::Legal => 5,
            Paper::A3 => 8,
            Paper::A4 => 9,
            Paper::A5 => 11,
        };
        let mm_pt = 72.0 / 25.4;
        let _ = writeln!(
            out,
            "\t\t\t<pageModel show-page-frame=\"false\" show-header-frame=\"false\" show-footer-frame=\"false\" header-footer-start-page=\"1\" paper-code=\"{code}\" orientation=\"{}\" print-single-page-width=\"false\" page-width=\"{}\" page-height=\"{}\">\n\t\t\t\t<margins left=\"{}\" right=\"{}\" top=\"{}\" bottom=\"{}\"/>\n\t\t\t\t<header use-full-page-width=\"false\"/>\n\t\t\t\t<footer use-full-page-width=\"false\"/>\n\t\t\t</pageModel>",
            if p.landscape { "landscape" } else { "portrait" },
            (w_mm * mm_pt * 10.0).round() / 10.0,
            (h_mm * mm_pt * 10.0).round() / 10.0,
            (p.margins[0] * mm_pt * 10.0).round() / 10.0,
            (p.margins[2] * mm_pt * 10.0).round() / 10.0,
            (p.margins[1] * mm_pt * 10.0).round() / 10.0,
            (p.margins[3] * mm_pt * 10.0).round() / 10.0,
        );
        out.push_str("\t\t\t<colorModel background-color=\"#fff\" default-highlight-color=\"#ffff80\"/>\n\t\t\t<language math=\"ru\" UI=\"ru\"/>\n\t\t</presentation>\n");
        out.push_str("\t\t<calculation>\n\t\t\t<builtInVariables array-origin=\"0\" convergence-tolerance=\"0.001\" constraint-tolerance=\"0.001\" random-seed=\"1\" prn-precision=\"4\" prn-col-width=\"8\"/>\n\t\t\t<calculationBehavior automatic-recalculation=\"true\" matrix-strict-singularity-check=\"false\" optimize-expressions=\"false\" exact-boolean=\"true\" strings-use-origin=\"false\" zero-over-zero=\"error\">\n\t\t\t\t<compatibility multiple-assignment=\"MC12\" local-assignment=\"MC11\"/>\n\t\t\t</calculationBehavior>\n\t\t\t<units>\n\t\t\t\t<currentUnitSystem name=\"si\" customized=\"false\"/>\n\t\t\t</units>\n\t\t</calculation>\n");
        out.push_str("\t\t<editor view-annotations=\"false\" view-regions=\"false\">\n\t\t\t<ruler is-visible=\"false\" ruler-unit=\"in\"/>\n\t\t\t<grid granularity-x=\"6\" granularity-y=\"6\"/>\n\t\t</editor>\n");
        out.push_str("\t\t<fileFormat image-type=\"image/png\" image-quality=\"75\" save-numeric-results=\"true\" exclude-large-results=\"true\" save-text-images=\"false\" screen-dpi=\"96\"/>\n");
        out.push_str("\t\t<miscellaneous>\n\t\t\t<handbook handbook-region-tag-ub=\"1\" can-delete-original-handbook-regions=\"true\" can-delete-user-regions=\"true\" can-print=\"true\" can-copy=\"true\" can-save=\"true\" file-permission-mask=\"0\"/>\n\t\t</miscellaneous>\n\t</settings>\n");
    }

    fn region_open(&self, r: &Region, w: f32, h: f32, out: &mut String) {
        let (ax, ay) = (r.x + r.anchor[0], r.y + r.anchor[1]);
        let _ = writeln!(
            out,
            "\t\t<region region-id=\"{}\" left=\"{}\" top=\"{}\" width=\"{}\" height=\"{}\" align-x=\"{}\" align-y=\"{}\" show-border=\"false\" show-highlight=\"false\" is-protected=\"true\" z-order=\"0\" background-color=\"inherit\" tag=\"\">",
            r.id,
            pt(r.x),
            pt(r.y),
            pt(w),
            pt(h),
            pt(ax),
            pt(ay)
        );
    }

    fn region(&mut self, r: &Region, out: &mut String) {
        match &r.kind {
            RegionKind::Math { source } => match parse_statement(source) {
                Ok(stmt) => {
                    let mut body = String::new();
                    self.pending_result = self.outputs.get(&r.id).and_then(|o| o.value.clone());
                    self.pending_sym = match self.outputs.get(&r.id).map(|o| &o.output) {
                        Some(crate::worksheet::Output::Symbolic { result, .. }) => Some(result.clone()),
                        _ => None,
                    };
                    let res = self.stmt(&stmt, &mut body, 4);
                    self.pending_result = None;
                    if let Err(e) = res {
                        self.warnings.push(format!("область {}: {e}; записана как текст", r.id));
                        self.text_region(r, source, &TextStyle::default(), 320.0, false, out);
                        return;
                    }
                    let w = (source.chars().count() as f32 * 7.0 + 12.0).max(24.0);
                    self.region_open(r, w, 20.0, out);
                    out.push_str("\t\t\t<math optimize=\"false\" disable-calc=\"false\">\n");
                    out.push_str(&body);
                    out.push_str("\t\t\t</math>\n\t\t</region>\n");
                }
                Err(e) => {
                    self.warnings.push(format!("область {}: «{source}» не разбирается ({}); записана как текст", r.id, e.msg));
                    self.text_region(r, source, &TextStyle::default(), 320.0, false, out);
                }
            },
            RegionKind::Text { text, width, style, lock_width } => {
                if *lock_width {
                    self.text_region(r, text, style, *width, true, out);
                } else {
                    // Mathcad wraps a text region at its stored width; xmath's "wrap at the page
                    // margin" is kept in a comment
                    let mut tmp = String::new();
                    self.text_region(r, text, style, *width, false, &mut tmp);
                    let at = tmp.rfind("\t\t</region>").unwrap_or(tmp.len());
                    tmp.insert_str(at, &xmath_comment("region", &serde_json::to_string(&r.kind).unwrap_or_default()));
                    out.push_str(&tmp);
                }
            }
            RegionKind::Image { png_base64, width, height } => {
                let id = self.items.len() + 1;
                self.items.push((png_base64.clone(), false));
                let kind = if png_base64.starts_with("/9j/") { "jpeg" } else { "png" };
                self.region_open(r, *width, *height, out);
                let _ = writeln!(
                    out,
                    "\t\t\t<picture>\n\t\t\t\t<{kind} item-idref=\"{id}\" display-width=\"{}\" display-height=\"{}\"/>\n\t\t\t</picture>\n\t\t\t<rendering item-idref=\"{id}\"/>\n\t\t</region>",
                    pt(*width),
                    pt(*height)
                );
            }
            RegionKind::Plot3d { kind, width, height, .. } => {
                self.picture_plot(r, kind.title(), *width, *height, out);
            }
            RegionKind::Plot { kind: PlotKind::Polar, width, height, .. } => {
                self.picture_plot(r, "Полярный график", *width, *height, out);
            }
            RegionKind::Plot { x, y, xs, width, height, x_min, x_max, y_min, y_max, fmt, kind: _ } => {
                let parse = |s: &str| crate::parser::parse_expr(s).unwrap_or_else(|_| Expr::Var(s.to_string()));
                let xs_list: Vec<&String> = if xs.is_empty() { if x.trim().is_empty() { vec![] } else { vec![x] } } else { xs.iter().collect() };
                let trace = |t: &crate::worksheet::TraceFormat| -> Option<TraceSpec> {
                    if t.color.is_none() && t.line.is_none() {
                        return None;
                    }
                    let line = match t.line {
                        Some(LineStyle::Dot) => 2,
                        Some(LineStyle::Dash) => 3,
                        Some(LineStyle::DashDot) => 4,
                        _ => 1,
                    };
                    Some(TraceSpec { line, rgb: t.color.unwrap_or([255, 0, 0]) })
                };
                let spec = PlotSpec {
                    x: xs_list.iter().map(|s| parse(s)).collect(),
                    y: y.iter().filter(|s| !s.trim().is_empty()).map(|s| parse(s)).collect(),
                    x_min: *x_min,
                    x_max: *x_max,
                    y_min: *y_min,
                    y_max: *y_max,
                    traces: fmt.traces.iter().take(16).map(trace).collect(),
                    rect: [
                        r.x.max(0.0).round() as u32,
                        r.y.max(0.0).round() as u32,
                        (r.x.max(0.0) + width).round() as u32,
                        (r.y.max(0.0) + height).round() as u32,
                    ],
                    region_id: r.id as u32,
                };
                let blob = self.add_gzip(&encode_plot(&spec));
                let rendering = self.add_rendering(r.id);
                self.region_open(r, *width, *height, out);
                let _ = writeln!(out, "\t\t\t<plot disable-calc=\"false\" item-idref=\"{blob}\"/>");
                if let Some(rid) = rendering {
                    let _ = writeln!(out, "\t\t\t<rendering item-idref=\"{rid}\"/>");
                }
                out.push_str(&xmath_comment("region", &serde_json::to_string(&r.kind).unwrap_or_default()));
                out.push_str("\t\t</region>\n");
            }
            RegionKind::Reference { href } => {
                let mut h = href.replace('\\', "/");
                if h.to_lowercase().ends_with(".xmath") {
                    let stem = &h[..h.len() - 6];
                    self.warnings.push(format!(
                        "область {}: ссылка на «{href}» записана как «{stem}.xmcd»; экспортируйте и этот файл в .xmcd",
                        r.id
                    ));
                    h = format!("{stem}.xmcd");
                }
                self.region_open(r, 300.0, 12.0, out);
                let _ = writeln!(
                    out,
                    "\t\t\t<reference>\n\t\t\t\t<originRef doc-id=\"00000000-0000-0000-0000-000000000000\" version-id=\"00000000-0000-0000-0000-000000000000\" branch-id=\"00000000-0000-0000-0000-000000000000\" revision-num=\"1\" is-modified=\"false\" region-id=\"0\" href=\"{0}\" xmlns=\"http://schemas.mathsoft.com/provenance10\">\n\t\t\t\t\t<hash/>\n\t\t\t\t</originRef>\n\t\t\t</reference>\n\t\t\t<link href=\"./{0}\"/>\n\t\t</region>",
                    esc(&h)
                );
            }
        }
    }

    /// gzip + base64 item; returns its id.
    fn add_gzip(&mut self, bytes: &[u8]) -> usize {
        use base64::Engine;
        use std::io::Write as _;
        let mut enc = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        let _ = enc.write_all(bytes);
        let gz = enc.finish().unwrap_or_default();
        self.items.push((base64::engine::general_purpose::STANDARD.encode(gz), true));
        self.items.len()
    }

    /// The picture of region `id` as an item, if the caller supplied one.
    fn add_rendering(&mut self, id: u64) -> Option<usize> {
        use base64::Engine;
        let png = self.renderings.get(&id)?;
        self.items.push((base64::engine::general_purpose::STANDARD.encode(png), false));
        Some(self.items.len())
    }

    /// A plot Mathcad cannot read: its picture (or a text note without one) plus the xmath
    /// comment with the complete plot.
    fn picture_plot(&mut self, r: &Region, what: &str, width: f32, height: f32, out: &mut String) {
        let json = match &r.kind {
            RegionKind::Plot3d { .. } | RegionKind::Plot { .. } => serde_json::to_string(&r.kind).unwrap_or_default(),
            _ => String::new(),
        };
        // the region shows the picture at its own size
        let (width, height) = match self.renderings.get(&r.id).and_then(|p| png_size(p)) {
            Some((w, h)) => (w as f32 / self.rendering_scale, h as f32 / self.rendering_scale),
            None => (width, height),
        };
        match self.add_rendering(r.id) {
            Some(id) => {
                self.region_open(r, width, height, out);
                let _ = writeln!(
                    out,
                    "\t\t\t<picture>\n\t\t\t\t<png item-idref=\"{id}\" display-width=\"{}\" display-height=\"{}\"/>\n\t\t\t</picture>\n\t\t\t<rendering item-idref=\"{id}\"/>",
                    pt(width),
                    pt(height)
                );
                out.push_str(&xmath_comment("region", &json));
                out.push_str("\t\t</region>\n");
            }
            None => {
                let mut tmp = String::new();
                self.text_region(r, &format!("[{what}: открывается в xmath]"), &TextStyle::default(), width, true, &mut tmp);
                let at = tmp.rfind("\t\t</region>").unwrap_or(tmp.len());
                tmp.insert_str(at, &xmath_comment("region", &json));
                out.push_str(&tmp);
            }
        }
        self.warnings.push(format!(
            "область {}: {what} записан в .xmcd как рисунок; Mathcad показывает его без возможности редактирования, xmath восстанавливает график при открытии",
            r.id
        ));
    }

    fn text_region(&mut self, r: &Region, text: &str, style: &TextStyle, width: f32, lock: bool, out: &mut String) {
        let lines: Vec<&str> = text.split('\n').collect();
        let h = lines.len() as f32 * style.size * 1.4 * SCALE + 4.0;
        self.region_open(r, width, h, out);
        let _ = writeln!(out, "\t\t\t<text use-page-width=\"false\" push-down=\"false\" lock-width=\"{lock}\">");
        // a line break that keeps the spaces at the line ends is a `<br/>` inside one paragraph
        // (Mathcad trims the ends of a paragraph)
        let mut paras: Vec<Vec<&str>> = Vec::new();
        for (i, l) in lines.iter().enumerate() {
            if i > 0 && (lines[i - 1].ends_with(' ') || l.starts_with(' ')) {
                paras.last_mut().expect("first line exists").push(l);
            } else {
                paras.push(vec![l]);
            }
        }
        for para in paras {
            let _ = write!(out, "\t\t\t\t<p style=\"Normal\" margin-left=\"inherit\" margin-right=\"inherit\" text-indent=\"inherit\" text-align=\"inherit\" list-style-type=\"inherit\" tabs=\"inherit\">");
            let mut s = para.iter().map(|l| esc_spaces(l)).collect::<Vec<_>>().join("<br/>");
            if style.underline {
                s = format!("<u>{s}</u>");
            }
            if style.italic {
                s = format!("<i>{s}</i>");
            }
            if style.bold {
                s = format!("<b>{s}</b>");
            }
            let family = match style.font {
                crate::worksheet::TextFont::Sans => "Arial",
                crate::worksheet::TextFont::Serif => "Times New Roman",
                crate::worksheet::TextFont::Mono => "Courier New",
            };
            let _ = write!(out, "<f family=\"{family}\" size=\"{}\">{s}</f>", style.size);
            out.push_str("</p>\n");
        }
        out.push_str("\t\t\t</text>\n\t\t</region>\n");
    }

    // ------------------------------------------------------------ math

    fn stmt(&mut self, s: &Stmt, o: &mut String, d: usize) -> Result<(), String> {
        match s {
            Stmt::Define { target, expr, global, show } => {
                let tag = if *global { "globalDefine" } else { "define" };
                ind(o, d);
                let _ = writeln!(o, "<ml:{tag}>");
                self.target(target, o, d + 1)?;
                match show {
                    None => self.expr(expr, o, d + 1)?,
                    Some(unit) => {
                        ind(o, d + 1);
                        o.push_str("<ml:eval placeholderMultiplicationStyle=\"default\">\n");
                        self.expr(expr, o, d + 2)?;
                        if let Some(u) = unit {
                            self.unit_override(u, o, d + 2)?;
                        }
                        self.result(o, d + 2);
                        ind(o, d + 1);
                        o.push_str("</ml:eval>\n");
                    }
                }
                ind(o, d);
                let _ = writeln!(o, "</ml:{tag}>");
            }
            Stmt::Eval { expr, unit } => {
                ind(o, d);
                o.push_str("<ml:eval placeholderMultiplicationStyle=\"default\">\n");
                self.expr(expr, o, d + 1)?;
                if let Some(u) = unit {
                    self.unit_override(u, o, d + 1)?;
                }
                self.result(o, d + 1);
                ind(o, d);
                o.push_str("</ml:eval>\n");
            }
            Stmt::Show(e) => {
                self.expr(e, o, d)?;
            }
            Stmt::Symbolic { target, expr, ops } => {
                let mut dd = d;
                let tag = target.as_ref().map(|(_, g)| if *g { "globalDefine" } else { "define" });
                if let (Some((t, _)), Some(tag)) = (target, tag) {
                    ind(o, d);
                    let _ = writeln!(o, "<ml:{tag}>");
                    self.target(t, o, d + 1)?;
                    dd = d + 1;
                }
                ind(o, dd);
                o.push_str("<ml:symEval style=\"default\" hide-keywords=\"false\" hide-lhs=\"false\">\n");
                self.expr(expr, o, dd + 1)?;
                // keywords as Mathcad writes them: <ml:command> with the keyword id, or a
                // <ml:sequence> of the keyword and its arguments; chained keywords are stacked
                for op in ops {
                    ind(o, dd + 1);
                    o.push_str("<ml:command>\n");
                    if op.args.is_empty() {
                        id(&op.name, o, dd + 2);
                    } else {
                        ind(o, dd + 2);
                        o.push_str("<ml:sequence>\n");
                        id(&op.name, o, dd + 3);
                        for a in &op.args {
                            self.expr(a, o, dd + 3)?;
                        }
                        ind(o, dd + 2);
                        o.push_str("</ml:sequence>\n");
                    }
                    ind(o, dd + 1);
                    o.push_str("</ml:command>\n");
                }
                if let Some(r) = self.pending_sym.take() {
                    ind(o, dd + 1);
                    o.push_str("<ml:symResult>\n");
                    self.expr(&r, o, dd + 2)?;
                    ind(o, dd + 1);
                    o.push_str("</ml:symResult>\n");
                }
                ind(o, dd);
                o.push_str("</ml:symEval>\n");
                if let Some(tag) = tag {
                    ind(o, d);
                    let _ = writeln!(o, "</ml:{tag}>");
                }
            }
        }
        Ok(())
    }

    /// The stored result of the region, inside its `ml:eval`.
    fn result(&mut self, o: &mut String, d: usize) {
        if let Some(sn) = self.pending_result.take() {
            if let Some(x) = result_xml(&sn, d) {
                o.push_str(&x);
            }
        }
    }

    fn unit_override(&mut self, u: &Expr, o: &mut String, d: usize) -> Result<(), String> {
        ind(o, d);
        o.push_str("<ml:unitOverride>\n");
        self.expr(u, o, d + 1)?;
        ind(o, d);
        o.push_str("</ml:unitOverride>\n");
        Ok(())
    }

    fn target(&mut self, t: &Target, o: &mut String, d: usize) -> Result<(), String> {
        match t {
            Target::Var(n) => id(n, o, d),
            Target::Func(name, params) => {
                if crate::fileio::is_write_name(name) && params.first().is_some_and(|p| p.starts_with('"')) {
                    ind(o, d);
                    o.push_str("<ml:apply>\n");
                    id(name, o, d + 1);
                    for p in params {
                        ind(o, d + 1);
                        let _ = writeln!(o, "<ml:str>{}</ml:str>", esc(p.trim_matches('"')));
                    }
                    ind(o, d);
                    o.push_str("</ml:apply>\n");
                } else {
                    ind(o, d);
                    o.push_str("<ml:function>\n");
                    id(name, o, d + 1);
                    ind(o, d + 1);
                    o.push_str("<ml:boundVars>\n");
                    for p in params {
                        id(p, o, d + 2);
                    }
                    ind(o, d + 1);
                    o.push_str("</ml:boundVars>\n");
                    ind(o, d);
                    o.push_str("</ml:function>\n");
                }
            }
            Target::Indexed(n, idx) => self.expr(&Expr::Index(Box::new(Expr::Var(n.clone())), idx.clone()), o, d)?,
            Target::Vector(names) => {
                ind(o, d);
                let _ = writeln!(o, "<ml:matrix rows=\"{}\" cols=\"1\">", names.len());
                for n in names {
                    id(n, o, d + 1);
                }
                ind(o, d);
                o.push_str("</ml:matrix>\n");
            }
        }
        Ok(())
    }

    /// `e`, wrapped in `<ml:parens>` when `paren`.
    fn child(&mut self, e: &Expr, paren: bool, o: &mut String, d: usize) -> Result<(), String> {
        if paren {
            ind(o, d);
            o.push_str("<ml:parens>\n");
            self.expr(e, o, d + 1)?;
            ind(o, d);
            o.push_str("</ml:parens>\n");
            Ok(())
        } else {
            self.expr(e, o, d)
        }
    }

    fn apply(&mut self, head: &str, args: &[&Expr], o: &mut String, d: usize) -> Result<(), String> {
        ind(o, d);
        o.push_str("<ml:apply>\n");
        ind(o, d + 1);
        let _ = writeln!(o, "<{head}/>");
        for a in args {
            self.expr(a, o, d + 1)?;
        }
        ind(o, d);
        o.push_str("</ml:apply>\n");
        Ok(())
    }

    /// Several arguments of a function call: a `<ml:sequence>` unless there is just one.
    fn args(&mut self, args: &[Expr], o: &mut String, d: usize) -> Result<(), String> {
        if args.len() == 1 {
            return self.expr(&args[0], o, d);
        }
        ind(o, d);
        o.push_str("<ml:sequence>\n");
        for a in args {
            self.expr(a, o, d + 1)?;
        }
        ind(o, d);
        o.push_str("</ml:sequence>\n");
        Ok(())
    }

    fn expr(&mut self, e: &Expr, o: &mut String, d: usize) -> Result<(), String> {
        match e {
            Expr::Num(x) => {
                if x.is_infinite() {
                    if *x < 0.0 {
                        self.apply("ml:minus", &[&Expr::Var("∞".into())], o, d)?;
                    } else {
                        id("∞", o, d);
                    }
                } else {
                    ind(o, d);
                    let _ = writeln!(o, "<ml:real>{x}</ml:real>");
                }
            }
            Expr::Imag(x) => {
                ind(o, d);
                let _ = writeln!(o, "<ml:imag>{x}</ml:imag>");
            }
            Expr::Str(s) => {
                ind(o, d);
                let _ = writeln!(o, "<ml:str>{}</ml:str>", esc(s));
            }
            Expr::Var(n) => id(n, o, d),
            Expr::Unary(op, a) => {
                let (head, paren) = match op {
                    UnOp::Neg => ("ml:minus", a.precedence() < 6),
                    UnOp::Not => ("ml:not", a.precedence() < 4),
                    UnOp::Factorial => ("ml:factorial", a.precedence() < 9),
                };
                ind(o, d);
                o.push_str("<ml:apply>\n");
                ind(o, d + 1);
                let _ = writeln!(o, "<{head}/>");
                self.child(a, paren, o, d + 1)?;
                ind(o, d);
                o.push_str("</ml:apply>\n");
            }
            Expr::Binary(op, a, b) => {
                let head = match op {
                    BinOp::Add => "ml:plus",
                    BinOp::Sub => "ml:minus",
                    BinOp::Mul => "ml:mult",
                    BinOp::Div => "ml:div",
                    BinOp::Pow => "ml:pow",
                    BinOp::Eq => "ml:equal",
                    BinOp::Ne => "ml:notEqual",
                    BinOp::Lt => "ml:lessThan",
                    BinOp::Gt => "ml:greaterThan",
                    BinOp::Le => "ml:lessOrEqual",
                    BinOp::Ge => "ml:greaterOrEqual",
                    BinOp::And => "ml:and",
                    BinOp::Or => "ml:or",
                    BinOp::Xor => "ml:xor",
                };
                let p = op.precedence();
                let (pa, pb) = if *op == BinOp::Pow {
                    (a.precedence() <= p, b.precedence() < p)
                } else {
                    (a.precedence() < p, b.precedence() <= p)
                };
                // a minus sign in front of a literal under `^` is also parenthesised
                ind(o, d);
                o.push_str("<ml:apply>\n");
                ind(o, d + 1);
                let _ = writeln!(o, "<{head}/>");
                self.child(a, pa, o, d + 1)?;
                self.child(b, pb, o, d + 1)?;
                ind(o, d);
                o.push_str("</ml:apply>\n");
            }
            Expr::Call(name, args) => self.call(name, args, o, d)?,
            Expr::Index(base, idx) => {
                ind(o, d);
                o.push_str("<ml:apply>\n");
                ind(o, d + 1);
                o.push_str("<ml:indexer/>\n");
                self.child(base, base.precedence() < 9, o, d + 1)?;
                self.args(idx, o, d + 1)?;
                ind(o, d);
                o.push_str("</ml:apply>\n");
            }
            Expr::Matrix(rows) => {
                let r = rows.len();
                let c = rows.first().map_or(0, Vec::len);
                if r == 0 || c == 0 || rows.iter().any(|row| row.len() != c) {
                    return Err("матрица неправильной формы".into());
                }
                ind(o, d);
                let _ = writeln!(o, "<ml:matrix rows=\"{r}\" cols=\"{c}\">");
                // Mathcad stores matrices column by column
                for j in 0..c {
                    for row in rows.iter() {
                        self.expr(&row[j], o, d + 1)?;
                    }
                }
                ind(o, d);
                o.push_str("</ml:matrix>\n");
            }
            // Mathcad: <ml:range> a c </ml:range>, with a step: <ml:range> <ml:sequence> a b </ml:sequence> c </ml:range>
            Expr::Range(a, b, c) => {
                ind(o, d);
                o.push_str("<ml:range>\n");
                match b {
                    Some(b) => {
                        ind(o, d + 1);
                        o.push_str("<ml:sequence>\n");
                        self.expr(a, o, d + 2)?;
                        self.expr(b, o, d + 2)?;
                        ind(o, d + 1);
                        o.push_str("</ml:sequence>\n");
                    }
                    None => self.expr(a, o, d + 1)?,
                }
                self.expr(c, o, d + 1)?;
                ind(o, d);
                o.push_str("</ml:range>\n");
            }
            Expr::Abs(a) => self.apply("ml:absval", &[a], o, d)?,
            Expr::Program(stmts) => {
                ind(o, d);
                o.push_str("<ml:program>\n");
                self.pstmts(stmts, o, d + 1)?;
                ind(o, d);
                o.push_str("</ml:program>\n");
            }
            Expr::OnError(a, b) => {
                ind(o, d);
                o.push_str("<ml:onError>\n");
                self.expr(a, o, d + 1)?;
                self.expr(b, o, d + 1)?;
                ind(o, d);
                o.push_str("</ml:onError>\n");
            }
            Expr::Vectorize(a) => {
                ind(o, d);
                o.push_str("<ml:apply>\n");
                ind(o, d + 1);
                o.push_str("<ml:vectorize/>\n");
                self.child(a, !matches!(**a, Expr::Var(_) | Expr::Matrix(_)), o, d + 1)?;
                ind(o, d);
                o.push_str("</ml:apply>\n");
            }
            Expr::MatCol(m, j) => self.apply("ml:matcol", &[m, j], o, d)?,
        }
        Ok(())
    }

    fn call(&mut self, name: &str, args: &[Expr], o: &mut String, d: usize) -> Result<(), String> {
        fn refs(a: &[Expr]) -> Vec<&Expr> {
            a.iter().collect()
        }
        match (name, args.len()) {
            ("sqrt", 1) => self.apply("ml:sqrt", &refs(args), o, d),
            ("nthroot", 2) => {
                // the built-in takes (radicand, index); Mathcad stores (root index, radicand)
                self.apply("ml:nthRoot", &[&args[1], &args[0]], o, d)
            }
            ("transpose", 1) => self.apply("ml:transpose", &refs(args), o, d),
            ("conj", 1) => self.apply("ml:conjugate", &refs(args), o, d),
            ("polyroots", 1) => self.apply("ml:polyroots", &refs(args), o, d),
            ("xor", 2) => self.apply("ml:xor", &refs(args), o, d),
            ("sum", 1) => self.apply("ml:vectorSum", &refs(args), o, d),
            ("integrate", 4) => {
                let Expr::Var(v) = &args[1] else { return Err("переменная интегрирования должна быть именем".into()) };
                self.binding("ml:integral auto-algorithm=\"true\" algorithm=\"adaptive\"", v, &args[0], &[&args[2], &args[3]], o, d)
            }
            ("diff", 2) => {
                let Expr::Var(v) = &args[1] else { return Err("переменная дифференцирования должна быть именем".into()) };
                // a nest of first derivatives is one derivative of higher order
                let (mut body, mut order) = (&args[0], 1usize);
                while let Expr::Call(n, a) = body {
                    if n == "diff" && a.len() == 2 && a[1] == args[1] {
                        body = &a[0];
                        order += 1;
                    } else {
                        break;
                    }
                }
                // Mathcad writes the order as a `<degree>` element, not as an attribute
                if order > 1 {
                    self.binding_with_degree(v, body, order, o, d)
                } else {
                    self.binding("ml:derivative", v, body, &[], o, d)
                }
            }
            ("sum" | "prod", 4) => {
                let Expr::Var(v) = &args[0] else { return Err("индекс суммы должен быть именем".into()) };
                let head = if name == "sum" { "ml:summation" } else { "ml:product" };
                self.binding(head, v, &args[3], &[&args[1], &args[2]], o, d)
            }
            ("sum" | "prod", 2) if matches!(args[0], Expr::Var(_)) => {
                // a sum over a range variable: Mathcad writes the same `summation`/`product`
                // operator with a lambda and no bounds
                let Expr::Var(v) = &args[0] else { unreachable!() };
                let head = if name == "sum" { "ml:summation" } else { "ml:product" };
                self.binding(head, v, &args[1], &[], o, d)
            }
            ("Find" | "Minerr" | "Maximize" | "Minimize", _) => {
                ind(o, d);
                o.push_str("<ml:apply>\n");
                ind(o, d + 1);
                let method = if matches!(name, "Maximize" | "Minimize") { "conjugate" } else { "levenberg" };
                let _ = writeln!(o, "<ml:{name} auto-method=\"true\" method=\"{method}\" derivative-estimation=\"central\" variable-estimation=\"tangent\" linear-check=\"false\" multistart=\"false\" evolutionary=\"false\"/>");
                self.args(args, o, d + 1)?;
                ind(o, d);
                o.push_str("</ml:apply>\n");
                Ok(())
            }
            _ => {
                ind(o, d);
                o.push_str("<ml:apply>\n");
                id(name, o, d + 1);
                if !args.is_empty() {
                    self.args(args, o, d + 1)?;
                }
                ind(o, d);
                o.push_str("</ml:apply>\n");
                Ok(())
            }
        }
    }

    /// A derivative of order `n`: the same lambda as [`binding`], plus `<ml:degree>`.
    fn binding_with_degree(&mut self, var: &str, body: &Expr, order: usize, o: &mut String, d: usize) -> Result<(), String> {
        ind(o, d);
        o.push_str("<ml:apply>\n");
        ind(o, d + 1);
        o.push_str("<ml:derivative/>\n");
        ind(o, d + 1);
        o.push_str("<ml:lambda>\n");
        ind(o, d + 2);
        o.push_str("<ml:boundVars>\n");
        id(var, o, d + 3);
        ind(o, d + 2);
        o.push_str("</ml:boundVars>\n");
        self.expr(body, o, d + 2)?;
        ind(o, d + 1);
        o.push_str("</ml:lambda>\n");
        ind(o, d + 1);
        o.push_str("<ml:degree>\n");
        ind(o, d + 2);
        let _ = writeln!(o, "<ml:real>{order}</ml:real>");
        ind(o, d + 1);
        o.push_str("</ml:degree>\n");
        ind(o, d);
        o.push_str("</ml:apply>\n");
        Ok(())
    }

    fn binding(&mut self, head: &str, var: &str, body: &Expr, bounds: &[&Expr], o: &mut String, d: usize) -> Result<(), String> {
        ind(o, d);
        o.push_str("<ml:apply>\n");
        ind(o, d + 1);
        let _ = writeln!(o, "<{head}/>");
        ind(o, d + 1);
        o.push_str("<ml:lambda>\n");
        ind(o, d + 2);
        o.push_str("<ml:boundVars>\n");
        id(var, o, d + 3);
        ind(o, d + 2);
        o.push_str("</ml:boundVars>\n");
        self.expr(body, o, d + 2)?;
        ind(o, d + 1);
        o.push_str("</ml:lambda>\n");
        if !bounds.is_empty() {
            ind(o, d + 1);
            o.push_str("<ml:bounds>\n");
            for b in bounds {
                self.expr(b, o, d + 2)?;
            }
            ind(o, d + 1);
            o.push_str("</ml:bounds>\n");
        }
        ind(o, d);
        o.push_str("</ml:apply>\n");
        Ok(())
    }

    // -------------------------------------------------------- programs

    fn pstmts(&mut self, stmts: &[PStmt], o: &mut String, d: usize) -> Result<(), String> {
        for s in stmts {
            self.pstmt(s, o, d)?;
        }
        Ok(())
    }

    /// A branch or loop body: a lone simple statement stands for itself, anything else is a
    /// nested `<ml:program>`.
    fn body(&mut self, stmts: &[PStmt], o: &mut String, d: usize) -> Result<(), String> {
        match stmts {
            [one] if !matches!(one, PStmt::If { else_: Some(_), .. }) => self.pstmt(one, o, d),
            _ => {
                ind(o, d);
                o.push_str("<ml:program>\n");
                self.pstmts(stmts, o, d + 1)?;
                ind(o, d);
                o.push_str("</ml:program>\n");
                Ok(())
            }
        }
    }

    fn pstmt(&mut self, s: &PStmt, o: &mut String, d: usize) -> Result<(), String> {
        match s {
            PStmt::Assign(t, e) => {
                ind(o, d);
                o.push_str("<ml:localDefine>\n");
                self.target(t, o, d + 1)?;
                self.expr(e, o, d + 1)?;
                ind(o, d);
                o.push_str("</ml:localDefine>\n");
            }
            PStmt::Expr(e) => self.expr(e, o, d)?,
            PStmt::If { cond, then, else_ } => {
                ind(o, d);
                o.push_str("<ml:ifThen>\n");
                self.child(cond, true, o, d + 1)?;
                self.body(then, o, d + 1)?;
                ind(o, d);
                o.push_str("</ml:ifThen>\n");
                if let Some(e) = else_ {
                    ind(o, d);
                    o.push_str("<ml:otherwise>\n");
                    self.body(e, o, d + 1)?;
                    ind(o, d);
                    o.push_str("</ml:otherwise>\n");
                }
            }
            PStmt::For { var, iter, body } => {
                ind(o, d);
                o.push_str("<ml:for>\n");
                id(var, o, d + 1);
                self.expr(iter, o, d + 1)?;
                self.body(body, o, d + 1)?;
                ind(o, d);
                o.push_str("</ml:for>\n");
            }
            PStmt::While { cond, body } => {
                ind(o, d);
                o.push_str("<ml:while>\n");
                self.child(cond, true, o, d + 1)?;
                self.body(body, o, d + 1)?;
                ind(o, d);
                o.push_str("</ml:while>\n");
            }
            PStmt::Break => {
                ind(o, d);
                o.push_str("<ml:break/>\n");
            }
            PStmt::Continue => {
                ind(o, d);
                o.push_str("<ml:continue/>\n");
            }
            PStmt::Return(e) => {
                ind(o, d);
                o.push_str("<ml:return>\n");
                self.expr(e, o, d + 1)?;
                ind(o, d);
                o.push_str("</ml:return>\n");
            }
        }
        Ok(())
    }
}

fn ind(o: &mut String, d: usize) {
    for _ in 0..d {
        o.push('\t');
    }
}

/// `<ml:id>`; `x.max` becomes `x` with the literal subscript `max`.
fn id(name: &str, o: &mut String, d: usize) {
    ind(o, d);
    match name.split_once('.') {
        Some((base, sub)) if !base.is_empty() && !sub.is_empty() => {
            let _ = writeln!(o, "<ml:id xml:space=\"preserve\" subscript=\"{}\">{}</ml:id>", esc(sub), esc(base));
        }
        _ => {
            let _ = writeln!(o, "<ml:id xml:space=\"preserve\">{}</ml:id>", esc(name));
        }
    }
}

/// `<!--xmath:name {json}-->` with `--` escaped (it is not allowed inside XML comments; the
/// JSON reader turns `\u002d` back into `-`; a double hyphen can only occur inside strings).
fn xmath_comment(name: &str, json: &str) -> String {
    if json.is_empty() {
        return String::new();
    }
    let mut safe = String::with_capacity(json.len());
    let mut prev = ' ';
    for c in json.chars() {
        if c == '-' && prev == '-' {
            safe.push_str("\\u002d");
            prev = ' ';
        } else {
            safe.push(c);
            prev = c;
        }
    }
    format!("\t\t\t<!--xmath:{name} {safe}-->\n")
}

fn real_text(v: f64) -> String {
    format!("{v:?}")
}

/// SI base units of `dim` as a `<unitMonomial>`.
fn unit_monomial(dim: crate::units::Dim, o: &mut String, d: usize) {
    const NAMES: [&str; 7] = ["meter", "kilogram", "second", "ampere", "kelvin", "mole", "candela"];
    ind(o, d);
    o.push_str("<unitMonomial xmlns=\"http://schemas.mathsoft.com/units10\">\n");
    for (i, e) in dim.0.iter().enumerate() {
        if *e == 0 {
            continue;
        }
        let g = gcd(e.abs(), crate::units::DIM_SCALE);
        let (num, den) = (e / g, crate::units::DIM_SCALE / g);
        ind(o, d + 1);
        let _ = write!(o, "<unitReference unit=\"{}\"", NAMES[i]);
        if num != 1 {
            let _ = write!(o, " power-numerator=\"{num}\"");
        }
        if den != 1 {
            let _ = write!(o, " power-denominator=\"{den}\"");
        }
        o.push_str("/>\n");
    }
    ind(o, d);
    o.push_str("</unitMonomial>\n");
}

fn gcd(a: i32, b: i32) -> i32 {
    if b == 0 {
        a.max(1)
    } else {
        gcd(b, a % b)
    }
}

fn scalar(n: &crate::value::Num, o: &mut String, d: usize) -> bool {
    let (re, im) = (n.v.re, n.v.im);
    if !re.is_finite() || !im.is_finite() {
        return false;
    }
    ind(o, d);
    if im == 0.0 {
        let _ = writeln!(o, "<ml:real>{}</ml:real>", real_text(re));
    } else {
        o.push_str("<ml:complex>\n");
        ind(o, d + 1);
        let _ = writeln!(o, "<ml:real>{}</ml:real>", real_text(re));
        ind(o, d + 1);
        let _ = writeln!(o, "<ml:imag>{}</ml:imag>", real_text(im));
        ind(o, d);
        o.push_str("</ml:complex>\n");
    }
    true
}

/// `<result>` for a computed value; `None` when Mathcad could not store it (NaN, infinity).
fn result_xml(s: &Snapshot, d: usize) -> Option<String> {
    let mut o = String::new();
    ind(&mut o, d);
    o.push_str("<result xmlns=\"http://schemas.mathsoft.com/math30\">\n");
    let dim = s.data.first().map(|n| n.dim).unwrap_or(crate::units::Dim::NONE);
    if s.data.iter().any(|n| n.dim != dim) {
        return None;
    }
    let united = !dim.is_none() && s.text.is_none();
    let mut dd = d + 1;
    if united {
        ind(&mut o, dd);
        o.push_str("<unitedValue>\n");
        dd += 1;
    }
    if let Some(t) = &s.text {
        ind(&mut o, dd);
        let _ = writeln!(o, "<ml:str>{}</ml:str>", esc(t));
    } else if s.rows == 1 && s.cols == 1 && s.data.len() == 1 {
        if !scalar(&s.data[0], &mut o, dd) {
            return None;
        }
    } else {
        if s.data.len() != s.rows * s.cols || s.data.is_empty() {
            return None;
        }
        ind(&mut o, dd);
        let _ = writeln!(o, "<ml:matrix rows=\"{}\" cols=\"{}\">", s.rows, s.cols);
        for c in 0..s.cols {
            for r in 0..s.rows {
                if !scalar(&s.data[r * s.cols + c], &mut o, dd + 1) {
                    return None;
                }
            }
        }
        ind(&mut o, dd);
        o.push_str("</ml:matrix>\n");
    }
    if united {
        unit_monomial(dim, &mut o, dd);
        ind(&mut o, dd - 1);
        o.push_str("</unitedValue>\n");
    }
    ind(&mut o, d);
    o.push_str("</result>\n");
    Some(o)
}

/// Worksheet-wide xmath settings that `.xmcd` cannot hold exactly.
fn worksheet_json(ws: &Worksheet) -> String {
    serde_json::json!({ "format": ws.format, "plot_defaults": ws.plot_defaults, "page": ws.page }).to_string()
}

/// Escapes a line of text; spaces that XML whitespace handling would lose (runs, and spaces at the
/// ends of the line) are written as `<sp count="n"/>`, as Mathcad does.
fn esc_spaces(line: &str) -> String {
    let chars: Vec<char> = line.chars().collect();
    let mut o = String::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == ' ' {
            let start = i;
            while i < chars.len() && chars[i] == ' ' {
                i += 1;
            }
            let n = i - start;
            if n >= 2 || start == 0 || i == chars.len() {
                if n == 1 {
                    o.push_str("<sp/>");
                } else {
                    let _ = write!(o, "<sp count=\"{n}\"/>");
                }
            } else {
                o.push(' ');
            }
        } else {
            o.push_str(&esc(&chars[i].to_string()));
            i += 1;
        }
    }
    o
}

/// Width and height of a PNG file (from its header).
fn png_size(png: &[u8]) -> Option<(u32, u32)> {
    if png.len() < 24 || &png[..8] != b"\x89PNG\r\n\x1a\n" {
        return None;
    }
    let be = |i: usize| u32::from_be_bytes([png[i], png[i + 1], png[i + 2], png[i + 3]]);
    Some((be(16), be(20)))
}

/// A transparent 1×1 PNG (Mathcad files always carry binary content).
const ONE_PIXEL_PNG: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==";

/// A GUID for Mathcad's `identityInfo`, different for every save.
fn guid(ws: &Worksheet, salt: u64) -> String {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0).hash(&mut h);
    salt.hash(&mut h);
    ws.regions.len().hash(&mut h);
    let a = h.finish();
    a.rotate_left(17).hash(&mut h);
    let b = h.finish();
    format!(
        "{:08X}-{:04X}-4{:03X}-{:04X}-{:012X}",
        a >> 32,
        (a >> 16) & 0xffff,
        a & 0xfff,
        0x8000 | ((b >> 48) & 0x3fff),
        b & 0xffff_ffff_ffff
    )
}

