//! Export of a worksheet to a Mathcad 15 `.xmcd` file (XML formats `worksheet30` / `math30`).
//!
//! The inverse of [`crate::xmcd`]: math regions become `ml:` trees (definitions, evaluations,
//! function definitions with `boundVars`, matrices stored column by column, ranges, integrals,
//! derivatives, sums and products as `lambda` + `bounds`, programs, `vectorize`, display units
//! as `unitOverride`), text regions become `<text>` paragraphs, pictures become embedded PNGs
//! and references become `<reference>` regions.
//!
//! Plots are stored in a binary format by Mathcad (see `docs/xmcd-plot-format.md`) that is only
//! understood for reading: an exported plot is replaced by a text region naming its
//! expressions, and a warning is returned. Cached results (`<result>`) are not written;
//! Mathcad recalculates the worksheet when it opens it.

use crate::ast::{BinOp, Expr, PStmt, Stmt, Target, UnOp};
use crate::parser::parse_statement;
use crate::worksheet::{Paper, Region, RegionKind, TextStyle, Worksheet};
use std::fmt::Write;

/// Mathcad coordinates are in points; the worksheet uses pixels of 96 dpi.
const SCALE: f32 = 96.0 / 72.0;

/// The exported file and what could not be carried over.
#[derive(Debug, Clone)]
pub struct ExportResult {
    pub xml: String,
    pub warnings: Vec<String>,
}

