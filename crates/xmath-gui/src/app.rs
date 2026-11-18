//! The main window: Mathcad 15 style menus, toolbars, palettes, worksheet
//! canvas and status bar.

use crate::mathview::{Lay, Node, Ts, RED};
use crate::palettes::{self, Act, Palette};
use crate::plot;
use egui::text::{CCursor, CCursorRange, LayoutJob, TextFormat};
use egui::text_edit::TextEditState;
use egui::scroll_area::ScrollBarVisibility;
use egui::{
    Align, Align2, Color32, Context, CursorIcon, Event, FontFamily, FontId, Id, Key, KeyboardShortcut, Layout,
    Modifiers, Pos2, Rect, RichText, ScrollArea, Sense, Shape, Stroke, TextEdit, Ui, Vec2,
};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};
use xmath_core::{
    FormatOptions, Output, Plot3dData, Plot3dKind, PlotFormat, PlotKind, Region, RegionKind, RegionOutput,
    UnitSystem, Worksheet,
};

#[path = "inplace.rs"]
mod inplace;
#[path = "plotdlg.rs"]
mod plotdlg;
#[path = "print_ui.rs"]
mod print_ui;
#[path = "structural.rs"]
mod structural;

const GRID: f32 = 8.0;
const BASE_PT: f32 = 10.0;

pub const PANEL: Color32 = Color32::from_rgb(236, 233, 216);
const SELECT_BLUE: Color32 = Color32::from_rgb(49, 106, 197);

fn px(pt: f32, zoom: f32) -> f32 {
    pt * 4.0 / 3.0 * zoom
}

fn snap(p: Pos2) -> Pos2 {
    Pos2::new((p.x / GRID).round() * GRID, (p.y / GRID).round() * GRID)
}

fn edit_id(id: u64) -> Id {
    Id::new(("xmath-edit", id))
}

