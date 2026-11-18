//! Two-dimensional math typesetting (fractions, powers, radicals, integrals,
//! matrices) in the style of Mathcad 15 worksheets.

use egui::epaint::text::{FontId, Fonts, Galley};
use egui::{Color32, Painter, Pos2, Rect, Shape, Stroke, Vec2};
use std::sync::Arc;
use xmath_core::ast::{BinOp, Expr, PStmt, Stmt, Target, UnOp};
use xmath_core::{NumberText, Piece, ResultView, UnitText};

#[derive(Clone)]
pub enum Item {
    Text { off: Vec2, galley: Arc<Galley>, color: Color32 },
    Line { a: Vec2, b: Vec2, w: f32, color: Color32 },
    Path { pts: Vec<Vec2>, w: f32, color: Color32 },
    /// A filled rectangle (placeholders).
    Rect { a: Vec2, b: Vec2, color: Color32 },
}

/// A meaningful node of a laid-out statement that can be clicked.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Node {
    /// A cell of a matrix literal. `mat` is the address of the `Expr::Matrix`
    /// while laying out; `inplace::resolve_boxes` turns it into the matrix
    /// ordinal of the statement.
    Cell { mat: usize, row: usize, col: usize },
    /// The target (name) of a definition.
    Target,
    /// The whole right-hand side of a definition.
    Rhs,
    /// Any sub-expression (recorded when `Ts::record` is on): the address of the `Expr`.
    Expr(usize),
}

/// A laid-out box. Coordinates are relative to the left end of the baseline;
/// `asc` extends upwards (negative y) and `desc` downwards.
#[derive(Clone, Default)]
pub struct Lay {
    pub w: f32,
    pub asc: f32,
    pub desc: f32,
    pub items: Vec<Item>,
    /// Hit boxes (relative to the baseline origin, like `items`).
    pub boxes: Vec<(Node, Rect)>,
}

impl Lay {
    pub fn h(&self) -> f32 {
        self.asc + self.desc
    }

    fn place(&mut self, other: Lay, dx: f32, dy: f32) {
        let d = Vec2::new(dx, dy);
        self.boxes.extend(other.boxes.into_iter().map(|(n, r)| (n, r.translate(d))));
        for it in other.items {
            self.items.push(match it {
                Item::Text { off, galley, color } => Item::Text { off: off + d, galley, color },
                Item::Line { a, b, w, color } => Item::Line { a: a + d, b: b + d, w, color },
                Item::Path { pts, w, color } => Item::Path { pts: pts.into_iter().map(|p| p + d).collect(), w, color },
                Item::Rect { a, b, color } => Item::Rect { a: a + d, b: b + d, color },
            });
        }
    }

    pub fn paint(&self, painter: &Painter, origin: Pos2) {
        let o = origin.to_vec2();
        for it in &self.items {
            match it {
                Item::Text { off, galley, color } => {
                    painter.galley((*off + o).to_pos2(), galley.clone(), *color);
                }
                Item::Line { a, b, w, color } => {
                    painter.line_segment([(*a + o).to_pos2(), (*b + o).to_pos2()], Stroke::new(*w, *color));
                }
                Item::Path { pts, w, color } => {
                    painter.add(Shape::line(pts.iter().map(|p| (*p + o).to_pos2()).collect(), Stroke::new(*w, *color)));
                }
                Item::Rect { a, b, color } => {
                    painter.rect_filled(Rect::from_min_max((*a + o).to_pos2(), (*b + o).to_pos2()), 0.0, *color);
                }
            }
        }
    }
}

pub const RED: Color32 = Color32::from_rgb(220, 0, 0);

/// Typesetter bound to the font system for one frame.
pub struct Ts<'a> {
    pub fonts: &'a Fonts,
    pub color: Color32,
    /// Matrix cell being edited in place: its address and the width of the
    /// editor box. The cell is laid out as an empty box of (at least) that width.
    pub edit_cell: Option<(usize, f32)>,
    /// Result of a symbolic evaluation (`expr →`) to draw after the arrow(s).
    pub sym: Option<Expr>,
    /// Record a hit box (`Node::Expr`) for every sub-expression (structural editing).
    pub record: bool,
    /// Colour of placeholders when it differs from the text colour.
    pub ph_color: Option<Color32>,
}

const ASC: f32 = 0.80;

