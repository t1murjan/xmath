//! In-place editing of formulas (Mathcad style): a matrix cell (or the name
//! of a definition) is edited with a small text box placed exactly over the
//! rendered node, while the rest of the formula stays typeset in 2D.
//!
//! The first half of the file is pure logic (AST walking, matrix row/column
//! operations, hit testing); the second half is the `App` glue. This module is
//! mounted inside `app` so that it can reach the private fields of `App`.

use super::*;
use crate::mathview::Node;
use xmath_core::ast::{Expr, PStmt, Stmt, Target};
use xmath_core::{parse_expr, ParseError};

pub const PLACEHOLDER: &str = "■";
/// Identifier standing for `■` while the source goes through the parser.
const PH_IDENT: &str = "ph__";

// ---------------------------------------------------------------------------
// Placeholder tolerant parsing

fn decode(e: &mut Expr) {
    if matches!(e, Expr::Var(n) if n == PH_IDENT) {
        *e = Expr::Var(PLACEHOLDER.into());
    }
}

/// Like `parse_statement`, but `■` placeholders are accepted (as `Var("■")`).
pub fn parse_stmt(src: &str) -> Result<Stmt, ParseError> {
    let mut st = xmath_core::parse_statement(&src.replace('■', PH_IDENT))?;
    walk_stmt_mut(&mut st, &mut decode);
    Ok(st)
}

/// Like `parse_expr`, but `■` placeholders are accepted.
pub fn parse_cell(src: &str) -> Result<Expr, ParseError> {
    let mut e = parse_expr(&src.replace('■', PH_IDENT))?;
    walk_mut(&mut e, &mut decode);
    Ok(e)
}