fn char_to_byte(s: &str, ci: usize) -> usize {
    s.char_indices().nth(ci).map(|(b, _)| b).unwrap_or(s.len())
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Action {
    New,
    Open,
    Save,
    SaveAs,
    OpenExample(usize),
    Exit,
    Undo,
    Redo,
    ToggleTextMode,
    Cut,
    Copy,
    Paste,
    Delete,
    SelectAll,
    InsertPlot,
    InsertPolar,
    InsertSurface,
    InsertContour,
    InsertBar,
    InsertScatter,
    InsertMatrix,
    InsertFunction,
    InsertUnit,
    InsertText,
    InsertMath,
    Calculate,
    ToggleAuto,
    ResultFormat,
    WorksheetOptions,
    PlotFormat,
    SeparateRegions,
    AlignAcross,
    AlignDown,
    Help,
    About,
    Zoom(u32),
    ZoomFitWidth,
    PageSetup,
    PrintPreview,
    Print,
    ExportPdf,
    InsertReference,
}

pub static EXAMPLES: &[(&str, &str)] = &[
    ("Знакомство с xmath", include_str!("../../../examples/demo.xmath")),
    ("Балка: прогиб и момент", include_str!("../../../examples/beam.xmath")),
    ("Графики: X-Y, полярные, 3D", include_str!("../../../examples/plots.xmath")),
];

struct MatrixDlg {
    rows: usize,
    cols: usize,
    /// Set by the «Удалить» button.
    delete: bool,
}

struct ListDlg {
    group: usize,
    item: usize,
}

struct FormatDlg {
    opts: FormatOptions,
    tab: usize,
}

/// Format dialog of a 3D plot.
struct Plot3dDlg {
    id: u64,
    fmt: xmath_core::Plot3dFormat,
    view: xmath_core::View3d,
}

/// Tools → Worksheet Options.
struct OptionsDlg {
    tab: usize,
    origin: i32,
    tol: String,
    ctol: String,
    units: UnitSystem,
}

/// An axis-limit box being edited: (plot id, 0..4 = x-min, x-max, y-min, y-max).
struct LimBuf {
    id: u64,
    k: usize,
    text: String,
    changed: bool,
}

/// Result of a background evaluation.
struct EvalDone {
    gen: u64,
    outputs: HashMap<u64, RegionOutput>,
}

#[derive(Clone, Copy, PartialEq)]
enum DlgKind {
    Open,
    Save,
    /// Export to PDF.
    Pdf,
    /// The file of a reference region.
    Reference,
}

/// A native file dialog running on a background thread.
struct FileJob {
    kind: DlgKind,
    rx: mpsc::Receiver<Result<Option<PathBuf>, ()>>,
    started: Instant,
    then: Option<Action>,
}

/// Built-in fallback: a text field for the path when no native dialog exists.
struct PathDlg {
    kind: DlgKind,
    text: String,
    then: Option<Action>,
}

struct Toolbars {
    standard: bool,
    formatting: bool,
    math: bool,
    status: bool,
}

pub struct App {
    ws: Worksheet,
    outputs: HashMap<u64, RegionOutput>,
    dirty: bool,
    modified: bool,
    path: Option<PathBuf>,
    untitled_no: u32,

    selection: Vec<u64>,
    editing: Option<u64>,
    focus_pending: bool,
    /// Red crosshair (insertion point) in worksheet coordinates.
    cursor: Pos2,
    zoom: f32,
    /// Region rectangles from the last frame, in worksheet coordinates.
    rects: HashMap<u64, Rect>,
    band_start: Option<Pos2>,
    plot_buf: Option<(u64, String, String)>,
    images: crate::image::ImageCache,

    undo: Vec<Worksheet>,
    redo: Vec<Worksheet>,
    clipboard: Vec<Region>,

    bars: Toolbars,
    palettes: HashSet<Palette>,
    auto_calc: bool,

    dlg_matrix: Option<MatrixDlg>,
    dlg_function: Option<ListDlg>,
    dlg_unit: Option<ListDlg>,
    dlg_format: Option<FormatDlg>,
    dlg_plot: Option<plotdlg::PlotDlg>,
    dlg_plot3d: Option<Plot3dDlg>,
    dlg_options: Option<OptionsDlg>,
    lim_buf: Option<LimBuf>,
    eval_tx: mpsc::Sender<EvalDone>,
    eval_rx: mpsc::Receiver<EvalDone>,
    eval_gen: u64,
    evaluating: bool,
    force_calc: bool,
    dirty_since: Option<Instant>,
    show_help: bool,
    show_about: bool,
    confirm: Option<Action>,
    allow_close: bool,

    file_job: Option<FileJob>,
    path_dlg: Option<PathDlg>,
    /// Context help of the hovered toolbar/palette button (shown in the status bar).
    hint: String,
    edit_snapshot: bool,
    /// Active box of in-place editing (matrix cell / definition target).
    cell: Option<inplace::CellEdit>,
    /// Structural (2D) editor of the region being edited.
    ed: Option<structural::EdState>,
    /// Click position (screen) that started editing: places the structural cursor.
    edit_click: Option<Pos2>,
    /// The edited formula is typed as a line of text (menu toggle, or the structural editor gave up).
    line_edit: bool,
    /// «Правка → Редактировать как текст».
    text_mode: bool,
    /// Hit boxes (screen coordinates) of the math regions laid out in the last frame.
    hits: HashMap<u64, Vec<(Node, Rect)>>,

    status: String,
    title: String,
    pending: Vec<Action>,
    has_bold_font: bool,
    import_log: Vec<String>,
    /// Debug harness (`XMATH_DUMP_RECTS=path`): writes the rendered region sizes and exits.
    dump_rects: Option<PathBuf>,
    dump_frames: u32,
    /// Scroll state of the worksheet canvas (screen pixels), from the last frame.
    scroll_off: Vec2,
    view_min: Pos2,
    view_size: Vec2,
    content_size: Vec2,
    /// New scroll offset requested by the keyboard / zoom (applied on the next frame).
    scroll_req: Option<Vec2>,
    /// Worksheet rectangle to scroll into view on the next frame.
    reveal: Option<Rect>,
    /// The rectangle of the edited region last time it was revealed.
    last_edit_rect: Option<(u64, Rect)>,
    /// Print preview, page setup, reference dialog.
    print: print_ui::PrintUi,
    /// Interface scale choice (None = auto), cached auto factor, preferences dialog.
    ui_scale: crate::uiscale::Choice,
    ui_scale_auto: Option<f32>,
    show_prefs: bool,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, path: Option<PathBuf>, has_bold_font: bool) -> App {
        let mut palettes = HashSet::new();
        let (eval_tx, eval_rx) = mpsc::channel();
        palettes.insert(Palette::Calculator);
        let mut app = App {
            ws: Worksheet::default(),
            outputs: HashMap::new(),
            dirty: true,
            modified: false,
            path: None,
            untitled_no: 1,
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
            clipboard: Vec::new(),
            bars: Toolbars { standard: true, formatting: true, math: true, status: true },
            palettes,
            auto_calc: true,
            dlg_matrix: None,
            dlg_function: None,
            dlg_unit: None,
            dlg_format: None,
            dlg_plot: None,
            dlg_plot3d: None,
            dlg_options: None,
            lim_buf: None,
            eval_tx,
            eval_rx,
            eval_gen: 0,
            evaluating: false,
            force_calc: false,
            dirty_since: None,
            show_help: false,
            show_about: false,
            confirm: None,
            allow_close: false,
            file_job: None,
            path_dlg: None,
            hint: String::new(),
            edit_snapshot: false,
            cell: None,
            ed: None,
            edit_click: None,
            line_edit: false,
            text_mode: false,
            hits: HashMap::new(),
            status: String::new(),
            title: String::new(),
            pending: Vec::new(),
            has_bold_font,
            import_log: Vec::new(),
            dump_rects: std::env::var_os("XMATH_DUMP_RECTS").map(PathBuf::from),
            dump_frames: 0,
            scroll_off: Vec2::ZERO,
            view_min: Pos2::ZERO,
            view_size: Vec2::new(1000.0, 700.0),
            content_size: Vec2::ZERO,
            scroll_req: None,
            reveal: None,
            last_edit_rect: None,
            print: Default::default(),
            ui_scale: None,
            ui_scale_auto: None,
            show_prefs: false,
        };
        if let Some(st) = cc.storage.and_then(|s| s.get_string("xmath-ui")) {
            app.restore_ui(&st);
        }
        if let Some(p) = path {
            app.load(&p);
        }
        app
    }

    /// `bars:SFMT;palettes:Calculator,Graph` — toolbar visibility and open palettes.
    fn restore_ui(&mut self, st: &str) {
        for part in st.split(';') {
            match part.split_once(':') {
                Some(("bars", v)) => {
                    self.bars.standard = v.contains('S');
                    self.bars.formatting = v.contains('F');
                    self.bars.math = v.contains('M');
                    self.bars.status = v.contains('T');
                }
                Some(("palettes", v)) => {
                    self.palettes = Palette::ALL.iter().copied().filter(|p| v.split(',').any(|n| n == p.title())).collect();
                }
                Some(("auto", v)) => self.auto_calc = v != "0",
                Some(("uiscale", v)) => self.ui_scale = crate::uiscale::parse(v),
                _ => {}
            }
        }
    }

    fn ui_state(&self) -> String {
        let b = &self.bars;
        let mut bars = String::new();
        for (on, c) in [(b.standard, 'S'), (b.formatting, 'F'), (b.math, 'M'), (b.status, 'T')] {
            if on {
                bars.push(c);
            }
        }
        let pal: Vec<&str> = Palette::ALL.iter().filter(|p| self.palettes.contains(p)).map(|p| p.title()).collect();
        format!("bars:{bars};palettes:{};auto:{};uiscale:{}", pal.join(","), self.auto_calc as u8, crate::uiscale::serialize(self.ui_scale))
    }

    // -----------------------------------------------------------------
    // Document helpers

    fn doc_name(&self) -> String {
        match &self.path {
            Some(p) => p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(),
            None => format!("Без имени:{}", self.untitled_no),
        }
    }

    fn checkpoint(&mut self) {
        self.undo.push(self.ws.clone());
        if self.undo.len() > 300 {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    fn changed(&mut self) {
        self.dirty = true;
        self.dirty_since = Some(Instant::now());
        self.modified = true;
    }

    fn region_mut(&mut self, id: u64) -> Option<&mut Region> {
        self.ws.regions.iter_mut().find(|r| r.id == id)
    }

    fn region(&self, id: u64) -> Option<&Region> {
        self.ws.regions.iter().find(|r| r.id == id)
    }

    fn load(&mut self, p: &Path) {
        let ext = p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
        let result: Result<(Worksheet, Vec<String>), String> = std::fs::read_to_string(p)
            .map_err(|e| e.to_string())
            .and_then(|s| {
                if ext == "xmcd" || s.trim_start().starts_with("<?xml") {
                    crate::import::xmcd(&s)
                } else if s.trim_start().starts_with('{') {
                    Worksheet::from_json(&s).map(|w| (w, vec![]))
                } else {
                    Ok((Worksheet::from_lines(&s), vec![]))
                }
            });
        match result {
            Ok((ws, warnings)) => {
                self.ws = ws;
                // an imported Mathcad file is saved as a new .xmath next to it
                self.path = if ext == "xmath" { Some(p.to_path_buf()) } else { None };
                self.after_load();
                self.status = if warnings.is_empty() {
                    format!("Открыт файл {}", p.display())
                } else {
                    format!("Импорт: {} предупреждений (см. Справка → Журнал импорта)", warnings.len())
                };
                self.import_log = warnings;
            }
            Err(e) => self.status = format!("Не удалось открыть {}: {e}", p.display()),
        }
    }

    fn after_load(&mut self) {
        self.undo.clear();
        self.redo.clear();
        self.selection.clear();
        self.editing = None;
        self.ed = None;
        self.modified = false;
        self.dirty = true;
        self.cursor = Pos2::new(24.0, 24.0);
    }

    fn save_to(&mut self, p: PathBuf) {
        if p.extension().is_some_and(|e| e.eq_ignore_ascii_case("xmcd")) {
            self.save_xmcd(&p);
            return;
        }
        let p = if p.extension().is_none() { p.with_extension("xmath") } else { p };
        match std::fs::write(&p, self.ws.to_json()) {
            Ok(()) => {
                self.status = format!("Сохранено: {}", p.display());
                self.path = Some(p);
                self.modified = false;
            }
            Err(e) => self.status = format!("Ошибка сохранения: {e}"),
        }
    }

    /// Saves to the current path, or asks for one.
    fn save_doc(&mut self, ctx: &Context, then: Option<Action>) {
        match self.path.clone() {
            Some(p) => {
                self.save_to(p);
                self.continue_after_save(ctx, then);
            }
            None => self.save_as(ctx, then),
        }
    }

    fn continue_after_save(&mut self, ctx: &Context, then: Option<Action>) {
        if let Some(a) = then {
            if !self.modified {
                self.do_action(ctx, a);
            }
        }
    }

    fn save_as(&mut self, ctx: &Context, then: Option<Action>) {
        self.start_dialog(ctx, DlgKind::Save, then);
    }

    fn open_dialog(&mut self, ctx: &Context) {
        self.start_dialog(ctx, DlgKind::Open, None);
    }

    /// Runs the native dialog (on a thread under Linux, where the xdg portal
    /// may block or be missing; directly elsewhere, macOS wants the main thread).
    fn start_dialog(&mut self, ctx: &Context, kind: DlgKind, then: Option<Action>) {
        if self.file_job.is_some() || self.path_dlg.is_some() {
            return;
        }
        let name = self.doc_name().replace(':', "") + if kind == DlgKind::Pdf { ".pdf" } else { ".xmath" };
        let (tx, rx) = mpsc::channel();
        let run = move || native_dialog(kind, &name);
        #[cfg(target_os = "linux")]
        {
            let c = ctx.clone();
            std::thread::spawn(move || {
                let r = std::panic::catch_unwind(run).map_err(|_| ());
                let _ = tx.send(r);
                c.request_repaint();
            });
            self.status = "Ожидание диалога выбора файла...".into();
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = tx.send(Ok(run()));
        }
        self.file_job = Some(FileJob { kind, rx, started: Instant::now(), then });
        ctx.request_repaint_after(Duration::from_millis(100));
    }

    fn poll_dialog(&mut self, ctx: &Context) {
        let Some(job) = &self.file_job else { return };
        let res = match job.rx.try_recv() {
            Ok(r) => r,
            Err(mpsc::TryRecvError::Empty) => {
                ctx.request_repaint_after(Duration::from_millis(100));
                return;
            }
            Err(mpsc::TryRecvError::Disconnected) => Err(()),
        };
        let job = self.file_job.take().unwrap();
        // a dialog that vanishes within a fraction of a second is a failure
        // (no portal / backend), not the user pressing Cancel
        let failed = res.is_err() || (cfg!(target_os = "linux") && matches!(res, Ok(None)) && job.started.elapsed() < Duration::from_millis(400));
        if failed {
            self.status = "Системный диалог выбора файла недоступен (нет xdg-desktop-portal?). Введите путь вручную.".into();
            let text = match (&self.path, job.kind) {
                (Some(p), DlgKind::Save) => p.display().to_string(),
                (None, DlgKind::Save) => std::env::current_dir().unwrap_or_default().join(self.doc_name().replace(':', "") + ".xmath").display().to_string(),
                (_, DlgKind::Pdf) => std::env::current_dir().unwrap_or_default().join(self.doc_name().replace(':', "") + ".pdf").display().to_string(),
                _ => String::new(),
            };
            self.path_dlg = Some(PathDlg { kind: job.kind, text, then: job.then });
            return;
        }
        self.status.clear();
        if let Some(p) = res.ok().flatten() {
            self.file_chosen(ctx, job.kind, p, job.then);
        }
    }

    fn file_chosen(&mut self, ctx: &Context, kind: DlgKind, p: PathBuf, then: Option<Action>) {
        match kind {
            DlgKind::Open => self.load(&p),
            DlgKind::Save => {
                self.save_to(p);
                self.continue_after_save(ctx, then);
            }
            DlgKind::Pdf => self.export_pdf_to(p),
            DlgKind::Reference => self.reference_chosen(p),
        }
    }

    // -----------------------------------------------------------------
    // Actions

    fn needs_confirm(a: Action) -> bool {
        matches!(a, Action::New | Action::Open | Action::OpenExample(_) | Action::Exit)
    }

    pub fn run(&mut self, ctx: &Context, a: Action) {
        if Self::needs_confirm(a) && self.modified && self.confirm.is_none() {
            self.confirm = Some(a);
            return;
        }
        self.do_action(ctx, a);
    }

    fn do_action(&mut self, ctx: &Context, a: Action) {
        match a {
            Action::New => {
                self.ws = Worksheet::default();
                self.path = None;
                self.untitled_no += 1;
                self.after_load();
            }
            Action::Open => self.open_dialog(ctx),
            Action::Save => self.save_doc(ctx, None),
            Action::SaveAs => self.save_as(ctx, None),
            Action::OpenExample(i) => {
                if let Some((name, src)) = EXAMPLES.get(i) {
                    if let Ok(ws) = Worksheet::from_json(src) {
                        self.ws = ws;
                        self.path = None;
                        self.after_load();
                        self.status = format!("Пример: {name}");
                    }
                }
            }
            Action::Exit => {
                self.allow_close = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            Action::Undo => {
                self.finish_edit(false);
                if let Some(prev) = self.undo.pop() {
                    self.redo.push(std::mem::replace(&mut self.ws, prev));
                    self.selection.clear();
                    self.changed();
                }
            }
            Action::Redo => {
                self.finish_edit(false);
                if let Some(next) = self.redo.pop() {
                    self.undo.push(std::mem::replace(&mut self.ws, next));
                    self.selection.clear();
                    self.changed();
                }
            }
            Action::ToggleTextMode => self.toggle_text_mode(ctx),
            Action::Copy => self.copy(ctx),
            Action::Cut => {
                self.copy(ctx);
                self.delete_selection();
            }
            Action::Paste => self.paste(),
            Action::Delete => self.delete_selection(),
            Action::SelectAll => {
                self.finish_edit(false);
                self.selection = self.ws.regions.iter().map(|r| r.id).collect();
            }
            Action::InsertPlot => {
                let k = self.blank_plot(PlotKind::Xy);
                self.new_region(k, true);
            }
            Action::InsertPolar => {
                let k = self.blank_plot(PlotKind::Polar);
                self.new_region(k, true);
            }
            Action::InsertSurface => {
                self.new_region(RegionKind::new_plot3d(Plot3dKind::Surface), true);
            }
            Action::InsertContour => {
                self.new_region(RegionKind::new_plot3d(Plot3dKind::Contour), true);
            }
            Action::InsertBar => {
                self.new_region(RegionKind::new_plot3d(Plot3dKind::Bar), true);
            }
            Action::InsertScatter => {
                self.new_region(RegionKind::new_plot3d(Plot3dKind::Scatter), true);
            }
            Action::InsertMatrix => self.dlg_matrix = Some(MatrixDlg { rows: if self.active_matrix_cell() { 1 } else { 3 }, cols: if self.active_matrix_cell() { 1 } else { 3 }, delete: false }),
            Action::InsertFunction => self.dlg_function = Some(ListDlg { group: 0, item: 0 }),
            Action::InsertUnit => self.dlg_unit = Some(ListDlg { group: 0, item: 0 }),
            Action::InsertText => {
                self.new_region(RegionKind::new_text(""), true);
            }
            Action::InsertMath => {
                self.new_region(RegionKind::Math { source: String::new() }, true);
            }
            Action::Calculate => {
                self.dirty = true;
                self.force_calc = true;
                self.dirty_since = None;
            }
            Action::ToggleAuto => {
                self.auto_calc = !self.auto_calc;
                self.dirty = true;
            }
            Action::ResultFormat => self.dlg_format = Some(FormatDlg { opts: self.ws.format, tab: 0 }),
            Action::WorksheetOptions => {
                let (origin, tol, ctol) = self.builtin_values();
                self.dlg_options = Some(OptionsDlg { tab: 0, origin, tol, ctol, units: self.ws.format.unit_system });
            }
            Action::PlotFormat => {
                let target = self.editing.into_iter().chain(self.selection.iter().copied()).find(|id| matches!(self.region(*id).map(|r| &r.kind), Some(RegionKind::Plot { .. } | RegionKind::Plot3d { .. })));
                match target.and_then(|id| self.region(id).map(|r| (id, r.kind.clone()))) {
                    Some((id, RegionKind::Plot { .. })) => self.open_plot_dialog(id),
                    Some((id, RegionKind::Plot3d { fmt, view, .. })) => self.dlg_plot3d = Some(Plot3dDlg { id, fmt, view }),
                    _ => self.status = "Выберите график".into(),
                }
            }
            Action::SeparateRegions => self.separate_regions(),
            Action::AlignAcross | Action::AlignDown => self.align(a == Action::AlignAcross),
            Action::Help => self.show_help = true,
            Action::About => self.show_about = true,
            Action::PageSetup | Action::PrintPreview | Action::Print | Action::ExportPdf | Action::InsertReference => self.print_action(ctx, a),
            Action::Zoom(p) => self.zoom_to(p as f32 / 100.0, None),
            Action::ZoomFitWidth => {
                let right = self.rects.values().map(|r| r.right()).fold(self.page_px().0, f32::max);
                let old = self.zoom;
                self.zoom = (((self.view_size.x - 8.0).max(200.0) / (right + 16.0)) * 100.0).floor().clamp(25.0, 400.0) / 100.0;
                self.scroll_req = Some(Vec2::new(0.0, self.scroll_off.y / old * self.zoom));
            }
        }
    }

    fn copy(&mut self, ctx: &Context) {
        let regions: Vec<Region> = self.ws.regions.iter().filter(|r| self.selection.contains(&r.id)).cloned().collect();
        if regions.is_empty() {
            return;
        }
        let text: Vec<String> = regions
            .iter()
            .map(|r| match &r.kind {
                RegionKind::Math { source } => source.clone(),
                RegionKind::Text { text, .. } => text.clone(),
                RegionKind::Plot { x, y, .. } => format!("plot({}; {x})", y.join(", ")),
                RegionKind::Plot3d { expr, .. } => format!("plot3d({expr})"),
                RegionKind::Image { .. } => "[изображение]".to_string(),
                RegionKind::Reference { href } => format!("[ссылка: {href}]"),
            })
            .collect();
        ctx.copy_text(text.join("\n"));
        self.clipboard = regions;
    }

    fn paste(&mut self) {
        if self.clipboard.is_empty() {
            return;
        }
        self.finish_edit(false);
        self.checkpoint();
        let min = self.clipboard.iter().fold(Pos2::new(f32::MAX, f32::MAX), |m, r| Pos2::new(m.x.min(r.x), m.y.min(r.y)));
        let mut next = self.ws.next_id();
        self.selection.clear();
        for r in self.clipboard.clone() {
            let mut r = r;
            r.id = next;
            next += 1;
            r.x = self.cursor.x + (r.x - min.x);
            r.y = self.cursor.y + (r.y - min.y);
            self.selection.push(r.id);
            self.ws.regions.push(r);
        }
        self.changed();
    }

    fn delete_selection(&mut self) {
        if self.selection.is_empty() {
            return;
        }
        self.checkpoint();
        let sel = std::mem::take(&mut self.selection);
        self.ws.regions.retain(|r| !sel.contains(&r.id));
        if self.editing.is_some_and(|e| sel.contains(&e)) {
            self.editing = None;
        }
        self.changed();
    }

    fn separate_regions(&mut self) {
        self.checkpoint();
        let order = self.ws.evaluation_order();
        let mut placed: Vec<Rect> = Vec::new();
        for i in order {
            let r = &self.ws.regions[i];
            let size = self.rects.get(&r.id).map(|q| q.size()).unwrap_or(Vec2::new(60.0, 20.0));
            let mut rect = Rect::from_min_size(Pos2::new(r.x, r.y), size);
            while let Some(hit) = placed.iter().find(|p| p.intersects(rect.shrink(0.5))) {
                let dy = hit.bottom() - rect.top() + GRID;
                rect = rect.translate(Vec2::new(0.0, dy));
            }
            placed.push(rect);
            let r = &mut self.ws.regions[i];
            r.y = rect.top();
        }
        self.changed();
    }

    fn align(&mut self, across: bool) {
        if self.selection.len() < 2 {
            self.status = "Выделите несколько областей (рамкой или Ctrl+щелчок)".into();
            return;
        }
        self.checkpoint();
        let sel = self.selection.clone();
        let rs: Vec<&Region> = self.ws.regions.iter().filter(|r| sel.contains(&r.id)).collect();
        let top = rs.iter().map(|r| r.y).fold(f32::MAX, f32::min);
        let left = rs.iter().map(|r| r.x).fold(f32::MAX, f32::min);
        for r in self.ws.regions.iter_mut().filter(|r| sel.contains(&r.id)) {
            if across {
                r.y = top;
            } else {
                r.x = left;
            }
        }
        self.changed();
    }

    /// A new empty plot with the document's default plot format.
    fn blank_plot(&self, kind: PlotKind) -> RegionKind {
        let mut k = if kind == PlotKind::Polar { RegionKind::new_polar_plot() } else { RegionKind::new_plot() };
        if let (RegionKind::Plot { fmt, .. }, Some(d)) = (&mut k, &self.ws.plot_defaults) {
            *fmt = d.clone();
        } else if let RegionKind::Plot { fmt, .. } = &mut k {
            *fmt = PlotFormat::new_plot_default();
        }
        k
    }

    fn new_region(&mut self, kind: RegionKind, edit: bool) -> u64 {
        self.finish_edit(false);
        self.checkpoint();
        let id = self.ws.next_id();
        let pos = self.cursor;
        self.ws.regions.push(Region::new(id, pos.x, pos.y, kind));
        self.selection = vec![id];
        if edit {
            self.begin_edit(id, false);
        }
        self.changed();
        id
    }

    /// `snapshot`: push an undo state now (already done when the region was just created).
    fn begin_edit(&mut self, id: u64, snapshot: bool) {
        if self.editing == Some(id) {
            return;
        }
        self.finish_edit(false);
        self.editing = Some(id);
        self.line_edit = self.text_mode;
        self.ed = None;
        self.focus_pending = true;
        if snapshot {
            self.undo.push(self.ws.clone());
            self.edit_snapshot = true;
        }
        self.redo.clear();
        match self.region(id).map(|r| &r.kind) {
            Some(RegionKind::Plot { x, y, .. }) => self.plot_buf = Some((id, y.join(", "), x.clone())),
            Some(RegionKind::Plot3d { expr, .. }) => self.plot_buf = Some((id, expr.clone(), String::new())),
            _ => {}
        }
    }

    /// Leaves edit mode; removes the region if it was left empty.
    fn finish_edit(&mut self, move_below: bool) {
        let Some(id) = self.editing.take() else { return };
        self.plot_buf = None;
        self.cell = None;
        self.ed = None;
        self.edit_click = None;
        // entering a region and leaving it unchanged must not leave an undo step behind
        if std::mem::take(&mut self.edit_snapshot) && self.undo.last().is_some_and(|u| u.to_json() == self.ws.to_json()) {
            self.undo.pop();
        }
        let empty = match self.region(id).map(|r| &r.kind) {
            Some(RegionKind::Math { source }) => source.trim().is_empty(),
            Some(RegionKind::Text { text, .. }) => text.trim().is_empty(),
            _ => false,
        };
        if empty {
            self.ws.regions.retain(|r| r.id != id);
            self.selection.retain(|s| *s != id);
            // an edit that created and removed a region leaves nothing to undo
            if self.undo.last().is_some_and(|u| u.regions.iter().all(|r| r.id != id)) {
                self.undo.pop();
            }
            self.dirty = true;
            return;
        }
        if move_below {
            if let (Some(r), Some(rect)) = (self.region(id), self.rects.get(&id)) {
                self.cursor = snap(Pos2::new(r.x, r.y + rect.height() + GRID * 2.0));
            }
        }
    }

    /// Inserts text at the caret of the region being edited, or starts a new
    /// math region at the crosshair. The first `■` placeholder gets selected.
    fn insert(&mut self, ctx: &Context, s: &str) {
        if self.insert_in_cell(ctx, s) {
            return;
        }
        // structural editor: palettes and shortcuts insert templates at its cursor
        if self.ed.is_some() {
            if self.ed_insert(ctx, s) {
                return;
            }
        } else if !self.text_mode && !self.line_edit {
            let id = match self.editing {
                Some(id) if self.editing_math() == Some(id) => Some(id),
                Some(_) => None,
                None => Some(self.new_region(RegionKind::Math { source: String::new() }, true)),
            };
            if let Some(id) = id {
                if self.begin_structural(id, None) && self.ed_insert(ctx, s) {
                    return;
                }
            }
        }
        let id = match self.editing {
            Some(id) if matches!(self.region(id).map(|r| &r.kind), Some(RegionKind::Math { .. } | RegionKind::Text { .. })) => id,
            _ => self.new_region(RegionKind::Math { source: String::new() }, true),
        };
        let te = edit_id(id);
        let mut st = TextEditState::load(ctx, te).unwrap_or_default();
        let Some(region) = self.region_mut(id) else { return };
        let text = match &mut region.kind {
            RegionKind::Math { source } => source,
            RegionKind::Text { text, .. } => text,
            _ => return,
        };
        let len = text.chars().count();
        let (a, b) = st
            .cursor
            .char_range()
            .map(|r| {
                let (x, y) = (r.primary.index.min(len), r.secondary.index.min(len));
                (x.min(y), x.max(y))
            })
            .unwrap_or((len, len));
        let (ba, bb) = (char_to_byte(text, a), char_to_byte(text, b));
        text.replace_range(ba..bb, s);
        let (sa, sb) = match s.chars().position(|c| c == '■') {
            Some(p) => (a + p, a + p + 1),
            None => {
                let e = a + s.chars().count();
                (e, e)
            }
        };
        st.cursor.set_char_range(Some(CCursorRange::two(CCursor::new(sa), CCursor::new(sb))));
        st.store(ctx, te);
        self.focus_pending = true;
        self.changed();
    }

    /// Tab: select the next `■` placeholder.
    fn next_placeholder(&mut self, ctx: &Context, id: u64, back: bool) {
        let te = edit_id(id);
        let Some(Region { kind: RegionKind::Math { source }, .. }) = self.region(id) else { return };
        let chars: Vec<char> = source.chars().collect();
        let mut st = TextEditState::load(ctx, te).unwrap_or_default();
        let range = st.cursor.char_range().map(|r| (r.primary.index.min(r.secondary.index), r.primary.index.max(r.secondary.index)));
        let (lo, hi) = range.unwrap_or((0, 0));
        let target = if back {
            let before = |end: usize| (0..end.min(chars.len())).rev().find(|&i| chars[i] == '■');
            before(lo).or_else(|| before(chars.len()))
        } else {
            let after = |start: usize| (start..chars.len()).find(|&i| chars[i] == '■');
            after(hi).or_else(|| after(0))
        };
        if let Some(p) = target {
            st.cursor.set_char_range(Some(CCursorRange::two(CCursor::new(p), CCursor::new(p + 1))));
            st.store(ctx, te);
            self.focus_pending = true;
        }
    }

    /// Ctrl+B / Ctrl+I / Ctrl+U in a text region: 0 bold, 1 italic, 2 underline.
    fn toggle_style(&mut self, which: u8) {
        let Some(id) = self.format_target() else { return };
        self.checkpoint();
        if let Some(Region { kind: RegionKind::Text { style, .. }, .. }) = self.region_mut(id) {
            match which {
                0 => style.bold = !style.bold,
                1 => style.italic = !style.italic,
                _ => style.underline = !style.underline,
            }
        }
        self.modified = true;
    }

    /// Ctrl+G: converts the letter before the caret to Greek (and back).
    fn greek(&mut self, ctx: &Context, id: u64) {
        let te = edit_id(id);
        let mut st = TextEditState::load(ctx, te).unwrap_or_default();
        let Some(region) = self.region_mut(id) else { return };
        let text = match &mut region.kind {
            RegionKind::Math { source } => source,
            RegionKind::Text { text, .. } => text,
            _ => return,
        };
        let len = text.chars().count();
        let pos = st.cursor.char_range().map(|r| r.primary.index.min(len)).unwrap_or(len);
        if pos == 0 {
            return;
        }
        let c = text.chars().nth(pos - 1).unwrap();
        if let Some(g) = palettes::to_greek(c) {
            let (a, b) = (char_to_byte(text, pos - 1), char_to_byte(text, pos));
            text.replace_range(a..b, &g.to_string());
            st.cursor.set_char_range(Some(CCursorRange::one(CCursor::new(pos))));
            st.store(ctx, te);
            self.focus_pending = true;
            self.changed();
        }
    }

    // -----------------------------------------------------------------
    // Keyboard

    fn editing_math(&self) -> Option<u64> {
        self.editing.filter(|id| matches!(self.region(*id).map(|r| &r.kind), Some(RegionKind::Math { .. })))
    }

    fn dialogs_open(&self) -> bool {
        self.dlg_matrix.is_some()
            || self.dlg_function.is_some()
            || self.dlg_unit.is_some()
            || self.dlg_format.is_some()
            || self.dlg_plot.is_some()
            || self.dlg_plot3d.is_some()
            || self.dlg_options.is_some()
            || self.path_dlg.is_some()
            || self.print.any_open()
            || self.file_job.is_some()
            || self.confirm.is_some()
    }

    /// Sets the worksheet zoom keeping the worksheet point under `anchor` (screen position;
    /// default: the centre of the view) fixed.
    fn zoom_to(&mut self, new: f32, anchor: Option<Pos2>) {
        let new = new.clamp(0.25, 4.0);
        if (new - self.zoom).abs() < 1e-4 {
            return;
        }
        let a = anchor.unwrap_or(self.view_min + self.view_size / 2.0);
        let rel = a - self.view_min;
        let p_ws = (rel + self.scroll_off) / self.zoom;
        let off = (p_ws * new - rel).max(Vec2::ZERO);
        self.zoom = new;
        self.scroll_req = Some(off);
    }

    /// Ctrl+wheel, touchpad pinch, Ctrl+Plus / Ctrl+Minus / Ctrl+0 zoom the whole worksheet.
    fn zoom_input(&mut self, ctx: &Context) {
        const LEVELS: [f32; 9] = [0.25, 0.5, 0.75, 1.0, 1.25, 1.5, 2.0, 3.0, 4.0];
        let (ctrl, raw, pinch, pos) = ctx.input(|i| (i.modifiers.command, i.raw_scroll_delta.y, i.zoom_delta(), i.pointer.hover_pos()));
        if self.dialogs_open() {
            return;
        }
        if pinch != 1.0 || (ctrl && raw != 0.0) {
            let factor = if raw != 0.0 { 1.1f32.powf(raw.signum()) } else { pinch };
            self.zoom_to(self.zoom * factor, pos.filter(|p| Rect::from_min_size(self.view_min, self.view_size).contains(*p)));
        }
        if self.editing_math().is_none() && !ctx.wants_keyboard_input() {
            let (plus, minus, zero) = ctx.input_mut(|i| {
                (
                    i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::Plus)) || i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND | Modifiers::SHIFT, Key::Equals)) || i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::Equals)),
                    i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::Minus)),
                    i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::Num0)),
                )
            });
            if plus {
                let n = LEVELS.iter().copied().find(|l| *l > self.zoom + 0.005).unwrap_or(4.0);
                self.zoom_to(n, None);
            }
            if minus {
                let n = LEVELS.iter().rev().copied().find(|l| *l < self.zoom - 0.005).unwrap_or(0.25);
                self.zoom_to(n, None);
            }
            if zero {
                self.zoom_to(1.0, None);
            }
        }
    }

    fn shortcuts(&mut self, ctx: &Context) {
        self.zoom_input(ctx);
        let ctrl = |k| KeyboardShortcut::new(Modifiers::COMMAND, k);
        let ctrl_shift = |k| KeyboardShortcut::new(Modifiers::COMMAND | Modifiers::SHIFT, k);
        let plain = |k| KeyboardShortcut::new(Modifiers::NONE, k);
        let mut acts = Vec::new();
        let mut fmt: Vec<u8> = Vec::new();
        let kbd_focus = ctx.wants_keyboard_input();
        let in_text_region = self.editing.is_some_and(|id| matches!(self.region(id).map(|r| &r.kind), Some(RegionKind::Text { .. })));
        let editing_text = self.editing.is_some_and(|id| !matches!(self.region(id).map(|r| &r.kind), Some(RegionKind::Plot { .. } | RegionKind::Plot3d { .. })));
        ctx.input_mut(|i| {
            if i.consume_shortcut(&ctrl_shift(Key::S)) {
                acts.push(Action::SaveAs);
            }
            if !i.modifiers.shift && i.consume_shortcut(&ctrl(Key::P)) {
                acts.push(Action::Print);
            }
            for (k, a) in [
                (Key::N, Action::New),
                (Key::O, Action::Open),
                (Key::S, Action::Save),
                (Key::M, Action::InsertMatrix),
                (Key::E, Action::InsertFunction),
                (Key::Num7, Action::InsertPolar),
                (Key::Num2, Action::InsertSurface),
                (Key::Num5, Action::InsertContour),
            ] {
                if i.consume_shortcut(&ctrl(k)) {
                    acts.push(a);
                }
            }
            // Ctrl+U inserts a unit in math, but underlines inside a text region
            if i.consume_shortcut(&ctrl(Key::U)) {
                if in_text_region {
                    fmt.push(2);
                } else {
                    acts.push(Action::InsertUnit);
                }
            }
            if in_text_region {
                if i.consume_shortcut(&ctrl(Key::B)) {
                    fmt.push(0);
                }
                if i.consume_shortcut(&ctrl(Key::I)) {
                    fmt.push(1);
                }
            }
            if i.consume_shortcut(&plain(Key::F9)) {
                acts.push(Action::Calculate);
            }
            if i.consume_shortcut(&plain(Key::F1)) {
                acts.push(Action::Help);
            }
            if i.consume_shortcut(&plain(Key::F2)) {
                acts.push(Action::ToggleTextMode);
            }
            if !editing_text && !kbd_focus {
                if i.consume_shortcut(&ctrl(Key::Z)) {
                    acts.push(Action::Undo);
                }
                if i.consume_shortcut(&ctrl(Key::Y)) {
                    acts.push(Action::Redo);
                }
                if i.consume_shortcut(&ctrl(Key::A)) {
                    acts.push(Action::SelectAll);
                }
                if i.consume_shortcut(&ctrl(Key::D)) {
                    acts.push(Action::Delete);
                }
            }
        });
        for a in acts {
            self.run(ctx, a);
        }
        for f in fmt {
            self.toggle_style(f);
        }

        // Mathcad keystrokes inside a math region
        if let Some(id) = self.editing_math() {
            if !ctx.memory(|m| m.has_focus(edit_id(id))) {
                return;
            }
            let src = match self.region(id).map(|r| &r.kind) {
                Some(RegionKind::Math { source }) => source.clone(),
                _ => String::new(),
            };
            let in_matrix = src.matches('[').count() > src.matches(']').count();
            let mut inserts: Vec<&'static str> = Vec::new();
            let mut greek = false;
            let mut tab = false;
            let mut tab_back = false;
            ctx.input_mut(|i| {
                for (k, s) in [
                    (Key::Num3, "≠"),
                    (Key::Num9, "≤"),
                    (Key::Num0, "≥"),
                    (Key::Equals, "=="),
                    (Key::Num1, "transpose(■)"),
                    (Key::Num8, "cross(■, ■)"),
                    (Key::Backslash, "nthroot(■, ■)"),
                ] {
                    if i.consume_shortcut(&ctrl(k)) {
                        inserts.push(s);
                    }
                }
                if i.consume_shortcut(&ctrl_shift(Key::P)) {
                    inserts.push("π");
                }
                if i.consume_shortcut(&ctrl_shift(Key::Z)) {
                    inserts.push("∞");
                }
                if i.consume_shortcut(&ctrl(Key::G)) {
                    greek = true;
                }
                if i.consume_key(Modifiers::SHIFT, Key::Tab) {
                    tab_back = true;
                } else if i.consume_key(Modifiers::NONE, Key::Tab) {
                    tab = true;
                }
                i.events.retain_mut(|ev| match ev {
                    Event::Text(t) => match t.as_str() {
                        ":" => {
                            *t = ":=".into();
                            true
                        }
                        "~" => {
                            *t = "≡".into();
                            true
                        }
                        ";" if !in_matrix => {
                            *t = "..".into();
                            true
                        }
                        "\\" => {
                            inserts.push("sqrt(■)");
                            false
                        }
                        "?" => {
                            inserts.push("diff(■, ■)");
                            false
                        }
                        "&" => {
                            inserts.push("integrate(■, ■, ■, ■)");
                            false
                        }
                        "$" => {
                            inserts.push("sum(■, ■)");
                            false
                        }
                        "#" => {
                            inserts.push("prod(■, ■)");
                            false
                        }
                        "|" => {
                            inserts.push("|■|");
                            false
                        }
                        _ => true,
                    },
                    _ => true,
                });
            });
            for s in inserts {
                self.insert(ctx, s);
            }
            if greek {
                self.greek(ctx, id);
            }
            if tab || tab_back {
                self.next_placeholder(ctx, id, tab_back);
            }
        } else if let Some(id) = self.editing {
            if ctx.memory(|m| m.has_focus(edit_id(id))) && ctx.input_mut(|i| i.consume_shortcut(&ctrl(Key::G))) {
                self.greek(ctx, id);
            }
        }
    }

    /// Typing on the empty worksheet creates regions, like in Mathcad.
    fn sheet_keys(&mut self, ctx: &Context) {
        if self.editing.is_some() || self.dialogs_open() || ctx.wants_keyboard_input() {
            return;
        }
        let events = ctx.input(|i| i.events.clone());
        let mut typed = String::new();
        for ev in events {
            match ev {
                Event::Text(t) => typed.push_str(&t),
                Event::Key { key, pressed: true, modifiers, .. } if modifiers.is_none() => {
                    let moved = self.cursor;
                    match key {
                        Key::Delete | Key::Backspace => self.delete_selection(),
                        Key::ArrowUp => self.cursor.y = (self.cursor.y - GRID).max(0.0),
                        Key::ArrowDown => self.cursor.y += GRID,
                        Key::ArrowLeft => self.cursor.x = (self.cursor.x - GRID).max(0.0),
                        Key::ArrowRight => self.cursor.x += GRID,
                        Key::Enter => self.cursor.y += GRID * 3.0,
                        Key::PageUp | Key::PageDown => {
                            // scroll by (almost) a screen; the crosshair travels with the view
                            let dy = (self.view_size.y * 0.9 / self.zoom / GRID).round() * GRID;
                            let dy = if key == Key::PageUp { -dy } else { dy };
                            let new_y = (self.cursor.y + dy).max(0.0);
                            let off = (self.scroll_off.y + (new_y - self.cursor.y) * self.zoom).max(0.0);
                            self.scroll_req = Some(Vec2::new(self.scroll_off.x, off));
                            self.cursor.y = new_y;
                        }
                        _ => {}
                    }
                    if self.cursor != moved && !matches!(key, Key::PageUp | Key::PageDown) {
                        self.reveal = Some(Rect::from_center_size(self.cursor, Vec2::splat(GRID * 2.0)));
                    }
                }
                Event::Key { key: key @ (Key::Home | Key::End), pressed: true, modifiers, .. } if modifiers.command && !modifiers.shift => {
                    if key == Key::End {
                        let last = self.rects.values().map(|r| r.bottom()).fold(0.0, f32::max);
                        self.cursor = snap(Pos2::new(24.0, last + 16.0));
                    } else {
                        self.cursor = Pos2::new(24.0, 24.0);
                        self.scroll_req = Some(Vec2::ZERO);
                    }
                    self.reveal = Some(Rect::from_center_size(self.cursor, Vec2::splat(GRID * 2.0)));
                }
                Event::Copy => self.run(ctx, Action::Copy),
                Event::Cut => self.run(ctx, Action::Cut),
                Event::Paste(_) => self.run(ctx, Action::Paste),
                _ => {}
            }
        }
        if typed.is_empty() {
            return;
        }
        if let Some(rest) = typed.strip_prefix('"') {
            let id = self.new_region(RegionKind::new_text(rest), true);
            self.place_caret_end(ctx, id);
        } else if typed.starts_with('@') {
            let k = self.blank_plot(PlotKind::Xy);
            self.new_region(k, true);
        } else {
            if !self.text_mode && typed.trim().is_empty() {
                return;
            }
            if !self.text_mode {
                // the structural editor of the new region reads this frame's keys itself
                self.new_region(RegionKind::Math { source: String::new() }, true);
                return;
            }
            let mapped: String = match typed.as_str() {
                "\\" => "sqrt(■)".into(),
                "?" => "diff(■, ■)".into(),
                "&" => "integrate(■, ■, ■, ■)".into(),
                "|" => "|■|".into(),
                _ => typed.replace(':', ":=").replace('~', "≡"),
            };
            self.insert(ctx, &mapped);
        }
    }

    fn place_caret_end(&mut self, ctx: &Context, id: u64) {
        let len = match self.region(id).map(|r| &r.kind) {
            Some(RegionKind::Math { source }) => source.chars().count(),
            Some(RegionKind::Text { text, .. }) => text.chars().count(),
            _ => 0,
        };
        let mut st = TextEditState::load(ctx, edit_id(id)).unwrap_or_default();
        st.cursor.set_char_range(Some(CCursorRange::one(CCursor::new(len))));
        st.store(ctx, edit_id(id));
    }
}

