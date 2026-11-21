//! Printing, print preview, page setup, PDF / Mathcad export and worksheet references in the
//! window application. This is a child module of `app`, so it can reach the private state of
//! [`App`]; the printing itself lives in `crate::print` and `crate::pdf`.

use super::*;
use crate::print::{self, PdfOptions, Sheet};
use xmath_core::worksheet::MM_TO_PX;
use xmath_core::{EvalEnv, PageSetup, Paper};

/// The state of the windows of this module.
#[derive(Default)]
pub struct PrintUi {
    preview: Option<Preview>,
    setup: Option<PageSetup>,
    print_dlg: bool,
    reference: Option<RefDlg>,
}

impl PrintUi {
    pub fn any_open(&self) -> bool {
        self.preview.is_some() || self.setup.is_some() || self.print_dlg || self.reference.is_some()
    }
}

struct Preview {
    page: usize,
    /// Zoom in percent (100 = 96 dpi).
    zoom: u32,
    sheet: Option<(PreviewKey, Sheet)>,
}

#[derive(PartialEq, Clone)]
struct PreviewKey {
    gen: u64,
    zoom: u32,
    page: PageSetup,
    regions: usize,
}

struct RefDlg {
    /// The region being edited (`None`: a new reference).
    id: Option<u64>,
    text: String,
}

impl App {
    /// File access rules for evaluating the worksheet: its own directory when it is saved.
    pub(super) fn eval_env(&self) -> EvalEnv {
        match self.doc.path.as_ref().and_then(|p| p.parent()).filter(|d| !d.as_os_str().is_empty()) {
            Some(d) => EvalEnv::in_dir(d),
            None => match self.doc.path.as_ref() {
                Some(_) => EvalEnv::in_dir("."),
                None => EvalEnv::default(),
            },
        }
    }

    /// Size of the printable area of the page in worksheet pixels.
    pub(super) fn page_px(&self) -> (f32, f32) {
        self.doc.ws.page.printable_px()
    }

    fn pdf_bytes(&self) -> Vec<u8> {
        print::export_pdf(&self.doc.ws, &self.doc.outputs, &PdfOptions { title: self.doc_name().replace(':', "") })
    }

    pub(super) fn export_pdf_to(&mut self, p: PathBuf) {
        let p = if p.extension().is_none() { p.with_extension("pdf") } else { p };
        let bytes = self.pdf_bytes();
        self.doc.status = match std::fs::write(&p, bytes) {
            Ok(()) => format!("PDF сохранён: {}", p.display()),
            Err(e) => format!("Не удалось сохранить PDF: {e}"),
        };
    }

    /// Writes the worksheet as a Mathcad 15 file: plots as Mathcad plot items with the pictures the
    /// program draws, results of the evaluation, xmath's own data in comments. Whatever Mathcad
    /// cannot show exactly is reported once after saving.
    pub(super) fn save_xmcd(&mut self, p: &Path) {
        let mut env = self.eval_env();
        env.allow_write = false;
        let outputs = self.doc.ws.evaluate_with(true, &env);
        let has_plots = self.doc.ws.regions.iter().any(|r| matches!(r.kind, xmath_core::RegionKind::Plot { .. } | xmath_core::RegionKind::Plot3d { .. }));
        let renderings = if has_plots { print::render_plot_pngs(&self.doc.ws, &outputs) } else { Default::default() };
        let opts = xmath_core::xmcd_export::ExportOptions { renderings, rendering_scale: print::PNG_SCALE, env: Some(env), outputs: Some(outputs) };
        let r = xmath_core::xmcd_export::export_xmcd_with(&self.doc.ws, &opts);
        match std::fs::write(p, r.xml) {
            Ok(()) => {
                self.doc.path = Some(p.to_path_buf());
                self.doc.modified = false;
                if r.warnings.is_empty() {
                    self.doc.status = format!("Сохранено: {}", p.display());
                } else {
                    self.doc.status = format!("Сохранено: {} ({} замечаний при записи в Mathcad 15)", p.display(), r.warnings.len());
                    if r.warnings != self.doc.shown_warnings {
                        self.doc.shown_warnings = r.warnings.clone();
                        self.doc.save_warnings = Some(r.warnings);
                    }
                }
            }
            Err(e) => self.doc.status = format!("Ошибка сохранения: {e}"),
        }
    }