fn parse_target(text: &str) -> Option<Target> {
    match parse_stmt(&format!("{text} := 0")) {
        Ok(Stmt::Define { target, .. }) => Some(target),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// AST walking (pre-order)

pub fn walk_mut(e: &mut Expr, f: &mut dyn FnMut(&mut Expr)) {
    f(e);
    match e {
        Expr::Unary(_, a) | Expr::Abs(a) | Expr::Vectorize(a) => walk_mut(a, f),
        Expr::Binary(_, a, b) | Expr::OnError(a, b) | Expr::MatCol(a, b) => {
            walk_mut(a, f);
            walk_mut(b, f);
        }
        Expr::Call(_, args) => args.iter_mut().for_each(|a| walk_mut(a, f)),
        Expr::Index(b, idx) => {
            walk_mut(b, f);
            idx.iter_mut().for_each(|a| walk_mut(a, f));
        }
        Expr::Matrix(rows) => rows.iter_mut().flatten().for_each(|c| walk_mut(c, f)),
        Expr::Range(a, b, c) => {
            walk_mut(a, f);
            if let Some(b) = b {
                walk_mut(b, f);
            }
            walk_mut(c, f);
        }
        Expr::Program(stmts) => walk_pstmts(stmts, f),
        Expr::Num(_) | Expr::Imag(_) | Expr::Str(_) | Expr::Var(_) => {}
    }
}

fn walk_target(t: &mut Target, f: &mut dyn FnMut(&mut Expr)) {
    if let Target::Indexed(_, idx) = t {
        idx.iter_mut().for_each(|a| walk_mut(a, f));
    }
}

fn walk_pstmts(stmts: &mut [PStmt], f: &mut dyn FnMut(&mut Expr)) {
    for s in stmts {
        match s {
            PStmt::Assign(t, e) => {
                walk_target(t, f);
                walk_mut(e, f);
            }
            PStmt::Expr(e) | PStmt::Return(e) => walk_mut(e, f),
            PStmt::If { cond, then, else_ } => {
                walk_mut(cond, f);
                walk_pstmts(then, f);
                if let Some(e) = else_ {
                    walk_pstmts(e, f);
                }
            }
            PStmt::For { iter, body, .. } => {
                walk_mut(iter, f);
                walk_pstmts(body, f);
            }
            PStmt::While { cond, body } => {
                walk_mut(cond, f);
                walk_pstmts(body, f);
            }
            PStmt::Break | PStmt::Continue => {}
        }
    }
}

pub fn walk_stmt_mut(st: &mut Stmt, f: &mut dyn FnMut(&mut Expr)) {
    match st {
        Stmt::Define { target, expr, show, .. } => {
            walk_target(target, f);
            walk_mut(expr, f);
            if let Some(Some(u)) = show {
                walk_mut(u, f);
            }
        }
        Stmt::Eval { expr, unit } => {
            walk_mut(expr, f);
            if let Some(u) = unit {
                walk_mut(u, f);
            }
        }
        Stmt::Show(e) => walk_mut(e, f),
    }
}

/// Runs `f` on the `n`-th matrix literal (pre-order) of the statement.
pub fn with_matrix<R>(st: &mut Stmt, n: usize, f: impl FnOnce(&mut Vec<Vec<Expr>>) -> R) -> Option<R> {
    let mut k = 0;
    let mut f = Some(f);
    let mut out = None;
    walk_stmt_mut(st, &mut |e| {
        if let Expr::Matrix(rows) = e {
            if k == n {
                if let Some(f) = f.take() {
                    out = Some(f(rows));
                }
            }
            k += 1;
        }
    });
    out
}

/// Number of matrix literals in the statement.
pub fn matrix_count(st: &mut Stmt) -> usize {
    let mut k = 0;
    walk_stmt_mut(st, &mut |e| {
        if matches!(e, Expr::Matrix(_)) {
            k += 1;
        }
    });
    k
}

/// Ordinal (pre-order) of the matrix literal whose node lives at `addr`.
fn matrix_ordinal(st: &mut Stmt, addr: usize) -> Option<usize> {
    let mut k = 0;
    let mut found = None;
    walk_stmt_mut(st, &mut |e| {
        if let Expr::Matrix(_) = e {
            if e as *const Expr as usize == addr {
                found = Some(k);
            }
            k += 1;
        }
    });
    found
}

/// Turns the matrix addresses recorded by the typesetter into matrix ordinals.
/// Call it on the statement that was laid out, before it is moved.
pub fn resolve_boxes(st: &mut Stmt, boxes: &mut Vec<(Node, Rect)>) {
    boxes.retain_mut(|(n, _)| match n {
        Node::Cell { mat, .. } => match matrix_ordinal(st, *mat) {
            Some(o) => {
                *mat = o;
                true
            }
            None => false,
        },
        _ => true,
    });
}

// ---------------------------------------------------------------------------
// Matrix editing

fn placeholder() -> Expr {
    Expr::Var(PLACEHOLDER.into())
}

/// Inserts `n` rows of placeholders after row `after`.
pub fn insert_rows(m: &mut Vec<Vec<Expr>>, after: usize, n: usize) {
    let cols = m.first().map_or(1, |r| r.len());
    let at = (after + 1).min(m.len());
    for _ in 0..n {
        m.insert(at, (0..cols).map(|_| placeholder()).collect());
    }
}

/// Inserts `n` columns of placeholders after column `after`.
pub fn insert_cols(m: &mut [Vec<Expr>], after: usize, n: usize) {
    for row in m.iter_mut() {
        let at = (after + 1).min(row.len());
        for _ in 0..n {
            row.insert(at, placeholder());
        }
    }
}

/// Deletes up to `n` rows starting at `from`; at least one row stays.
pub fn delete_rows(m: &mut Vec<Vec<Expr>>, from: usize, n: usize) {
    let n = n.min(m.len().saturating_sub(from)).min(m.len().saturating_sub(1));
    if n > 0 {
        m.drain(from..from + n);
    }
}

/// Deletes up to `n` columns starting at `from`; at least one column stays.
pub fn delete_cols(m: &mut [Vec<Expr>], from: usize, n: usize) {
    let cols = m.first().map_or(0, |r| r.len());
    let n = n.min(cols.saturating_sub(from)).min(cols.saturating_sub(1));
    if n > 0 {
        for row in m.iter_mut() {
            row.drain(from..from + n);
        }
    }
}

/// Where Tab / arrows lead from cell `(r, c)` of an `rows x cols` matrix.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Move {
    Next,
    Prev,
    Up,
    Down,
    Left,
    Right,
}

/// Neighbouring cell, or `None` when there is none in that direction.
pub fn neighbour(rows: usize, cols: usize, (r, c): (usize, usize), mv: Move) -> Option<(usize, usize)> {
    match mv {
        Move::Next | Move::Right => {
            if c + 1 < cols {
                Some((r, c + 1))
            } else if r + 1 < rows {
                Some((r + 1, 0))
            } else {
                None
            }
        }
        Move::Prev | Move::Left => {
            if c > 0 {
                Some((r, c - 1))
            } else if r > 0 {
                Some((r - 1, cols - 1))
            } else {
                None
            }
        }
        Move::Up => (r > 0).then(|| (r - 1, c)),
        Move::Down => (r + 1 < rows).then(|| (r + 1, c)),
    }
}

// ---------------------------------------------------------------------------
// Hit testing

fn dist(r: &Rect, p: Pos2) -> f32 {
    let dx = (r.left() - p.x).max(0.0).max(p.x - r.right());
    let dy = (r.top() - p.y).max(0.0).max(p.y - r.bottom());
    dx.hypot(dy)
}

/// What a click at `p` edits: the cell under it, the definition target, or
/// else the nearest cell. `None` when the formula has no cells.
pub fn pick(hits: &[(Node, Rect)], p: Pos2) -> Option<CellKind> {
    let cell = |n: &Node| match n {
        Node::Cell { mat, row, col } => Some(CellKind::Cell { mat: *mat, row: *row, col: *col }),
        _ => None,
    };
    if let Some(k) = hits.iter().find(|(n, r)| cell(n).is_some() && r.contains(p)).and_then(|(n, _)| cell(n)) {
        return Some(k);
    }
    if hits.iter().any(|(n, r)| *n == Node::Target && r.expand(3.0).contains(p)) {
        return Some(CellKind::Target);
    }
    hits.iter()
        .filter_map(|(n, r)| cell(n).map(|k| (k, dist(r, p))))
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(k, _)| k)
}

