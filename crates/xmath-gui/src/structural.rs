//! Glue of the structural (2D, Mathcad style) formula editor with the application:
//! key handling, typesetting with Mathcad's blue editing lines, mouse placement of the
//! cursor and the hand-over to the line editor / in-place matrix cell editing.
//!
//! The model lives in `editor.rs`. This module is mounted inside `app` so that it can
//! reach the private fields of `App`.

use super::*;
use crate::editor::{self, Cursor, Ed, Path, K};
use xmath_core::ast::{BinOp, Expr};

/// The structural editor of the region being edited.
pub struct EdState {
    pub region: u64,
    pub ed: Ed,
}

fn region_source(app: &App, id: u64) -> Option<String> {
    match app.region(id).map(|r| &r.kind) {
        Some(RegionKind::Math { source }) => Some(source.clone()),
        _ => None,
    }
}

/// Applies the keys and typed text of this frame to the editor.
/// Returns `Some(true)` for Enter and `Some(false)` for Escape.
fn feed_keys(ctx: &Context, ed: &mut Ed) -> Option<bool> {
    let events = ctx.input(|i| i.events.clone());
    let mut finish = None;
    for ev in events {
        match ev {
            Event::Text(t) => {
                for c in t.chars() {
                    ed.type_char(c);
                }
            }
            Event::Key { key, pressed: true, modifiers: m, .. } => {
                if m.command {
                    match key {
                        Key::Z if m.shift => ed.type_char('∞'),
                        Key::Z => {
                            ed.undo();
                        }
                        Key::Y => {
                            ed.redo();
                        }
                        Key::G => ed.greek(palettes::to_greek),
                        Key::Num3 => ed.type_char('≠'),
                        Key::Num9 => ed.type_char('≤'),
                        Key::Num0 => ed.type_char('≥'),
                        Key::Equals => ed.type_binop(BinOp::Eq),
                        Key::Num1 => ed.template("transpose(■)"),
                        Key::Num8 => ed.template("cross(■, ■)"),
                        Key::Backslash => ed.template("nthroot(■, ■)"),
                        Key::P if m.shift => ed.type_char('π'),
                        _ => {}
                    }
                    continue;
                }
                match key {
                    Key::Backspace => ed.press(K::Backspace),
                    Key::Delete => ed.press(K::Delete),
                    Key::ArrowLeft => ed.press(if m.shift { K::ShrinkLeft } else { K::Left }),
                    Key::ArrowRight => ed.press(if m.shift { K::GrowRight } else { K::Right }),
                    Key::ArrowUp => ed.press(K::Up),
                    Key::ArrowDown => ed.press(K::Down),
                    Key::Home => ed.press(K::Home),
                    Key::End => ed.press(K::End),
                    Key::Tab => ed.press(if m.shift { K::BackTab } else { K::Tab }),
                    Key::Insert => ed.press(K::Insert),
                    Key::Enter => finish = Some(true),
                    Key::Escape => finish = Some(false),
                    _ => {}
                }
            }
            Event::Copy => ctx.copy_text(ed.copy_text()),
            Event::Cut => ctx.copy_text(ed.cut_text()),
            Event::Paste(t) => ed.paste_text(&t),
            _ => {}
        }
        if ed.fallback.is_some() {
            break;
        }
    }
    finish
}

/// Screen position of a cursor stop: `(x, top, bottom)`.
fn stop_geom(ctx: &Context, ed: &Ed, c: &Cursor, rects: &HashMap<Path, Rect>, unit_h: f32) -> Option<(f32, f32, f32)> {
    let r = rects.get(&c.path)?;
    match ed.node(&c.path)? {
        Expr::Var(n) if n != editor::PH => {
            let chars: Vec<char> = n.chars().collect();
            let k = c.pos.min(chars.len());
            let w = if k == 0 {
                0.0
            } else if k >= chars.len() {
                r.width()
            } else {
                let fsz = (r.height() / unit_h).max(4.0);
                let prefix: String = chars[..k].iter().collect();
                ctx.fonts(|f| f.layout_no_wrap(prefix, FontId::proportional(fsz), Color32::BLACK).size().x)
            };
            Some((r.left() + w, r.top(), r.bottom()))
        }
        Expr::Var(_) => Some((r.left(), r.top(), r.bottom())),
        _ => Some((if c.pos == 0 { r.left() } else { r.right() }, r.top(), r.bottom())),
    }
}