// =====================================================================
// UI

/// Windows-like scroll bars: wide, solid and always taking space.
fn classic_scrollbars(ui: &mut Ui) {
    let style = ui.style_mut();
    style.spacing.scroll = egui::style::ScrollStyle::solid();
    style.spacing.scroll.bar_width = 16.0;
    style.spacing.scroll.bar_inner_margin = 0.0;
    style.spacing.scroll.handle_min_length = 24.0;
    let v = &mut style.visuals;
    v.extreme_bg_color = Color32::from_gray(238);
    v.widgets.inactive.bg_fill = Color32::from_gray(200);
    v.widgets.hovered.bg_fill = Color32::from_gray(160);
    v.widgets.active.bg_fill = Color32::from_gray(110);
    v.widgets.inactive.corner_radius = egui::CornerRadius::ZERO;
    v.widgets.hovered.corner_radius = egui::CornerRadius::ZERO;
    v.widgets.active.corner_radius = egui::CornerRadius::ZERO;
}

fn native_dialog(kind: DlgKind, name: &str) -> Option<PathBuf> {
    match kind {
        DlgKind::Save => rfd::FileDialog::new()
            .add_filter("Рабочий лист xmath (*.xmath)", &["xmath"])
            .add_filter("Mathcad 15 (*.xmcd)", &["xmcd"])
            .set_file_name(name)
            .save_file(),
        DlgKind::Pdf => rfd::FileDialog::new().add_filter("PDF (*.pdf)", &["pdf"]).set_file_name(name).save_file(),
        DlgKind::Reference => rfd::FileDialog::new()
            .add_filter("Рабочие листы (*.xmath, *.xmcd)", &["xmath", "xmcd"])
            .pick_file(),
        DlgKind::Open => rfd::FileDialog::new()
            .add_filter("Все поддерживаемые", &["xmath", "xmcd", "txt"])
            .add_filter("Рабочий лист xmath (*.xmath)", &["xmath"])
            .add_filter("Mathcad 15 (*.xmcd)", &["xmcd"])
            .add_filter("Текст, по области на строку (*.txt)", &["txt"])
            .pick_file(),
    }
}

/// Remembers the tooltip of the hovered control for the status bar.
fn set_hint(ui: &Ui, resp: &egui::Response, tip: &str) {
    if !tip.is_empty() && ui.rect_contains_pointer(resp.rect) {
        let tip = tip.to_string();
        ui.ctx().data_mut(|d| d.insert_temp(Id::new("xmath-hint"), tip));
    }
}

/// Toolbar and palette buttons never take keyboard focus (Sense::CLICK is not
/// focusable), so typing and Enter keep going to the worksheet.
fn plain_button(ui: &mut Ui, b: egui::Button, enabled: bool, tip: &str) -> egui::Response {
    let resp = ui.add_enabled(enabled, b.sense(Sense::CLICK));
    set_hint(ui, &resp, tip);
    if tip.is_empty() {
        return resp;
    }
    if enabled {
        resp.on_hover_text(tip)
    } else {
        resp.on_disabled_hover_text(format!("{tip} — пока не реализовано"))
    }
}

fn tb_button(ui: &mut Ui, label: impl Into<egui::WidgetText>, tip: &str, enabled: bool) -> bool {
    plain_button(ui, egui::Button::new(label).min_size(Vec2::new(24.0, 22.0)), enabled, tip).clicked()
}

/// A toolbar button with a painted icon.
fn icon_button(ui: &mut Ui, tip: &str, enabled: bool, draw: impl FnOnce(&egui::Painter, Rect, Color32)) -> bool {
    let resp = plain_button(ui, egui::Button::new("").min_size(Vec2::new(24.0, 22.0)), enabled, tip);
    let color = if enabled { Color32::BLACK } else { Color32::from_gray(160) };
    draw(ui.painter(), resp.rect.shrink(4.0), color);
    resp.clicked()
}

fn separator(ui: &mut Ui) {
    ui.add_space(2.0);
    let (r, _) = ui.allocate_exact_size(Vec2::new(2.0, 20.0), Sense::hover());
    ui.painter().line_segment([r.center_top(), r.center_bottom()], Stroke::new(1.0_f32, Color32::from_gray(160)));
    ui.painter().line_segment(
        [r.center_top() + Vec2::X, r.center_bottom() + Vec2::X],
        Stroke::new(1.0_f32, Color32::WHITE),
    );
    ui.add_space(2.0);
}

fn icon_page(p: &egui::Painter, r: Rect, c: Color32) {
    let r = Rect::from_center_size(r.center(), Vec2::new(11.0, 14.0));
    p.rect_filled(r, 0.0, Color32::WHITE);
    let fold = 4.0;
    p.add(Shape::closed_line(
        vec![
            r.left_top(),
            r.right_top() - Vec2::X * fold,
            r.right_top() + Vec2::Y * fold,
            r.right_bottom(),
            r.left_bottom(),
        ],
        Stroke::new(1.0_f32, c),
    ));
}

fn icon_folder(p: &egui::Painter, r: Rect, c: Color32) {
    let r = Rect::from_center_size(r.center(), Vec2::new(15.0, 11.0));
    let fill = if c == Color32::BLACK { Color32::from_rgb(240, 200, 80) } else { Color32::LIGHT_GRAY };
    p.rect_filled(r, 1.0, fill);
    p.add(Shape::closed_line(vec![r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom()], Stroke::new(1.0_f32, c)));
    p.rect_filled(Rect::from_min_size(r.left_top() - Vec2::Y * 2.0, Vec2::new(6.0, 2.5)), 0.0, fill);
}

fn icon_floppy(p: &egui::Painter, r: Rect, c: Color32) {
    let r = Rect::from_center_size(r.center(), Vec2::new(13.0, 13.0));
    let fill = if c == Color32::BLACK { Color32::from_rgb(60, 90, 180) } else { Color32::LIGHT_GRAY };
    p.rect_filled(r, 1.0, fill);
    p.rect_filled(Rect::from_min_size(r.left_top() + Vec2::new(3.0, 1.0), Vec2::new(7.0, 4.0)), 0.0, Color32::WHITE);
    p.rect_filled(Rect::from_min_size(r.left_top() + Vec2::new(2.0, 8.0), Vec2::new(9.0, 5.0)), 0.0, Color32::from_gray(220));
}