// ---------------------------------------------------------------------------
// Editing state

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum CellKind {
    Cell { mat: usize, row: usize, col: usize },
    Target,
}

#[derive(Clone, Debug)]
pub struct CellEdit {
    pub region: u64,
    pub kind: CellKind,
    /// Text of the box (may be unparsable while typing).
    pub text: String,
}

fn cell_id(id: u64) -> Id {
    Id::new(("xmath-cell", id))
}

fn math_source(app: &App, id: u64) -> Option<String> {
    match app.region(id).map(|r| &r.kind) {
        Some(RegionKind::Math { source }) => Some(source.clone()),
        _ => None,
    }
}

/// Source text of the node being edited.
fn text_of(st: &mut Stmt, kind: CellKind) -> Option<String> {
    match kind {
        CellKind::Cell { mat, row, col } => with_matrix(st, mat, |m| m.get(row).and_then(|r| r.get(col)).map(|c| c.to_string())).flatten(),
        CellKind::Target => match st {
            Stmt::Define { target, .. } => Some(target.to_string()),
            _ => None,
        },
    }
}

fn text_valid(kind: CellKind, text: &str) -> bool {
    match kind {
        CellKind::Cell { .. } => text.trim().is_empty() || parse_cell(text).is_ok(),
        CellKind::Target => parse_target(text).is_some(),
    }
}

impl App {
    /// Selects the node to edit: loads its text and puts the caret in it.
    fn select_cell(&mut self, ctx: &Context, id: u64, kind: CellKind, st: &mut Stmt) {
        let Some(text) = text_of(st, kind) else { return };
        let mut state = TextEditState::load(ctx, cell_id(id)).unwrap_or_default();
        let n = text.chars().count();
        let range = if text == PLACEHOLDER { CCursorRange::two(CCursor::new(0), CCursor::new(n)) } else { CCursorRange::one(CCursor::new(n)) };
        state.cursor.set_char_range(Some(range));
        state.store(ctx, cell_id(id));
        self.cell = Some(CellEdit { region: id, kind, text });
        self.focus_pending = true;
        ctx.request_repaint();
    }

    /// Enters cell editing of region `id`; `at` is the clicked screen position
    /// (the first cell when `None`). Returns false when the region has nothing
    /// to edit in place (it is then edited as a line of text).
    pub(super) fn begin_inplace(&mut self, ctx: &Context, id: u64, at: Option<Pos2>) -> bool {
        let Some(src) = math_source(self, id) else { return false };
        let Ok(mut st) = parse_stmt(&src) else { return false };
        let kind = match at {
            Some(p) => self.hits.get(&id).and_then(|h| pick(h, p)),
            None => (matrix_count(&mut st) > 0).then_some(CellKind::Cell { mat: 0, row: 0, col: 0 }),
        };
        let Some(kind) = kind else { return false };
        if self.editing != Some(id) {
            self.begin_edit(id, true);
        }
        self.select_cell(ctx, id, kind, &mut st);
        self.cell.is_some()
    }

