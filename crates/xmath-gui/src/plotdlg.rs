//! Mathcad 15's «Форматирование выбранного графика X-Y»: a modal dialog with the
//! tabs «Оси X, Y», «Трассировка», «Формат числа», «Подписи» and «По умолчанию»,
//! plus the small format dialog of 3D plots. Mounted inside `app`, so it can use
//! the private parts of `App`.

use super::*;
use egui::Painter;
use xmath_core::{AxisStyle, Colormap, LegendPos, LineStyle, NumKind, Symbol, TraceFormat, TraceType};

const BG: Color32 = Color32::from_rgb(240, 240, 240);
const LINE: Color32 = Color32::from_rgb(160, 160, 160);

/// State of the plot format dialog: a working copy that is written back by «ОК» / «Применить».
pub struct PlotDlg {
    pub id: u64,
    pub kind: PlotKind,
    pub fmt: PlotFormat,
    /// What the region currently has (to enable «Применить»).
    pub applied: (PlotFormat, [Option<f64>; 4]),
    pub limits: [Option<f64>; 4],
    /// Autoscale values: x-min, x-max, y-min, y-max, y2-min, y2-max.
    pub auto: [f64; 6],
    pub tab: usize,
    pub y_tab: usize,
    pub use_as_default: bool,
    /// Selected cell of the trace table: (row, column).
    pub cell: Option<(usize, usize)>,
}

impl PlotDlg {
    pub fn new(id: u64, fmt: PlotFormat, kind: PlotKind) -> PlotDlg {
        PlotDlg { id, kind, applied: (fmt.clone(), [None; 4]), fmt, limits: [None; 4], auto: [0.0, 1.0, 0.0, 1.0, 0.0, 1.0], tab: 0, y_tab: 0, use_as_default: false, cell: None }
    }
    fn changed(&self) -> bool {
        self.applied != (self.fmt.clone(), self.limits)
    }
}

// ------------------------------------------------------------ widgets

/// Windows-like controls: bordered white check boxes, bordered buttons.
fn dialog_visuals(ui: &mut Ui) {
    let v = ui.visuals_mut();
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, LINE);
    for w in [&mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active] {
        w.bg_stroke = Stroke::new(1.0_f32, Color32::from_gray(110));
        w.weak_bg_fill = Color32::from_rgb(225, 225, 225);
    }
    v.widgets.inactive.bg_fill = Color32::WHITE;
    v.selection.bg_fill = Color32::from_rgb(0, 120, 215);
    v.extreme_bg_color = Color32::WHITE;
}

fn tabs(ui: &mut Ui, cur: &mut usize, names: &[&str]) -> Rect {
    let mut sel_rect = Rect::NOTHING;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        for (i, n) in names.iter().enumerate() {
            let g = ui.painter().layout_no_wrap(n.to_string(), FontId::proportional(13.0), Color32::BLACK);
            let sel = i == *cur;
            let w = g.size().x + 18.0;
            let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, 24.0), Sense::click());
            let r = if sel { rect } else { Rect::from_min_max(rect.min + Vec2::new(0.0, 2.0), rect.max) };
            let p = ui.painter();
            p.rect_filled(r, 0.0, if sel { BG } else { Color32::from_rgb(232, 232, 232) });
            let st = Stroke::new(1.0_f32, LINE);
            p.line_segment([r.left_bottom(), r.left_top()], st);
            p.line_segment([r.left_top(), r.right_top()], st);
            p.line_segment([r.right_top(), r.right_bottom()], st);
            p.galley(Pos2::new(r.center().x - g.size().x / 2.0, r.center().y - g.size().y / 2.0 + if sel { 0.0 } else { 1.0 }), g, Color32::BLACK);
            if resp.clicked() {
                *cur = i;
            }
            if sel {
                sel_rect = r;
            }
        }
    });
    sel_rect
}

/// A group box with its title cut into the top border.
fn group(ui: &mut Ui, title: &str, width: f32, add: impl FnOnce(&mut Ui)) {
    let r = egui::Frame::NONE.stroke(Stroke::new(1.0_f32, LINE)).inner_margin(egui::Margin { left: 8, right: 8, top: 14, bottom: 8 }).show(ui, |ui| {
        ui.set_width(width - 18.0);
        ui.vertical(|ui| add(ui));
    });
    if title.is_empty() {
        return;
    }
    let rect = r.response.rect;
    let g = ui.painter().layout_no_wrap(title.to_string(), FontId::proportional(13.0), Color32::BLACK);
    let tr = Rect::from_min_size(Pos2::new(rect.left() + 8.0, rect.top() - g.size().y / 2.0), g.size()).expand2(Vec2::new(3.0, 0.0));
    ui.painter().rect_filled(tr, 0.0, BG);
    ui.painter().galley(tr.min + Vec2::new(3.0, 0.0), g, Color32::BLACK);
}