fn enabled_color(c: Color32, v: Color32) -> Color32 {
    if c == Color32::BLACK {
        v
    } else {
        Color32::from_gray(215)
    }
}

fn outline(p: &egui::Painter, r: Rect, fill: Color32, c: Color32) {
    p.rect_filled(r, 0.0, fill);
    p.add(Shape::closed_line(vec![r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom()], Stroke::new(1.0_f32, c)));
}

fn icon_printer(p: &egui::Painter, r: Rect, c: Color32) {
    let m = r.center();
    let st = Stroke::new(1.0_f32, c);
    outline(p, Rect::from_min_size(m + Vec2::new(-4.0, -7.0), Vec2::new(8.0, 5.0)), Color32::WHITE, c);
    outline(p, Rect::from_min_size(m + Vec2::new(-7.0, -3.0), Vec2::new(14.0, 6.0)), enabled_color(c, Color32::from_gray(170)), c);
    outline(p, Rect::from_min_size(m + Vec2::new(-4.0, 1.0), Vec2::new(8.0, 6.0)), Color32::WHITE, c);
    p.line_segment([m + Vec2::new(-2.0, 3.5), m + Vec2::new(2.0, 3.5)], st);
    p.line_segment([m + Vec2::new(-2.0, 5.5), m + Vec2::new(2.0, 5.5)], st);
}

fn icon_preview(p: &egui::Painter, r: Rect, c: Color32) {
    let m = r.center();
    outline(p, Rect::from_min_size(m + Vec2::new(-7.0, -7.0), Vec2::new(10.0, 13.0)), Color32::WHITE, c);
    for k in 0..3 {
        let y = m.y - 4.5 + k as f32 * 3.0;
        p.line_segment([Pos2::new(m.x - 5.0, y), Pos2::new(m.x + 1.0, y)], Stroke::new(1.0_f32, enabled_color(c, Color32::from_gray(150))));
    }
    let lens = m + Vec2::new(2.5, 1.5);
    p.circle(lens, 4.0, enabled_color(c, Color32::from_rgba_unmultiplied(150, 200, 255, 160)), Stroke::new(1.3_f32, c));
    p.line_segment([lens + Vec2::splat(2.9), lens + Vec2::splat(6.5)], Stroke::new(2.0_f32, c));
}

fn icon_copy(p: &egui::Painter, r: Rect, c: Color32) {
    let m = r.center();
    outline(p, Rect::from_min_size(m + Vec2::new(-6.5, -7.0), Vec2::new(9.0, 11.0)), Color32::WHITE, c);
    outline(p, Rect::from_min_size(m + Vec2::new(-2.5, -3.0), Vec2::new(9.0, 11.0)), Color32::WHITE, c);
    for k in 0..3 {
        let y = m.y - 0.5 + k as f32 * 2.6;
        p.line_segment([Pos2::new(m.x - 0.5, y), Pos2::new(m.x + 4.5, y)], Stroke::new(1.0_f32, enabled_color(c, Color32::from_gray(150))));
    }
}

fn icon_paste(p: &egui::Painter, r: Rect, c: Color32) {
    let m = r.center();
    outline(p, Rect::from_min_size(m + Vec2::new(-7.0, -6.0), Vec2::new(11.0, 13.0)), enabled_color(c, Color32::from_rgb(190, 140, 70)), c);
    outline(p, Rect::from_min_size(m + Vec2::new(-4.0, -8.0), Vec2::new(5.0, 3.5)), enabled_color(c, Color32::from_gray(150)), c);
    outline(p, Rect::from_min_size(m + Vec2::new(-1.0, -2.0), Vec2::new(8.0, 10.0)), Color32::WHITE, c);
    for k in 0..3 {
        let y = m.y + 0.5 + k as f32 * 2.6;
        p.line_segment([Pos2::new(m.x + 1.0, y), Pos2::new(m.x + 5.0, y)], Stroke::new(1.0_f32, enabled_color(c, Color32::from_gray(150))));
    }
}

fn icon_cut(p: &egui::Painter, r: Rect, c: Color32) {
    let m = r.center();
    let st = Stroke::new(1.3_f32, c);
    p.line_segment([m + Vec2::new(-5.0, -7.0), m + Vec2::new(2.0, 2.0)], st);
    p.line_segment([m + Vec2::new(5.0, -7.0), m + Vec2::new(-2.0, 2.0)], st);
    for dx in [-3.5_f32, 3.5] {
        p.circle(m + Vec2::new(dx, 4.5), 2.3, Color32::WHITE, st);
    }
}

/// Curved arrow: undo (`flip` false) or redo (mirrored).
fn icon_undo(p: &egui::Painter, r: Rect, c: Color32, flip: bool) {
    let m = r.center() + Vec2::Y * 0.5;
    let sx = if flip { -1.0 } else { 1.0 };
    let rad = 5.5;
    let pt = |deg: f32| {
        let t = deg.to_radians();
        m + Vec2::new(sx * rad * t.cos(), rad * t.sin())
    };
    let pts: Vec<Pos2> = (0..=16).map(|k| pt(20.0 - k as f32 * (220.0 / 16.0))).collect();
    let st = Stroke::new(1.6_f32, c);
    p.add(Shape::line(pts.clone(), st));
    let end = *pts.last().unwrap();
    let prev = pts[pts.len() - 2];
    let dir = (end - prev).normalized();
    let perp = Vec2::new(-dir.y, dir.x);
    p.add(Shape::convex_polygon(vec![end + dir * 4.0, end + perp * 3.2, end - perp * 3.2], c, Stroke::NONE));
}

fn icon_plot(p: &egui::Painter, r: Rect, c: Color32) {
    let s = Stroke::new(1.0_f32, c);
    p.line_segment([r.left_bottom(), r.left_top()], s);
    p.line_segment([r.left_bottom(), r.right_bottom()], s);
    let red = if c == Color32::BLACK { Color32::RED } else { Color32::GRAY };
    let pts: Vec<Pos2> = (0..=12)
        .map(|k| {
            let t = k as f32 / 12.0;
            Pos2::new(r.left() + 1.0 + t * (r.width() - 2.0), r.center().y - (t * 6.3).sin() * r.height() * 0.35)
        })
        .collect();
    p.add(Shape::line(pts, Stroke::new(1.5_f32, red)));
}

fn icon_calculator(p: &egui::Painter, r: Rect, c: Color32) {
    let r = Rect::from_center_size(r.center(), Vec2::new(11.0, 14.0));
    p.add(Shape::closed_line(vec![r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom()], Stroke::new(1.0_f32, c)));
    p.rect_filled(Rect::from_min_size(r.left_top() + Vec2::new(2.0, 2.0), Vec2::new(7.0, 3.0)), 0.0, c);
    for i in 0..3 {
        for j in 0..2 {
            let q = r.left_top() + Vec2::new(2.5 + i as f32 * 3.0, 7.0 + j as f32 * 3.5);
            p.rect_filled(Rect::from_min_size(q, Vec2::new(1.6, 1.6)), 0.0, c);
        }
    }
}

fn icon_align(p: &egui::Painter, r: Rect, c: Color32, across: bool) {
    let blue = if c == Color32::BLACK { SELECT_BLUE } else { Color32::GRAY };
    if across {
        p.line_segment([r.left_top(), r.right_top()], Stroke::new(1.0_f32, c));
        p.rect_filled(Rect::from_min_size(r.left_top() + Vec2::new(1.0, 2.0), Vec2::new(5.0, 9.0)), 0.0, blue);
        p.rect_filled(Rect::from_min_size(r.left_top() + Vec2::new(8.0, 2.0), Vec2::new(5.0, 6.0)), 0.0, blue);
    } else {
        p.line_segment([r.left_top(), r.left_bottom()], Stroke::new(1.0_f32, c));
        p.rect_filled(Rect::from_min_size(r.left_top() + Vec2::new(2.0, 1.0), Vec2::new(10.0, 5.0)), 0.0, blue);
        p.rect_filled(Rect::from_min_size(r.left_top() + Vec2::new(2.0, 8.0), Vec2::new(6.0, 5.0)), 0.0, blue);
    }
}

fn status_box(ui: &mut Ui, text: &str, w: f32) {
    let (r, _) = ui.allocate_exact_size(Vec2::new(w, 18.0), Sense::hover());
    let p = ui.painter();
    p.line_segment([r.left_top(), r.right_top()], Stroke::new(1.0_f32, Color32::from_gray(150)));
    p.line_segment([r.left_top(), r.left_bottom()], Stroke::new(1.0_f32, Color32::from_gray(150)));
    p.line_segment([r.left_bottom(), r.right_bottom()], Stroke::new(1.0_f32, Color32::WHITE));
    p.line_segment([r.right_top(), r.right_bottom()], Stroke::new(1.0_f32, Color32::WHITE));
    p.text(r.left_center() + Vec2::X * 4.0, Align2::LEFT_CENTER, text, FontId::proportional(12.0), Color32::BLACK);
}

fn menu_item(ui: &mut Ui, label: &str, shortcut: &str, enabled: bool) -> bool {
    let mut b = egui::Button::new(label);
    if !shortcut.is_empty() {
        b = b.shortcut_text(shortcut);
    }
    let r = ui.add_enabled(enabled, b);
    if r.clicked() {
        ui.close_menu();
        true
    } else {
        false
    }
}

impl App {
    fn menu_bar(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        let mut act: Option<Action> = None;
        let sel = !self.selection.is_empty();
        egui::menu::bar(ui, |ui| {
            ui.menu_button("Файл", |ui| {
                if menu_item(ui, "Создать", "Ctrl+N", true) {
                    act = Some(Action::New);
                }
                if menu_item(ui, "Открыть...", "Ctrl+O", true) {
                    act = Some(Action::Open);
                }
                ui.menu_button("Примеры", |ui| {
                    for (i, (name, _)) in EXAMPLES.iter().enumerate() {
                        if menu_item(ui, name, "", true) {
                            act = Some(Action::OpenExample(i));
                        }
                    }
                });
                ui.separator();
                if menu_item(ui, "Сохранить", "Ctrl+S", true) {
                    act = Some(Action::Save);
                }
                if menu_item(ui, "Сохранить как...", "Ctrl+Shift+S", true) {
                    act = Some(Action::SaveAs);
                }
                ui.separator();
                if menu_item(ui, "Экспорт в PDF...", "", true) {
                    act = Some(Action::ExportPdf);
                }
                ui.separator();
                if menu_item(ui, "Параметры страницы...", "", true) {
                    act = Some(Action::PageSetup);
                }
                if menu_item(ui, "Предварительный просмотр", "", true) {
                    act = Some(Action::PrintPreview);
                }
                if menu_item(ui, "Печать...", "Ctrl+P", true) {
                    act = Some(Action::Print);
                }
                ui.separator();
                if menu_item(ui, "Выход", "Alt+F4", true) {
                    act = Some(Action::Exit);
                }
            });
            ui.menu_button("Правка", |ui| {
                if menu_item(ui, "Отменить", "Ctrl+Z", !self.undo.is_empty()) {
                    act = Some(Action::Undo);
                }
                if menu_item(ui, "Повторить", "Ctrl+Y", !self.redo.is_empty()) {
                    act = Some(Action::Redo);
                }
                ui.separator();
                if menu_item(ui, if self.text_mode { "✔ Редактировать как текст" } else { "Редактировать как текст" }, "F2", true) {
                    act = Some(Action::ToggleTextMode);
                }
                ui.separator();
                if menu_item(ui, "Вырезать", "Ctrl+X", sel) {
                    act = Some(Action::Cut);
                }
                if menu_item(ui, "Копировать", "Ctrl+C", sel) {
                    act = Some(Action::Copy);
                }
                if menu_item(ui, "Вставить", "Ctrl+V", !self.clipboard.is_empty()) {
                    act = Some(Action::Paste);
                }
                if menu_item(ui, "Удалить", "Ctrl+D", sel) {
                    act = Some(Action::Delete);
                }
                ui.separator();
                if menu_item(ui, "Выделить всё", "Ctrl+A", true) {
                    act = Some(Action::SelectAll);
                }
                menu_item(ui, "Найти...", "Ctrl+F", false);
                menu_item(ui, "Заменить...", "Ctrl+H", false);
            });
            ui.menu_button("Вид", |ui| {
                ui.menu_button("Панели инструментов", |ui| {
                    ui.checkbox(&mut self.bars.standard, "Стандартная");
                    ui.checkbox(&mut self.bars.formatting, "Форматирование");
                    ui.checkbox(&mut self.bars.math, "Математика");
                    ui.separator();
                    for p in Palette::ALL {
                        let mut on = self.palettes.contains(&p);
                        if ui.checkbox(&mut on, p.menu_name()).changed() {
                            if on {
                                self.palettes.insert(p);
                            } else {
                                self.palettes.remove(&p);
                            }
                        }
                    }
                });
                ui.checkbox(&mut self.bars.status, "Строка состояния");
                ui.separator();
                ui.menu_button("Масштаб интерфейса", |ui| {
                    let mut choices: Vec<crate::uiscale::Choice> = vec![None];
                    choices.extend(crate::uiscale::LEVELS.iter().map(|p| Some(*p)));
                    for c in choices {
                        if ui.radio(self.ui_scale == c, crate::uiscale::label(c)).clicked() {
                            self.ui_scale = c;
                            ui.close_menu();
                        }
                    }
                    ui.separator();
                    let eff = ctx.zoom_factor();
                    if menu_item(ui, "Увеличить интерфейс", "", true) {
                        self.ui_scale = crate::uiscale::step(self.ui_scale, eff, true);
                    }
                    if menu_item(ui, "Уменьшить интерфейс", "", true) {
                        self.ui_scale = crate::uiscale::step(self.ui_scale, eff, false);
                    }
                });
                ui.menu_button("Масштаб", |ui| {
                    for z in [50, 75, 100, 125, 150, 200] {
                        if menu_item(ui, &format!("{z}%"), "", true) {
                            act = Some(Action::Zoom(z));
                        }
                    }
                    ui.separator();
                    if menu_item(ui, "По ширине", "", true) {
                        act = Some(Action::ZoomFitWidth);
                    }
                });
                if menu_item(ui, "Обновить", "Ctrl+R", true) {
                    ctx.request_repaint();
                }
            });
            ui.menu_button("Вставка", |ui| {
                ui.menu_button("График", |ui| {
                    if menu_item(ui, "Декартов график", "@", true) {
                        act = Some(Action::InsertPlot);
                    }
                    if menu_item(ui, "Полярный график", "Ctrl+7", true) {
                        act = Some(Action::InsertPolar);
                    }
                    if menu_item(ui, "График поверхности", "Ctrl+2", true) {
                        act = Some(Action::InsertSurface);
                    }
                    if menu_item(ui, "Контурный график", "Ctrl+5", true) {
                        act = Some(Action::InsertContour);
                    }
                    if menu_item(ui, "3D гистограмма", "", true) {
                        act = Some(Action::InsertBar);
                    }
                    if menu_item(ui, "3D точечный график", "", true) {
                        act = Some(Action::InsertScatter);
                    }
                });
                if menu_item(ui, "Матрица...", "Ctrl+M", true) {
                    act = Some(Action::InsertMatrix);
                }
                if menu_item(ui, "Функция...", "Ctrl+E", true) {
                    act = Some(Action::InsertFunction);
                }
                if menu_item(ui, "Единицы...", "Ctrl+U", true) {
                    act = Some(Action::InsertUnit);
                }
                menu_item(ui, "Рисунок", "Ctrl+T", false);
                if menu_item(ui, "Ссылка...", "", true) {
                    act = Some(Action::InsertReference);
                }
                ui.separator();
                if menu_item(ui, "Математическая область", "", true) {
                    act = Some(Action::InsertMath);
                }
                if menu_item(ui, "Текстовая область", "\"", true) {
                    act = Some(Action::InsertText);
                }
                menu_item(ui, "Разрыв страницы", "Ctrl+Enter", false);
            });
            ui.menu_button("Формат", |ui| {
                menu_item(ui, "Уравнение...", "", false);
                if menu_item(ui, "Результат...", "", true) {
                    act = Some(Action::ResultFormat);
                }
                menu_item(ui, "Текст...", "", false);
                if menu_item(ui, "График...", "", true) {
                    act = Some(Action::PlotFormat);
                }
                ui.separator();
                ui.menu_button("Выровнять области", |ui| {
                    if menu_item(ui, "По горизонтали", "", true) {
                        act = Some(Action::AlignAcross);
                    }
                    if menu_item(ui, "По вертикали", "", true) {
                        act = Some(Action::AlignDown);
                    }
                });
                if menu_item(ui, "Разделить области", "", true) {
                    act = Some(Action::SeparateRegions);
                }
            });
            ui.menu_button("Инструменты", |ui| {
                menu_item(ui, "Орфография...", "", false);
                ui.menu_button("Вычислить", |ui| {
                    if menu_item(ui, "Вычислить сейчас", "F9", true) {
                        act = Some(Action::Calculate);
                    }
                    let mut auto = self.auto_calc;
                    if ui.checkbox(&mut auto, "Автоматические вычисления").clicked() {
                        act = Some(Action::ToggleAuto);
                        ui.close_menu();
                    }
                });
                if menu_item(ui, "Параметры рабочего листа...", "", true) {
                    act = Some(Action::WorksheetOptions);
                }
                if menu_item(ui, "Параметры...", "", true) {
                    self.show_prefs = true;
                }
            });
            ui.menu_button("Символика", |ui| {
                for s in ["Вычислить", "Упростить", "Раскрыть", "Разложить", "Привести подобные", "Решить", "Подставить"] {
                    menu_item(ui, s, "", false);
                }
                ui.separator();
                ui.label(RichText::new("Символьный движок в разработке").italics().weak());
            });
            ui.menu_button("Окно", |ui| {
                let name = self.doc_name();
                ui.add_enabled(false, egui::Button::new(format!("1 {name}")));
            });
            ui.menu_button("Справка", |ui| {
                if menu_item(ui, "Справка xmath", "F1", true) {
                    act = Some(Action::Help);
                }
                ui.separator();
                if menu_item(ui, "О программе xmath...", "", true) {
                    act = Some(Action::About);
                }
            });
        });
        if let Some(a) = act {
            self.pending.push(a);
        }
    }