    /// Writes the statement back into the region source.
    fn store_stmt(&mut self, id: u64, st: &Stmt) {
        let text = st.to_string();
        if let Some(Region { kind: RegionKind::Math { source }, .. }) = self.region_mut(id) {
            if *source != text {
                *source = text;
                self.changed();
            }
        }
    }

    /// A newly typed text of the active box: applies it when it parses.
    fn commit_cell(&mut self, id: u64, text: String, st: &mut Stmt) {
        let Some(cell) = self.cell.as_mut() else { return };
        cell.text = text.clone();
        match cell.kind {
            CellKind::Cell { mat, row, col } => {
                let e = if text.trim().is_empty() { Some(placeholder()) } else { parse_cell(&text).ok() };
                if let Some(e) = e {
                    with_matrix(st, mat, |m| {
                        if let Some(c) = m.get_mut(row).and_then(|r| r.get_mut(col)) {
                            *c = e;
                        }
                    });
                    self.store_stmt(id, st);
                }
            }
            CellKind::Target => {
                if let (Some(t), Stmt::Define { target, .. }) = (parse_target(&text), &mut *st) {
                    *target = t;
                    self.store_stmt(id, st);
                }
            }
        }
    }

    /// Creates a math region from `source` (a matrix literal) and edits its first cell.
    pub(super) fn new_matrix_region(&mut self, ctx: &Context, source: String) {
        let id = self.new_region(RegionKind::Math { source }, true);
        self.begin_inplace(ctx, id, None);
    }

    /// After a template was typed into the linear editor: switches to cell
    /// editing when it contains a matrix of placeholders.
    pub(super) fn linear_to_cells(&mut self, ctx: &Context) {
        let Some(id) = self.editing_math() else { return };
        let Some(src) = math_source(self, id) else { return };
        let Ok(mut st) = parse_stmt(&src) else { return };
        let n = matrix_count(&mut st);
        let fresh = (0..n).find(|&k| with_matrix(&mut st, k, |m| m.iter().flatten().all(|c| matches!(c, Expr::Var(v) if v == PLACEHOLDER))).unwrap_or(false));
        if let Some(mat) = fresh {
            self.select_cell(ctx, id, CellKind::Cell { mat, row: 0, col: 0 }, &mut st);
        }
    }

    /// True when a cell of a matrix is the active box (Ctrl+M edits rows/columns).
    pub(super) fn active_matrix_cell(&self) -> bool {
        matches!(&self.cell, Some(CellEdit { kind: CellKind::Cell { .. }, .. }))
    }

    /// Rows/columns dialog: insert after / delete from the active cell.
    pub(super) fn matrix_edit(&mut self, ctx: &Context, insert: bool, rows: usize, cols: usize) {
        let Some(CellEdit { region: id, kind: CellKind::Cell { mat, row, col }, .. }) = self.cell.clone() else { return };
        let Some(src) = math_source(self, id) else { return };
        let Ok(mut st) = parse_stmt(&src) else { return };
        let (mut nr, mut nc) = (row, col);
        with_matrix(&mut st, mat, |m| {
            if insert {
                insert_rows(m, row, rows);
                insert_cols(m, col, cols);
            } else {
                delete_cols(m, col, cols);
                delete_rows(m, row, rows);
            }
            nr = row.min(m.len().saturating_sub(1));
            nc = col.min(m.first().map_or(0, |r| r.len()).saturating_sub(1));
        });
        self.store_stmt(id, &st);
        // after an insertion the cursor moves to the first new cell
        if insert {
            if rows > 0 {
                nr = row + 1;
            } else if cols > 0 {
                nc = col + 1;
            }
            if rows > 0 && cols > 0 {
                nc = col;
            }
        }
        self.select_cell(ctx, id, CellKind::Cell { mat, row: nr, col: nc }, &mut st);
    }

