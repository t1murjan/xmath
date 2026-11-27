//! Open worksheets and their placement inside the main window.

use super::*;
use egui::{LayerId, Order, UiBuilder};

pub type DocumentId = u64;

pub(super) struct Document {
    pub(super) id: DocumentId,
    floating: bool,
    pub(super) ws: Worksheet,
    pub(super) outputs: HashMap<u64, RegionOutput>,
    pub(super) dirty: bool,
    pub(super) modified: bool,
    pub(super) path: Option<PathBuf>,
    pub(super) selection: Vec<u64>,
    pub(super) editing: Option<u64>,
    pub(super) focus_pending: bool,
    /// Red crosshair (insertion point) in worksheet coordinates.
    pub(super) cursor: Pos2,
    pub(super) zoom: f32,
    /// Region rectangles from the last frame, in worksheet coordinates.
    pub(super) rects: HashMap<u64, Rect>,
    pub(super) band_start: Option<Pos2>,
    pub(super) plot_buf: Option<(u64, String, String)>,
    pub(super) images: crate::image::ImageCache,
    pub(super) undo: Vec<Worksheet>,
    pub(super) redo: Vec<Worksheet>,
    pub(super) lim_buf: Option<LimBuf>,
    pub(super) eval_tx: mpsc::Sender<EvalDone>,
    pub(super) eval_rx: mpsc::Receiver<EvalDone>,
    pub(super) eval_gen: u64,
    pub(super) evaluating: bool,
    pub(super) force_calc: bool,
    pub(super) dirty_since: Option<Instant>,
    pub(super) edit_snapshot: bool,
    /// Active box of in-place editing (matrix cell / definition target).
    pub(super) cell: Option<inplace::CellEdit>,
    /// Structural (2D) editor of the region being edited.
    pub(super) ed: Option<structural::EdState>,
    /// Click position (screen) that started editing: places the structural cursor.
    pub(super) edit_click: Option<Pos2>,
    /// The edited formula is typed as a line of text (menu toggle, or the structural editor gave up).
    pub(super) line_edit: bool,
    /// Hit boxes (screen coordinates) of the math regions laid out in the last frame.
    pub(super) hits: HashMap<u64, Vec<(Node, Rect)>>,
    pub(super) status: String,
    pub(super) import_log: Vec<String>,
    /// What the last save to .xmcd could not write (shown once in a dialog).
    pub(super) save_warnings: Option<Vec<String>>,
    /// A fresh hash key for each document, including untitled sheets with identical contents.
    #[cfg(all(unix, not(target_os = "android")))]
    pub(super) live_document: std::hash::RandomState,
    /// When the window last had the keyboard focus (seconds since 1970): the MCP server picks
    /// the window used last.
    pub(super) focused_at: f64,
    /// The Mathcad export notes last shown in the dialog (the same notes are not shown again on every Ctrl+S).
    pub(super) shown_warnings: Vec<String>,
    /// Scroll state of the worksheet canvas (screen pixels), from the last frame.
    pub(super) scroll_off: Vec2,
    pub(super) view_min: Pos2,
    pub(super) view_size: Vec2,
    pub(super) content_size: Vec2,
    /// New scroll offset requested by the keyboard / zoom (applied on the next frame).
    pub(super) scroll_req: Option<Vec2>,
    /// Worksheet rectangle to scroll into view on the next frame.
    pub(super) reveal: Option<Rect>,
    /// The rectangle of the edited region last time it was revealed.
    pub(super) last_edit_rect: Option<(u64, Rect)>,
}

