//! Matrix dimensions and row/column commands for the active worksheet.

use super::App;
use egui::text::{CCursor, CCursorRange};
use egui::text_edit::TextEditState;
use egui::{Button, Color32, Context, Event, Id, Key, Modifiers, RichText, Stroke, TextEdit, Ui};

const MAX_SIZE: usize = 100;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Control {
    Rows,
    Columns,
    Ok,
    Insert,
    Delete,
    Cancel,
}

const ACTIONS: [(Control, &str); 4] = [
    (Control::Ok, "ОК"),
    (Control::Insert, "Вставить"),
    (Control::Delete, "Удалить"),
    (Control::Cancel, "Отмена"),
];

pub(super) enum Command {
    Apply {
        insert: bool,
        rows: usize,
        cols: usize,
    },
    Cancel,
}

pub(super) struct MatrixDlg {
    fields: [String; 2],
    editing: bool,
    focus: Control,
    focus_ids: [Option<Id>; 6],
    select_field: bool,
    invalid: bool,
}

impl MatrixDlg {
    pub(super) fn new(editing: bool) -> Self {
        let initial = if editing { "1" } else { "3" };
        Self {
            fields: [initial.into(), initial.into()],
            editing,
            focus: Control::Rows,
            focus_ids: [None; 6],
            select_field: true,
            invalid: false,
        }
    }

    fn navigate(&mut self, key: Key, modifiers: Modifiers) {
        use Control::*;
        let controls: &[Control] = if self.editing {
            &[Rows, Columns, Ok, Insert, Delete, Cancel]
        } else {
            &[Rows, Columns, Ok, Cancel]
        };
        let index = controls.iter().position(|&c| c == self.focus).unwrap_or(0);
        let next = match (self.focus, key) {
            (_, Key::Tab) if modifiers.shift => {
                controls[(index + controls.len() - 1) % controls.len()]
            }
            (_, Key::Tab) => controls[(index + 1) % controls.len()],
            (Rows, Key::ArrowDown) => Columns,
            (Columns, Key::ArrowUp) => Rows,
            (Rows, Key::ArrowRight) => Ok,
            (Columns, Key::ArrowRight) if self.editing => Insert,
            (Columns, Key::ArrowRight) => Cancel,
            (Ok, Key::ArrowLeft) => Rows,
            (Insert | Delete | Cancel, Key::ArrowLeft) => Columns,
            (Ok | Insert | Delete, Key::ArrowDown) => controls[index + 1],
            (Insert | Delete | Cancel, Key::ArrowUp) => controls[index - 1],
            _ => self.focus,
        };
        if next != self.focus {
            self.focus = next;
            self.select_field = true;
        }
    }

    fn take_keys(ctx: &Context) -> Vec<(Key, Modifiers)> {
        let mut keys = Vec::new();
        ctx.input_mut(|input| {
            input.events.retain(|event| {
                let Event::Key {
                    key,
                    pressed: true,
                    modifiers,
                    ..
                } = *event
                else {
                    return true;
                };
                let handled = match key {
                    Key::Escape => true,
                    Key::Tab => modifiers == Modifiers::NONE || modifiers == Modifiers::SHIFT,
                    Key::Enter
                    | Key::ArrowUp
                    | Key::ArrowDown
                    | Key::ArrowLeft
                    | Key::ArrowRight => modifiers == Modifiers::NONE,
                    _ => false,
                };
                if handled {
                    keys.push((key, modifiers));
                }
                !handled
            });
        });
        keys
    }

    fn field(&mut self, ui: &mut Ui, control: Control, label: &str) {
        let index = control as usize;
        let id = ui.make_persistent_id(("matrix-size", index));
        self.focus_ids[index] = Some(id);
        let text = &mut self.fields[index];
        if self.focus == control && self.select_field {
            let mut state = TextEditState::load(ui.ctx(), id).unwrap_or_default();
            state.cursor.set_char_range(Some(CCursorRange::two(
                CCursor::new(0),
                CCursor::new(text.chars().count()),
            )));
            state.store(ui.ctx(), id);
            ui.memory_mut(|m| m.request_focus(id));
            self.select_field = false;
        }
        let caption = ui.label(label);
        let response = ui
            .add(
                TextEdit::singleline(text)
                    .id(id)
                    .desired_width(56.0)
                    .return_key(None),
            )
            .labelled_by(caption.id);
        if response.clicked() || response.is_pointer_button_down_on() {
            self.focus = control;
        }
        if response.changed() {
            self.invalid = false;
        }
        ui.end_row();
    }