    /// Palette / shortcut insertion into the active box. Returns false when no box is active.
    pub(super) fn insert_in_cell(&mut self, ctx: &Context, s: &str) -> bool {
        let Some(cell) = self.cell.clone() else { return false };
        let id = cell.region;
        let mut state = TextEditState::load(ctx, cell_id(id)).unwrap_or_default();
        let mut text = cell.text.clone();
        let len = text.chars().count();
        let (a, b) = state
            .cursor
            .char_range()
            .map(|r| {
                let (x, y) = (r.primary.index.min(len), r.secondary.index.min(len));
                (x.min(y), x.max(y))
            })
            .unwrap_or((len, len));
        text.replace_range(char_to_byte(&text, a)..char_to_byte(&text, b), s);
        let (sa, sb) = match s.chars().position(|c| c == '■') {
            Some(p) => (a + p, a + p + 1),
            None => (a + s.chars().count(), a + s.chars().count()),
        };
        state.cursor.set_char_range(Some(CCursorRange::two(CCursor::new(sa), CCursor::new(sb))));
        state.store(ctx, cell_id(id));
        if let Some(src) = math_source(self, id) {
            if let Ok(mut st) = parse_stmt(&src) {
                self.commit_cell(id, text, &mut st);
            }
        }
        self.focus_pending = true;
        true
    }

    /// Ctrl+G inside the active box.
    fn greek_in_cell(&mut self, ctx: &Context, id: u64) {
        let Some(cell) = self.cell.clone() else { return };
        let mut state = TextEditState::load(ctx, cell_id(id)).unwrap_or_default();
        let mut text = cell.text.clone();
        let len = text.chars().count();
        let pos = state.cursor.char_range().map(|r| r.primary.index.min(len)).unwrap_or(len);
        if pos == 0 {
            return;
        }
        let Some(g) = text.chars().nth(pos - 1).and_then(palettes::to_greek) else { return };
        text.replace_range(char_to_byte(&text, pos - 1)..char_to_byte(&text, pos), &g.to_string());
        state.cursor.set_char_range(Some(CCursorRange::one(CCursor::new(pos))));
        state.store(ctx, cell_id(id));
        if let Some(src) = math_source(self, id) {
            if let Ok(mut st) = parse_stmt(&src) {
                self.commit_cell(id, text, &mut st);
            }
        }
        self.focus_pending = true;
    }