fn swatch(ui: &mut Ui, c: &mut [u8; 3], enabled: bool) -> bool {
    let mut col = *c;
    let r = ui.add_enabled_ui(enabled, |ui| {
        ui.spacing_mut().interact_size = Vec2::new(40.0, 18.0);
        ui.color_edit_button_srgb(&mut col)
    });
    let changed = col != *c;
    *c = col;
    let _ = r;
    changed
}

fn spinner(ui: &mut Ui, v: &mut i64, lo: i64, hi: i64, enabled: bool) {
    ui.add_enabled_ui(enabled, |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = Vec2::ZERO;
            ui.add_sized([44.0, 22.0], egui::DragValue::new(v).range(lo..=hi).speed(0.1));
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing = Vec2::ZERO;
                ui.spacing_mut().button_padding = Vec2::ZERO;
                if ui.add_sized([17.0, 11.0], egui::Button::new(RichText::new("▲").size(7.0))).clicked() {
                    *v = (*v + 1).min(hi);
                }
                if ui.add_sized([17.0, 11.0], egui::Button::new(RichText::new("▼").size(7.0))).clicked() {
                    *v = (*v - 1).max(lo);
                }
            });
        });
    });
}

fn line_text(l: LineStyle) -> &'static str {
    match l {
        LineStyle::Solid => "──────",
        LineStyle::Dot => "······",
        LineStyle::Dash => "─ ─ ─ ─",
        LineStyle::DashDot => "─·─·─·",
        LineStyle::Points => "• • •",
    }
}

const GRID_LINE: Color32 = Color32::from_rgb(190, 190, 190);

/// A static cell of the white grid tables; true when clicked.
fn grid_show(ui: &mut Ui, w: f32, h: f32, header: bool, show: impl FnOnce(&Painter, Rect)) -> bool {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, h), Sense::click());
    let p = ui.painter();
    p.rect_filled(rect, 0.0, if header { Color32::from_rgb(240, 240, 240) } else { Color32::WHITE });
    p.rect_stroke(rect, 0.0, Stroke::new(1.0_f32, GRID_LINE), egui::StrokeKind::Inside);
    show(&ui.painter_at(rect), rect);
    resp.clicked()
}

/// The selected cell: shows its editor.
fn grid_edit(ui: &mut Ui, w: f32, h: f32, edit: impl FnOnce(&mut Ui)) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(w, h), Sense::hover());
    ui.painter().rect_filled(rect, 0.0, Color32::WHITE);
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink2(Vec2::new(1.0, 0.0))).layout(Layout::left_to_right(Align::Center)));
    child.spacing_mut().interact_size.y = h - 2.0;
    edit(&mut child);
    ui.painter().rect_stroke(rect, 0.0, Stroke::new(1.0_f32, Color32::BLACK), egui::StrokeKind::Inside);
}

macro_rules! cellx {
    ($ui:expr, $w:expr, $h:expr, $sel:expr, $clicked:ident, $c:expr, |$p:ident, $r:ident| $show:expr, |$u:ident| $edit:expr) => {
        if $sel {
            grid_edit($ui, $w, $h, |$u| $edit);
        } else if grid_show($ui, $w, $h, false, |$p, $r| $show) {
            $clicked = Some($c);
        }
    };
}

fn right_text(p: &Painter, r: Rect, t: &str) {
    p.text(Pos2::new(r.right() - 4.0, r.center().y), Align2::RIGHT_CENTER, t, FontId::proportional(13.0), Color32::BLACK);
}

fn left_text(p: &Painter, r: Rect, t: &str, c: Color32) {
    p.text(Pos2::new(r.left() + 4.0, r.center().y), Align2::LEFT_CENTER, t, FontId::proportional(13.0), c);
}

// -------------------------------------------------------------- tabs