    fn standard_toolbar(&mut self, ui: &mut Ui) {
        let mut act = None;
        let sel = !self.selection.is_empty();
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 1.0;
            if icon_button(ui, "Создать (Ctrl+N)", true, icon_page) {
                act = Some(Action::New);
            }
            if icon_button(ui, "Открыть (Ctrl+O)", true, icon_folder) {
                act = Some(Action::Open);
            }
            if icon_button(ui, "Сохранить (Ctrl+S)", true, icon_floppy) {
                act = Some(Action::Save);
            }
            separator(ui);
            if icon_button(ui, "Печать (Ctrl+P)", true, icon_printer) {
                act = Some(Action::Print);
            }
            if icon_button(ui, "Предварительный просмотр", true, icon_preview) {
                act = Some(Action::PrintPreview);
            }
            tb_button(ui, RichText::new("ABC").size(9.0).color(Color32::from_gray(160)), "Орфография", false);
            separator(ui);
            if icon_button(ui, "Вырезать (Ctrl+X)", sel, icon_cut) {
                act = Some(Action::Cut);
            }
            if icon_button(ui, "Копировать (Ctrl+C)", sel, icon_copy) {
                act = Some(Action::Copy);
            }
            if icon_button(ui, "Вставить (Ctrl+V)", !self.clipboard.is_empty(), icon_paste) {
                act = Some(Action::Paste);
            }
            separator(ui);
            if icon_button(ui, "Отменить (Ctrl+Z)", !self.undo.is_empty(), |p, r, c| icon_undo(p, r, c, false)) {
                act = Some(Action::Undo);
            }
            if icon_button(ui, "Повторить (Ctrl+Y)", !self.redo.is_empty(), |p, r, c| icon_undo(p, r, c, true)) {
                act = Some(Action::Redo);
            }
            separator(ui);
            if icon_button(ui, "Выровнять по горизонтали", true, |p, r, c| icon_align(p, r, c, true)) {
                act = Some(Action::AlignAcross);
            }
            if icon_button(ui, "Выровнять по вертикали", true, |p, r, c| icon_align(p, r, c, false)) {
                act = Some(Action::AlignDown);
            }
            separator(ui);
            if tb_button(ui, RichText::new("f(x)").italics().size(12.0), "Вставить функцию (Ctrl+E)", true) {
                act = Some(Action::InsertFunction);
            }
            if tb_button(ui, RichText::new("[■]").size(11.0), "Вставить единицы (Ctrl+U)", true) {
                act = Some(Action::InsertUnit);
            }
            if tb_button(ui, RichText::new("=").strong().size(16.0), "Вычислить (F9)", true) {
                act = Some(Action::Calculate);
            }
            separator(ui);
            let mut z = (self.zoom * 100.0).round() as u32;
            egui::ComboBox::from_id_salt("zoom")
                .width(64.0)
                .selected_text(format!("{z}%"))
                .show_ui(ui, |ui| {
                    for v in [25, 50, 75, 100, 125, 150, 200, 300, 400] {
                        ui.selectable_value(&mut z, v, format!("{v}%"));
                    }
                });
            if z != (self.zoom * 100.0).round() as u32 {
                act = Some(Action::Zoom(z));
            }
            separator(ui);
            if tb_button(ui, RichText::new("?").strong(), "Справка (F1)", true) {
                act = Some(Action::Help);
            }
        });
        if let Some(a) = act {
            self.pending.push(a);
        }
    }

    /// The text region the formatting toolbar acts on.
    fn format_target(&self) -> Option<u64> {
        let id = self.editing.or_else(|| (self.selection.len() == 1).then(|| self.selection[0]))?;
        matches!(self.region(id).map(|r| &r.kind), Some(RegionKind::Text { .. })).then_some(id)
    }

    fn formatting_toolbar(&mut self, ui: &mut Ui) {
        let target = self.format_target();
        let mut style = match target.and_then(|id| self.region(id)) {
            Some(Region { kind: RegionKind::Text { style, .. }, .. }) => style.clone(),
            _ => Default::default(),
        };
        let before = style.clone();
        let enabled = target.is_some();
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            ui.add_enabled_ui(enabled, |ui| {
                let style_name = match (style.size as i32, style.bold) {
                    (14, true) => "Heading 1",
                    (12, true) => "Heading 2",
                    (18, true) => "Title",
                    _ => "Normal",
                };
                egui::ComboBox::from_id_salt("style").width(110.0).selected_text(style_name).show_ui(ui, |ui| {
                    for (n, size, bold) in [("Normal", 10.0, false), ("Heading 1", 14.0, true), ("Heading 2", 12.0, true), ("Title", 18.0, true)] {
                        if ui.selectable_label(style_name == n, n).clicked() {
                            style.size = size;
                            style.bold = bold;
                        }
                    }
                });
                egui::ComboBox::from_id_salt("font").width(140.0).selected_text("Arial").show_ui(ui, |ui| {
                    let _ = ui.selectable_label(true, "Arial");
                });
                egui::ComboBox::from_id_salt("size").width(44.0).selected_text(format!("{}", style.size)).show_ui(ui, |ui| {
                    for s in [8.0, 9.0, 10.0, 11.0, 12.0, 14.0, 16.0, 18.0, 20.0, 22.0, 24.0, 26.0, 28.0, 36.0, 48.0, 72.0] {
                        ui.selectable_value(&mut style.size, s, format!("{s}"));
                    }
                });
                separator(ui);
                ui.toggle_value(&mut style.bold, RichText::new("B").strong()).on_hover_text("Полужирный (Ctrl+B)");
                ui.toggle_value(&mut style.italic, RichText::new("I").italics()).on_hover_text("Курсив (Ctrl+I)");
                ui.toggle_value(&mut style.underline, RichText::new("U").underline()).on_hover_text("Подчёркнутый (Ctrl+U)");
            });
        });
        if style != before {
            if let Some(id) = target {
                self.checkpoint();
                if let Some(Region { kind: RegionKind::Text { style: s, .. }, .. }) = self.region_mut(id) {
                    *s = style;
                }
                self.modified = true;
            }
        }
    }

    fn math_toolbar(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 1.0;
            for p in Palette::ALL {
                let on = self.palettes.contains(&p);
                let tip = format!("Панель «{}»", p.menu_name());
                let clicked = match p {
                    Palette::Calculator => icon_button(ui, &tip, true, icon_calculator),
                    Palette::Graph => icon_button(ui, &tip, true, icon_plot),
                    Palette::Matrix => tb_button(ui, RichText::new("[:::]").size(10.0), &tip, true),
                    Palette::Evaluation => tb_button(ui, RichText::new("x=").size(12.0), &tip, true),
                    Palette::Calculus => tb_button(ui, RichText::new("∫d/dx").size(10.0), &tip, true),
                    Palette::Boolean => tb_button(ui, RichText::new("<≤").size(12.0), &tip, true),
                    Palette::Programming => tb_button(ui, RichText::new("{⋮").size(12.0), &tip, true),
                    Palette::Greek => tb_button(ui, RichText::new("αβ").size(13.0), &tip, true),
                    Palette::Symbolic => tb_button(ui, RichText::new("→").strong().size(13.0), &tip, true),
                };
                if clicked {
                    if on {
                        self.palettes.remove(&p);
                    } else {
                        self.palettes.insert(p);
                    }
                }
            }
        });
    }

    fn palette_windows(&mut self, ctx: &Context) {
        let mut close = Vec::new();
        let mut acts: Vec<Act> = Vec::new();
        for (k, p) in Palette::ALL.iter().enumerate() {
            if !self.palettes.contains(p) {
                continue;
            }
            let default_pos = Pos2::new(ctx.screen_rect().right() - 210.0 - 30.0 * k as f32, 110.0 + 40.0 * k as f32);
            egui::Window::new(p.title())
                .id(Id::new(("palette", p.title())))
                .title_bar(false)
                .resizable(false)
                .default_pos(default_pos)
                .frame(egui::Frame::window(&ctx.style()).inner_margin(2.0).fill(PANEL))
                .show(ctx, |ui| {
                    // XP-style blue caption bar
                    let (bar, _) = ui.allocate_exact_size(Vec2::new(ui.available_width().max(150.0), 18.0), Sense::hover());
                    let painter = ui.painter();
                    painter.rect_filled(bar, 2.0, Color32::from_rgb(0, 84, 227));
                    painter.text(bar.left_center() + Vec2::X * 4.0, Align2::LEFT_CENTER, p.title(), FontId::proportional(12.0), Color32::WHITE);
                    let xr = Rect::from_center_size(bar.right_center() - Vec2::X * 9.0, Vec2::splat(14.0));
                    // Sense::CLICK is not focusable: Enter/typing never reach the close button
                    let xresp = ui.interact(xr, Id::new(("palette-close", p.title())), Sense::CLICK);
                    set_hint(ui, &xresp, "Закрыть панель");
                    let xfill = if xresp.hovered() { Color32::from_rgb(232, 90, 60) } else { Color32::from_rgb(214, 70, 40) };
                    painter.rect_filled(xr, 2.0, xfill);
                    painter.text(xr.center(), Align2::CENTER_CENTER, "✕", FontId::proportional(10.0), Color32::WHITE);
                    if xresp.clicked() {
                        close.push(*p);
                    }
                    ui.add_space(2.0);
                    let cols = p.columns();
                    let btns = palettes::buttons(*p);
                    let wide = matches!(p, Palette::Graph | Palette::Programming | Palette::Symbolic);
                    let size = if wide { Vec2::new(64.0, 22.0) } else { Vec2::new(28.0, 22.0) };
                    egui::Grid::new(("palette-grid", p.title())).spacing(Vec2::new(2.0, 2.0)).show(ui, |ui| {
                        for (i, b) in btns.iter().enumerate() {
                            let enabled = !matches!(b.act, Act::Todo);
                            let text = RichText::new(b.label).size(if b.label.chars().count() > 3 { 11.0 } else { 13.0 });
                            let r = plain_button(ui, egui::Button::new(text).min_size(size), enabled, b.tip);
                            if r.clicked() {
                                acts.push(b.act);
                            }
                            if (i + 1) % cols == 0 {
                                ui.end_row();
                            }
                        }
                    });
                });
        }
        for p in close {
            self.palettes.remove(&p);
        }
        for a in acts {
            match a {
                Act::Insert(s) => self.insert(ctx, s),
                Act::InsertPlot => self.run(ctx, Action::InsertPlot),
                Act::InsertPolar => self.run(ctx, Action::InsertPolar),
                Act::InsertSurface => self.run(ctx, Action::InsertSurface),
                Act::InsertContour => self.run(ctx, Action::InsertContour),
                Act::InsertBar => self.run(ctx, Action::InsertBar),
                Act::InsertScatter => self.run(ctx, Action::InsertScatter),
                Act::MatrixDialog => self.run(ctx, Action::InsertMatrix),
                Act::Todo => {}
            }
        }
    }

    fn status_bar(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            let msg = if !self.hint.is_empty() {
                &self.hint
            } else if self.evaluating || (self.dirty && self.auto_calc) {
                "Вычисление…"
            } else if self.status.is_empty() {
                "Нажмите F1 для справки."
            } else {
                &self.status
            };
            ui.label(RichText::new(msg).size(12.0));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let page = (self.scroll_off.y / self.zoom / self.page_px().1).floor().max(0.0) as i32 + 1;
                status_box(ui, &format!("Страница {page}"), 84.0);
                status_box(ui, "NUM", 40.0);
                status_box(ui, if self.auto_calc { "АВТО" } else { "ВРУЧН." }, 56.0);
            });
        });
    }

    // -----------------------------------------------------------------
    // Worksheet canvas

    fn text_galley(&self, ctx: &Context, text: &str, style: &xmath_core::worksheet::TextStyle, wrap: f32) -> std::sync::Arc<egui::Galley> {
        let size = px(style.size, self.zoom);
        let family = if style.bold && self.has_bold_font { FontFamily::Name("bold".into()) } else { FontFamily::Proportional };
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
                ..Default::default()
            },
        );
        ctx.fonts(|f| f.layout_job(job))
    }

    fn math_layout(&self, ctx: &Context, id: u64, source: &str) -> (Lay, Option<String>) {
        let fs = px(BASE_PT, self.zoom);
        let out = self.outputs.get(&id);
        ctx.fonts(|f| {
            let mut ts = Ts::new(f);
            let err = out.and_then(|o| o.error());
            let parsed = match out.and_then(|o| o.stmt.clone()) {
                Some(Ok(s)) => Ok(s),
                // unparsable for the evaluator (e.g. `■` placeholders): still typeset what is possible
                _ => inplace::parse_stmt(source),
            };
            match parsed {
                Ok(mut stmt) => {
                    if err.is_some() {
                        ts.color = RED;
                    }
                    let result = match out.map(|o| &o.output) {
                        Some(Output::Value { view, unit }) => Some((view, unit.as_ref())),
                        _ => None,
                    };
                    let mut lay = ts.statement(&stmt, result, fs);
                    inplace::resolve_boxes(&mut stmt, &mut lay.boxes);
                    (lay, err)
                }
                Err(e) => (ts.text_c(source, fs, RED), Some(e.msg)),
            }
        })
    }

    fn canvas(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        if self.ed.as_ref().is_some_and(|e| Some(e.region) != self.editing) {
            self.ed = None;
        }
        let z = self.zoom;
        let fs = px(BASE_PT, z);
        // the canvas reaches just past the last region in both directions
        let bottom = self
            .ws
            .regions
            .iter()
            .map(|r| r.y + self.rects.get(&r.id).map_or(40.0, |q| q.height()))
            .fold(self.cursor.y, f32::max);
        let right = self
            .ws
            .regions
            .iter()
            .map(|r| r.x + self.rects.get(&r.id).map_or(40.0, |q| q.width()))
            .fold(self.cursor.x, f32::max);
        let orig_style = ui.style().clone();
        classic_scrollbars(ui);
        let mut area = ScrollArea::both().id_salt("sheet").auto_shrink([false, false]).scroll_bar_visibility(ScrollBarVisibility::AlwaysVisible);
        if let Some(o) = self.scroll_req.take() {
            area = area.scroll_offset(o);
        }
        let inner_style = orig_style.clone();
        let out = area.show(ui, |ui| {
            ui.set_style(inner_style);
            let avail = ui.available_size();
            let size = Vec2::new(((right + 120.0) * z).max(avail.x), ((bottom + 240.0) * z).max(avail.y));
            let visible = ui.clip_rect();
            let (rect, bg) = ui.allocate_exact_size(size, Sense::click_and_drag());
            let origin = rect.min;
            let to_screen = |p: Pos2| origin + p.to_vec2() * z;
            let to_ws = |s: Pos2| ((s - origin) / z).to_pos2();
            let painter = ui.painter_at(rect);

            // right margin and page breaks
            let (page_w, page_h) = self.page_px();
            let mx = origin.x + page_w * z;
            painter.line_segment([Pos2::new(mx, rect.top()), Pos2::new(mx, rect.bottom())], Stroke::new(1.0_f32, Color32::from_gray(190)));
            let mut k = 1.0;
            while k * page_h * z < rect.height() {
                let y = origin.y + k * page_h * z;
                painter.extend(Shape::dashed_line(&[Pos2::new(rect.left(), y), Pos2::new(mx, y)], Stroke::new(1.0_f32, Color32::from_gray(170)), 4.0, 4.0));
                k += 1.0;
            }

            let mut new_rects = HashMap::new();
            let mut anchor_updates: Vec<(u64, [f32; 2])> = Vec::new();
            let mut clicked_region = false;
            // the region being edited is drawn last, so its live preview covers regions below
            let mut ids: Vec<u64> = self.ws.regions.iter().map(|r| r.id).collect();
            if let Some(e) = self.editing {
                if let Some(i) = ids.iter().position(|x| *x == e) {
                    let v = ids.remove(i);
                    ids.push(v);
                }
            }
            for id in ids {
                let Some(region) = self.region(id).cloned() else { continue };
                let pos = to_screen(Pos2::new(region.x, region.y));
                let editing = self.editing == Some(id);
                let selected = self.selection.contains(&id);
                // regions far outside the viewport are not laid out again (large worksheets)
                if !editing && !selected && self.dump_rects.is_none() {
                    if let Some(r) = self.rects.get(&id).copied() {
                        let sr = Rect::from_min_max(to_screen(r.min), to_screen(r.max));
                        if !sr.expand(64.0).intersects(visible) {
                            new_rects.insert(id, r);
                            continue;
                        }
                    }
                }
                let mut region_rect;
                let mut error: Option<String> = None;
                match &region.kind {
                    RegionKind::Math { source } => {
                        let in_cell = if editing && self.cell.as_ref().is_some_and(|c| c.region == id) {
                            let r = self.cell_canvas(ui, id, pos, fs);
                            if r.is_none() {
                                self.focus_pending = true;
                            }
                            r
                        } else if editing && self.cell.is_none() && !self.line_edit {
                            if self.ed.is_none() {
                                let click = self.edit_click.take();
                                self.begin_structural(id, click);
                            }
                            if self.ed.is_some() { self.ed_canvas(ui, id, pos, fs) } else { None }
                        } else {
                            None
                        };
                        if let Some((r, err, finished)) = in_cell {
                            region_rect = r;
                            error = err;
                            if finished {
                                self.pending_rect_fix(id, region_rect, to_ws, &mut new_rects);
                                continue;
                            }
                        } else if editing {
                            let mut src = source.clone();
                            let font = FontId::proportional(fs);
                            let tw = ctx.fonts(|f| f.layout_no_wrap(src.clone(), font.clone(), Color32::BLACK).size().x);
                            let w = (tw + fs * 2.0).max(fs * 6.0);
                            // the box replaces the formula: its text baseline is the baseline of the region
                            let asc = self.math_layout(&ctx, id, source).0.asc;
                            let te_top = pos.y + 3.0 + asc - fs * 0.95 - 1.0;
                            let te_rect = Rect::from_min_size(Pos2::new(pos.x, te_top.max(pos.y - fs)), Vec2::new(w, fs * 1.6));
                            painter.rect_filled(te_rect, 0.0, Color32::WHITE);
                            let resp = ui.put(
                                te_rect,
                                TextEdit::singleline(&mut src)
                                    .id(edit_id(id))
                                    .font(font)
                                    .desired_width(w)
                                    .margin(Vec2::new(2.0, 1.0))
                                    .frame(false)
                                    .lock_focus(true)
                                    .text_color(Color32::from_rgb(0, 0, 140)),
                            );
                            painter.rect_stroke(te_rect, 0.0, Stroke::new(1.0_f32, SELECT_BLUE), egui::StrokeKind::Outside);
                            if self.focus_pending {
                                resp.request_focus();
                                self.focus_pending = false;
                            }
                            if src != *source {
                                if let Some(Region { kind: RegionKind::Math { source }, .. }) = self.region_mut(id) {
                                    *source = src.clone();
                                }
                                self.changed();
                            }
                            let (lay, err) = self.math_layout(&ctx, id, &src);
                            error = err;
                            let prev_origin = te_rect.left_bottom() + Vec2::new(2.0, 4.0 + lay.asc);
                            let prev_rect = Rect::from_min_size(te_rect.left_bottom() + Vec2::new(0.0, 2.0), Vec2::new(lay.w + 4.0, lay.h() + 4.0));
                            if !src.trim().is_empty() {
                                painter.rect_filled(prev_rect, 0.0, Color32::WHITE);
                                painter.rect_stroke(prev_rect, 0.0, Stroke::new(1.0_f32, Color32::from_gray(190)), egui::StrokeKind::Inside);
                                lay.paint(&painter, prev_origin);
                                // swallow clicks so they do not reach regions under the preview
                                ui.interact(prev_rect, Id::new(("preview", id)), Sense::CLICK);
                            }
                            region_rect = te_rect.union(prev_rect);
                            let enter = ctx.input(|i| i.key_pressed(Key::Enter));
                            let esc = ctx.input(|i| i.key_pressed(Key::Escape));
                            if resp.lost_focus() && (enter || esc) {
                                self.finish_edit(enter);
                                self.pending_rect_fix(id, region_rect, to_ws, &mut new_rects);
                                continue;
                            }
                        } else {
                            let (lay, err) = self.math_layout(&ctx, id, source);
                            error = err;
                            anchor_updates.push((id, [0.0, (lay.asc + 3.0) / z]));
                            region_rect = Rect::from_min_size(pos, Vec2::new(lay.w + 4.0, lay.h() + 6.0));
                            let origin = pos + Vec2::new(2.0, 3.0 + lay.asc);
                            lay.paint(&painter, origin);
                            self.hits.insert(id, lay.boxes.iter().map(|(n, r)| (*n, r.translate(origin.to_vec2()))).collect());
                        }
                    }
                    RegionKind::Text { text, style, width, lock_width } => {
                        let wrap = if *lock_width { (*width * z).max(20.0) } else { (page_w * z - (pos.x - origin.x)).max(120.0 * z) };
                        if editing {
                            let mut t = text.clone();
                            let family = if style.bold && self.has_bold_font { FontFamily::Name("bold".into()) } else { FontFamily::Proportional };
                            let g = self.text_galley(&ctx, &t, style, wrap);
                            let r = Rect::from_min_size(pos, Vec2::new(wrap.min(g.size().x + fs * 4.0).max(fs * 8.0), g.size().y + 6.0));
                            let resp = ui.put(
                                r,
                                TextEdit::multiline(&mut t)
                                    .id(edit_id(id))
                                    .font(FontId::new(px(style.size, z), family))
                                    .desired_width(r.width())
                                    .desired_rows(1)
                                    .frame(false)
                                    .margin(Vec2::new(2.0, 2.0)),
                            );
                            painter.rect_stroke(resp.rect, 0.0, Stroke::new(1.0_f32, SELECT_BLUE), egui::StrokeKind::Outside);
                            if self.focus_pending {
                                resp.request_focus();
                                self.focus_pending = false;
                            }
                            if t != *text {
                                if let Some(Region { kind: RegionKind::Text { text, .. }, .. }) = self.region_mut(id) {
                                    *text = t;
                                }
                                self.modified = true;
                            }
                            region_rect = resp.rect;
                            if resp.lost_focus() && ctx.input(|i| i.key_pressed(Key::Escape)) {
                                self.finish_edit(true);
                            }
                        } else {
                            let g = self.text_galley(&ctx, text, style, wrap);
                            region_rect = Rect::from_min_size(pos, g.size() + Vec2::splat(4.0));
                            let at = pos + Vec2::splat(2.0);
                            painter.galley(at, g.clone(), Color32::BLACK);
                            if style.bold && !self.has_bold_font {
                                painter.galley(at + Vec2::X * 0.7, g, Color32::BLACK);
                            }
                        }
                    }
                    RegionKind::Reference { href } => {
                        let (galley, size) = crate::print::reference_layout(&ctx, href, z);
                        region_rect = Rect::from_min_size(pos, size);
                        crate::print::paint_reference(&painter, pos, &galley, size);
                    }
                    RegionKind::Image { png_base64, width, height } => {
                        region_rect = Rect::from_min_size(pos, Vec2::new(width * z, height * z));
                        let tex = self.images.texture(&ctx, id, png_base64);
                        crate::image::paint(&painter, region_rect, tex);
                    }
                    RegionKind::Plot { x, y, width, height, x_min, x_max, y_min, y_max, fmt, kind: pkind, .. } => {
                        let polar = *pkind == PlotKind::Polar;
                        // like in Mathcad the region size includes the label placeholders around the plot
                        let data = match self.outputs.get(&id).map(|o| &o.output) {
                            Some(Output::Plot(d)) => d.clone(),
                            _ => Default::default(),
                        };
                        let text_w = |t: &str| painter.layout_no_wrap(t.to_string(), FontId::proportional(fs), Color32::BLACK).size().x;
                        let mut y_text_w = if fmt.hide_args && !editing { 0.0 } else { y.iter().map(|l| text_w(if l.is_empty() { "■" } else { l })).fold(0.0, f32::max) };
                        if let (true, Some((_, yb, _))) = (editing, &self.plot_buf) {
                            y_text_w = y_text_w.max(text_w(yb) + 10.0);
                        }
                        let geom = plot::plot_geom(fmt, &data, polar, width * z, height * z, fs, z, y_text_w);
                        let (legend_w, m) = (geom.legend_w, geom.margins);
                        let plot_rect = geom.plot_rect.translate(pos.to_vec2());
                        let view = plot::View { x_min: *x_min, x_max: *x_max, y_min: *y_min, y_max: *y_max, fmt, aspect: plot_rect.width() / plot_rect.height() };
                        let axes = if polar {
                            let g = plot::paint_polar(&painter, plot_rect, &data, &view, fs);
                            plot::Axes { x0: 0.0, x1: std::f64::consts::TAU, xs: 1.0, y0: g.r_min, y1: g.r_max, ys: g.r_step, log_x: false, log_y: false, y2: None, log_y2: false }
                        } else {
                            plot::paint(&painter, plot_rect, &data, &view, fs)
                        };
                        let label_row = plot_rect.bottom() + m.bottom + fs * 0.4;
                        let mut bottom = label_row + fs * 1.6;
                        // region interaction (drag/click) under the placeholders
                        region_rect = Rect::from_min_max(pos, Pos2::new(plot_rect.right() + m.right, bottom));
                        if !editing && !fmt.hide_args {
                            let n = y.len().max(1) as f32;
                            let mut ly = plot_rect.center().y - n * fs * 1.2;
                            for (i, label) in y.iter().enumerate() {
                                let l = if label.is_empty() { "■" } else { label };
                                let r = painter.text(Pos2::new(pos.x + legend_w - 8.0, ly), Align2::RIGHT_TOP, l, FontId::proportional(fs), Color32::BLACK);
                                let (lc, ls, lw) = plot::trace_style(i, fmt);
                                plot::sample(&painter, Pos2::new(r.left(), r.bottom() + 3.0), Pos2::new(r.right().max(r.left() + 24.0), r.bottom() + 3.0), lc, ls, lw);
                                ly += fs * 2.4;
                            }
                            // an empty abscissa shows the variable QuickPlot detected, in grey
                            let (xl, xc) = match (x.is_empty(), &data.x_var) {
                                (false, _) => (x.as_str(), Color32::BLACK),
                                (true, Some(v)) => (v.as_str(), Color32::GRAY),
                                (true, None) if polar => ("θ", Color32::GRAY),
                                (true, None) => ("■", Color32::BLACK),
                            };
                            painter.text(Pos2::new(plot_rect.center().x, label_row), Align2::CENTER_TOP, xl, FontId::proportional(fs), xc);
                        }
                        for e in &data.errors {
                            painter.text(Pos2::new(plot_rect.left(), bottom), Align2::LEFT_TOP, e, FontId::proportional(fs * 0.85), RED);
                            bottom += fs * 1.2;
                        }
                        region_rect = region_rect.union(Rect::from_min_max(pos, Pos2::new(plot_rect.right() + m.right, bottom)));
                        // move / select
                        let resp = ui.interact(region_rect, Id::new(("region", id)), Sense::click_and_drag());
                        self.region_interaction(&ctx, id, &resp, z);
                        clicked_region |= resp.clicked();
                        if resp.double_clicked() {
                            self.open_plot_dialog(id);
                        }
                        if selected || editing {
                            self.axis_limit_boxes(ui, id, plot_rect, fs, &axes, [*x_min, *x_max, *y_min, *y_max], &m, polar, fmt.y2_axis.then(|| [fmt.y2_min, fmt.y2_max]));
                        }
                        if editing {
                            if let Some((bid, mut ybuf, mut xbuf)) = self.plot_buf.clone().filter(|b| b.0 == id) {
                                let yr = Rect::from_min_size(Pos2::new(pos.x, plot_rect.center().y - fs), Vec2::new(legend_w - 6.0, fs * 1.6));
                                let r1 = ui.put(yr, TextEdit::singleline(&mut ybuf).id(edit_id(id)).font(FontId::proportional(fs)).lock_focus(true).hint_text("f(x)"));
                                let xr = Rect::from_center_size(Pos2::new(plot_rect.center().x, label_row + fs * 0.8), Vec2::new(140.0 * z, fs * 1.6));
                                let xhint = data.x_var.clone().unwrap_or_else(|| "x".into());
                                let r2 = ui.put(
                                    xr,
                                    TextEdit::singleline(&mut xbuf)
                                        .id(Id::new(("xmath-plot-x", id)))
                                        .font(FontId::proportional(fs))
                                        .lock_focus(true)
                                        .horizontal_align(Align::Center)
                                        .hint_text(RichText::new(xhint).color(Color32::GRAY)),
                                );
                                // Tab / Shift+Tab hop between the two fields
                                let back = ctx.input_mut(|i| i.consume_key(Modifiers::SHIFT, Key::Tab));
                                let fwd = !back && ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Tab));
                                if (fwd && r1.has_focus()) || (back && r2.has_focus()) {
                                    if fwd {
                                        r2.request_focus();
                                    } else {
                                        r1.request_focus();
                                    }
                                } else if (fwd && r2.has_focus()) || (back && r1.has_focus()) {
                                    if fwd {
                                        r1.request_focus();
                                    } else {
                                        r2.request_focus();
                                    }
                                }
                                if self.focus_pending {
                                    r1.request_focus();
                                    self.focus_pending = false;
                                }
                                if r1.changed() || r2.changed() {
                                    let ys: Vec<String> = plot::split_traces(&ybuf);
                                    if let Some(Region { kind: RegionKind::Plot { x, y, .. }, .. }) = self.region_mut(id) {
                                        *y = ys;
                                        *x = xbuf.trim().to_string();
                                    }
                                    self.changed();
                                }
                                self.plot_buf = Some((bid, ybuf, xbuf));
                            }
                        }
                        if selected || editing {
                            // resize handle
                            let h = Rect::from_center_size(plot_rect.right_bottom() + Vec2::new(m.right, m.bottom), Vec2::splat(8.0));
                            painter.rect_filled(h, 0.0, Color32::BLACK);
                            let hr = ui.interact(h.expand(2.0), Id::new(("plot-resize", id)), Sense::drag());
                            if hr.hovered() || hr.dragged() {
                                ctx.set_cursor_icon(CursorIcon::ResizeNwSe);
                            }
                            if hr.drag_started() {
                                self.checkpoint();
                            }
                            if hr.dragged() {
                                let d = hr.drag_delta() / z;
                                if let Some(Region { kind: RegionKind::Plot { width, height, .. }, .. }) = self.region_mut(id) {
                                    *width = (*width + d.x).max(120.0);
                                    *height = (*height + d.y).max(80.0);
                                }
                                self.modified = true;
                            }
                        }
                    }
                    RegionKind::Plot3d { kind: k3, expr, width, height, view: v3, fmt: f3 } => {
                        let plot_rect = Rect::from_min_size(pos, Vec2::new((*width * z).max(80.0), (*height * z - fs * 2.0).max(60.0)));
                        let data = match self.outputs.get(&id).map(|o| &o.output) {
                            Some(Output::Plot3d(d)) => d.clone(),
                            _ => Plot3dData { kind: *k3, ..Default::default() },
                        };
                        painter.rect_filled(plot_rect, 0.0, Color32::WHITE);
                        let thr = self.ws.format.exp_threshold;
                        let size = [plot_rect.width(), plot_rect.height()];
                        let scene = if *k3 == Plot3dKind::Contour {
                            xmath_core::plot3d::contour_scene(&data, f3, size, fs, thr)
                        } else {
                            xmath_core::plot3d::scene3d(&data, v3, f3, size, fs, thr)
                        };
                        plot::paint_scene(&painter.with_clip_rect(plot_rect), plot_rect.min, &scene);
                        painter.rect_stroke(plot_rect, 0.0, Stroke::new(1.0_f32, Color32::from_gray(90)), egui::StrokeKind::Middle);
                        let mut bottom = plot_rect.bottom() + fs * 0.4;
                        let label_row = bottom;
                        bottom += fs * 1.6;
                        if let Some(e) = &data.error {
                            painter.text(Pos2::new(plot_rect.left(), bottom), Align2::LEFT_TOP, e, FontId::proportional(fs * 0.85), RED);
                            bottom += fs * 1.2;
                        }
                        region_rect = Rect::from_min_max(pos, Pos2::new(plot_rect.right(), bottom));
                        if !editing {
                            let (txt, col) = if expr.is_empty() { ("■".to_string(), Color32::BLACK) } else { (expr.clone(), Color32::BLACK) };
                            painter.text(Pos2::new(plot_rect.left() + 4.0, label_row), Align2::LEFT_TOP, txt, FontId::proportional(fs), col);
                        }
                        let resp = ui.interact(region_rect, Id::new(("region", id)), Sense::click_and_drag());
                        self.region_interaction(&ctx, id, &resp, z);
                        clicked_region |= resp.clicked();
                        if resp.double_clicked() {
                            self.dlg_plot3d = Some(Plot3dDlg { id, fmt: f3.clone(), view: *v3 });
                        }
                        if editing {
                            if let Some((bid, mut buf, _)) = self.plot_buf.clone().filter(|b| b.0 == id) {
                                let r = Rect::from_min_size(Pos2::new(plot_rect.left(), label_row - 2.0), Vec2::new((plot_rect.width() - 20.0).min(320.0 * z), fs * 1.6));
                                let hint = match k3 {
                                    Plot3dKind::Scatter => "X, Y, Z",
                                    Plot3dKind::Contour => "M или f",
                                    _ => "M или f",
                                };
                                let r1 = ui.put(r, TextEdit::singleline(&mut buf).id(edit_id(id)).font(FontId::proportional(fs)).lock_focus(true).hint_text(hint));
                                if self.focus_pending {
                                    r1.request_focus();
                                    self.focus_pending = false;
                                }
                                if r1.changed() {
                                    if let Some(Region { kind: RegionKind::Plot3d { expr, .. }, .. }) = self.region_mut(id) {
                                        *expr = buf.trim().to_string();
                                    }
                                    self.changed();
                                }
                                self.plot_buf = Some((bid, buf, String::new()));
                            }
                        }
                        if *k3 != Plot3dKind::Contour {
                            // rotate with the left mouse button (selected plot), zoom with the plain wheel
                            let live = selected || editing;
                            let rot = ui.interact(plot_rect, Id::new(("plot3d-rot", id)), if live { Sense::drag() } else { Sense::hover() });
                            if live && (rot.hovered() || rot.dragged()) {
                                ctx.set_cursor_icon(CursorIcon::Grab);
                            }
                            if rot.drag_started() {
                                self.checkpoint();
                            }
                            // the plain mouse wheel zooms the 3D view (Ctrl+wheel zooms the worksheet)
                            let wheel = if live && rot.hovered() && !ctx.input(|i| i.modifiers.command) { ctx.input(|i| i.smooth_scroll_delta.y) } else { 0.0 };
                            if wheel != 0.0 {
                                ctx.input_mut(|i| i.smooth_scroll_delta = Vec2::ZERO);
                            }
                            let zoom = (wheel * 0.002).exp();
                            if rot.dragged() || zoom != 1.0 {
                                let d = rot.drag_delta();
                                if let Some(Region { kind: RegionKind::Plot3d { view, .. }, .. }) = self.region_mut(id) {
                                    view.azimuth -= d.x * 0.5;
                                    view.elevation = (view.elevation + d.y * 0.5).clamp(-90.0, 90.0);
                                    view.zoom = (view.zoom * zoom).clamp(0.3, 4.0);
                                    *view = view.clamped();
                                }
                                self.modified = true;
                            }
                        }
                        if selected || editing {
                            let h = Rect::from_center_size(plot_rect.right_bottom(), Vec2::splat(8.0));
                            painter.rect_filled(h, 0.0, Color32::BLACK);
                            let hr = ui.interact(h.expand(2.0), Id::new(("plot-resize", id)), Sense::drag());
                            if hr.hovered() || hr.dragged() {
                                ctx.set_cursor_icon(CursorIcon::ResizeNwSe);
                            }
                            if hr.drag_started() {
                                self.checkpoint();
                            }
                            if hr.dragged() {
                                let d = hr.drag_delta() / z;
                                if let Some(Region { kind: RegionKind::Plot3d { width, height, .. }, .. }) = self.region_mut(id) {
                                    *width = (*width + d.x).max(120.0);
                                    *height = (*height + d.y).max(100.0);
                                }
                                self.modified = true;
                            }
                        }
                    }
                }

                if !matches!(region.kind, RegionKind::Plot { .. } | RegionKind::Plot3d { .. }) && !editing {
                    let resp = ui.interact(region_rect, Id::new(("region", id)), Sense::click_and_drag());
                    self.region_interaction(&ctx, id, &resp, z);
                    clicked_region |= resp.clicked();
                    if resp.double_clicked() {
                        if let RegionKind::Reference { href } = &region.kind {
                            self.open_reference_dialog(Some(id), href);
                        }
                    }
                    if resp.hovered() {
                        if let Some(e) = &error {
                            resp.on_hover_text(RichText::new(e).color(RED));
                        }
                    }
                }

                if let Some(e) = &error {
                    if editing || selected {
                        // Mathcad shows the message in a pale yellow tooltip next to the region
                        let g = painter.layout_no_wrap(e.clone(), FontId::proportional(fs * 0.9), RED);
                        let at = region_rect.left_bottom() + Vec2::new(0.0, if editing { 4.0 } else { 6.0 });
                        let box_r = Rect::from_min_size(at, g.size() + Vec2::new(8.0, 4.0));
                        painter.rect_filled(box_r, 0.0, Color32::from_rgb(255, 255, 225));
                        painter.rect_stroke(box_r, 0.0, Stroke::new(1.0_f32, Color32::BLACK), egui::StrokeKind::Inside);
                        painter.galley(box_r.min + Vec2::new(4.0, 2.0), g, RED);
                    }
                }
                if selected && !editing {
                    let r = region_rect.expand(2.0);
                    let pts = [r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom(), r.left_top()];
                    painter.extend(Shape::dashed_line(&pts, Stroke::new(1.0_f32, Color32::BLACK), 3.0, 2.0));
                }
                new_rects.insert(id, Rect::from_min_max(to_ws(region_rect.min), to_ws(region_rect.max)));
            }
            self.rects = new_rects;
            // keep the edited region in view while it is created or grows
            if let Some(e) = self.editing {
                if let Some(r) = self.rects.get(&e).copied() {
                    if self.last_edit_rect != Some((e, r)) {
                        self.last_edit_rect = Some((e, r));
                        self.reveal = Some(r);
                    }
                }
            } else {
                self.last_edit_rect = None;
            }
            if let Some(r) = self.reveal.take() {
                let sr = Rect::from_min_max(to_screen(r.min), to_screen(r.max)).expand(24.0);
                ui.scroll_to_rect(sr, None);
            }
            self.images.retain(|id| self.ws.regions.iter().any(|r| r.id == id));
            // the baseline of the math regions decides the evaluation order (like in Mathcad);
            // this is layout bookkeeping, so the document is not marked as modified
            let mut order_changed = false;
            for (id, a) in anchor_updates {
                if let Some(r) = self.ws.regions.iter_mut().find(|r| r.id == id) {
                    if (r.anchor[1] - a[1]).abs() > 0.25 && r.anchor[0] == 0.0 {
                        r.anchor = a;
                        order_changed = true;
                    }
                }
            }
            if order_changed {
                self.dirty = true;
            }

            // crosshair
            if self.editing.is_none() {
                let c = to_screen(self.cursor);
                let s = Stroke::new(1.0_f32, Color32::RED);
                painter.line_segment([c - Vec2::X * 6.0, c + Vec2::X * 6.0], s);
                painter.line_segment([c - Vec2::Y * 6.0, c + Vec2::Y * 6.0], s);
            }

            // background: click places the crosshair, drag selects with a rubber band
            if bg.clicked() && !clicked_region {
                self.finish_edit(false);
                self.selection.clear();
                if let Some(p) = bg.interact_pointer_pos() {
                    self.cursor = snap(to_ws(p));
                }
                ctx.memory_mut(|m| m.stop_text_input());
            }
            if bg.drag_started() {
                self.finish_edit(false);
                self.band_start = bg.interact_pointer_pos();
            }
            if let (Some(a), Some(b)) = (self.band_start, bg.interact_pointer_pos().or(ctx.pointer_latest_pos())) {
                let band = Rect::from_two_pos(a, b);
                if bg.dragged() {
                    painter.rect_filled(band, 0.0, Color32::from_rgba_unmultiplied(49, 106, 197, 30));
                    let pts = [band.left_top(), band.right_top(), band.right_bottom(), band.left_bottom(), band.left_top()];
                    painter.extend(Shape::dashed_line(&pts, Stroke::new(1.0_f32, SELECT_BLUE), 3.0, 2.0));
                }
                if bg.drag_stopped() {
                    let wsband = Rect::from_two_pos(to_ws(band.min), to_ws(band.max));
                    self.selection = self.rects.iter().filter(|(_, r)| r.intersects(wsband)).map(|(id, _)| *id).collect();
                    self.band_start = None;
                }
            }
        });
        self.scroll_off = out.state.offset;
        self.view_min = out.inner_rect.min;
        self.view_size = out.inner_rect.size();
        self.content_size = out.content_size;
        ui.set_style(orig_style);
    }

    /// `XMATH_DUMP_RECTS`: once the evaluation has settled, write `id kind x y w h` (worksheet
    /// pixels, math regions without the 4 px padding) for every region and quit.
    fn dump_rects_tick(&mut self, ctx: &Context) {
        let Some(path) = self.dump_rects.clone() else { return };
        ctx.request_repaint();
        if self.ws.regions.is_empty() || self.dirty || self.evaluating || self.rects.len() != self.ws.regions.len() {
            return;
        }
        self.dump_frames += 1;
        if self.dump_frames < 5 {
            return;
        }
        let mut out = String::new();
        for r in &self.ws.regions {
            if let Some(q) = self.rects.get(&r.id) {
                let src = match &r.kind {
                    RegionKind::Math { source } => source.replace(['\n', '\t'], " "),
                    _ => String::new(),
                };
                let (kind, pad) = match r.kind {
                    RegionKind::Math { .. } => ("math", 4.0), // width pad; the 2 px extra height stays
                    RegionKind::Text { .. } => ("text", 4.0),
                    RegionKind::Plot { .. } => ("plot", 0.0),
                    RegionKind::Plot3d { .. } => ("plot3d", 0.0),
                    RegionKind::Image { .. } => ("image", 0.0),
                    RegionKind::Reference { .. } => ("reference", 0.0),
                };
                out.push_str(&format!("{} {} {:.2} {:.2} {:.2} {:.2} {}\n", r.id, kind, r.x, r.y, q.width() - pad, q.height() - pad, src));
            }
        }
        let _ = std::fs::write(path, out);
        std::process::exit(0);
    }

    fn pending_rect_fix(&self, id: u64, r: Rect, to_ws: impl Fn(Pos2) -> Pos2, rects: &mut HashMap<u64, Rect>) {
        rects.insert(id, Rect::from_min_max(to_ws(r.min), to_ws(r.max)));
    }

    fn region_interaction(&mut self, ctx: &Context, id: u64, resp: &egui::Response, z: f32) {
        let ctrl = ctx.input(|i| i.modifiers.command);
        if resp.drag_started() {
            if !self.selection.contains(&id) {
                self.selection = vec![id];
            }
            if self.editing != Some(id) {
                self.finish_edit(false);
            }
            self.checkpoint();
        }
        if resp.dragged() {
            ctx.set_cursor_icon(CursorIcon::Grabbing);
            let d = resp.drag_delta() / z;
            let sel = self.selection.clone();
            for r in self.ws.regions.iter_mut().filter(|r| sel.contains(&r.id)) {
                r.x = (r.x + d.x).max(0.0);
                r.y = (r.y + d.y).max(0.0);
            }
            self.changed();
        }
        if resp.drag_stopped() {
            let sel = self.selection.clone();
            for r in self.ws.regions.iter_mut().filter(|r| sel.contains(&r.id)) {
                let p = snap(Pos2::new(r.x, r.y));
                r.x = p.x;
                r.y = p.y;
            }
            self.changed();
        }
        if resp.clicked() {
            if ctrl {
                if let Some(i) = self.selection.iter().position(|s| *s == id) {
                    self.selection.remove(i);
                } else {
                    self.selection.push(id);
                }
            } else {
                self.selection = vec![id];
                self.begin_edit(id, true);
                // matrix cells are edited in place, cell by cell; the rest structurally
                self.edit_click = resp.interact_pointer_pos();
                self.begin_inplace(ctx, id, resp.interact_pointer_pos());
            }
        }
    }

    // -----------------------------------------------------------------
    // Dialogs

    fn dialogs(&mut self, ctx: &Context) {
        let mut insert: Option<String> = None;

        let in_matrix = self.active_matrix_cell();
        let mut matrix_op: Option<(bool, usize, usize)> = None;
        if let Some(d) = &mut self.dlg_matrix {
            let mut open = true;
            let mut done = false;
            egui::Window::new("Вставить матрицу").collapsible(false).resizable(false).open(&mut open).show(ctx, |ui| {
                egui::Grid::new("mdlg").show(ui, |ui| {
                    ui.label("Строк:");
                    ui.add(egui::DragValue::new(&mut d.rows).range(1..=100));
                    ui.end_row();
                    ui.label("Столбцов:");
                    ui.add(egui::DragValue::new(&mut d.cols).range(1..=100));
                    ui.end_row();
                });
                ui.horizontal(|ui| {
                    if ui.button("Вставить").clicked() {
                        done = true;
                    }
                    // with an active matrix cell: delete rows / columns starting at it
                    if ui.add_enabled(in_matrix, egui::Button::new("Удалить")).clicked() {
                        d.delete = true;
                        done = true;
                    }
                    if ui.button("Отмена").clicked() {
                        d.rows = 0;
                        done = true;
                    }
                });
            });
            if done || !open {
                if done && d.rows > 0 {
                    if in_matrix {
                        matrix_op = Some((!d.delete, d.rows, d.cols));
                    } else {
                        let row = vec!["■"; d.cols].join(", ");
                        insert = Some(format!("[{}]", vec![row; d.rows].join("; ")));
                    }
                }
                self.dlg_matrix = None;
            }
        }

        if let Some(d) = &mut self.dlg_function {
            let mut open = true;
            let mut done = false;
            let mut chosen = None;
            egui::Window::new("Вставить функцию").collapsible(false).resizable(false).default_width(520.0).open(&mut open).show(ctx, |ui| {
                ui.horizontal_top(|ui| {
                    ui.vertical(|ui| {
                        ui.label("Категория функции");
                        ScrollArea::vertical().id_salt("fcat").max_height(220.0).min_scrolled_height(220.0).show(ui, |ui| {
                            ui.set_width(200.0);
                            for (i, (cat, _)) in palettes::FUNCTIONS.iter().enumerate() {
                                if ui.selectable_label(d.group == i, *cat).clicked() {
                                    d.group = i;
                                    d.item = 0;
                                }
                            }
                        });
                    });
                    ui.vertical(|ui| {
                        ui.label("Имя функции");
                        ScrollArea::vertical().id_salt("fname").max_height(220.0).min_scrolled_height(220.0).show(ui, |ui| {
                            ui.set_width(200.0);
                            for (i, (name, _, _)) in palettes::FUNCTIONS[d.group].1.iter().enumerate() {
                                let r = ui.selectable_label(d.item == i, *name);
                                if r.clicked() {
                                    d.item = i;
                                }
                                if r.double_clicked() {
                                    d.item = i;
                                    done = true;
                                }
                            }
                        });
                    });
                });
                let (_, template, desc) = palettes::FUNCTIONS[d.group].1[d.item];
                ui.separator();
                ui.label(RichText::new(template).monospace().strong());
                ui.label(desc);
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("ОК").clicked() {
                        done = true;
                    }
                    if ui.button("Вставить").clicked() {
                        chosen = Some(template.to_string());
                    }
                    if ui.button("Отмена").clicked() {
                        d.group = usize::MAX;
                        done = true;
                    }
                });
            });
            if let Some(t) = chosen {
                insert = Some(t);
            }
            if done || !open {
                if done && d.group != usize::MAX {
                    insert = Some(palettes::FUNCTIONS[d.group].1[d.item].1.to_string());
                }
                self.dlg_function = None;
            }
        }

        if let Some(d) = &mut self.dlg_unit {
            let mut open = true;
            let mut done = false;
            let mut chosen = None;
            egui::Window::new("Вставить единицы").collapsible(false).resizable(false).open(&mut open).show(ctx, |ui| {
                ui.horizontal_top(|ui| {
                    ui.vertical(|ui| {
                        ui.label("Размерность");
                        ScrollArea::vertical().id_salt("udim").max_height(240.0).min_scrolled_height(240.0).show(ui, |ui| {
                            ui.set_width(200.0);
                            for (i, (dim, _)) in palettes::UNITS.iter().enumerate() {
                                if ui.selectable_label(d.group == i, *dim).clicked() {
                                    d.group = i;
                                    d.item = 0;
                                }
                            }
                        });
                    });
                    ui.vertical(|ui| {
                        ui.label("Единица");
                        ScrollArea::vertical().id_salt("uname").max_height(240.0).min_scrolled_height(240.0).show(ui, |ui| {
                            ui.set_width(140.0);
                            for (i, name) in palettes::UNITS[d.group].1.iter().enumerate() {
                                let r = ui.selectable_label(d.item == i, *name);
                                if r.clicked() {
                                    d.item = i;
                                }
                                if r.double_clicked() {
                                    d.item = i;
                                    done = true;
                                }
                            }
                        });
                    });
                });
                ui.horizontal(|ui| {
                    if ui.button("ОК").clicked() {
                        done = true;
                    }
                    if ui.button("Вставить").clicked() {
                        chosen = Some(palettes::UNITS[d.group].1[d.item]);
                    }
                    if ui.button("Отмена").clicked() {
                        d.group = usize::MAX;
                        done = true;
                    }
                });
            });
            let unit_text = |u: &str| format!(" {u}");
            if let Some(u) = chosen {
                insert = Some(unit_text(u));
            }
            if done || !open {
                if done && d.group != usize::MAX {
                    insert = Some(unit_text(palettes::UNITS[d.group].1[d.item]));
                }
                self.dlg_unit = None;
            }
        }

        if let Some(d) = &mut self.dlg_format {
            let mut open = true;
            let mut result: Option<bool> = None;
            egui::Window::new("Формат результата").collapsible(false).resizable(false).open(&mut open).show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut d.tab, 0, "Формат чисел");
                    ui.selectable_value(&mut d.tab, 1, "Допуск");
                });
                ui.separator();
                egui::Grid::new("fmt").num_columns(2).show(ui, |ui| {
                    if d.tab == 0 {
                        ui.label("Формат:");
                        ui.label("Основной");
                        ui.end_row();
                        ui.label("Система единиц:");
                        egui::ComboBox::from_id_salt("fmt-units").selected_text(d.opts.unit_system.label()).show_ui(ui, |ui| {
                            for u in UnitSystem::ALL {
                                ui.selectable_value(&mut d.opts.unit_system, u, u.label());
                            }
                        });
                        ui.end_row();
                        ui.label("Число десятичных знаков:");
                        ui.add(egui::DragValue::new(&mut d.opts.precision).range(0..=15));
                        ui.end_row();
                        ui.label("Порог экспоненты:");
                        ui.add(egui::DragValue::new(&mut d.opts.exp_threshold).range(0..=15));
                        ui.end_row();
                    } else {
                        ui.label("Порог нуля:");
                        ui.add(egui::DragValue::new(&mut d.opts.zero_threshold).range(0..=307));
                        ui.end_row();
                    }
                });
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("ОК").clicked() {
                        result = Some(true);
                    }
                    if ui.button("Отмена").clicked() {
                        result = Some(false);
                    }
                    if ui.button("По умолчанию").clicked() {
                        d.opts = FormatOptions::default();
                    }
                });
            });
            if result.is_some() || !open {
                if result == Some(true) && d.opts != self.ws.format {
                    let opts = d.opts;
                    self.dlg_format = None;
                    self.checkpoint();
                    self.ws.format = opts;
                    self.changed();
                } else {
                    self.dlg_format = None;
                }
            }
        }

        if self.show_prefs {
            let mut open = true;
            egui::Window::new("Параметры").collapsible(false).resizable(false).open(&mut open).anchor(Align2::CENTER_CENTER, Vec2::ZERO).show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Масштаб интерфейса:");
                    egui::ComboBox::from_id_salt("ui-scale").selected_text(crate::uiscale::label(self.ui_scale)).show_ui(ui, |ui| {
                        let mut choices: Vec<crate::uiscale::Choice> = vec![None];
                        choices.extend(crate::uiscale::LEVELS.iter().map(|p| Some(*p)));
                        for c in choices {
                            ui.selectable_value(&mut self.ui_scale, c, crate::uiscale::label(c));
                        }
                    });
                });
                ui.label(RichText::new("Не влияет на масштаб рабочего листа (Вид → Масштаб).").weak());
            });
            self.show_prefs = open;
        }

        if let Some(d) = &mut self.dlg_options {
            let mut result: Option<bool> = None;
            egui::Window::new("Параметры рабочего листа").collapsible(false).resizable(false).anchor(Align2::CENTER_CENTER, Vec2::ZERO).show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut d.tab, 0, "Встроенные переменные");
                    ui.selectable_value(&mut d.tab, 1, "Система единиц");
                });
                ui.separator();
                if d.tab == 0 {
                    egui::Grid::new("opt-vars").num_columns(2).spacing(Vec2::new(12.0, 6.0)).show(ui, |ui| {
                        ui.label("Начало отсчёта массивов (ORIGIN):");
                        ui.add(egui::DragValue::new(&mut d.origin).range(0..=100));
                        ui.end_row();
                        ui.label("Допуск сходимости (TOL):");
                        ui.add(TextEdit::singleline(&mut d.tol).desired_width(90.0));
                        ui.end_row();
                        ui.label("Допуск сходимости комплексных (CTOL):");
                        ui.add(TextEdit::singleline(&mut d.ctol).desired_width(90.0));
                        ui.end_row();
                    });
                    ui.label(RichText::new("Значения записываются в лист как глобальные определения (≡).").weak().size(11.0));
                } else {
                    for u in UnitSystem::ALL {
                        ui.radio_value(&mut d.units, u, u.label());
                    }
                    ui.label(RichText::new("Влияет на единицы результатов без явной единицы отображения.").weak().size(11.0));
                }
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("ОК").clicked() {
                        result = Some(true);
                    }
                    if ui.button("Отмена").clicked() {
                        result = Some(false);
                    }
                });
            });
            if let Some(ok) = result {
                let d = self.dlg_options.take().unwrap();
                if ok {
                    self.apply_options(&d);
                }
            }
        }

        self.plot_dialogs(ctx);

        if let Some(d) = &mut self.path_dlg {
            let mut res: Option<bool> = None;
            let title = match d.kind {
                DlgKind::Open | DlgKind::Reference => "Открыть файл",
                DlgKind::Pdf => "Экспорт в PDF",
                DlgKind::Save => "Сохранить как",
            };
            egui::Window::new(title).collapsible(false).resizable(false).anchor(Align2::CENTER_CENTER, Vec2::ZERO).show(ctx, |ui| {
                ui.label("Системный диалог недоступен. Введите путь к файлу:");
                let r = ui.add(TextEdit::singleline(&mut d.text).desired_width(460.0));
                if !r.has_focus() && !r.lost_focus() {
                    r.request_focus();
                }
                if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                    res = Some(true);
                }
                ui.horizontal(|ui| {
                    if ui.button("ОК").clicked() {
                        res = Some(true);
                    }
                    if ui.button("Отмена").clicked() {
                        res = Some(false);
                    }
                });
            });
            if let Some(ok) = res {
                let d = self.path_dlg.take().unwrap();
                let text = d.text.trim().to_string();
                if ok && !text.is_empty() {
                    self.file_chosen(ctx, d.kind, PathBuf::from(text), d.then);
                }
            }
        }

        if let Some(a) = self.confirm {
            let mut choice: Option<u8> = None;
            let name = self.doc_name();
            egui::Window::new("xmath")
                .collapsible(false)
                .resizable(false)
                .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
                .show(ctx, |ui| {
                    ui.label(format!("Сохранить изменения в «{name}»?"));
                    ui.horizontal(|ui| {
                        if ui.button("Да").clicked() {
                            choice = Some(0);
                        }
                        if ui.button("Нет").clicked() {
                            choice = Some(1);
                        }
                        if ui.button("Отмена").clicked() {
                            choice = Some(2);
                        }
                    });
                });
            match choice {
                Some(0) => {
                    self.confirm = None;
                    self.save_doc(ctx, Some(a));
                }
                Some(1) => {
                    self.confirm = None;
                    self.modified = false;
                    self.do_action(ctx, a);
                }
                Some(_) => self.confirm = None,
                None => {}
            }
        }

        if self.show_help {
            egui::Window::new("Справка xmath").open(&mut self.show_help).default_size([560.0, 520.0]).show(ctx, |ui| {
                ScrollArea::vertical().show(ui, |ui| {
                    ui.label(RichText::new(HELP).size(13.0));
                    if !self.import_log.is_empty() {
                        ui.separator();
                        ui.label(RichText::new("Журнал импорта").strong());
                        for w in &self.import_log {
                            ui.label(w);
                        }
                    }
                });
            });
        }
        if self.show_about {
            egui::Window::new("О программе xmath").open(&mut self.show_about).collapsible(false).resizable(false).show(ctx, |ui| {
                ui.heading("xmath");
                ui.label(format!("Версия {}", env!("CARGO_PKG_VERSION")));
                ui.label("Свободный аналог Mathcad 15 на Rust.");
                ui.label("Лицензия: MIT или Apache-2.0.");
                ui.hyperlink("https://github.com/t1murjan/xmath");
            });
        }

        if let Some((ins, r, c)) = matrix_op {
            // Mathcad: rows and columns are counted separately; the cell stays active
            self.matrix_edit(ctx, ins, r, c);
        }
        if let Some(s) = insert {
            let matrix = s.starts_with('[') && s.contains('■');
            if matrix && self.editing_math().is_none() {
                self.new_matrix_region(ctx, s);
            } else {
                self.insert(ctx, &s);
                if matrix {
                    self.linear_to_cells(ctx);
                }
            }
        }
    }
}