    fn show(&mut self, ctx: &Context) -> Option<Command> {
        let keys = Self::take_keys(ctx);
        if keys.iter().any(|&(key, _)| key == Key::Escape) {
            return Some(Command::Cancel);
        }

        if let Some(id) = self.focus_ids[self.focus as usize] {
            ctx.memory_mut(|m| {
                if !m.has_focus(id) {
                    m.request_focus(id);
                }
            });
        }

        let frame = egui::Frame::window(&ctx.style())
            .fill(super::PANEL)
            .inner_margin(14);
        let clicked = egui::Modal::new(Id::new("matrix-dialog"))
            .frame(frame)
            .backdrop_color(Color32::from_black_alpha(24))
            .show(ctx, |ui| {
                ui.heading(if self.editing {
                    "Строки и столбцы"
                } else {
                    "Вставка матрицы"
                });
                ui.add_space(6.0);
                let clicked = ui
                    .horizontal_top(|ui| {
                        egui::Grid::new("matrix-dimensions")
                            .spacing([10.0, 7.0])
                            .show(ui, |ui| {
                                self.field(ui, Control::Rows, "Строки");
                                self.field(ui, Control::Columns, "Столбцы");
                            });
                        ui.add_space(10.0);
                        ui.vertical(|ui| {
                            let mut clicked = None;
                            for (control, label) in ACTIONS {
                                let enabled = self.editing
                                    || !matches!(control, Control::Insert | Control::Delete);
                                let button = Button::new(label).min_size(egui::vec2(92.0, 25.0));
                                let button = if self.focus == control {
                                    button.stroke(Stroke::new(1.0_f32, super::SELECT_BLUE))
                                } else {
                                    button.stroke(ui.visuals().widgets.hovered.bg_stroke)
                                };
                                let response = ui.add_enabled(enabled, button);
                                self.focus_ids[control as usize] = Some(response.id);
                                if response.clicked() {
                                    clicked = Some(control);
                                }
                            }
                            clicked
                        })
                        .inner
                    })
                    .inner;

                if self.invalid {
                    let range = if self.editing {
                        "Введите числа от 0 до 100. Хотя бы одно больше нуля."
                    } else {
                        "Введите размеры от 1 до 100."
                    };
                    ui.label(RichText::new(range).color(ui.visuals().error_fg_color));
                }
                clicked
            })
            .inner;
        // Keep navigation inside the dialog, even when a text field owns the caret.
        if let Some(id) = self.focus_ids[self.focus as usize] {
            ctx.memory_mut(|m| {
                m.set_focus_lock_filter(
                    id,
                    egui::EventFilter {
                        tab: true,
                        horizontal_arrows: true,
                        vertical_arrows: true,
                        escape: true,
                    },
                )
            });
        }
        // Let the focused field receive any text from this frame before moving or applying.
        let old_focus = self.focus;
        let mut action = clicked;
        for (key, modifiers) in keys {
            if key == Key::Enter {
                action = Some(match self.focus {
                    Control::Rows | Control::Columns => Control::Ok,
                    button => button,
                });
                break;
            }
            self.navigate(key, modifiers);
        }
        if self.focus != old_focus {
            ctx.request_repaint();
        }
        let action = action?;
        if action == Control::Cancel {
            return Some(Command::Cancel);
        }
        let [rows, cols] = self
            .fields
            .each_ref()
            .map(|text| text.trim().parse::<usize>());
        match (rows, cols) {
            (Ok(rows), Ok(cols))
                if rows <= MAX_SIZE
                    && cols <= MAX_SIZE
                    && (self.editing || (rows > 0 && cols > 0))
                    && (rows > 0 || cols > 0) =>
            {
                Some(Command::Apply {
                    insert: action != Control::Delete,
                    rows,
                    cols,
                })
            }
            _ => {
                self.invalid = true;
                ctx.request_repaint();
                None
            }
        }
    }
}

impl App {
    pub(super) fn matrix_dialog(&mut self, ctx: &Context) -> Option<Command> {
        self.dlg_matrix.as_mut()?.show(ctx)
    }