fn axes_tab(ui: &mut Ui, d: &mut PlotDlg) {
    ui.checkbox(&mut d.fmt.y2_axis, "Включить дополнительную ось Y");
    ui.add_space(4.0);
    ui.horizontal_top(|ui| {
        group(ui, "Ось X", 250.0, |ui| axis_options(ui, d, 0));
        ui.add_space(8.0);
        ui.vertical(|ui| {
            let mut t = d.y_tab;
            let sel = tabs(ui, &mut t, &["Основная ось Y", "Дополнительная ось Y"]);
            let _ = sel;
            d.y_tab = t;
            let k = 1 + d.y_tab;
            group(ui, "", 250.0, |ui| {
                ui.add_enabled_ui(k == 1 || d.fmt.y2_axis, |ui| axis_options(ui, d, k));
            });
        });
    });
    ui.add_space(8.0);
    group(ui, "Отображение осей", 520.0, |ui| {
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.radio_value(&mut d.fmt.axes, AxisStyle::Boxed, "По краям");
                ui.radio_value(&mut d.fmt.axes, AxisStyle::Crossed, "По центру");
                ui.radio_value(&mut d.fmt.axes, AxisStyle::None, "Не отображать");
            });
            ui.add_space(70.0);
            ui.checkbox(&mut d.fmt.equal_scale, "В одинаковом масштабе");
        });
    });
}

fn axis_options(ui: &mut Ui, d: &mut PlotDlg, k: usize) {
    ui.set_min_width(228.0);
    let log = match k {
        0 => &mut d.fmt.log_x,
        1 => &mut d.fmt.log_y,
        _ => &mut d.fmt.log_y2,
    };
    ui.checkbox(log, "Логарифмический масштаб");
    ui.horizontal(|ui| {
        let mut g = d.fmt.axis[k].grid.unwrap_or(d.fmt.grid);
        if ui.checkbox(&mut g, "Линии сетки").changed() {
            d.fmt.axis[k].grid = Some(g);
            if k == 0 {
                d.fmt.grid = g;
            }
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            swatch(ui, &mut d.fmt.axis[k].grid_color, true);
        });
    });
    ui.checkbox(&mut d.fmt.axis[k].numbering, "Нумерация");
    // autoscale: on while the axis has no fixed limits
    let (lo_i, hi_i) = match k {
        0 => (0, 1),
        1 => (2, 3),
        _ => (4, 5),
    };
    let get = |d: &PlotDlg, i: usize| if i < 4 { d.limits[i] } else if i == 4 { d.fmt.y2_min } else { d.fmt.y2_max };
    let mut auto = get(d, lo_i).is_none() && get(d, hi_i).is_none();
    if ui.checkbox(&mut auto, "Автомасштабирование").changed() {
        for (i, a) in [(lo_i, d.auto[lo_i]), (hi_i, d.auto[hi_i])] {
            let v = if auto { None } else { Some(a) };
            if i < 4 {
                d.limits[i] = v;
            } else if i == 4 {
                d.fmt.y2_min = v;
            } else {
                d.fmt.y2_max = v;
            }
        }
    }
    ui.horizontal(|ui| {
        ui.checkbox(&mut d.fmt.axis[k].markers, "Показывать метки");
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            swatch(ui, &mut d.fmt.axis[k].marker_color, true);
        });
    });
    if d.fmt.axis[k].markers {
        ui.horizontal(|ui| {
            ui.label("   Значения:");
            ui.add(TextEdit::singleline(&mut d.fmt.axis[k].marker_values).desired_width(110.0).hint_text("0.5, 2"));
        });
    }
    ui.checkbox(&mut d.fmt.axis[k].autogrid, "Автосетка");
    ui.horizontal(|ui| {
        ui.label("Количество сеток:");
        let mut n = d.fmt.axis[k].grids as i64;
        spinner(ui, &mut n, 1, 100, !d.fmt.axis[k].autogrid);
        d.fmt.axis[k].grids = n as u32;
    });
}