    /// Writes a temporary PDF and hands it to the system: `lp` under Linux (the default
    /// printer), the default PDF application elsewhere (and under Linux without `lp`).
    fn print_now(&mut self) {
        let bytes = self.pdf_bytes();
        let file = std::env::temp_dir().join(format!("xmath-print-{}.pdf", std::process::id()));
        if let Err(e) = std::fs::write(&file, bytes) {
            self.doc.status = format!("Не удалось подготовить печать: {e}");
            return;
        }
        use std::process::Command;
        #[cfg(target_os = "linux")]
        {
            let lp = Command::new("lp").arg(&file).output();
            if matches!(&lp, Ok(o) if o.status.success()) {
                self.doc.status = "Документ отправлен на печать (lp)".into();
                return;
            }
            let why = match lp {
                Ok(o) => String::from_utf8_lossy(&o.stderr).trim().to_string(),
                Err(e) => e.to_string(),
            };
            self.doc.status = match Command::new("xdg-open").arg(&file).spawn() {
                Ok(_) => format!("lp недоступен ({why}); PDF открыт в программе просмотра: {}", file.display()),
                Err(e) => format!("Не удалось напечатать ({why}; xdg-open: {e}). PDF: {}", file.display()),
            };
        }
        #[cfg(target_os = "macos")]
        {
            self.doc.status = match Command::new("open").arg(&file).spawn() {
                Ok(_) => "PDF открыт в программе просмотра: распечатайте его оттуда".into(),
                Err(e) => format!("Не удалось открыть PDF: {e}"),
            };
        }
        #[cfg(windows)]
        {
            self.doc.status = match Command::new("cmd").args(["/C", "start", ""]).arg(&file).spawn() {
                Ok(_) => "PDF открыт в программе просмотра: распечатайте его оттуда".into(),
                Err(e) => format!("Не удалось открыть PDF: {e}"),
            };
        }
        #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
        {
            let _ = Command::new("xdg-open");
            self.doc.status = format!("PDF: {}", file.display());
        }
    }

    pub(super) fn open_reference_dialog(&mut self, id: Option<u64>, text: &str) {
        self.print.reference = Some(RefDlg { id, text: text.to_string() });
    }

    /// The file chosen with «Обзор…» in the reference dialog (relative to the worksheet).
    pub(super) fn reference_chosen(&mut self, p: PathBuf) {
        let rel = self.doc
            .path
            .as_ref()
            .and_then(|w| w.parent())
            .and_then(|d| p.strip_prefix(d).ok())
            .map(|r| r.to_string_lossy().replace('\\', "/"))
            .unwrap_or_else(|| p.to_string_lossy().into_owned());
        match &mut self.print.reference {
            Some(d) => d.text = rel,
            None => self.print.reference = Some(RefDlg { id: None, text: rel }),
        }
    }

    /// The actions of this module.
    pub(super) fn print_action(&mut self, ctx: &Context, a: Action) {
        match a {
            Action::PageSetup => self.print.setup = Some(self.doc.ws.page.clone()),
            Action::PrintPreview => {
                self.finish_edit(false);
                self.print.preview = Some(Preview { page: 0, zoom: 80, sheet: None });
            }
            Action::Print => {
                self.finish_edit(false);
                self.print.print_dlg = true;
            }
            Action::ExportPdf => self.start_dialog(ctx, DlgKind::Pdf, None),
            Action::InsertReference => self.open_reference_dialog(None, ""),
            _ => {}
        }
    }