impl Document {
    pub(super) fn new(id: DocumentId) -> Self {
        let (eval_tx, eval_rx) = mpsc::channel();
        Self {
            id,
            floating: false,
            ws: Worksheet::default(),
            outputs: HashMap::new(),
            dirty: true,
            modified: false,
            path: None,
            selection: Vec::new(),
            editing: None,
            focus_pending: false,
            cursor: Pos2::new(24.0, 24.0),
            zoom: 1.0,
            rects: HashMap::new(),
            band_start: None,
            plot_buf: None,
            images: Default::default(),
            undo: Vec::new(),
            redo: Vec::new(),
            lim_buf: None,
            eval_tx,
            eval_rx,
            eval_gen: 0,
            evaluating: false,
            force_calc: false,
            dirty_since: None,
            edit_snapshot: false,
            cell: None,
            ed: None,
            edit_click: None,
            line_edit: false,
            hits: HashMap::new(),
            status: String::new(),
            import_log: Vec::new(),
            save_warnings: None,
            #[cfg(all(unix, not(target_os = "android")))]
            live_document: std::hash::RandomState::new(),
            focused_at: 0.0,
            shown_warnings: Vec::new(),
            scroll_off: Vec2::ZERO,
            view_min: Pos2::ZERO,
            view_size: Vec2::new(1000.0, 700.0),
            content_size: Vec2::ZERO,
            scroll_req: None,
            reveal: None,
            last_edit_rect: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SplitAxis {
    SideBySide,
    Stacked,
}

#[derive(Clone, Copy)]
struct Comparison {
    documents: [DocumentId; 2],
    axis: SplitAxis,
    fraction: f32,
}

struct SheetView {
    document: DocumentId,
    layer: LayerId,
    rect: Rect,
}

pub(super) struct Workspace {
    /// The active document lives in App::doc so the editor and commands share the same target.
    inactive: Vec<Document>,
    next_id: DocumentId,
    docked: Option<DocumentId>,
    comparison: Option<Comparison>,
    visible: Vec<SheetView>,
    pub(super) exit_discarded: Vec<DocumentId>,
}

impl Workspace {
    pub(super) fn new() -> Self {
        Self {
            inactive: Vec::new(),
            next_id: 2,
            docked: Some(1),
            comparison: None,
            visible: Vec::new(),
            exit_discarded: Vec::new(),
        }
    }
}

fn floating_window_id(document: DocumentId) -> Id {
    Id::new(("xmath-floating-document", document))
}

impl Document {
    pub(super) fn name(&self) -> String {
        self.path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| format!("Без имени:{}", self.id))
    }

    #[cfg(all(unix, not(target_os = "android")))]
    pub(super) fn live_info(&self, active: bool) -> serde_json::Value {
        serde_json::json!({
            "pid": std::process::id(),
            "document_id": self.id,
            "active": active,
            "title": self.name(),
            "path": self.path.as_ref().and_then(|p| std::path::absolute(p).ok()),
            "modified": self.modified,
            "regions": self.ws.regions.len(),
            "focused_at": self.focused_at,
        })
    }

    fn tab_label(&self) -> String {
        format!("{}{}", self.name(), if self.modified { " *" } else { "" })
    }
}

impl App {
    pub(super) fn documents(&self) -> impl Iterator<Item = &Document> {
        std::iter::once(&self.doc).chain(self.workspace.inactive.iter())
    }

    pub(super) fn document_for_path(&self, path: &Path) -> Option<DocumentId> {
        let canonical = path.canonicalize().ok();
        self.documents().find_map(|doc| {
            let open = doc.path.as_deref()?;
            let same = open == path
                || canonical
                    .as_ref()
                    .is_some_and(|p| open.canonicalize().as_ref().is_ok_and(|q| q == p));
            same.then_some(doc.id)
        })
    }

    pub(super) fn with_document<T>(
        &mut self,
        id: DocumentId,
        f: impl FnOnce(&mut Self) -> T,
    ) -> Option<T> {
        if self.doc.id == id {
            return Some(f(self));
        }
        let index = self.workspace.inactive.iter().position(|d| d.id == id)?;
        std::mem::swap(&mut self.doc, &mut self.workspace.inactive[index]);
        let value = f(self);
        std::mem::swap(&mut self.doc, &mut self.workspace.inactive[index]);
        Some(value)
    }

    fn activate_document(&mut self, ctx: &Context, id: DocumentId) {
        if self.doc.id == id {
            return;
        }
        let Some(index) = self.workspace.inactive.iter().position(|d| d.id == id) else {
            return;
        };
        self.finish_edit(false);
        self.doc.focus_pending = false;
        ctx.memory_mut(|m| m.stop_text_input());
        std::mem::swap(&mut self.doc, &mut self.workspace.inactive[index]);
        self.mark_document_focused();
        ctx.request_repaint();
    }

    pub(super) fn mark_document_focused(&mut self) {
        self.doc.focused_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs_f64())
            .unwrap_or_default();
    }

    pub(super) fn show_document(&mut self, ctx: &Context, id: DocumentId) {
        self.activate_document(ctx, id);
        if self.doc.id != id {
            return;
        }
        if self.doc.floating {
            ctx.move_to_top(LayerId::new(Order::Middle, floating_window_id(id)));
        } else if !self
            .workspace
            .comparison
            .is_some_and(|split| split.documents.contains(&id))
        {
            self.workspace.docked = Some(id);
            self.workspace.comparison = None;
        }
        ctx.request_repaint();
    }

    pub(super) fn push_document(&mut self, ctx: &Context, ws: Worksheet, path: Option<PathBuf>) {
        let id = self.workspace.next_id;
        self.workspace.next_id += 1;
        let mut document = Document::new(id);
        document.ws = ws;
        document.path = path;
        self.workspace.inactive.push(document);
        self.show_document(ctx, id);
    }

    pub(super) fn request_document_close(&mut self, ctx: &Context, id: DocumentId) {
        if self.documents().any(|d| d.id == id && d.modified) {
            self.show_document(ctx, id);
            self.confirm = Some(Action::CloseDocument(id));
        } else {
            self.close_document(ctx, id);
        }
    }

    pub(super) fn close_document(&mut self, ctx: &Context, id: DocumentId) {
        if self.doc.id == id {
            let next = self
                .workspace
                .comparison
                .and_then(|c| c.documents.into_iter().find(|&other| other != id))
                .or(self.workspace.docked.filter(|&docked| docked != id))
                .or_else(|| {
                    self.workspace
                        .inactive
                        .iter()
                        .max_by_key(|d| d.id)
                        .map(|d| d.id)
                });
            if let Some(next) = next {
                self.activate_document(ctx, next);
            } else {
                self.push_document(ctx, Worksheet::default(), None);
            }
        }
        self.workspace.inactive.retain(|d| d.id != id);

        if self
            .workspace
            .comparison
            .is_some_and(|split| split.documents.contains(&id))
        {
            self.workspace.comparison = None;
        }
        if self.workspace.inactive.is_empty() {
            self.doc.floating = false;
        }
        self.choose_docked();
        ctx.request_repaint();
    }

    fn choose_docked(&mut self) {
        self.workspace.docked = self
            .workspace
            .docked
            .filter(|&id| self.documents().any(|d| d.id == id && !d.floating))
            .or_else(|| self.documents().find(|d| !d.floating).map(|d| d.id));
    }

    pub(super) fn advance_exit(&mut self, ctx: &Context) {
        let next = self
            .documents()
            .find(|d| d.modified && !self.workspace.exit_discarded.contains(&d.id))
            .map(|d| d.id);
        if let Some(id) = next {
            self.show_document(ctx, id);
            self.confirm = Some(Action::Exit);
        } else {
            self.allow_close = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    pub(super) fn dock_document(&mut self, ctx: &Context, id: DocumentId) {
        self.activate_document(ctx, id);
        if self.doc.id != id {
            return;
        }
        self.doc.floating = false;
        self.workspace.comparison = None;
        self.workspace.docked = Some(id);
        ctx.request_repaint();
    }

    pub(super) fn float_document(&mut self, ctx: &Context, id: DocumentId) {
        if self.workspace.inactive.is_empty() {
            return;
        }
        self.activate_document(ctx, id);
        if self.doc.id != id {
            return;
        }
        self.doc.floating = true;
        if self
            .workspace
            .comparison
            .is_some_and(|split| split.documents.contains(&id))
        {
            let other = self
                .workspace
                .comparison
                .take()
                .unwrap()
                .documents
                .into_iter()
                .find(|&other| other != id);
            self.workspace.docked = other;
        }
        self.choose_docked();
        self.show_document(ctx, id);
    }

    pub(super) fn compare_documents(&mut self, ctx: &Context, other: DocumentId, axis: SplitAxis) {
        let active = self.doc.id;
        if active == other {
            return;
        }
        if self
            .with_document(other, |app| {
                app.doc.floating = false;
            })
            .is_none()
        {
            return;
        }
        self.doc.floating = false;
        self.workspace.docked = Some(active);
        self.workspace.comparison = Some(Comparison {
            documents: [active, other],
            axis,
            fraction: 0.5,
        });
        ctx.request_repaint();
    }

    pub(super) fn focus_clicked_sheet(&mut self, ctx: &Context) {
        if self.dialogs_open() {
            return;
        }
        let pos = ctx.input(|i| {
            i.pointer
                .any_pressed()
                .then(|| i.pointer.interact_pos())
                .flatten()
        });
        let Some(pos) = pos else { return };
        let layer = ctx.layer_id_at(pos);
        let target = self
            .workspace
            .visible
            .iter()
            .find(|v| Some(v.layer) == layer && v.rect.contains(pos))
            .map(|v| v.document);
        if let Some(id) = target {
            self.activate_document(ctx, id);
        }
    }

    pub(super) fn next_document(&mut self, ctx: &Context, backwards: bool) {
        let mut ids: Vec<_> = self.documents().map(|d| d.id).collect();
        ids.sort_unstable();
        let index = ids.binary_search(&self.doc.id).unwrap();
        let step = if backwards { ids.len() - 1 } else { 1 };
        self.show_document(ctx, ids[(index + step) % ids.len()]);
    }

    fn document_menu(&self, ui: &mut Ui, document: DocumentId) -> Option<Action> {
        let doc = self.documents().find(|d| d.id == document)?;
        let (label, action) = if doc.floating {
            (
                "Закрепить в рабочей области",
                Action::DockDocument(document),
            )
        } else {
            ("Открепить", Action::FloatDocument(document))
        };
        let multiple = !self.workspace.inactive.is_empty();
        let mut selected = None;
        if ui.add_enabled(multiple, egui::Button::new(label)).clicked() {
            selected = Some(action);
        }
        if multiple {
            for (label, axis) in [
                ("Сравнить слева направо", SplitAxis::SideBySide),
                ("Сравнить сверху вниз", SplitAxis::Stacked),
            ] {
                ui.menu_button(label, |ui| {
                    for other in self.documents().filter(|d| d.id != document) {
                        if ui.button(other.tab_label()).clicked() {
                            selected = Some(Action::CompareDocuments(document, other.id, axis));
                            ui.close_menu();
                        }
                    }
                });
            }
        }
        if self.workspace.comparison.is_some() && ui.button("Один лист").clicked() {
            selected = Some(Action::DockDocument(document));
        }
        if ui.button("Закрыть лист").clicked() {
            selected = Some(Action::CloseDocument(document));
        }
        if selected.is_some() {
            ui.close_menu();
        }
        selected
    }

    pub(super) fn window_menu(&mut self, ui: &mut Ui) -> Option<Action> {
        let mut action = self.document_menu(ui, self.doc.id);
        ui.separator();
        let mut docs: Vec<_> = self.documents().collect();
        docs.sort_by_key(|d| d.id);
        for doc in docs {
            if ui
                .selectable_label(doc.id == self.doc.id, doc.tab_label())
                .clicked()
            {
                action = Some(Action::ShowDocument(doc.id));
                ui.close_menu();
            }
        }
        action
    }

    pub(super) fn document_bar(&mut self, ctx: &Context) {
        if self.workspace.inactive.is_empty() {
            return;
        }
        let enabled = !self.dialogs_open();
        let mut action = None;
        egui::TopBottomPanel::bottom("open-documents")
            .frame(
                egui::Frame::default()
                    .fill(PANEL)
                    .inner_margin(egui::Margin::symmetric(4, 3)),
            )
            .show(ctx, |ui| {
                ui.add_enabled_ui(enabled, |ui| {
                    ui.horizontal(|ui| {
                        if ui
                            .button("+")
                            .on_hover_text("Новый лист (Ctrl+N)")
                            .clicked()
                        {
                            action = Some(Action::New);
                        }
                        ui.menu_button("Расположение", |ui| {
                            if let Some(command) = self.document_menu(ui, self.doc.id) {
                                action = Some(command);
                            }
                        });
                        ui.separator();
                        ScrollArea::horizontal()
                            .scroll_bar_visibility(ScrollBarVisibility::VisibleWhenNeeded)
                            .show(ui, |ui| {
                                let mut docs: Vec<_> = self.documents().collect();
                                docs.sort_by_key(|d| d.id);
                                for doc in docs {
                                    let label = if doc.floating {
                                        format!("↗ {}", doc.tab_label())
                                    } else {
                                        doc.tab_label()
                                    };
                                    let tab = ui.selectable_label(doc.id == self.doc.id, label);
                                    let tab = if let Some(path) = &doc.path {
                                        tab.on_hover_text(path.display().to_string())
                                    } else {
                                        tab
                                    };
                                    if tab.clicked() {
                                        action = Some(Action::ShowDocument(doc.id));
                                    }
                                    if tab.clicked_by(egui::PointerButton::Middle) {
                                        action = Some(Action::CloseDocument(doc.id));
                                    }
                                    tab.context_menu(|ui| {
                                        if let Some(command) = self.document_menu(ui, doc.id) {
                                            action = Some(command);
                                        }
                                    });
                                }
                            });
                    });
                });
            });
        self.pending.extend(action);
    }

    fn draw_document(&mut self, ui: &mut Ui, id: DocumentId) {
        let blocked = id != self.doc.id && self.dialogs_open();
        self.with_document(id, |app| {
            ui.push_id(("worksheet", id), |ui| {
                if blocked {
                    ui.disable();
                }
                app.canvas(ui);
            });
        });
    }

    fn draw_comparison(&mut self, ui: &mut Ui, mut split: Comparison) {
        let rect = ui.available_rect_before_wrap();
        let horizontal = split.axis == SplitAxis::SideBySide;
        let available = if horizontal {
            rect.width()
        } else {
            rect.height()
        };
        let start = if horizontal { rect.left() } else { rect.top() };
        let cursor = start + available * split.fraction;
        let handle = if horizontal {
            Rect::from_min_max(
                Pos2::new(cursor - 3.0, rect.top()),
                Pos2::new(cursor + 3.0, rect.bottom()),
            )
        } else {
            Rect::from_min_max(
                Pos2::new(rect.left(), cursor - 3.0),
                Pos2::new(rect.right(), cursor + 3.0),
            )
        };
        let response = ui.interact(handle, Id::new("worksheet-divider"), Sense::drag());
        let response = response.on_hover_cursor(if horizontal {
            CursorIcon::ResizeHorizontal
        } else {
            CursorIcon::ResizeVertical
        });
        if response.dragged() || response.drag_stopped() {
            if let Some(pos) = ui.input(|input| input.pointer.interact_pos()) {
                let pointer = if horizontal { pos.x } else { pos.y };
                split.fraction = ((pointer - start) / available.max(1.0)).clamp(0.2, 0.8);
                self.workspace.comparison = Some(split);
            }
        }
        ui.painter().rect_filled(
            handle,
            0.0,
            if response.hovered() || response.dragged() {
                SELECT_BLUE
            } else {
                Color32::from_gray(180)
            },
        );

        let panes = if horizontal {
            [
                Rect::from_min_max(rect.min, Pos2::new(handle.left(), rect.bottom())),
                Rect::from_min_max(Pos2::new(handle.right(), rect.top()), rect.max),
            ]
        } else {
            [
                Rect::from_min_max(rect.min, Pos2::new(rect.right(), handle.top())),
                Rect::from_min_max(Pos2::new(rect.left(), handle.bottom()), rect.max),
            ]
        };
        for (id, bounds) in split.documents.into_iter().zip(panes) {
            let Some(doc) = self.documents().find(|d| d.id == id) else {
                continue;
            };
            let label = doc.tab_label();
            let mut pane = ui.new_child(UiBuilder::new().id_salt(id).max_rect(bounds));
            pane.set_clip_rect(ui.clip_rect().intersect(bounds));
            let active = self.doc.id == id;
            egui::Frame::default()
                .fill(if active { SELECT_BLUE } else { PANEL })
                .inner_margin(egui::Margin::symmetric(6, 3))
                .show(&mut pane, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new(label).color(if active {
                        Color32::WHITE
                    } else {
                        Color32::BLACK
                    }));
                });
            self.workspace.visible.push(SheetView {
                document: id,
                layer: pane.layer_id(),
                rect: bounds,
            });
            self.draw_document(&mut pane, id);
        }
    }

    pub(super) fn draw_workspace(&mut self, ctx: &Context) {
        self.workspace.visible.clear();
        let central = egui::CentralPanel::default()
            .frame(egui::Frame::default().fill(Color32::WHITE))
            .show(ctx, |ui| {
                let rect = ui.available_rect_before_wrap();
                if let Some(split) = self.workspace.comparison {
                    self.draw_comparison(ui, split);
                } else if let Some(id) = self.workspace.docked {
                    self.workspace.visible.push(SheetView {
                        document: id,
                        layer: ui.layer_id(),
                        rect,
                    });
                    self.draw_document(ui, id);
                }
                rect
            });
        let bounds = central.inner;
        let mut floating: Vec<_> = self
            .documents()
            .filter(|d| d.floating)
            .map(|d| (d.id, d.tab_label()))
            .collect();
        floating.sort_by_key(|(id, _)| *id);
        let enabled = !self.dialogs_open();
        for (index, (id, label)) in floating.into_iter().enumerate() {
            let mut open = true;
            let mut dock = false;
            let window_id = floating_window_id(id);
            let offset = 24.0 + 26.0 * (index % 6) as f32;
            let default_size = (bounds.size() * 0.66).max(egui::vec2(180.0, 120.0));
            let shown = egui::Window::new(label)
                .id(window_id)
                .collapsible(false)
                .open(&mut open)
                .default_pos(bounds.left_top() + Vec2::splat(offset))
                .default_size(default_size)
                .min_size([180.0, 120.0])
                .constrain_to(bounds)
                .show(ctx, |ui| {
                    ui.set_min_size(ui.available_size());
                    dock = ui
                        .add_enabled(enabled, egui::Button::new("Закрепить"))
                        .clicked();
                    egui::Frame::NONE.fill(Color32::WHITE).show(ui, |ui| {
                        self.draw_document(ui, id);
                    });
                });
            if let Some(shown) = shown {
                self.workspace.visible.push(SheetView {
                    document: id,
                    layer: shown.response.layer_id,
                    rect: shown.response.rect,
                });
            }
            if enabled && (!open || dock) {
                self.pending.push(if dock {
                    Action::DockDocument(id)
                } else {
                    Action::CloseDocument(id)
                });
            }
        }
    }

    pub(super) fn evaluate_documents(&mut self, ctx: &Context) {
        self.drive_evaluation(ctx);
        let visible: Vec<_> = self
            .workspace
            .visible
            .iter()
            .filter_map(|v| (v.document != self.doc.id).then_some(v.document))
            .collect();
        for id in visible {
            self.with_document(id, |app| app.drive_evaluation(ctx));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::App as _;

    fn tick(app: &mut App, ctx: &Context, events: Vec<Event>) {
        let mut frame = eframe::Frame::_new_kittest();
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(1000.0, 700.0))),
            events,
            ..Default::default()
        };
        let _ = ctx.run(input, |ctx| app.update(ctx, &mut frame));
    }