fn traces_tab(ui: &mut Ui, d: &mut PlotDlg) {
    let widths = [230.0, 66.0, 62.0, 70.0, 82.0, 70.0, 56.0, 100.0, 54.0];
    let heads = ["Обозначение в легенде", "Частота символов", "Символ", "Ширина символа", "Линия", "Ширина линии", "Цвет", "Тип", "Ось Y"];
    let rows = d.fmt.traces.len().max(16);
    egui::Frame::NONE.stroke(Stroke::new(1.0_f32, GRID_LINE)).show(ui, |ui| {
        super::classic_scrollbars(ui);
        egui::ScrollArea::both().id_salt("pf-traces").scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible).max_height(214.0).max_width(512.0).auto_shrink(false).show(ui, |ui| {
            ui.spacing_mut().item_spacing = Vec2::ZERO;
            ui.horizontal(|ui| {
                for (w, h) in widths.iter().zip(heads) {
                    grid_show(ui, *w, 34.0, true, |p, r| {
                        let g = p.layout(h.to_string(), FontId::proportional(12.0), Color32::BLACK, r.width() - 4.0);
                        p.galley(Pos2::new(r.center().x - g.size().x / 2.0, r.center().y - g.size().y / 2.0), g, Color32::BLACK);
                    });
                }
            });
            for i in 0..rows {
                let mut tf = d.fmt.traces.get(i).cloned().unwrap_or_default();
                let before = tf.clone();
                let (c0, l0, w0) = plot::trace_style(i, &d.fmt);
                let mut color = [c0.r(), c0.g(), c0.b()];
                let mut line = l0;
                let mut lw = w0;
                let mut clicked = None;
                ui.horizontal(|ui| {
                    let sel = |c: usize| d.cell == Some((i, c));
                    let h = 21.0;
                    // 0: legend text
                    cellx!(ui, widths[0], h, sel(0), clicked, 0, |p, r| {
                        if tf.legend.is_empty() {
                            left_text(p, r, &format!("кривая {}", i + 1), Color32::BLACK);
                        } else {
                            left_text(p, r, &tf.legend, Color32::BLACK);
                        }
                    }, |ui| {
                        let r = ui.add(TextEdit::singleline(&mut tf.legend).frame(false).desired_width(widths[0] - 8.0).hint_text(format!("кривая {}", i + 1)));
                        r.request_focus();
                    });
                    // 1: symbol frequency
                    cellx!(ui, widths[1], h, sel(1), clicked, 1, |p, r| right_text(p, r, &tf.symbol_freq.unwrap_or(1).to_string()), |ui| {
                        let mut f = tf.symbol_freq.unwrap_or(1) as i64;
                        ui.add_sized([widths[1] - 4.0, h - 2.0], egui::DragValue::new(&mut f).range(1..=1000));
                        tf.symbol_freq = (f != 1).then_some(f as u32);
                    });
                    // 2: symbol
                    cellx!(ui, widths[2], h, sel(2), clicked, 2, |p, r| {
                        p.text(r.center(), Align2::CENTER_CENTER, tf.symbol.glyph(), FontId::proportional(14.0), Color32::BLACK);
                    }, |ui| {
                        egui::ComboBox::from_id_salt(("pf-sym", i)).width(widths[2] - 22.0).selected_text(tf.symbol.glyph()).show_ui(ui, |ui| {
                            for sy in Symbol::ALL {
                                ui.selectable_value(&mut tf.symbol, sy, if sy == Symbol::None { "нет" } else { sy.glyph() });
                            }
                        });
                    });
                    // 3: symbol width
                    cellx!(ui, widths[3], h, sel(3), clicked, 3, |p, r| right_text(p, r, &format!("{}", tf.symbol_width.unwrap_or(1.0))), |ui| {
                        let mut f = tf.symbol_width.unwrap_or(1.0) as f64;
                        ui.add_sized([widths[3] - 4.0, h - 2.0], egui::DragValue::new(&mut f).range(1.0..=10.0).speed(0.1).max_decimals(1));
                        tf.symbol_width = ((f - 1.0).abs() > 1e-6).then_some(f as f32);
                    });
                    // 4: line style, drawn as a sample
                    cellx!(ui, widths[4], h, sel(4), clicked, 4, |p, r| {
                        plot::sample(p, Pos2::new(r.left() + 6.0, r.center().y), Pos2::new(r.right() - 6.0, r.center().y), c0_of(&color), line, 1.5);
                    }, |ui| {
                        egui::ComboBox::from_id_salt(("pf-line", i)).width(widths[4] - 22.0).selected_text(line_text(line)).show_ui(ui, |ui| {
                            for l in [LineStyle::Solid, LineStyle::Dot, LineStyle::Dash, LineStyle::DashDot, LineStyle::Points] {
                                ui.selectable_value(&mut line, l, line_text(l));
                            }
                        });
                    });
                    // 5: line width
                    cellx!(ui, widths[5], h, sel(5), clicked, 5, |p, r| right_text(p, r, &format!("{}", lw)), |ui| {
                        ui.add_sized([widths[5] - 4.0, h - 2.0], egui::DragValue::new(&mut lw).range(1.0..=10.0).speed(0.1).max_decimals(1));
                    });
                    // 6: colour swatch
                    cellx!(ui, widths[6], h, sel(6), clicked, 6, |p, r| {
                        p.rect_filled(r.shrink2(Vec2::new(8.0, 4.0)), 0.0, c0_of(&color));
                    }, |ui| {
                        ui.spacing_mut().interact_size = Vec2::new(widths[6] - 12.0, h - 6.0);
                        ui.color_edit_button_srgb(&mut color);
                    });
                    // 7: type
                    cellx!(ui, widths[7], h, sel(7), clicked, 7, |p, r| left_text(p, r, tf.kind.name(), Color32::BLACK), |ui| {
                        egui::ComboBox::from_id_salt(("pf-type", i)).width(widths[7] - 22.0).selected_text(tf.kind.name()).show_ui(ui, |ui| {
                            for t in TraceType::ALL {
                                ui.selectable_value(&mut tf.kind, t, t.name());
                            }
                        });
                    });
                    // 8: axis
                    cellx!(ui, widths[8], h, sel(8), clicked, 8, |p, r| left_text(p, r, if tf.y2 { "Y2" } else { "Y1" }, Color32::BLACK), |ui| {
                        let mut y2 = tf.y2 as i32;
                        egui::ComboBox::from_id_salt(("pf-y2", i)).width(widths[8] - 22.0).selected_text(if y2 == 1 { "Y2" } else { "Y1" }).show_ui(ui, |ui| {
                            ui.selectable_value(&mut y2, 0, "Y1");
                            ui.selectable_value(&mut y2, 1, "Y2");
                        });
                        tf.y2 = y2 == 1;
                    });
                });
                if let Some(c) = clicked {
                    d.cell = Some((i, c));
                }
                if color != [c0.r(), c0.g(), c0.b()] {
                    tf.color = Some(color);
                }
                if line != l0 {
                    tf.line = Some(line);
                }
                if (lw - w0).abs() > 1e-6 {
                    tf.width = Some(lw);
                }
                if tf != before {
                    while d.fmt.traces.len() <= i {
                        d.fmt.traces.push(TraceFormat::default());
                    }
                    d.fmt.traces[i] = tf;
                }
            }
        });
    });
    ui.add_space(8.0);
    ui.horizontal_top(|ui| {
        ui.checkbox(&mut d.fmt.hide_args, "Скрыть аргументы");
        ui.add_space(60.0);
        group(ui, "", 268.0, |ui| {
            ui.checkbox(&mut d.fmt.hide_legend, "Скрыть легенду");
            ui.add_enabled_ui(!d.fmt.hide_legend, |ui| {
                egui::Grid::new("pf-legend-pos").num_columns(2).spacing(Vec2::new(30.0, 4.0)).show(ui, |ui| {
                    ui.radio_value(&mut d.fmt.legend_pos, LegendPos::TopLeft, "Сверху слева");
                    ui.radio_value(&mut d.fmt.legend_pos, LegendPos::TopRight, "Сверху справа");
                    ui.end_row();
                    ui.radio_value(&mut d.fmt.legend_pos, LegendPos::BottomLeft, "Снизу слева");
                    ui.radio_value(&mut d.fmt.legend_pos, LegendPos::BottomRight, "Снизу справа");
                    ui.end_row();
                    ui.radio_value(&mut d.fmt.legend_pos, LegendPos::Bottom, "Снизу");
                    ui.end_row();
                });
            });
        });
    });
}