// =====================================================================
// Worksheet options, plot limits, background evaluation

/// Splits `NAME ≡ value` / `NAME := value` source into the value text.
fn builtin_def<'a>(source: &'a str, name: &str) -> Option<&'a str> {
    let rest = source.trim().strip_prefix(name)?.trim_start();
    let rest = rest.strip_prefix('≡').or_else(|| rest.strip_prefix(":="))?;
    Some(rest.trim())
}

impl App {
    /// Current ORIGIN / TOL / CTOL as defined by regions of the worksheet.
    fn builtin_values(&self) -> (i32, String, String) {
        let find = |name: &str| {
            self.ws.regions.iter().find_map(|r| match &r.kind {
                RegionKind::Math { source } => builtin_def(source, name).map(str::to_string),
                _ => None,
            })
        };
        let origin = find("ORIGIN").and_then(|v| v.parse::<i32>().ok()).unwrap_or(0);
        (origin, find("TOL").unwrap_or_else(|| "0.001".into()), find("CTOL").unwrap_or_else(|| "0.001".into()))
    }

    /// Writes `name ≡ value` into the worksheet: updates the existing definition,
    /// or inserts a global definition at the top (not needed for default values).
    fn set_builtin(&mut self, name: &str, value: &str, default: &str) {
        let src = format!("{name} ≡ {value}");
        if let Some(r) = self.ws.regions.iter_mut().find(|r| matches!(&r.kind, RegionKind::Math { source } if builtin_def(source, name).is_some())) {
            if let RegionKind::Math { source } = &mut r.kind {
                *source = src;
            }
            return;
        }
        if value == default {
            return;
        }
        let min_y = self.ws.regions.iter().map(|r| r.y).fold(f32::MAX, f32::min);
        let mut y = if min_y == f32::MAX { 8.0 } else { min_y - 28.0 };
        if y < 4.0 {
            let d = 4.0 - y;
            for r in &mut self.ws.regions {
                r.y += d;
            }
            y = 4.0;
        }
        let id = self.ws.next_id();
        self.ws.regions.push(Region::new(id, 24.0, y, RegionKind::Math { source: src }));
    }