impl<'a> Ts<'a> {
    pub fn new(fonts: &'a Fonts) -> Ts<'a> {
        Ts { fonts, color: Color32::BLACK, edit_cell: None, record: false, ph_color: None, sym: None }
    }

    pub fn text_c(&self, s: &str, size: f32, color: Color32) -> Lay {
        let galley = self.fonts.layout_no_wrap(s.to_string(), FontId::proportional(size), color);
        let sz = galley.size();
        let asc = sz.y * ASC;
        Lay {
            w: sz.x,
            asc,
            desc: sz.y - asc,
            items: vec![Item::Text { off: Vec2::new(0.0, -asc), galley, color }],
            boxes: vec![],
        }
    }

    pub fn text(&self, s: &str, size: f32) -> Lay {
        self.text_c(s, size, self.color)
    }

    fn space(&self, w: f32) -> Lay {
        Lay { w, ..Default::default() }
    }

    fn axis(&self, size: f32) -> f32 {
        size * 0.30
    }

    pub fn hcat(&self, parts: Vec<Lay>) -> Lay {
        let mut out = Lay::default();
        for p in parts {
            out.asc = out.asc.max(p.asc);
            out.desc = out.desc.max(p.desc);
            let w = p.w;
            let x = out.w;
            out.place(p, x, 0.0);
            out.w += w;
        }
        out
    }

    fn sup(&self, base: Lay, exp: Lay, size: f32) -> Lay {
        let shift = (base.asc - exp.asc * 0.55).max(size * 0.45);
        let mut out = Lay::default();
        out.w = base.w + exp.w + size * 0.05;
        // Mathcad reserves the exponent's full line box above the base
        out.asc = base.asc.max(shift + exp.asc) + size * 0.3;
        out.desc = base.desc.max(exp.desc - shift);
        let bw = base.w;
        out.place(base, 0.0, 0.0);
        out.place(exp, bw + size * 0.05, -shift);
        out
    }

    fn sub(&self, base: Lay, sub: Lay, size: f32) -> Lay {
        let shift = size * 0.12;
        let mut out = Lay::default();
        out.w = base.w + sub.w;
        out.asc = base.asc.max(sub.asc - shift);
        out.desc = base.desc.max(sub.desc + shift);
        let bw = base.w;
        out.place(base, 0.0, 0.0);
        out.place(sub, bw, shift);
        out
    }

    fn frac(&self, num: Lay, den: Lay, size: f32) -> Lay {
        let pad = size * 0.15;
        let gap = size * 0.2;
        let axis = self.axis(size);
        let w = num.w.max(den.w) + 2.0 * pad;
        let mut out = Lay { w, asc: axis + gap + num.h(), desc: (den.h() + gap - axis).max(0.0), items: vec![], boxes: vec![] };
        let (nw, nd) = (num.w, num.desc);
        let (dw, da) = (den.w, den.asc);
        out.place(num, (w - nw) / 2.0, -axis - gap - nd);
        out.place(den, (w - dw) / 2.0, -axis + gap + da);
        out.items.push(Item::Line {
            a: Vec2::new(pad * 0.5, -axis),
            b: Vec2::new(w - pad * 0.5, -axis),
            w: (size / 14.0).max(1.0),
            color: self.color,
        });
        out
    }

    fn radical(&self, inner: Lay, index: Option<Lay>, size: f32) -> Lay {
        let gap = size * 0.12;
        let tick = size * 0.55;
        let ix = index.as_ref().map(|l| (l.w - tick * 0.45).max(0.0)).unwrap_or(0.0);
        let top = -inner.asc - gap;
        let bottom = inner.desc;
        let x0 = ix;
        let w = x0 + tick + inner.w + size * 0.15;
        let mid = (top + bottom) * 0.5 + (bottom - top) * 0.15;
        let pts = vec![
            Vec2::new(x0, mid),
            Vec2::new(x0 + tick * 0.25, mid - size * 0.08),
            Vec2::new(x0 + tick * 0.55, bottom),
            Vec2::new(x0 + tick, top),
            Vec2::new(w, top),
        ];
        let mut out = Lay { w, asc: -top + 1.0, desc: bottom, items: vec![], boxes: vec![] };
        let iw = inner.w;
        let _ = iw;
        out.place(inner, x0 + tick + size * 0.05, 0.0);
        out.items.push(Item::Path { pts, w: (size / 14.0).max(1.0), color: self.color });
        if let Some(idx) = index {
            let y = mid - size * 0.2 - idx.desc;
            out.asc = out.asc.max(-y + idx.asc);
            out.place(idx, 0.0, y);
        }
        out
    }

    /// Wraps `inner` in a pair of delimiters: '(', '[', '|'.
    pub fn fence(&self, inner: Lay, kind: char, size: f32) -> Lay {
        let top = -inner.asc - size * 0.05;
        let bottom = inner.desc + size * 0.05;
        let h = bottom - top;
        let lw = (size / 14.0).max(1.0);
        let small = h <= size * 1.3;
        if small && kind == '(' {
            let l = self.text("(", size);
            let r = self.text(")", size);
            return self.hcat(vec![l, inner, r]);
        }
        let dw = match kind {
            '(' => (size * 0.12 + h * 0.06).min(size * 0.5),
            '[' => size * 0.5,
            _ => size * 0.2,
        };
        let pad = size * 0.08;
        let mut out = Lay { w: 2.0 * dw + 2.0 * pad + inner.w, asc: -top + 1.0, desc: bottom + 1.0, items: vec![], boxes: vec![] };
        let right = out.w;
        let mut side = |x0: f32, dir: f32| match kind {
            '(' => {
                let n = 24;
                let pts = (0..=n)
                    .map(|k| {
                        let t = k as f32 / n as f32;
                        let s = (t * std::f32::consts::PI).sin();
                        Vec2::new(x0 + dir * dw * 0.8 * (1.0 - s) - dir * dw * 0.4, top + h * t)
                    })
                    .collect();
                out.items.push(Item::Path { pts, w: lw, color: self.color });
            }
            '[' => {
                let xi = x0 - dir * dw * 0.35;
                let xo = x0 + dir * dw * 0.35;
                out.items.push(Item::Path {
                    pts: vec![Vec2::new(xo, top), Vec2::new(xi, top), Vec2::new(xi, bottom), Vec2::new(xo, bottom)],
                    w: lw,
                    color: self.color,
                });
            }
            _ => {
                out.items.push(Item::Line { a: Vec2::new(x0, top), b: Vec2::new(x0, bottom), w: lw, color: self.color });
            }
        };
        side(dw * 0.5 + pad * 0.5, 1.0);
        side(right - dw * 0.5 - pad * 0.5, -1.0);
        out.place(inner, dw + pad, 0.0);
        out
    }

    pub fn matrix(&self, cells: Vec<Vec<Lay>>, size: f32) -> Lay {
        self.matrix_id(cells, size, None)
    }

    /// `id`: address of the matrix literal; hit boxes of its cells are recorded.
    pub fn matrix_id(&self, cells: Vec<Vec<Lay>>, size: f32, id: Option<usize>) -> Lay {
        let rows = cells.len();
        let cols = cells.first().map_or(0, |r| r.len());
        let hgap = size * 0.9;
        let vgap = size * 0.47;
        let mut colw = vec![0f32; cols];
        let mut rasc = vec![0f32; rows];
        let mut rdesc = vec![0f32; rows];
        for (i, row) in cells.iter().enumerate() {
            for (j, c) in row.iter().enumerate() {
                colw[j] = colw[j].max(c.w);
                rasc[i] = rasc[i].max(c.asc);
                rdesc[i] = rdesc[i].max(c.desc);
            }
        }
        let total_h: f32 = (0..rows).map(|i| rasc[i] + rdesc[i]).sum::<f32>() + vgap * rows.saturating_sub(1) as f32;
        let total_w: f32 = colw.iter().sum::<f32>() + hgap * cols.saturating_sub(1) as f32;
        let axis = self.axis(size);
        let top = -axis - total_h / 2.0;
        let mut out = Lay { w: total_w, asc: -top, desc: total_h + top, items: vec![], boxes: vec![] };
        let mut y = top;
        for (i, row) in cells.into_iter().enumerate() {
            let base = y + rasc[i];
            let mut x = 0.0;
            for (j, c) in row.into_iter().enumerate() {
                let cw = c.w;
                // numbers are right-aligned in Mathcad matrices
                if let Some(mat) = id {
                    let r = Rect::from_min_max(Pos2::new(x, base - rasc[i]), Pos2::new(x + colw[j], base + rdesc[i]));
                    out.boxes.push((Node::Cell { mat, row: i, col: j }, r));
                }
                out.place(c, x + (colw[j] - cw), base);
                x += colw[j] + hgap;
            }
            y += rasc[i] + rdesc[i] + vgap;
        }
        self.fence(out, '[', size)
    }

    /// Big operator (Σ, Π, ∫) with optional limits above/below.
    fn bigop(&self, sym: &str, size: f32, above: Option<Lay>, below: Option<Lay>) -> Lay {
        let s = self.text(sym, size * 1.6);
        let axis = self.axis(size);
        // centre the symbol on the math axis
        let sh = s.h();
        let s_base = -axis + sh / 2.0 - s.desc;
        let w = s.w.max(above.as_ref().map_or(0.0, |l| l.w)).max(below.as_ref().map_or(0.0, |l| l.w));
        let mut out = Lay { w, asc: axis + sh / 2.0, desc: (sh / 2.0 - axis).max(0.0), items: vec![], boxes: vec![] };
        let sw = s.w;
        out.place(s, (w - sw) / 2.0, s_base);
        if let Some(a) = above {
            let y = -axis - sh / 2.0 - a.desc;
            out.asc = out.asc.max(-y + a.asc);
            let aw = a.w;
            out.place(a, (w - aw) / 2.0, y);
        }
        if let Some(b) = below {
            let y = -axis + sh / 2.0 + b.asc;
            out.desc = out.desc.max(y + b.desc);
            let bw = b.w;
            out.place(b, (w - bw) / 2.0, y);
        }
        out
    }

    // ---------------------------------------------------------------------
    // Expressions

    pub fn ident(&self, name: &str, size: f32) -> Lay {
        if name == "■" {
            return self.placeholder(size);
        }
        // numbers typed in the structural editor ("1.5") are not subscripted
        if name.starts_with(|c: char| c.is_ascii_digit()) {
            return self.text(name, size);
        }
        // like in Mathcad only a dot starts a literal subscript; an underscore is an ordinary letter
        let split = name.char_indices().skip(1).find(|(_, c)| *c == '.');
        match split {
            Some((i, _)) if i + 1 < name.len() => {
                let base = self.text(&name[..i], size);
                let sub = self.text(&name[i + 1..], size * 0.72);
                self.sub(base, sub, size)
            }
            _ => self.text(name, size),
        }
    }

    /// Mathcad's empty placeholder: a small black square.
    pub fn placeholder(&self, size: f32) -> Lay {
        let t = self.text("x", size);
        let side = size * 0.5;
        Lay {
            w: side + size * 0.2,
            asc: t.asc,
            desc: t.desc,
            items: vec![Item::Rect { a: Vec2::new(size * 0.1, -side - size * 0.04), b: Vec2::new(size * 0.1 + side, -size * 0.04), color: self.ph_color.unwrap_or(self.color) }],
            boxes: vec![],
        }
    }

    /// A number literal; very large/small values are shown as `m·10ⁿ` with a real superscript.
    fn literal(&self, x: f64, size: f32) -> Lay {
        if x.is_infinite() || x >= 1e300 {
            self.text("∞", size)
        } else if x != 0.0 && (x.abs() >= 1e15 || x.abs() < 1e-5) {
            let s = format!("{x:e}");
            let (m, e) = s.split_once('e').unwrap_or((&s, "0"));
            let ten = self.sup(self.text("10", size), self.text(&e.replace('-', "−"), size * 0.75), size);
            if m == "1" {
                ten
            } else {
                self.hcat(vec![self.text(&m.replace('-', "−"), size), self.text("·", size), ten])
            }
        } else {
            self.text(&format!("{x}").replace('-', "−"), size)
        }
    }

    fn paren_if(&self, l: Lay, cond: bool, size: f32) -> Lay {
        if cond {
            self.fence(l, '(', size)
        } else {
            l
        }
    }

    fn list(&self, items: &[Expr], size: f32) -> Lay {
        let mut parts = Vec::new();
        for (i, a) in items.iter().enumerate() {
            if i > 0 {
                parts.push(self.text(", ", size));
            }
            parts.push(self.expr(a, size));
        }
        self.hcat(parts)
    }

    fn op(&self, s: &str, size: f32) -> Lay {
        let pad = self.space(size * 0.18);
        let t = self.text(s, size);
        self.hcat(vec![pad.clone(), t, pad])
    }

    pub fn expr(&self, e: &Expr, size: f32) -> Lay {
        let mut l = self.expr_inner(e, size);
        if self.record {
            let r = Rect::from_min_max(Pos2::new(0.0, -l.asc), Pos2::new(l.w, l.desc));
            l.boxes.push((Node::Expr(e as *const Expr as usize), r));
        }
        l
    }

    fn expr_inner(&self, e: &Expr, size: f32) -> Lay {
        match e {
            Expr::Num(x) => self.literal(*x, size),
            Expr::Imag(x) => self.hcat(vec![self.literal(*x, size), self.text("i", size)]),
            Expr::Str(s) => self.text(&format!("\"{s}\""), size),
            Expr::Var(n) if n.eq_ignore_ascii_case("given") => self.bold(self.text(n, size)),
            Expr::Var(n) => self.ident(n, size),
            Expr::Unary(UnOp::Neg, a) => {
                let inner = self.expr(a, size);
                let inner = self.paren_if(inner, a.precedence() < 6, size);
                self.hcat(vec![self.text("−", size), inner])
            }
            Expr::Unary(UnOp::Not, a) => {
                let inner = self.expr(a, size);
                let inner = self.paren_if(inner, a.precedence() < 4, size);
                self.hcat(vec![self.text("¬", size), inner])
            }
            Expr::Unary(UnOp::Factorial, a) => {
                let inner = self.expr(a, size);
                let inner = self.paren_if(inner, a.precedence() < 9, size);
                self.hcat(vec![inner, self.text("!", size)])
            }
            Expr::Binary(BinOp::Div, a, b) => self.frac(self.expr(a, size), self.expr(b, size), size),
            Expr::Binary(BinOp::Pow, a, b) => {
                let base = self.expr(a, size);
                let needs = a.precedence() < 9 || matches!(**a, Expr::Binary(BinOp::Div, ..) | Expr::Binary(BinOp::Pow, ..));
                let base = self.paren_if(base, needs, size);
                let exp = self.expr(b, size * 0.75);
                self.sup(base, exp, size)
            }
            Expr::Binary(op, a, b) => {
                let p = op.precedence();
                let la = self.expr(a, size);
                let la = self.paren_if(la, a.precedence() < p && !is_frac(a), size);
                let lb = self.expr(b, size);
                let right_paren = match op {
                    BinOp::Sub => b.precedence() <= p && !is_frac(b),
                    BinOp::Mul => (b.precedence() < p || matches!(**b, Expr::Unary(UnOp::Neg, _))) && !is_frac(b),
                    _ => b.precedence() < p && !is_frac(b),
                };
                let lb = self.paren_if(lb, right_paren, size);
                let sym = match op {
                    BinOp::Add => self.op("+", size),
                    BinOp::Sub => self.op("−", size),
                    BinOp::Mul => self.text("·", size),
                    BinOp::Eq => self.op("=", size), // drawn bold-ish below
                    BinOp::Ne => self.op("≠", size),
                    BinOp::Lt => self.op("<", size),
                    BinOp::Gt => self.op(">", size),
                    BinOp::Le => self.op("≤", size),
                    BinOp::Ge => self.op("≥", size),
                    BinOp::And => self.op("∧", size),
                    BinOp::Or => self.op("∨", size),
                    BinOp::Div | BinOp::Pow => unreachable!(),
                };
                let sym = if *op == BinOp::Eq { self.bold(sym) } else { sym };
                self.hcat(vec![la, sym, lb])
            }
            Expr::Call(name, args) => self.call(name, args, size),
            Expr::Index(base, idx) => {
                let b = self.expr(base, size);
                let b = self.paren_if(b, base.precedence() < 9, size);
                let s = self.list(idx, size * 0.72);
                self.sub(b, s, size)
            }
            Expr::Matrix(rows) => {
                let cells = rows
                    .iter()
                    .map(|r| {
                        r.iter()
                            .map(|c| {
                                let l = self.expr(c, size);
                                match self.edit_cell {
                                    Some((p, w)) if p == c as *const Expr as usize => Lay {
                                        w: l.w.max(w),
                                        asc: l.asc.max(size * ASC),
                                        desc: l.desc.max(size * 0.25),
                                        ..Default::default()
                                    },
                                    _ => l,
                                }
                            })
                            .collect()
                    })
                    .collect();
                self.matrix_id(cells, size, Some(e as *const Expr as usize))
            }
            Expr::Range(a, b, c) => {
                let mut parts = vec![self.expr(a, size)];
                if let Some(b) = b {
                    parts.push(self.text(",", size));
                    parts.push(self.space(size * 0.25));
                    parts.push(self.expr(b, size));
                }
                parts.push(self.text("..", size));
                parts.push(self.expr(c, size));
                self.hcat(parts)
            }
            Expr::Abs(a) => self.fence(self.expr(a, size), '|', size),
            Expr::Program(stmts) => self.program(stmts, size),
            Expr::Vectorize(a) => self.vector_arrow(self.expr(a, size), size),
            Expr::MatCol(m, j) => {
                let b = self.expr(m, size);
                let b = self.paren_if(b, m.precedence() < 9, size);
                let small = size * 0.75;
                let idx = self.hcat(vec![self.text("<", small), self.expr(j, small), self.text(">", small)]);
                self.sup(b, idx, size)
            }
            Expr::OnError(a, b) => {
                let la = self.expr(a, size);
                let lb = self.expr(b, size);
                self.hcat(vec![la, self.kw(" on error ", size), lb])
            }
        }
    }

    /// Mathcad's vectorize operator: a right arrow drawn above the expression.
    fn vector_arrow(&self, inner: Lay, size: f32) -> Lay {
        let gap = size * 0.14;
        let head = size * 0.16;
        let y = -inner.asc - gap;
        let w = inner.w.max(size * 0.5);
        let lw = (size / 14.0).max(1.0);
        let mut out = Lay { w, asc: -y + head + 1.0, desc: inner.desc, items: vec![], boxes: vec![] };
        let off = (w - inner.w) / 2.0;
        out.place(inner, off, 0.0);
        let (a, b) = (Vec2::new(size * 0.05, y), Vec2::new(w - size * 0.05, y));
        out.items.push(Item::Line { a, b, w: lw, color: self.color });
        out.items.push(Item::Path {
            pts: vec![Vec2::new(b.x - head, y - head * 0.7), b, Vec2::new(b.x - head, y + head * 0.7)],
            w: lw,
            color: self.color,
        });
        out
    }

    /// A bold keyword (padded with the spaces given in `s`).
    fn kw(&self, s: &str, size: f32) -> Lay {
        self.bold(self.text(s, size))
    }

    /// Mathcad program: a vertical bar on the left, one statement per line.
    pub fn program(&self, stmts: &[PStmt], size: f32) -> Lay {
        let rows = self.program_rows(stmts, size);
        let vgap = size * 0.22;
        let total_h: f32 = rows.iter().map(|r| r.h()).sum::<f32>() + vgap * rows.len().saturating_sub(1) as f32;
        let axis = self.axis(size);
        let top = -axis - total_h / 2.0;
        let indent = size * 0.55;
        let width = rows.iter().map(|r| r.w).fold(0.0f32, f32::max);
        let mut out = Lay { w: indent + width + size * 0.2, asc: -top + size * 0.1, desc: total_h + top + size * 0.1, items: vec![], boxes: vec![] };
        let mut y = top;
        for r in rows {
            let base = y + r.asc;
            y += r.h() + vgap;
            out.place(r, indent, base);
        }
        out.items.push(Item::Line {
            a: Vec2::new(size * 0.15, top - size * 0.05),
            b: Vec2::new(size * 0.15, top + total_h + size * 0.05),
            w: (size / 14.0).max(1.0),
            color: self.color,
        });
        out
    }

    fn block_row(&self, body: &[PStmt], size: f32) -> Lay {
        self.hcat(vec![self.space(size * 0.9), self.program(body, size)])
    }

    fn program_rows(&self, stmts: &[PStmt], size: f32) -> Vec<Lay> {
        let mut rows = Vec::new();
        for s in stmts {
            match s {
                PStmt::Assign(t, e) => {
                    let lt = self.target(t, size);
                    rows.push(self.hcat(vec![lt, self.op("←", size), self.expr(e, size)]));
                }
                PStmt::Expr(e) => rows.push(self.expr(e, size)),
                PStmt::If { cond, then, else_ } => {
                    if else_.is_none() && then.len() == 1 && then[0].is_simple() {
                        let simple = self.program_rows(then, size).remove(0);
                        rows.push(self.hcat(vec![simple, self.kw(" if ", size), self.expr(cond, size)]));
                    } else {
                        rows.push(self.hcat(vec![self.kw("if ", size), self.expr(cond, size)]));
                        rows.push(self.block_row(then, size));
                        if let Some(e) = else_ {
                            rows.push(self.kw("otherwise", size));
                            rows.push(self.block_row(e, size));
                        }
                    }
                }
                PStmt::For { var, iter, body } => {
                    rows.push(self.hcat(vec![
                        self.kw("for ", size),
                        self.ident(var, size),
                        self.kw(" ∈ ", size),
                        self.expr(iter, size),
                    ]));
                    rows.push(self.block_row(body, size));
                }
                PStmt::While { cond, body } => {
                    rows.push(self.hcat(vec![self.kw("while ", size), self.expr(cond, size)]));
                    rows.push(self.block_row(body, size));
                }
                PStmt::Break => rows.push(self.kw("break", size)),
                PStmt::Continue => rows.push(self.kw("continue", size)),
                PStmt::Return(e) => rows.push(self.hcat(vec![self.kw("return ", size), self.expr(e, size)])),
            }
        }
        rows
    }

    fn bold(&self, mut l: Lay) -> Lay {
        let extra: Vec<Item> = l
            .items
            .iter()
            .filter_map(|it| match it {
                Item::Text { off, galley, color } => {
                    Some(Item::Text { off: *off + Vec2::new(0.7, 0.0), galley: galley.clone(), color: *color })
                }
                _ => None,
            })
            .collect();
        l.items.extend(extra);
        l
    }

    fn call(&self, name: &str, args: &[Expr], size: f32) -> Lay {
        let small = size * 0.72;
        match (name, args) {
            // explicit parentheses typed in the structural editor
            ("", [a]) => self.fence(self.expr(a, size), '(', size),
            ("sqrt", [a]) => self.radical(self.expr(a, size), None, size),
            ("nthroot", [a, n]) => self.radical(self.expr(a, size), Some(self.expr(n, small)), size),
            ("abs", [a]) => self.fence(self.expr(a, size), '|', size),
            ("exp", [a]) if !matches!(a, Expr::Matrix(_)) => {
                self.sup(self.text("e", size), self.expr(a, size * 0.75), size)
            }
            ("transpose", [a]) => {
                let b = self.expr(a, size);
                let b = self.paren_if(b, a.precedence() < 9, size);
                self.sup(b, self.text("T", size * 0.75), size)
            }
            ("integrate" | "∫", [f, x, a, b]) => {
                let op = self.bigop("∫", size, Some(self.expr(b, small)), Some(self.expr(a, small)));
                let body = self.expr(f, size);
                let body = self.paren_if(body, f.precedence() < 5, size);
                self.hcat(vec![op, self.space(size * 0.15), body, self.text(" d", size), self.expr(x, size)])
            }
            ("integrate" | "∫", [f, x]) => {
                let op = self.bigop("∫", size, None, None);
                let body = self.expr(f, size);
                let body = self.paren_if(body, f.precedence() < 5, size);
                self.hcat(vec![op, self.space(size * 0.15), body, self.text(" d", size), self.expr(x, size)])
            }
            ("lim" | "limit", [f, x, a, rest @ ..]) => {
                let sign = match rest.first() {
                    Some(Expr::Num(d)) if *d > 0.0 => "+",
                    Some(Expr::Num(d)) if *d < 0.0 => "−",
                    Some(Expr::Unary(UnOp::Neg, _)) => "−",
                    _ => "",
                };
                let mut below = vec![self.expr(x, small), self.text(" → ", small), self.expr(a, small)];
                if !sign.is_empty() {
                    below.push(self.text(sign, small * 0.85));
                }
                let word = self.text("lim", size);
                let below = self.hcat(below);
                let w = word.w.max(below.w);
                let mut stack = Lay { w, asc: word.asc, desc: word.desc + below.asc + below.desc + size * 0.1, items: vec![], boxes: vec![] };
                let (ww, bw) = (word.w, below.w);
                let by = word.desc + below.asc + size * 0.1;
                stack.place(word, (w - ww) / 2.0, 0.0);
                stack.place(below, (w - bw) / 2.0, by);
                let body = self.expr(f, size);
                let body = self.paren_if(body, f.precedence() < 5, size);
                self.hcat(vec![stack, self.space(size * 0.15), body])
            }
            ("diff", [f, x, ..]) => {
                let dx = self.hcat(vec![self.text("d", size), self.expr(x, size)]);
                let d = self.frac(self.text("d", size), dx, size);
                let body = self.expr(f, size);
                let body = self.paren_if(body, f.precedence() < 5, size);
                self.hcat(vec![d, body])
            }
            ("sum" | "Σ" | "prod" | "Π", [k, a, b, f]) => {
                let sym = if name == "sum" || name == "Σ" { "Σ" } else { "Π" };
                let below = self.hcat(vec![self.expr(k, small), self.text(" = ", small), self.expr(a, small)]);
                let op = self.bigop(sym, size, Some(self.expr(b, small)), Some(below));
                let body = self.expr(f, size);
                let body = self.paren_if(body, f.precedence() < 5, size);
                self.hcat(vec![op, self.space(size * 0.1), body])
            }
            ("sum" | "Σ" | "prod" | "Π", [k, f]) => {
                let sym = if name == "sum" || name == "Σ" { "Σ" } else { "Π" };
                let op = self.bigop(sym, size, None, Some(self.expr(k, small)));
                let body = self.expr(f, size);
                let body = self.paren_if(body, f.precedence() < 5, size);
                self.hcat(vec![op, self.space(size * 0.1), body])
            }
            _ => {
                let n = self.ident(name, size);
                let a = self.list(args, size);
                self.hcat(vec![n, self.fence(a, '(', size)])
            }
        }
    }

    fn target(&self, t: &Target, size: f32) -> Lay {
        match t {
            Target::Var(n) => self.ident(n, size),
            Target::Func(n, params) => {
                let mut parts = Vec::new();
                for (i, p) in params.iter().enumerate() {
                    if i > 0 {
                        parts.push(self.text(", ", size));
                    }
                    parts.push(self.ident(p, size));
                }
                let args = self.hcat(parts);
                self.hcat(vec![self.ident(n, size), self.fence(args, '(', size)])
            }
            Target::Indexed(n, idx) => self.sub(self.ident(n, size), self.list(idx, size * 0.72), size),
            Target::Vector(names) => {
                self.matrix(names.iter().map(|n| vec![self.ident(n, size)]).collect(), size)
            }
        }
    }

    // ---------------------------------------------------------------------
    // Results

    pub fn number(&self, n: &NumberText, size: f32) -> Lay {
        let mut parts = Vec::new();
        for p in &n.0 {
            match p {
                Piece::Text(t) => parts.push(self.text(&t.replace('-', "−"), size)),
                Piece::Pow10(e) => {
                    parts.push(self.text(" × ", size));
                    let ten = self.text("10", size);
                    let exp = self.text(&e.to_string().replace('-', "−"), size * 0.75);
                    let ten_asc = ten.asc;
                    let mut l = self.sup(ten, exp, size);
                    // results do not reserve the extra exponent box (matrix rows stay compact)
                    l.asc = l.asc.min(ten_asc + size * 0.05);
                    parts.push(l);
                }
            }
        }
        self.hcat(parts)
    }

    fn unit_factor(&self, name: &str, num: i32, den: i32, size: f32) -> Lay {
        let base = self.ident(name, size);
        if num == 1 && den == 1 {
            return base;
        }
        let e = if den == 1 { num.to_string() } else { format!("{num}/{den}") };
        self.sup(base, self.text(&e, size * 0.75), size)
    }

    pub fn units(&self, u: &UnitText, size: f32) -> Lay {
        let product = |items: Vec<(String, i32, i32)>| {
            let mut parts = Vec::new();
            for (i, (n, a, b)) in items.into_iter().enumerate() {
                if i > 0 {
                    parts.push(self.text("·", size));
                }
                parts.push(self.unit_factor(&n, a, b, size));
            }
            self.hcat(parts)
        };
        let pos: Vec<_> = u.0.iter().filter(|p| p.num > 0).map(|p| (p.name.clone(), p.num, p.den)).collect();
        let neg: Vec<_> = u.0.iter().filter(|p| p.num < 0).map(|p| (p.name.clone(), -p.num, p.den)).collect();
        let num = if pos.is_empty() { self.text("1", size) } else { product(pos) };
        if neg.is_empty() {
            num
        } else {
            self.frac(num, product(neg), size)
        }
    }

    pub fn result(&self, v: &ResultView, size: f32) -> Lay {
        let with_units = |l: Lay, u: &UnitText| {
            if u.is_empty() {
                l
            } else {
                self.hcat(vec![l, self.space(size * 0.3), self.units(u, size)])
            }
        };
        match v {
            ResultView::Scalar { number, units } => with_units(self.number(number, size), units),
            ResultView::Matrix { rows, cols, cells, cell_units, units } => {
                const MAX_ROWS: usize = 16;
                let shown = (*rows).min(MAX_ROWS);
                let mut grid = Vec::new();
                for r in 0..shown {
                    let mut row = Vec::new();
                    for c in 0..*cols {
                        let k = r * cols + c;
                        let mut l = self.number(&cells[k], size);
                        if let Some(cu) = cell_units {
                            l = with_units(l, &cu[k]);
                        }
                        row.push(l);
                    }
                    grid.push(row);
                }
                if *rows > MAX_ROWS {
                    grid.push((0..*cols).map(|_| self.text("⋮", size)).collect());
                }
                with_units(self.matrix(grid, size), units)
            }
            ResultView::Text(s) => self.text(&format!("\"{s}\""), size),
            ResultView::Function(s) => self.text(s, size),
        }
    }

    /// ` = result unit` after an evaluated expression (a placeholder while there is no result).
    fn shown_tail(&self, result: Option<(&ResultView, Option<&Expr>)>, unit: Option<&Expr>, size: f32) -> Vec<Lay> {
        let mut parts = vec![self.op("=", size)];
        match result {
            Some((view, user_unit)) => {
                parts.push(self.result(view, size));
                if let Some(u) = user_unit.or(unit) {
                    parts.push(self.space(size * 0.3));
                    parts.push(self.expr(u, size));
                }
            }
            None => {
                if let Some(u) = unit {
                    parts.push(self.text("■ ", size));
                    parts.push(self.expr(u, size));
                } else {
                    parts.push(self.text("■", size));
                }
            }
        }
        parts
    }

    /// A statement being edited structurally: `lhs` (with the global flag) `:=` `body`, and the
    /// value after `=` when `shown`. Every sub-expression records a hit box when `record` is on.
    pub fn statement_ed(
        &self,
        lhs: Option<(&Expr, bool)>,
        body: &Expr,
        shown: Option<Option<&Expr>>,
        result: Option<(&ResultView, Option<&Expr>)>,
        size: f32,
    ) -> Lay {
        let mut parts = Vec::new();
        if let Some((l, global)) = lhs {
            parts.push(self.expr(l, size));
            parts.push(self.op(if global { "≡" } else { ":=" }, size));
        }
        parts.push(self.expr(body, size));
        if let Some(unit) = shown {
            parts.extend(self.shown_tail(result, unit, size));
        }
        self.hcat(parts)
    }

    /// Lays out a whole math region.
    pub fn statement(&self, stmt: &Stmt, result: Option<(&ResultView, Option<&Expr>)>, size: f32) -> Lay {
        match stmt {
            Stmt::Define { target, expr, global, show } => {
                let sym = if *global { "≡" } else { ":=" };
                let mut lt = self.target(target, size);
                if let Some((0, w)) = self.edit_cell {
                    // definition target being edited in place (see `inplace`)
                    lt = Lay { w: lt.w.max(w), asc: lt.asc.max(size * ASC), desc: lt.desc.max(size * 0.25), ..Default::default() };
                }
                let lo = self.op(sym, size);
                let lr = self.expr(expr, size);
                let (tr, rr) = (
                    Rect::from_min_max(Pos2::new(0.0, -lt.asc), Pos2::new(lt.w, lt.desc)),
                    Rect::from_min_max(Pos2::new(lt.w + lo.w, -lr.asc), Pos2::new(lt.w + lo.w + lr.w, lr.desc)),
                );
                let mut parts = vec![lt, lo, lr];
                if let Some(unit) = show {
                    parts.extend(self.shown_tail(result, unit.as_ref(), size));
                }
                let mut out = self.hcat(parts);
                out.boxes.push((Node::Target, tr));
                out.boxes.push((Node::Rhs, rr));
                out
            }
            Stmt::Eval { expr, unit } => {
                let mut parts = vec![self.expr(expr, size)];
                parts.extend(self.shown_tail(result, unit.as_ref(), size));
                self.hcat(parts)
            }
            Stmt::Show(e) => self.expr(e, size),
            Stmt::Symbolic { target, expr, ops } => {
                let mut parts = Vec::new();
                if let Some((t, global)) = target {
                    parts.push(self.target(t, size));
                    parts.push(self.op(if *global { "≡" } else { ":=" }, size));
                }
                parts.push(self.expr(expr, size));
                if ops.is_empty() {
                    parts.push(self.sym_arrow(None, size));
                }
                for op in ops {
                    let small = size * 0.75;
                    let mut kw = vec![self.kw(&op.name, small)];
                    for a in &op.args {
                        kw.push(self.text(", ", small));
                        kw.push(self.expr(a, small));
                    }
                    parts.push(self.sym_arrow(Some(self.hcat(kw)), size));
                }
                if let Some(r) = &self.sym {
                    parts.push(self.space(size * 0.15));
                    parts.push(self.expr(r, size));
                } else if let Some((view, _)) = result {
                    parts.push(self.space(size * 0.15));
                    parts.push(self.result(view, size));
                }
                self.hcat(parts)
            }
        }
    }

    /// Mathcad's symbolic arrow with the keyword stacked above it.
    fn sym_arrow(&self, kw: Option<Lay>, size: f32) -> Lay {
        let axis = self.axis(size);
        let head = size * 0.17;
        let pad = size * 0.2;
        let lw = (size / 14.0).max(1.0);
        let kw_w = kw.as_ref().map_or(0.0, |k| k.w);
        let w = (kw_w + 2.0 * pad).max(size * 1.5);
        let y = -axis;
        let mut out = Lay { w, asc: axis + head + 1.0, desc: (head - axis).max(0.0) + 1.0, items: vec![], boxes: vec![] };
        let (a, b) = (Vec2::new(pad * 0.5, y), Vec2::new(w - pad * 0.5, y));
        out.items.push(Item::Line { a, b, w: lw, color: self.color });
        out.items.push(Item::Path {
            pts: vec![Vec2::new(b.x - head * 1.1, y - head * 0.7), b, Vec2::new(b.x - head * 1.1, y + head * 0.7)],
            w: lw,
            color: self.color,
        });
        if let Some(k) = kw {
            let gap = size * 0.08;
            let by = y - gap - k.desc;
            out.asc = out.asc.max(-by + k.asc);
            let kwid = k.w;
            out.place(k, (w - kwid) / 2.0, by);
        }
        out
    }
}

fn is_frac(e: &Expr) -> bool {
    matches!(e, Expr::Binary(BinOp::Div, ..))
}