fn c0_of(c: &[u8; 3]) -> Color32 {
    plot::rgb(*c)
}

fn number_tab(ui: &mut Ui, d: &mut PlotDlg) {
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.label("Формат");
            egui::Frame::NONE.fill(Color32::WHITE).stroke(Stroke::new(1.0_f32, LINE)).inner_margin(2).show(ui, |ui| {
                ui.set_min_size(Vec2::new(120.0, 150.0));
                ui.spacing_mut().item_spacing.y = 0.0;
                for (k, n) in [
                    (NumKind::General, "Общие"),
                    (NumKind::Decimal, "Десятичный"),
                    (NumKind::Scientific, "Научный"),
                    (NumKind::Engineering, "Инженерный"),
                    (NumKind::Fraction, "Дробь"),
                ] {
                    ui.selectable_value(&mut d.fmt.number.kind, k, n);
                }
            });
        });
        ui.add_space(24.0);
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                ui.label("Число десятичных знаков");
                let mut p = d.fmt.number.precision as i64;
                spinner(ui, &mut p, 0, 15, true);
                d.fmt.number.precision = p as u32;
            });
            ui.checkbox(&mut d.fmt.number.trailing_zeros, "Показывать конечные нули");
            ui.checkbox(&mut d.fmt.number.eng_exponents, "Показатели степени в\nинженерном формате");
            ui.horizontal(|ui| {
                ui.label("Экспоненциальный порог");
                let mut p = d.fmt.number.exp_threshold as i64;
                spinner(ui, &mut p, 0, 99, true);
                d.fmt.number.exp_threshold = p as i32;
            });
        });
    });
}