impl App {
    /// Starts the structural editor on math region `id` (which must be the edited region).
    /// Returns false when the formula needs the line editor.
    pub(super) fn begin_structural(&mut self, id: u64, click: Option<Pos2>) -> bool {
        if self.line_edit || self.text_mode {
            return false;
        }
        let Some(src) = region_source(self, id) else { return false };
        let Some(ed) = Ed::from_source(&src) else {
            self.line_edit = true;
            self.focus_pending = true;
            return false;
        };
        self.ed = Some(EdState { region: id, ed });
        self.edit_click = click;
        true
    }

    fn store_source(&mut self, id: u64, text: &str) {
        if let Some(Region { kind: RegionKind::Math { source }, .. }) = self.region_mut(id) {
            if source != text {
                *source = text.to_string();
                self.changed();
            }
        }
    }

    /// Continues editing `text` in the line editor (the structural editor cannot express it).
    fn to_line_editor(&mut self, ctx: &Context, id: u64, text: &str) {
        self.store_source(id, text);
        self.ed = None;
        self.line_edit = true;
        self.focus_pending = true;
        self.place_caret_end(ctx, id);
    }

    /// Palette / shortcut insertion into the structural editor. `false` when it is not active
    /// (or the text needs the line editor, which then takes over).
    pub(super) fn ed_insert(&mut self, ctx: &Context, s: &str) -> bool {
        let Some(mut es) = self.ed.take() else { return false };
        let id = es.region;
        if !es.ed.insert_str(s) {
            let src = es.ed.source();
            self.to_line_editor(ctx, id, &src);
            return false;
        }
        let src = es.ed.source();
        self.store_source(id, &src);
        self.ed = Some(es);
        if s.starts_with('[') && s.contains('■') {
            // a fresh matrix: its cells are edited in place
            self.ed = None;
            self.linear_to_cells(ctx);
            if self.cell.is_none() {
                self.begin_structural(id, None);
            }
        }
        true
    }

    /// Toggles «Редактировать как текст» (F2).
    pub(super) fn toggle_text_mode(&mut self, ctx: &Context) {
        self.text_mode = !self.text_mode;
        self.status = if self.text_mode { "Формулы редактируются как текст (F2)".into() } else { "Структурное редактирование формул".into() };
        let Some(id) = self.editing_math() else { return };
        if self.cell.is_some() {
            return;
        }
        if self.text_mode {
            if self.ed.take().is_some() {
                self.line_edit = true;
                self.focus_pending = true;
                self.place_caret_end(ctx, id);
            }
        } else {
            self.line_edit = false;
            self.ed = None;
        }
    }