    /// Draws the region with the active box and handles its keys.
    /// Returns the region rectangle (screen), the error text, and whether editing ended.
    pub(super) fn cell_canvas(&mut self, ui: &mut Ui, id: u64, pos: Pos2, fs: f32) -> Option<(Rect, Option<String>, bool)> {
        let ctx = ui.ctx().clone();
        let cell = self.cell.clone().filter(|c| c.region == id)?;
        let Some(source) = math_source(self, id) else {
            self.cell = None;
            return None;
        };
        let Ok(mut st) = parse_stmt(&source) else {
            self.cell = None;
            return None;
        };
        let te_id = cell_id(id);
        let focused = ctx.memory(|m| m.has_focus(te_id));

        // ---- keys (consumed before the text box sees them)
        let mut mv: Option<Move> = None;
        let mut finish: Option<bool> = None;
        let mut greek = false;
        if focused || self.focus_pending {
            let state = TextEditState::load(&ctx, te_id).unwrap_or_default();
            let n = cell.text.chars().count();
            let collapsed = state.cursor.char_range().map(|r| (r.primary.index == r.secondary.index).then_some(r.primary.index));
            let (at_start, at_end) = match collapsed {
                Some(Some(i)) => (i == 0, i >= n),
                _ => (false, false),
            };
            ctx.input_mut(|i| {
                if i.consume_key(Modifiers::SHIFT, Key::Tab) {
                    mv = Some(Move::Prev);
                } else if i.consume_key(Modifiers::NONE, Key::Tab) {
                    mv = Some(Move::Next);
                } else if i.consume_key(Modifiers::NONE, Key::ArrowUp) {
                    mv = Some(Move::Up);
                } else if i.consume_key(Modifiers::NONE, Key::ArrowDown) {
                    mv = Some(Move::Down);
                } else if at_start && i.consume_key(Modifiers::NONE, Key::ArrowLeft) {
                    mv = Some(Move::Left);
                } else if at_end && i.consume_key(Modifiers::NONE, Key::ArrowRight) {
                    mv = Some(Move::Right);
                }
                if i.consume_key(Modifiers::NONE, Key::Enter) {
                    finish = Some(true);
                } else if i.consume_key(Modifiers::NONE, Key::Escape) {
                    finish = Some(false);
                }
                if i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::G)) {
                    greek = true;
                }
            });
        }
        if greek {
            self.greek_in_cell(&ctx, id);
        }
        if let Some(mv) = mv {
            let next = match cell.kind {
                CellKind::Cell { mat, row, col } => {
                    let dims = with_matrix(&mut st, mat, |m| (m.len(), m.first().map_or(0, |r| r.len()))).unwrap_or((0, 0));
                    neighbour(dims.0, dims.1, (row, col), mv).map(|(r, c)| CellKind::Cell { mat, row: r, col: c })
                }
                CellKind::Target => matches!(mv, Move::Next | Move::Right | Move::Down).then_some(CellKind::Cell { mat: 0, row: 0, col: 0 }),
            };
            if let Some(k) = next {
                self.select_cell(&ctx, id, k, &mut st);
            }
        }
        let cell = self.cell.clone().filter(|c| c.region == id)?;

        // ---- layout with the active box reserved
        let font = FontId::proportional(fs);
        let tw = ctx.fonts(|f| f.layout_no_wrap(cell.text.clone(), font.clone(), Color32::BLACK).size().x);
        let box_w = tw + 10.0;
        let reserved = match cell.kind {
            CellKind::Cell { mat, row, col } => {
                let addr = with_matrix(&mut st, mat, |m| m.get(row).and_then(|r| r.get(col)).map(|c| c as *const Expr as usize)).flatten();
                if addr.is_none() {
                    self.cell = None;
                    return None;
                }
                addr
            }
            CellKind::Target => Some(0),
        };
        let out = self.outputs.get(&id);
        let error = out.and_then(|o| o.error());
        let mut lay = ctx.fonts(|f| {
            let mut ts = Ts::new(f, fs);
            if error.is_some() {
                ts.color = RED;
            }
            ts.edit_cell = reserved.map(|a| (a, box_w));
            let result = match out.map(|o| &o.output) {
                Some(Output::Value { view, unit }) => Some((view, unit.as_ref())),
                _ => None,
            };
            ts.statement(&st, result, fs)
        });
        resolve_boxes(&mut st, &mut lay.boxes);
        let origin = pos + Vec2::new(2.0, 2.0 + lay.asc);
        lay.paint(ui.painter(), origin);
        let hits: Vec<(Node, Rect)> = lay.boxes.iter().map(|(n, r)| (*n, r.translate(origin.to_vec2()))).collect();
        let slot = hits
            .iter()
            .find(|(n, _)| match (n, cell.kind) {
                (Node::Cell { mat, row, col }, CellKind::Cell { mat: m2, row: r2, col: c2 }) => (*mat, *row, *col) == (m2, r2, c2),
                (Node::Target, CellKind::Target) => true,
                _ => false,
            })
            .map(|(_, r)| *r);
        self.hits.insert(id, hits);
        let region_rect = Rect::from_min_size(pos, Vec2::new(lay.w + 4.0, lay.h() + 4.0));
        let Some(slot) = slot else {
            self.cell = None;
            return None;
        };

        // clicks on the other parts of the formula (the box itself is on top)
        let region_resp = ui.interact(region_rect, Id::new(("cellreg", id)), Sense::click());

        // ---- the box
        let er = Rect::from_center_size(slot.center(), Vec2::new(slot.width().max(box_w) + 2.0, slot.height().max(fs * 1.35) + 2.0));
        let er = Rect::from_min_size(Pos2::new(slot.left() - 1.0, er.top()), er.size());
        let valid = text_valid(cell.kind, &cell.text);
        let mut text = cell.text.clone();
        ui.painter().rect_filled(er, 0.0, Color32::WHITE);
        let resp = ui.put(
            er,
            TextEdit::singleline(&mut text)
                .id(te_id)
                .font(font)
                .desired_width(er.width())
                .margin(Vec2::new(2.0, 1.0))
                .frame(false)
                .lock_focus(true)
                .text_color(if valid { Color32::BLACK } else { RED }),
        );
        ui.painter().rect_stroke(er, 0.0, Stroke::new(1.0_f32, SELECT_BLUE), egui::StrokeKind::Outside);
        if self.focus_pending {
            resp.request_focus();
            self.focus_pending = false;
        }
        if text != cell.text {
            self.commit_cell(id, text, &mut st);
            ctx.request_repaint();
        }
        if region_resp.clicked() {
            if let Some(p) = region_resp.interact_pointer_pos() {
                let hits = self.hits.get(&id).cloned().unwrap_or_default();
                if let Some(k) = pick(&hits, p) {
                    if Some(k) != self.cell.as_ref().map(|c| c.kind) {
                        self.select_cell(&ctx, id, k, &mut st);
                    }
                }
            }
        }
        if let Some(enter) = finish {
            self.finish_edit(enter);
            return Some((region_rect, error, true));
        }
        Some((region_rect, error, false))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mat(src: &str) -> Vec<Vec<Expr>> {
        match parse_cell(src).unwrap() {
            Expr::Matrix(m) => m,
            e => panic!("not a matrix: {e}"),
        }
    }

    fn show(m: &[Vec<Expr>]) -> String {
        Expr::Matrix(m.to_vec()).to_string()
    }

    #[test]
    fn placeholders_parse_and_print() {
        let st = parse_stmt("A := [■, 2; 3, ■]").unwrap();
        assert_eq!(st.to_string(), "A := [■, 2; 3, ■]");
        assert_eq!(parse_cell("■").unwrap(), Expr::Var("■".into()));
    }

    #[test]
    fn insert_and_delete() {
        let mut m = mat("[1, 2; 3, 4]");
        insert_rows(&mut m, 0, 1);
        assert_eq!(show(&m), "[1, 2; ■, ■; 3, 4]");
        insert_cols(&mut m, 1, 2);
        assert_eq!(show(&m), "[1, 2, ■, ■; ■, ■, ■, ■; 3, 4, ■, ■]");
        delete_cols(&mut m, 2, 5);
        assert_eq!(show(&m), "[1, 2; ■, ■; 3, 4]");
        delete_rows(&mut m, 1, 1);
        assert_eq!(show(&m), "[1, 2; 3, 4]");
        // the last row / column can not be deleted
        delete_rows(&mut m, 0, 9);
        assert_eq!(m.len(), 1);
        delete_cols(&mut m, 0, 9);
        assert_eq!(show(&m), "[4]");
    }

    #[test]
    fn replace_cell_by_ordinal() {
        let mut st = parse_stmt("B := [1, 2] + [3, 4]").unwrap();
        assert_eq!(matrix_count(&mut st), 2);
        with_matrix(&mut st, 1, |m| m[0][1] = Expr::Num(9.0));
        assert_eq!(st.to_string(), "B := [1, 2] + [3, 9]");
        assert!(with_matrix(&mut st, 2, |_| ()).is_none());
    }

    #[test]
    fn navigation() {
        assert_eq!(neighbour(2, 2, (0, 1), Move::Next), Some((1, 0)));
        assert_eq!(neighbour(2, 2, (1, 1), Move::Next), None);
        assert_eq!(neighbour(2, 2, (1, 0), Move::Prev), Some((0, 1)));
        assert_eq!(neighbour(3, 1, (0, 0), Move::Down), Some((1, 0)));
        assert_eq!(neighbour(3, 1, (0, 0), Move::Up), None);
    }

    #[test]
    fn picking() {
        let r = |x0: f32, x1: f32| Rect::from_min_max(Pos2::new(x0, 0.0), Pos2::new(x1, 10.0));
        let hits = vec![
            (Node::Target, r(0.0, 10.0)),
            (Node::Cell { mat: 0, row: 0, col: 0 }, r(30.0, 40.0)),
            (Node::Cell { mat: 0, row: 0, col: 1 }, r(50.0, 60.0)),
        ];
        assert_eq!(pick(&hits, Pos2::new(5.0, 5.0)), Some(CellKind::Target));
        assert_eq!(pick(&hits, Pos2::new(55.0, 5.0)), Some(CellKind::Cell { mat: 0, row: 0, col: 1 }));
        assert_eq!(pick(&hits, Pos2::new(100.0, 5.0)), Some(CellKind::Cell { mat: 0, row: 0, col: 1 }));
    }
}