fn labels_tab(ui: &mut Ui, d: &mut PlotDlg) {
    group(ui, "Заголовок", 520.0, |ui| {
        ui.add(TextEdit::singleline(&mut d.fmt.title).desired_width(496.0));
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.radio_value(&mut d.fmt.title_bottom, false, "Сверху");
            ui.add_space(50.0);
            ui.radio_value(&mut d.fmt.title_bottom, true, "Снизу");
            ui.add_space(40.0);
            ui.checkbox(&mut d.fmt.show_title, "Показывать заголовок");
        });
    });
    ui.add_space(14.0);
    group(ui, "Подписи осей", 520.0, |ui| {
        for (k, n) in ["Ось X:", "Ось Y:", "Ось Y2:"].iter().enumerate() {
            ui.add_enabled_ui(k < 2 || d.fmt.y2_axis, |ui| {
                ui.horizontal(|ui| {
                    ui.checkbox(&mut d.fmt.labels[k].show, *n);
                    ui.add_space(8.0 + if k == 2 { 0.0 } else { 6.0 });
                    ui.add(TextEdit::singleline(&mut d.fmt.labels[k].text).desired_width(380.0));
                });
            });
        }
    });
}

fn defaults_tab(ui: &mut Ui, d: &mut PlotDlg, doc_defaults: &Option<PlotFormat>) {
    ui.horizontal_top(|ui| {
        if ui.add(egui::Button::new("Использовать умолчания").min_size(Vec2::new(220.0, 22.0))).clicked() {
            d.fmt = doc_defaults.clone().unwrap_or_else(PlotFormat::new_plot_default);
            d.limits = [None; 4];
        }
        ui.add_space(6.0);
        ui.add(egui::Label::new("Параметры графика принимают значения, установленные по умолчанию для этого документа.").wrap());
    });
    ui.add_space(16.0);
    ui.horizontal_top(|ui| {
        ui.allocate_ui(Vec2::new(220.0, 40.0), |ui| {
            ui.checkbox(&mut d.use_as_default, "Использовать в качестве значений по умолчанию");
        });
        ui.add_space(6.0);
        ui.add(egui::Label::new("Текущие параметры графика будут использованы как значения по умолчанию для этого документа.").wrap());
    });
}

/// Result of a dialog frame.
#[derive(PartialEq)]
enum Btn {
    None,
    Ok,
    Cancel,
    Apply,
    Help,
}

fn buttons(ui: &mut Ui, can_apply: bool) -> Btn {
    let mut res = Btn::None;
    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
        let sz = Vec2::new(90.0, 24.0);
        if ui.add(egui::Button::new("Справка").min_size(sz)).clicked() {
            res = Btn::Help;
        }
        if ui.add_enabled(can_apply, egui::Button::new("Применить").min_size(sz)).clicked() {
            res = Btn::Apply;
        }
        if ui.add(egui::Button::new("Отмена").min_size(sz)).clicked() {
            res = Btn::Cancel;
        }
        if ui.add(egui::Button::new("ОК").min_size(sz)).clicked() {
            res = Btn::Ok;
        }
    });
    res
}

/// Windows-like caption: title on the left in a normal font, ✕ on the right. True when ✕ was clicked.
fn caption(ui: &mut Ui, title: &str) -> bool {
    let mut closed = false;
    ui.horizontal(|ui| {
        ui.label(RichText::new(title).size(13.0).color(Color32::BLACK));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let (rect, resp) = ui.allocate_exact_size(Vec2::new(26.0, 20.0), Sense::click());
            if resp.hovered() {
                ui.painter().rect_filled(rect, 0.0, Color32::from_rgb(232, 17, 35));
            }
            let c = if resp.hovered() { Color32::WHITE } else { Color32::BLACK };
            let m = rect.center();
            let st = Stroke::new(1.2_f32, c);
            ui.painter().line_segment([m + Vec2::new(-5.0, -5.0), m + Vec2::new(5.0, 5.0)], st);
            ui.painter().line_segment([m + Vec2::new(-5.0, 5.0), m + Vec2::new(5.0, -5.0)], st);
            closed = resp.clicked();
        });
    });
    ui.add_space(4.0);
    closed
}

/// Blocks clicks on everything behind a modal dialog.
fn block_background(ctx: &Context, name: &str) {
    egui::Area::new(Id::new(name)).order(egui::Order::Middle).fixed_pos(Pos2::ZERO).show(ctx, |ui| {
        let r = ctx.screen_rect();
        ui.allocate_response(r.size(), Sense::click_and_drag());
        ui.painter().rect_filled(r, 0.0, Color32::from_black_alpha(18));
    });
}