    fn apply_options(&mut self, d: &OptionsDlg) {
        let (o, t, c) = self.builtin_values();
        let tol = d.tol.trim().replace(',', ".");
        let ctol = d.ctol.trim().replace(',', ".");
        let tol_ok = tol.parse::<f64>().is_ok_and(|v| v > 0.0);
        let ctol_ok = ctol.parse::<f64>().is_ok_and(|v| v > 0.0);
        let changed = d.origin != o || (tol_ok && tol != t) || (ctol_ok && ctol != c) || d.units != self.ws.format.unit_system;
        if !changed {
            return;
        }
        self.checkpoint();
        self.set_builtin("ORIGIN", &d.origin.to_string(), "0");
        if tol_ok {
            self.set_builtin("TOL", &tol, "0.001");
        }
        if ctol_ok {
            self.set_builtin("CTOL", &ctol, "0.001");
        }
        self.ws.format.unit_system = d.units;
        self.changed();
    }

    /// The four editable axis-limit boxes around a selected plot.
    #[allow(clippy::too_many_arguments)]
    fn axis_limit_boxes(&mut self, ui: &mut Ui, id: u64, pr: Rect, fs: f32, axes: &plot::Axes, user: [Option<f64>; 4], m: &plot::Margins, polar: bool, y2: Option<[Option<f64>; 2]>) {
        let mut auto = axes.limits().to_vec();
        let mut user = user.to_vec();
        let (w, h) = (fs * 4.6, fs * 1.5);
        let below = pr.bottom() + m.num_bottom.max(0.0) + 2.0;
        let left = pr.left() - m.num_left - w - 2.0;
        let mut slots = vec![
            Rect::from_min_size(Pos2::new(pr.left(), below), Vec2::new(w, h)),
            Rect::from_min_size(Pos2::new(pr.right() - w, below), Vec2::new(w, h)),
            Rect::from_min_size(Pos2::new(left, pr.bottom() - h), Vec2::new(w, h)),
            Rect::from_min_size(Pos2::new(left, pr.top()), Vec2::new(w, h)),
        ];
        if let Some(u2) = y2 {
            let right = pr.right() + 4.0;
            slots.push(Rect::from_min_size(Pos2::new(right, pr.bottom() - h), Vec2::new(w, h)));
            slots.push(Rect::from_min_size(Pos2::new(right, pr.top()), Vec2::new(w, h)));
            auto.extend(axes.limits2());
            user.extend(u2);
        }
        for (k, slot) in slots.iter().enumerate() {
            if polar && k < 2 {
                continue;
            }
            let tid = Id::new(("plot-lim", id, k));
            let mine = self.lim_buf.as_ref().is_some_and(|b| b.id == id && b.k == k);
            let mut text = match &self.lim_buf {
                Some(b) if mine => b.text.clone(),
                _ => user[k].map(plot::fmt_limit).unwrap_or_default(),
            };
            let hint = RichText::new(plot::fmt_limit(auto[k])).color(Color32::from_gray(120));
            let r = ui.put(
                *slot,
                TextEdit::singleline(&mut text)
                    .id(tid)
                    .font(FontId::proportional(fs * 0.85))
                    .horizontal_align(Align::Center)
                    .margin(Vec2::new(1.0, 1.0))
                    .hint_text(hint)
                    .lock_focus(true),
            );
            if r.has_focus() {
                if !mine {
                    self.lim_buf = Some(LimBuf { id, k, text: text.clone(), changed: false });
                }
                if r.changed() {
                    let t = text.trim().replace(',', ".");
                    let parsed: Option<Option<f64>> = if t.is_empty() { Some(None) } else { t.parse::<f64>().ok().filter(|v| v.is_finite()).map(Some) };
                    let mut first = false;
                    if let Some(b) = &mut self.lim_buf {
                        b.text = text.clone();
                        if parsed.is_some() {
                            first = !std::mem::replace(&mut b.changed, true);
                        }
                    }
                    if let Some(v) = parsed {
                        if first {
                            self.checkpoint();
                        }
                        if let Some(Region { kind: RegionKind::Plot { x_min, x_max, y_min, y_max, fmt, .. }, .. }) = self.region_mut(id) {
                            if k < 4 {
                                *[x_min, x_max, y_min, y_max][k] = v;
                            } else {
                                *[&mut fmt.y2_min, &mut fmt.y2_max][k - 4] = v;
                            }
                        }
                        self.changed();
                    }
                }
                if ui.input(|i| i.key_pressed(Key::Enter) || i.key_pressed(Key::Escape)) {
                    r.surrender_focus();
                    self.lim_buf = None;
                }
            } else if mine {
                self.lim_buf = None;
            }
        }
    }