    /// Draws the windows (preview, page setup, print, reference).
    pub(super) fn print_windows(&mut self, ctx: &Context) {
        let mut act: Option<Action> = None;

        // ------------------------------------------------------------ preview
        if let Some(pv) = self.print.preview.as_mut() {
            let key = PreviewKey { gen: self.doc.eval_gen, zoom: pv.zoom, page: self.doc.ws.page.clone(), regions: self.doc.ws.regions.len() };
            if pv.sheet.as_ref().map_or(true, |(k, _)| *k != key) {
                let sheet = Sheet::layout(ctx, &self.doc.ws, &self.doc.outputs, pv.zoom as f32 / 100.0, self.has_bold_font);
                pv.page = pv.page.min(sheet.page_count() - 1);
                pv.sheet = Some((key, sheet));
            }
            let mut open = true;
            egui::Window::new("Предварительный просмотр")
                .open(&mut open)
                .default_size([820.0, 860.0])
                .resizable(true)
                .show(ctx, |ui| {
                    let n = pv.sheet.as_ref().map_or(1, |(_, s)| s.page_count());
                    ui.horizontal(|ui| {
                        if ui.button("Печать...").clicked() {
                            act = Some(Action::Print);
                        }
                        if ui.button("Экспорт в PDF...").clicked() {
                            act = Some(Action::ExportPdf);
                        }
                        if ui.button("Параметры страницы...").clicked() {
                            act = Some(Action::PageSetup);
                        }
                        ui.separator();
                        if ui.add_enabled(pv.page > 0, egui::Button::new("◄")).clicked() {
                            pv.page -= 1;
                        }
                        ui.label(format!("Страница {} из {n}", pv.page + 1));
                        if ui.add_enabled(pv.page + 1 < n, egui::Button::new("►")).clicked() {
                            pv.page += 1;
                        }
                        ui.separator();
                        egui::ComboBox::from_id_salt("preview-zoom").width(64.0).selected_text(format!("{}%", pv.zoom)).show_ui(ui, |ui| {
                            for z in [50, 60, 80, 100, 125, 150] {
                                ui.selectable_value(&mut pv.zoom, z, format!("{z}%"));
                            }
                        });
                    });
                    ui.separator();
                    let Some((_, sheet)) = &pv.sheet else { return };
                    let z = pv.zoom as f32 / 100.0;
                    let (pw_mm, ph_mm) = sheet.setup.paper_mm();
                    let page_size = Vec2::new(pw_mm * MM_TO_PX * z, ph_mm * MM_TO_PX * z);
                    ScrollArea::both().auto_shrink([false, false]).show(ui, |ui| {
                        let avail = ui.available_width();
                        let pad = 12.0;
                        let (rect, _) = ui.allocate_exact_size(Vec2::new(page_size.x.max(avail - 2.0 * pad) + 2.0 * pad, page_size.y + 2.0 * pad), Sense::hover());
                        let painter = ui.painter_at(rect);
                        painter.rect_filled(rect, 0.0, Color32::from_gray(150));
                        let x0 = rect.left() + ((rect.width() - page_size.x) / 2.0).max(pad);
                        let page = Rect::from_min_size(Pos2::new(x0, rect.top() + pad), page_size);
                        painter.rect_filled(page.translate(Vec2::splat(3.0)), 0.0, Color32::from_black_alpha(70));
                        painter.rect_filled(page, 0.0, Color32::WHITE);
                        let m = sheet.setup.margins;
                        let origin = page.min + Vec2::new(m[0], m[1]) * MM_TO_PX * z;
                        let clip = sheet.page_clip(pv.page, origin, page.left(), page.width());
                        sheet.paint_page(&painter.with_clip_rect(clip.intersect(page)), pv.page, origin);
                        // the margins
                        let faint = Stroke::new(1.0_f32, Color32::from_gray(225));
                        let area = Rect::from_min_size(origin, Vec2::new(sheet.page_w(), sheet.page_h()));
                        painter.rect_stroke(area, 0.0, faint, egui::StrokeKind::Outside);
                    });
                });
            if !open {
                self.print.preview = None;
            }
        }

        // --------------------------------------------------------- page setup
        if let Some(d) = self.print.setup.as_mut() {
            let mut open = true;
            let mut res: Option<bool> = None;
            egui::Window::new("Параметры страницы").collapsible(false).resizable(false).open(&mut open).anchor(Align2::CENTER_CENTER, Vec2::ZERO).show(ctx, |ui| {
                egui::Grid::new("page-setup").num_columns(2).show(ui, |ui| {
                    ui.label("Бумага:");
                    egui::ComboBox::from_id_salt("paper").selected_text(d.paper.name()).show_ui(ui, |ui| {
                        for p in Paper::ALL {
                            let (w, h) = p.size_mm();
                            ui.selectable_value(&mut d.paper, p, format!("{} ({w:.0} × {h:.0} мм)", p.name()));
                        }
                    });
                    ui.end_row();
                    ui.label("Ориентация:");
                    ui.horizontal(|ui| {
                        ui.radio_value(&mut d.landscape, false, "Книжная");
                        ui.radio_value(&mut d.landscape, true, "Альбомная");
                    });
                    ui.end_row();
                    for (i, name) in ["Поле слева, мм:", "Поле сверху, мм:", "Поле справа, мм:", "Поле снизу, мм:"].into_iter().enumerate() {
                        ui.label(name);
                        ui.add(egui::DragValue::new(&mut d.margins[i]).range(0.0..=100.0).speed(0.5).fixed_decimals(1));
                        ui.end_row();
                    }
                });
                ui.checkbox(&mut d.within_margins, "Печатать только в пределах полей");
                let (pw, ph) = d.printable_px();
                ui.label(RichText::new(format!("Область печати: {pw:.0} × {ph:.0} пикселей рабочего листа")).weak());
                ui.horizontal(|ui| {
                    if ui.button("ОК").clicked() {
                        res = Some(true);
                    }
                    if ui.button("По умолчанию").clicked() {
                        *d = PageSetup::default();
                    }
                    if ui.button("Отмена").clicked() {
                        res = Some(false);
                    }
                });
            });
            if res.is_some() || !open {
                if res == Some(true) {
                    if let Some(d) = self.print.setup.take() {
                        if d != self.doc.ws.page {
                            self.checkpoint();
                            self.doc.ws.page = d;
                            self.changed();
                        }
                    }
                }
                self.print.setup = None;
            }
        }

        // -------------------------------------------------------------- print
        if self.print.print_dlg {
            let mut res: Option<bool> = None;
            let pages = Sheet::layout(ctx, &self.doc.ws, &self.doc.outputs, 1.0, self.has_bold_font).page_count();
            let p = &self.doc.ws.page;
            egui::Window::new("Печать").collapsible(false).resizable(false).anchor(Align2::CENTER_CENTER, Vec2::ZERO).show(ctx, |ui| {
                ui.label(format!("Страниц: {pages}. Бумага: {}, {}.", p.paper.name(), if p.landscape { "альбомная" } else { "книжная" }));
                ui.label("Документ будет отправлен на принтер по умолчанию (команда lp);");
                ui.label("если её нет, PDF откроется в программе просмотра.");
                ui.horizontal(|ui| {
                    if ui.button("Печать").clicked() {
                        res = Some(true);
                    }
                    if ui.button("Отмена").clicked() {
                        res = Some(false);
                    }
                });
            });
            if let Some(ok) = res {
                self.print.print_dlg = false;
                if ok {
                    self.print_now();
                }
            }
        }

        // ---------------------------------------------------------- reference
        if let Some(d) = self.print.reference.as_mut() {
            let mut res: Option<bool> = None;
            let mut browse = false;
            egui::Window::new("Ссылка на рабочий лист").collapsible(false).resizable(false).anchor(Align2::CENTER_CENTER, Vec2::ZERO).show(ctx, |ui| {
                ui.label("Файл (.xmath или .xmcd), путь от папки этого листа:");
                ui.horizontal(|ui| {
                    let r = ui.add(TextEdit::singleline(&mut d.text).desired_width(360.0).hint_text("библиотека/константы.xmath"));
                    if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                        res = Some(true);
                    }
                    if ui.button("Обзор...").clicked() {
                        browse = true;
                    }
                });
                ui.label(RichText::new("Определения из файла будут видны в областях ниже ссылки.").weak());
                ui.horizontal(|ui| {
                    if ui.button("ОК").clicked() {
                        res = Some(true);
                    }
                    if ui.button("Отмена").clicked() {
                        res = Some(false);
                    }
                });
            });
            if browse {
                self.start_dialog(ctx, DlgKind::Reference, None);
            }
            if let Some(ok) = res {
                if let Some(d) = self.print.reference.take() {
                    let href = d.text.trim().to_string();
                    if ok && !href.is_empty() {
                        match d.id {
                            Some(id) => {
                                self.checkpoint();
                                if let Some(Region { kind: RegionKind::Reference { href: h }, .. }) = self.region_mut(id) {
                                    *h = href;
                                }
                                self.changed();
                            }
                            None => {
                                self.new_region(RegionKind::Reference { href }, false);
                            }
                        }
                    }
                }
            }
        }

        if let Some(a) = act {
            self.pending.push(a);
        }
    }
}