impl App {
    /// Opens the format dialog of plot `id` with its current settings.
    pub fn open_plot_dialog(&mut self, id: u64) {
        let Some(Region { kind: RegionKind::Plot { fmt, kind, x_min, x_max, y_min, y_max, .. }, .. }) = self.region(id).cloned() else { return };
        let mut d = PlotDlg::new(id, fmt.clone(), kind);
        d.limits = [x_min, x_max, y_min, y_max];
        d.applied = (fmt.clone(), d.limits);
        let data = match self.outputs.get(&id).map(|o| &o.output) {
            Some(Output::Plot(p)) => p.clone(),
            _ => Default::default(),
        };
        let view = plot::View { x_min: None, x_max: None, y_min: None, y_max: None, fmt: &fmt, aspect: 1.5 };
        let ax = plot::axes(&data, &view);
        let l = ax.limits();
        let l2 = ax.limits2();
        d.auto = [l[0], l[1], l[2], l[3], l2[0], l2[1]];
        self.dlg_plot = Some(d);
    }

    fn apply_plot_dialog(&mut self, d: &mut PlotDlg) {
        while d.fmt.traces.last().is_some_and(|t| *t == TraceFormat::default()) {
            d.fmt.traces.pop();
        }
        if !d.changed() && !d.use_as_default {
            return;
        }
        self.checkpoint();
        let (fmt, lim) = (d.fmt.clone(), d.limits);
        if let Some(Region { kind: RegionKind::Plot { fmt: f, x_min, x_max, y_min, y_max, .. }, .. }) = self.region_mut(d.id) {
            *f = fmt;
            (*x_min, *x_max, *y_min, *y_max) = (lim[0], lim[1], lim[2], lim[3]);
        }
        if d.use_as_default {
            self.ws.plot_defaults = Some(d.fmt.clone());
            d.use_as_default = false;
        }
        d.applied = (d.fmt.clone(), d.limits);
        self.changed();
    }