    /// Draws the region being edited structurally and handles its input.
    /// Returns the region rectangle (screen), the error text, and whether editing ended.
    pub(super) fn ed_canvas(&mut self, ui: &mut Ui, id: u64, pos: Pos2, fs: f32) -> Option<(Rect, Option<String>, bool)> {
        let ctx = ui.ctx().clone();
        let mut es = self.ed.take().filter(|e| e.region == id)?;
        let key_id = Id::new(("xmath-sed", id));
        let dialogs = self.dialogs_open();

        // ---- keys
        let mut finish = None;
        if !dialogs {
            finish = feed_keys(&ctx, &mut es.ed);
        }
        if let Some(text) = es.ed.fallback.take() {
            self.to_line_editor(&ctx, id, &text);
            return None;
        }
        let src = es.ed.source();
        self.store_source(id, &src);

        // ---- layout
        let out = self.outputs.get(&id);
        let raw_error = out.and_then(|o| o.error());
        let has_ph = es.ed.has_placeholder();
        let error = raw_error.clone().filter(|_| !has_ph);
        let lay = ctx.fonts(|f| {
            let mut ts = Ts::new(f);
            ts.record = true;
            if raw_error.is_some() {
                if has_ph {
                    ts.ph_color = Some(RED);
                } else {
                    ts.color = RED;
                }
            }
            let result = match out.map(|o| &o.output) {
                Some(Output::Value { view, unit }) => Some((view, unit.as_ref())),
                _ => None,
            };
            let ed = &es.ed;
            let (lhs, body) = match ed.form {
                editor::Form::Define { global } => (Some((&ed.slots[0], global)), &ed.slots[1]),
                _ => (None, &ed.slots[0]),
            };
            // the unit placeholder only shows while the cursor is in it
            let unit = ed.unit_expr().filter(|u| !editor::is_ph(u) || ed.cur.path[0] == ed.base_slots());
            let shown = match ed.form {
                editor::Form::Eval => Some(unit),
                editor::Form::Define { .. } if ed.show => Some(unit),
                _ => None,
            };
            ts.statement_ed(lhs, body, shown, result, fs)
        });
        let origin = pos + Vec2::new(2.0, 3.0 + lay.asc);
        let addr = es.ed.addr_map();
        let mats = es.ed.matrix_order();
        let mut rects: HashMap<Path, Rect> = HashMap::new();
        let mut hits: Vec<(Node, Rect)> = Vec::new();
        for (n, r) in &lay.boxes {
            let r = r.translate(origin.to_vec2());
            match n {
                Node::Expr(a) => {
                    if let Some(p) = addr.get(a) {
                        rects.insert(p.clone(), r);
                    }
                }
                Node::Cell { mat, row, col } => {
                    if let Some(o) = mats.iter().position(|m| m == mat) {
                        hits.push((Node::Cell { mat: o, row: *row, col: *col }, r));
                    }
                }
                _ => {}
            }
        }
        self.hits.insert(id, hits.clone());
        let region_rect = Rect::from_min_size(pos, Vec2::new(lay.w + 4.0, lay.h() + 6.0));
        let unit_h = ctx.fonts(|f| f.layout_no_wrap("x".into(), FontId::proportional(100.0), Color32::BLACK).size().y) / 100.0;

        // ---- interaction
        let resp = ui.interact(region_rect, key_id, Sense::click());
        if !dialogs {
            ctx.memory_mut(|m| {
                if !m.has_focus(key_id) {
                    m.request_focus(key_id);
                }
                m.set_focus_lock_filter(key_id, egui::EventFilter { tab: true, horizontal_arrows: true, vertical_arrows: true, escape: true });
            });
        }
        if resp.hovered() {
            ctx.set_cursor_icon(CursorIcon::Text);
        }
        let click = self.edit_click.take().or_else(|| if resp.clicked() { resp.interact_pointer_pos() } else { None });
        if let Some(p) = click {
            if let Some((Node::Cell { .. }, _)) = hits.iter().find(|(n, r)| matches!(n, Node::Cell { .. }) && r.expand(1.0).contains(p)) {
                // a matrix cell: continue in the in-place cell editor
                self.begin_inplace(&ctx, id, Some(p));
                if self.cell.is_some() {
                    return Some((region_rect, error, false));
                }
            }
            let stops = es.ed.stops();
            let best = stops
                .iter()
                .filter_map(|c| stop_geom(&ctx, &es.ed, c, &rects, unit_h).map(|g| (c, g)))
                .map(|(c, (x, y0, y1))| {
                    let dy = if p.y < y0 { y0 - p.y } else if p.y > y1 { p.y - y1 } else { 0.0 };
                    (c, (x - p.x).abs() + 3.0 * dy)
                })
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .map(|(c, _)| c.clone());
            if let Some(c) = best {
                es.ed.set_cursor(c);
            }
        }

        // ---- paint
        let painter = ui.painter();
        painter.rect_filled(region_rect, 0.0, Color32::WHITE);
        lay.paint(painter, origin);
        let blue = Stroke::new(1.5_f32, SELECT_BLUE);
        let sp = es.ed.selected_path();
        let sel_rect = rects.get(&sp).copied();
        if let Some(r) = sel_rect {
            let y = r.bottom() + 1.5;
            painter.line_segment([Pos2::new(r.left(), y), Pos2::new(r.right(), y)], blue);
        }
        if let Some((x, y0, y1)) = stop_geom(&ctx, &es.ed, &es.ed.cur, &rects, unit_h) {
            let x = match (es.ed.insert_left, sel_rect) {
                (true, Some(r)) => r.left(),
                _ => x,
            };
            painter.line_segment([Pos2::new(x, y0 - 1.0), Pos2::new(x, y1 + 2.0)], blue);
        }

        if let Some(enter) = finish {
            self.finish_edit(enter);
            return Some((region_rect, error, true));
        }
        self.ed = Some(es);
        Some((region_rect, error, false))
    }
}