    fn pointer(pos: Pos2, pressed: bool) -> Vec<Event> {
        vec![
            Event::PointerMoved(pos),
            Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: Modifiers::NONE,
            },
        ]
    }

    #[test]
    fn comparison_and_floating_preserve_each_documents_view_and_history() {
        let ctx = Context::default();
        let cc = eframe::CreationContext::_new_kittest(ctx.clone());
        let mut app = App::new(&cc, None, false);
        app.palettes.clear();
        app.doc.ws = Worksheet::from_lines("a := 1");
        let left = app.doc.id;
        app.checkpoint();
        app.doc.ws = Worksheet::from_lines("a := 2");
        app.changed();
        app.doc.zoom = 1.5;

        app.push_document(&ctx, Worksheet::from_lines("b := 3"), None);
        let right = app.doc.id;
        app.run(
            &ctx,
            Action::CompareDocuments(left, right, SplitAxis::SideBySide),
        );
        tick(&mut app, &ctx, Vec::new());
        assert_eq!(app.workspace.visible.len(), 2);
        let [first, second] = app.workspace.visible.as_slice() else {
            unreachable!()
        };
        assert_eq!((first.document, second.document), (left, right));
        assert!(first.rect.right() < second.rect.left());
        let divider = Pos2::new(
            (first.rect.right() + second.rect.left()) * 0.5,
            first.rect.center().y,
        );
        let moved = divider - egui::vec2(115.0, 0.0);
        tick(&mut app, &ctx, pointer(divider, true));
        tick(&mut app, &ctx, vec![Event::PointerMoved(moved)]);
        tick(&mut app, &ctx, pointer(moved, false));
        assert!(app.workspace.comparison.unwrap().fraction < 0.45);
        let second = &app.workspace.visible[1];
        let point = second.rect.right_bottom() - egui::vec2(60.0, 60.0);
        tick(&mut app, &ctx, pointer(point, true));
        tick(&mut app, &ctx, pointer(point, false));
        assert_eq!(app.doc.id, right);
        assert_eq!(app.doc.zoom, 1.0);
        assert!(app.doc.undo.is_empty());

        app.run(&ctx, Action::FloatDocument(right));
        tick(&mut app, &ctx, Vec::new());
        tick(&mut app, &ctx, Vec::new());
        assert!(app.doc.floating);
        assert_eq!(app.workspace.docked, Some(left));
        assert!(app.workspace.comparison.is_none());
        assert!(app
            .workspace
            .visible
            .iter()
            .any(|view| view.document == right));

        app.run(&ctx, Action::DockDocument(right));
        app.run(
            &ctx,
            Action::CompareDocuments(left, right, SplitAxis::Stacked),
        );
        tick(&mut app, &ctx, Vec::new());
        let [first, second] = app.workspace.visible.as_slice() else {
            unreachable!()
        };
        assert!(first.rect.bottom() < second.rect.top());
        assert_eq!(app.doc.zoom, 1.5);
        app.run(&ctx, Action::Undo);
        assert_eq!(
            app.doc.ws.to_json(),
            Worksheet::from_lines("a := 1").to_json()
        );
        app.with_document(right, |app| {
            assert!(matches!(&app.doc.ws.regions[0].kind, RegionKind::Math { source } if source == "b := 3"));
        }).unwrap();
    }

    #[test]
    fn opening_and_saving_target_only_the_selected_document() {
        let ctx = Context::default();
        let cc = eframe::CreationContext::_new_kittest(ctx.clone());
        let mut app = App::new(&cc, None, false);
        let dir = std::env::temp_dir().join(format!("xmath-open-documents-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let first = dir.join("first.xmath");
        let second = dir.join("second.xmath");
        let first_on_disk = Worksheet::from_lines("a := 1").to_json();
        let second_on_disk = Worksheet::from_lines("b := 2").to_json();
        std::fs::write(&first, &first_on_disk).unwrap();
        std::fs::write(&second, &second_on_disk).unwrap();

        let blank = app.doc.id;
        app.load(&ctx, &first);
        let left = app.doc.id;
        assert_ne!(blank, left);
        assert_eq!(app.documents().count(), 1);
        app.doc.ws = Worksheet::from_lines("a := 101");
        app.changed();
        app.load(&ctx, &second);
        let right = app.doc.id;
        assert_ne!(left, right);
        assert_ne!(
            app.region_ui_id(("edit", 1)),
            app.with_document(left, |app| app.region_ui_id(("edit", 1)))
                .unwrap()
        );
        app.doc.ws = Worksheet::from_lines("b := 202");
        app.changed();
        app.save_to(first.clone());
        assert!(app.doc.modified);
        assert_eq!(std::fs::read_to_string(&first).unwrap(), first_on_disk);
        app.run(&ctx, Action::Save);
        assert_eq!(
            std::fs::read_to_string(&second).unwrap(),
            app.doc.ws.to_json()
        );
        assert_eq!(std::fs::read_to_string(&first).unwrap(), first_on_disk);

        app.load(&ctx, &first);
        assert_eq!(app.doc.id, left);
        assert_eq!(app.documents().count(), 2);
        assert!(app.doc.modified);
        app.run(&ctx, Action::CloseDocument(left));
        assert_eq!(app.confirm, Some(Action::CloseDocument(left)));
        assert_eq!(app.documents().count(), 2);
        app.confirm = None;
        app.show_document(&ctx, right);
        app.run(&ctx, Action::Exit);
        assert_eq!(app.doc.id, left);
        assert_eq!(app.confirm, Some(Action::Exit));
        assert!(!app.allow_close);

        std::fs::remove_dir_all(dir).unwrap();
    }

    #[cfg(all(unix, not(target_os = "android")))]
    #[test]
    fn live_write_follows_the_document_after_focus_changes() {
        use serde_json::json;

        let ctx = Context::default();
        let cc = eframe::CreationContext::_new_kittest(ctx.clone());
        let mut app = App::new(&cc, None, false);
        let first = app.doc.id;
        let read = app.handle_live(&json!({"cmd": "get", "document_id": first}));
        app.push_document(&ctx, Worksheet::from_lines("y := 9"), None);
        let second = app.doc.id;
        let list = app.handle_live(&json!({"cmd": "info"}));
        assert_eq!(list["document_id"], second);
        assert_eq!(list["documents"].as_array().unwrap().len(), 2);

        let sheet = Worksheet::from_lines("x := 7");
        let edit = json!({"cmd": "set_if_current", "document_id": first, "revision": read["revision"], "worksheet": sheet});
        assert_eq!(app.handle_live(&edit)["ok"], true);
        assert_eq!(app.doc.id, second);
        assert_eq!(
            app.doc.ws.to_json(),
            Worksheet::from_lines("y := 9").to_json()
        );
        assert_eq!(
            app.handle_live(&json!({"cmd": "get", "document_id": first}))["worksheet"],
            json!(sheet)
        );

        app.close_document(&ctx, first);
        assert!(app.handle_live(&edit)["error"].is_string());
    }
}