    pub fn plot_dialogs(&mut self, ctx: &Context) {
        if let Some(mut d) = self.dlg_plot.take() {
            block_background(ctx, "plot-dlg-block");
            let title = if d.kind == PlotKind::Polar { "Форматирование выбранного полярного графика" } else { "Форматирование выбранного графика X-Y" };
            let mut res = Btn::None;
            let defaults = self.ws.plot_defaults.clone();
            let can_apply = d.changed() || d.use_as_default;
            egui::Window::new("plot-format").title_bar(false).order(egui::Order::Foreground).collapsible(false).resizable(false).anchor(Align2::CENTER_CENTER, Vec2::ZERO).frame(
                egui::Frame::window(&ctx.style()).fill(BG).inner_margin(10),
            ).show(ctx, |ui| {
                ui.set_width(548.0);
                dialog_visuals(ui);
                if caption(ui, title) {
                    res = Btn::Cancel;
                }
                let sel = tabs(ui, &mut d.tab, &["Оси X, Y", "Трассировка", "Формат числа", "Подписи", "По умолчанию"]);
                let top = ui.cursor().top();
                egui::Frame::NONE.stroke(Stroke::new(1.0_f32, LINE)).inner_margin(8).show(ui, |ui| {
                    ui.set_min_size(Vec2::new(530.0, 378.0));
                    ui.set_max_size(Vec2::new(530.0, 378.0));
                    match d.tab {
                        0 => axes_tab(ui, &mut d),
                        1 => traces_tab(ui, &mut d),
                        2 => number_tab(ui, &mut d),
                        3 => labels_tab(ui, &mut d),
                        _ => defaults_tab(ui, &mut d, &defaults),
                    }
                });
                ui.painter().line_segment([Pos2::new(sel.left() + 1.0, top), Pos2::new(sel.right() - 1.0, top)], Stroke::new(1.5_f32, BG));
                ui.add_space(8.0);
                let bres = buttons(ui, can_apply);
                if bres != Btn::None {
                    res = bres;
                }
            });
            match res {
                Btn::Ok => {
                    self.apply_plot_dialog(&mut d);
                }
                Btn::Apply => {
                    self.apply_plot_dialog(&mut d);
                    self.dlg_plot = Some(d);
                }
                Btn::Help => {
                    self.show_help = true;
                    self.dlg_plot = Some(d);
                }
                Btn::None => self.dlg_plot = Some(d),
                Btn::Cancel => {}
            }
        }

        if let Some(mut d) = self.dlg_plot3d.take() {
            block_background(ctx, "plot3d-dlg-block");
            let mut res = Btn::None;
            let applied = match self.region(d.id).map(|r| &r.kind) {
                Some(RegionKind::Plot3d { fmt, view, .. }) => (fmt.clone(), *view),
                _ => (d.fmt.clone(), d.view),
            };
            let can_apply = applied != (d.fmt.clone(), d.view);
            let contour = matches!(self.region(d.id).map(|r| &r.kind), Some(RegionKind::Plot3d { kind: Plot3dKind::Contour, .. }));
            let title = if contour { "Форматирование выбранного контурного графика" } else { "Форматирование выбранного 3D графика" };
            let mut tab = ctx.data_mut(|m| *m.get_temp_mut_or_default::<usize>(Id::new("plot3d-tab")));
            egui::Window::new("plot3d-format").title_bar(false).order(egui::Order::Foreground).collapsible(false).resizable(false).anchor(Align2::CENTER_CENTER, Vec2::ZERO).frame(
                egui::Frame::window(&ctx.style()).fill(BG).inner_margin(10),
            ).show(ctx, |ui| {
                ui.set_width(400.0);
                dialog_visuals(ui);
                if caption(ui, title) {
                    res = Btn::Cancel;
                }
                let sel = tabs(ui, &mut tab, &["Вид", "Контур"]);
                let top = ui.cursor().top();
                egui::Frame::NONE.stroke(Stroke::new(1.0_f32, LINE)).inner_margin(8).show(ui, |ui| {
                    ui.set_min_size(Vec2::new(382.0, 210.0));
                    if tab == 0 {
                        ui.horizontal(|ui| {
                            ui.label("Цветовая схема:");
                            egui::ComboBox::from_id_salt("p3-cmap").selected_text(d.fmt.colormap.name()).show_ui(ui, |ui| {
                                for c in Colormap::ALL {
                                    ui.selectable_value(&mut d.fmt.colormap, c, c.name());
                                }
                            });
                        });
                        ui.checkbox(&mut d.fmt.fill, "Заливка");
                        ui.checkbox(&mut d.fmt.wireframe, "Каркас (линии сетки)");
                        ui.checkbox(&mut d.fmt.box_axes, "Оси в рамке с числами");
                        ui.add_space(6.0);
                        egui::Grid::new("p3-view").num_columns(2).show(ui, |ui| {
                            ui.label("Азимут, °:");
                            ui.add(egui::DragValue::new(&mut d.view.azimuth).range(0.0..=360.0).speed(1.0));
                            ui.end_row();
                            ui.label("Наклон, °:");
                            ui.add(egui::DragValue::new(&mut d.view.elevation).range(-90.0..=90.0).speed(1.0));
                            ui.end_row();
                            ui.label("Масштаб:");
                            ui.add(egui::DragValue::new(&mut d.view.zoom).range(0.3..=4.0).speed(0.02).max_decimals(2));
                            ui.end_row();
                        });
                    } else {
                        ui.horizontal(|ui| {
                            ui.label("Число уровней:");
                            let mut n = d.fmt.levels as i64;
                            spinner(ui, &mut n, 1, 100, true);
                            d.fmt.levels = n as u32;
                        });
                        ui.checkbox(&mut d.fmt.labels, "Подписи уровней");
                        ui.checkbox(&mut d.fmt.fill, "Цветная заливка");
                        ui.checkbox(&mut d.fmt.wireframe, "Линии уровней");
                    }
                });
                ui.painter().line_segment([Pos2::new(sel.left() + 1.0, top), Pos2::new(sel.right() - 1.0, top)], Stroke::new(1.5_f32, BG));
                ui.add_space(8.0);
                let bres = buttons(ui, can_apply);
                if bres != Btn::None {
                    res = bres;
                }
            });
            ctx.data_mut(|m| *m.get_temp_mut_or_default::<usize>(Id::new("plot3d-tab")) = tab);
            let apply = |app: &mut App, d: &Plot3dDlg| {
                app.checkpoint();
                if let Some(Region { kind: RegionKind::Plot3d { fmt, view, .. }, .. }) = app.region_mut(d.id) {
                    *fmt = d.fmt.clone();
                    *view = d.view.clamped();
                }
                app.changed();
            };
            match res {
                Btn::Ok => {
                    if can_apply {
                        apply(self, &d);
                    }
                }
                Btn::Apply => {
                    apply(self, &d);
                    self.dlg_plot3d = Some(d);
                }
                Btn::Help => {
                    self.show_help = true;
                    self.dlg_plot3d = Some(d);
                }
                Btn::None => self.dlg_plot3d = Some(d),
                Btn::Cancel => {}
            }
        }
    }
}