    pub(super) fn apply_matrix_dialog(&mut self, ctx: &Context, command: Command) {
        let Some(dialog) = self.dlg_matrix.take() else {
            return;
        };
        ctx.memory_mut(|m| m.stop_text_input());
        self.focus_pending = self.editing.is_some();

        if let Command::Apply { insert, rows, cols } = command {
            if dialog.editing {
                self.matrix_edit(ctx, insert, rows, cols);
            } else {
                let row = vec!["■"; cols].join(", ");
                let source = format!("[{}]", vec![row; rows].join("; "));
                if self.editing_math().is_none() {
                    self.new_matrix_region(ctx, source);
                } else {
                    self.insert(ctx, &source);
                    self.linear_to_cells(ctx);
                }
            }
        }
        ctx.request_repaint();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::Action;
    use eframe::App as _;

    fn key(key: Key) -> Event {
        Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        }
    }

    struct TestApp {
        ctx: Context,
        app: App,
        frame: eframe::Frame,
    }

    impl TestApp {
        fn new() -> Self {
            let ctx = Context::default();
            let cc = eframe::CreationContext::_new_kittest(ctx.clone());
            Self {
                ctx,
                app: App::new(&cc, None, false),
                frame: eframe::Frame::_new_kittest(),
            }
        }

        fn tick(&mut self, events: Vec<Event>) {
            let input = egui::RawInput {
                events,
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(800.0, 600.0),
                )),
                ..Default::default()
            };
            let _ = self
                .ctx
                .run(input, |ctx| self.app.update(ctx, &mut self.frame));
        }

        fn open(&mut self) {
            self.app.run(&self.ctx, Action::InsertMatrix);
            self.tick(Vec::new());
            self.tick(Vec::new());
        }

        fn math(&self) -> &str {
            match &self.app.ws.regions[0].kind {
                xmath_core::RegionKind::Math { source } => source,
                _ => panic!("expected a math region"),
            }
        }
    }

    #[test]
    fn keyboard_creates_matrix_and_keeps_invalid_dimensions_in_the_dialog() {
        let mut t = TestApp::new();
        t.tick(vec![Event::Key {
            key: Key::M,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::COMMAND,
        }]);
        t.tick(Vec::new());
        t.tick(vec![Event::Text("0".into()), key(Key::Enter)]);
        assert!(t.app.dlg_matrix.as_ref().unwrap().invalid);
        assert!(t.app.ws.regions.is_empty());

        // Navigate away and back to select the invalid value for replacement.
        t.tick(vec![key(Key::ArrowDown)]);
        t.tick(vec![Event::Text("4".into())]);
        t.tick(vec![key(Key::ArrowRight)]);
        t.tick(vec![key(Key::ArrowLeft)]);
        t.tick(vec![key(Key::ArrowUp)]);
        t.tick(vec![Event::Text("2".into()), key(Key::Enter)]);
        assert!(t.app.dlg_matrix.is_none());
        assert_eq!(t.math(), "[■, ■, ■, ■; ■, ■, ■, ■]");
        assert!(t.app.active_matrix_cell());
    }

    #[test]
    fn insert_delete_and_escape_operate_on_the_active_matrix() {
        let mut t = TestApp::new();
        t.app.new_matrix_region(&t.ctx, "[1, 2; 3, 4]".into());
        t.tick(Vec::new());
        t.open();
        t.tick(vec![key(Key::ArrowDown)]);
        t.tick(vec![Event::Text("0".into())]);
        t.tick(vec![key(Key::ArrowRight)]);
        t.tick(vec![key(Key::Enter)]);
        assert_eq!(t.math(), "[1, 2; ■, ■; 3, 4]");

        t.tick(Vec::new());
        t.open();
        t.tick(vec![key(Key::ArrowRight)]);
        t.tick(vec![key(Key::ArrowDown)]);
        t.tick(vec![key(Key::ArrowDown)]);
        t.tick(vec![key(Key::ArrowLeft)]);
        t.tick(vec![Event::Text("0".into()), key(Key::ArrowRight)]);
        t.tick(vec![key(Key::ArrowDown)]);
        t.tick(vec![key(Key::Enter)]);
        assert_eq!(t.math(), "[1, 2; 3, 4]");

        t.tick(Vec::new());
        t.open();
        t.tick(vec![Event::Text("9".into()), key(Key::Escape)]);
        assert!(t.app.dlg_matrix.is_none());
        assert!(t.app.active_matrix_cell());
        assert_eq!(t.math(), "[1, 2; 3, 4]");
    }
}