    /// Starts a background evaluation once typing has paused (debounce).
    fn drive_evaluation(&mut self, ctx: &Context) {
        while let Ok(done) = self.eval_rx.try_recv() {
            if done.gen == self.eval_gen {
                self.outputs = done.outputs;
                self.evaluating = false;
                ctx.request_repaint();
            }
        }
        let wanted = self.dirty && (self.auto_calc || self.force_calc);
        if wanted {
            let since = *self.dirty_since.get_or_insert_with(|| Instant::now() - Duration::from_secs(1));
            let wait = Duration::from_millis(150);
            if since.elapsed() >= wait {
                self.eval_gen += 1;
                let gen = self.eval_gen;
                let ws = self.ws.clone();
                let env = self.eval_env();
                let tx = self.eval_tx.clone();
                let c = ctx.clone();
                self.dirty = false;
                self.force_calc = false;
                self.dirty_since = None;
                self.evaluating = true;
                std::thread::spawn(move || {
                    let outputs = xmath_core::evaluate_worksheet_with(&ws, false, &env);
                    let _ = tx.send(EvalDone { gen, outputs });
                    c.request_repaint();
                });
            } else {
                ctx.request_repaint_after(wait - since.elapsed());
            }
        }
    }
}

const HELP: &str = "\
Ввод (как в Mathcad 15)
  щёлкните на пустом месте — появится красный крестик, затем печатайте
  x : 5 m          определение  (:  →  :=)
  f(x) : x^2 + 1   функция
  g ~ 9.81         глобальное определение  (~  →  ≡)
  i : 0 ; 10       ранжированная переменная  (;  →  ..)
  v[i] : i^2       индексированная переменная
  x =              вычислить
  F = kN           вычислить в выбранных единицах
  \"                 текстовая область
  @                X-Y график (в поле слева — функции через запятую, снизу — переменная)

Редактирование формулы (2D, как в Mathcad)
  формула остаётся двумерной во время набора: «/» — дробь, «^» — степень, «\\» — корень,
  «[» — индекс, «(» и «'» — скобки, имя функции + «(» — вызов
  Пробел  расширить выделение (подчёркнутый операнд): a+b, Пробел, /c  →  (a+b)/c
  ← → ↑ ↓ Home End  перемещение по полям;  Tab / Shift+Tab  следующее / предыдущее поле
  Backspace / Delete  удаляют символ или оператор; пустая структура удаляется целиком
  Insert  оператор слева/справа от операнда;  Ctrl+Z / Ctrl+Y  отмена шагов ввода
  Ctrl+C / Ctrl+V  копировать / вставить подвыражение как текст
  F2 (Правка → Редактировать как текст)  ввод строкой — для строк \"\" и программ { }

Клавиши
  \\  корень   |  модуль   ?  производная   &  интеграл   $  сумма   #  произведение
  Ctrl+=  логическое равенство   Ctrl+3  ≠   Ctrl+9  ≤   Ctrl+0  ≥
  Ctrl+G  греческая буква   Ctrl+M  матрица   Ctrl+E  функция   Ctrl+U  единицы
  Tab  следующее поле ■   Enter  завершить ввод   Esc  отмена
  F9  вычислить   Ctrl+Z / Ctrl+Y  отмена / повтор   Ctrl+D  удалить область

Функции
  sum(k, 1, n, f)   prod(k, 1, n, f)   integrate(f(x), x, a, b)   diff(f(x), x)
  root(f(x), x) — от начального значения,  root(f(x), x, a, b) — на отрезке
  if(условие, a, b)   lsolve(M, v)   polyroots(v)   max, min, mean, stdev ...
  Полный список: Вставка → Функция (Ctrl+E).

Единицы: m, cm, mm, kg, gm (грамм), s, min, hr, N, kN, Pa, MPa, J, W, V, A, Ω, deg ...
  g — ускорение свободного падения, как в Mathcad.";

impl eframe::App for App {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        storage.set_string("xmath-ui", self.ui_state());
    }

    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        if ctx.input(|i| i.viewport().close_requested()) && self.modified && !self.allow_close {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.confirm = Some(Action::Exit);
        }

        self.hint = ctx.data_mut(|d| d.remove_temp::<String>(Id::new("xmath-hint"))).unwrap_or_default();
        if !self.hint.is_empty() {
            ctx.request_repaint();
        }
        let f = crate::uiscale::effective(ctx, self.ui_scale, &mut self.ui_scale_auto);
        crate::uiscale::apply(ctx, f);
        self.poll_dialog(ctx);
        self.shortcuts(ctx);
        self.sheet_keys(ctx);

        egui::TopBottomPanel::top("menu").frame(egui::Frame::default().fill(PANEL).inner_margin(2.0)).show(ctx, |ui| {
            self.menu_bar(ui);
        });
        if self.bars.standard || self.bars.formatting || self.bars.math {
            egui::TopBottomPanel::top("toolbars").frame(egui::Frame::default().fill(PANEL).inner_margin(egui::Margin::symmetric(4, 2))).show(ctx, |ui| {
                if self.bars.standard {
                    self.standard_toolbar(ui);
                }
                if self.bars.formatting || self.bars.math {
                    ui.horizontal(|ui| {
                        if self.bars.formatting {
                            self.formatting_toolbar(ui);
                            separator(ui);
                        }
                        if self.bars.math {
                            self.math_toolbar(ui);
                        }
                    });
                }
            });
        }
        if self.bars.status {
            egui::TopBottomPanel::bottom("status").frame(egui::Frame::default().fill(PANEL).inner_margin(egui::Margin::symmetric(4, 2))).show(ctx, |ui| {
                self.status_bar(ui);
            });
        }
        egui::CentralPanel::default().frame(egui::Frame::default().fill(Color32::WHITE)).show(ctx, |ui| {
            self.canvas(ui);
        });
        self.palette_windows(ctx);
        self.dialogs(ctx);
        self.print_windows(ctx);

        for a in std::mem::take(&mut self.pending) {
            self.run(ctx, a);
        }

        self.drive_evaluation(ctx);
        self.dump_rects_tick(ctx);

        let title = format!("xmath - [{}{}]", self.doc_name(), if self.modified { " *" } else { "" });
        if title != self.title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.title = title;
        }
    }
}