/// Exports `ws` as `.xmcd` XML text.
pub fn export_xmcd(ws: &Worksheet) -> ExportResult {
    let mut x = Exporter { warnings: Vec::new(), items: Vec::new() };
    let mut out = String::new();
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"no\"?>\n");
    out.push_str(
        "<worksheet version=\"3.0.3\" xmlns=\"http://schemas.mathsoft.com/worksheet30\" \
         xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\" xmlns:ws=\"http://schemas.mathsoft.com/worksheet30\" \
         xmlns:ml=\"http://schemas.mathsoft.com/math30\" xmlns:u=\"http://schemas.mathsoft.com/units10\" \
         xmlns:p=\"http://schemas.mathsoft.com/provenance10\">\n",
    );
    out.push_str("\t<pointReleaseData/>\n\t<metadata>\n\t\t<generator>xmath</generator>\n\t\t<userData>\n\t\t\t<title/>\n\t\t\t<description/>\n\t\t\t<author/>\n\t\t\t<company/>\n\t\t\t<keywords/>\n\t\t\t<revisedBy/>\n\t\t</userData>\n\t</metadata>\n");
    x.settings(ws, &mut out);
    out.push_str("\t<regions>\n");
    // regions are written in evaluation order (the order Mathcad evaluates by position anyway)
    for i in ws.evaluation_order() {
        x.region(&ws.regions[i], &mut out);
    }
    out.push_str("\t</regions>\n");
    if !x.items.is_empty() {
        out.push_str("\t<binaryContent>\n");
        for (i, b64) in x.items.iter().enumerate() {
            let _ = write!(out, "\t\t<item item-id=\"{}\">", i + 1);
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
    let r = (v / SCALE * 100.0).round() / 100.0;
    format!("{r}")
}

struct Exporter {
    warnings: Vec<String>,
    /// Base64 payloads of `binaryContent` items (item id = index + 1).
    items: Vec<String>,
}

impl Exporter {
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
        for (n, f, w, s) in [
            ("Variables", "Times New Roman", "normal", "normal"),
            ("Constants", "Times New Roman", "normal", "normal"),
            ("User 1", "Arial", "normal", "normal"),
            ("Math Text Font", "Times New Roman", "normal", "normal"),
        ] {
            let _ = writeln!(out, "\t\t\t\t\t<mathStyle name=\"{n}\" font-family=\"{f}\" font-charset=\"0\" font-size=\"10\" font-weight=\"{w}\" font-style=\"{s}\" underline=\"false\"/>");
        }
        out.push_str("\t\t\t\t</mathStyles>\n");
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
        out.push_str("\t\t<fileFormat image-type=\"image/png\" image-quality=\"75\" save-numeric-results=\"true\" exclude-large-results=\"true\" save-text-images=\"false\" screen-dpi=\"96\"/>\n\t</settings>\n");
    }

    fn region_open(&self, r: &Region, w: f32, h: f32, out: &mut String) {
        let (ax, ay) = (r.x + r.anchor[0], r.y + r.anchor[1]);
        let _ = writeln!(
            out,
            "\t\t<region region-id=\"{}\" left=\"{}\" top=\"{}\" width=\"{}\" height=\"{}\" align-x=\"{}\" align-y=\"{}\" show-border=\"false\" show-highlight=\"false\" is-protected=\"false\" z-order=\"0\" background-color=\"inherit\" tag=\"\">",
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
                    if let Err(e) = self.stmt(&stmt, &mut body, 4) {
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
            RegionKind::Text { text, width, style, lock_width } => self.text_region(r, text, style, *width, *lock_width, out),
            RegionKind::Image { png_base64, width, height } => {
                let id = self.items.len() + 1;
                self.items.push(png_base64.clone());
                let kind = if png_base64.starts_with("/9j/") { "jpeg" } else { "png" };
                self.region_open(r, *width, *height, out);
                let _ = writeln!(
                    out,
                    "\t\t\t<picture>\n\t\t\t\t<{kind} item-idref=\"{id}\" display-width=\"{}\" display-height=\"{}\"/>\n\t\t\t</picture>\n\t\t\t<rendering item-idref=\"{id}\"/>\n\t\t</region>",
                    pt(*width),
                    pt(*height)
                );
            }
            RegionKind::Plot3d { kind, expr, width, .. } => {
                self.warnings.push(format!(
                    "область {}: {} не может быть записан в .xmcd (бинарный формат Mathcad); вместо него записан текст",
                    r.id,
                    kind.title()
                ));
                let what = format!("[{}: {}]", kind.title(), expr);
                self.text_region(r, &what, &TextStyle::default(), *width, true, out);
            }
            RegionKind::Plot { x, y, width, height, kind, .. } => {
                self.warnings.push(format!(
                    "область {}: график не может быть записан в .xmcd (бинарный формат Mathcad); вместо него записан текст",
                    r.id
                ));
                let what = if *kind == crate::worksheet::PlotKind::Polar {
                    format!("[Полярный график: r = {} от θ = {}]", y.join(", "), if x.is_empty() { "θ" } else { x })
                } else {
                    format!("[График: {} от {}]", y.join(", "), if x.is_empty() { "x" } else { x })
                };
                self.text_region(r, &what, &TextStyle::default(), *width, true, out);
                let _ = height;
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

    fn text_region(&mut self, r: &Region, text: &str, style: &TextStyle, width: f32, lock: bool, out: &mut String) {
        let lines: Vec<&str> = text.split('\n').collect();
        let h = lines.len() as f32 * style.size * 1.4 * SCALE + 4.0;
        self.region_open(r, width, h, out);
        let _ = writeln!(out, "\t\t\t<text use-page-width=\"false\" push-down=\"false\" lock-width=\"{lock}\">");
        for l in lines {
            let _ = write!(out, "\t\t\t\t<p style=\"Normal\" margin-left=\"inherit\" margin-right=\"inherit\" text-indent=\"inherit\" text-align=\"inherit\" list-style-type=\"inherit\" tabs=\"inherit\">");
            let mut s = esc(l);
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
                ind(o, d);
                o.push_str("</ml:eval>\n");
            }
            Stmt::Show(e) => {
                self.expr(e, o, d)?;
            }
        }
        Ok(())
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
            Expr::Range(a, b, c) => {
                ind(o, d);
                o.push_str("<ml:apply>\n");
                ind(o, d + 1);
                o.push_str("<ml:range/>\n");
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
                o.push_str("</ml:apply>\n");
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
            ("nthroot", 2) => self.apply("ml:nthRoot", &refs(args), o, d),
            ("transpose", 1) => self.apply("ml:transpose", &refs(args), o, d),
            ("conj", 1) => self.apply("ml:conjugate", &refs(args), o, d),
            ("polyroots", 1) => self.apply("ml:polyroots", &refs(args), o, d),
            ("xor", 2) => self.apply("ml:xor", &refs(args), o, d),
            ("sum", 1) => self.apply("ml:vectorSum", &refs(args), o, d),
            ("integrate", 4) => {
                let Expr::Var(v) = &args[1] else { return Err("переменная интегрирования должна быть именем".into()) };
                self.binding("ml:integral auto-algorithm=\"true\" algorithm=\"adaptive\"", v, &args[0], &[&args[2], &args[3]], o, d)
            }
            ("diff", 2) | ("diff", 3) if args.len() == 2 || matches!(args[2], Expr::Num(_)) => {
                let Expr::Var(v) = &args[1] else { return Err("переменная дифференцирования должна быть именем".into()) };
                let head = match args.get(2) {
                    Some(Expr::Num(n)) if *n != 1.0 => format!("ml:derivative order=\"{n}\""),
                    _ => "ml:derivative".to_string(),
                };
                self.binding(&head, v, &args[0], &[], o, d)
            }
            ("sum" | "prod", 4) => {
                let Expr::Var(v) = &args[0] else { return Err("индекс суммы должен быть именем".into()) };
                let head = if name == "sum" { "ml:summation" } else { "ml:product" };
                self.binding(head, v, &args[3], &[&args[1], &args[2]], o, d)
            }
            ("sum" | "prod", 2) if matches!(args[0], Expr::Var(_)) => {
                let Expr::Var(v) = &args[0] else { unreachable!() };
                let head = if name == "sum" { "ml:rangeSum" } else { "ml:rangeProduct" };
                self.binding(head, v, &args[1], &[], o, d)
            }
            ("Find" | "Minerr" | "Maximize" | "Minimize", _) => {
                ind(o, d);
                o.push_str("<ml:apply>\n");
                ind(o, d + 1);
                let _ = writeln!(o, "<ml:{name}/>");
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
