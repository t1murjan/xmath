//! The worksheet document: freely placed regions evaluated top-to-bottom,
//! left-to-right, like in Mathcad.

use crate::ast::{Expr, Stmt, Target};
use crate::eval::{Evaluator, Scope};
use crate::fileio::EvalEnv;
use crate::format::{FormatOptions, ResultView};
use crate::solve;
use crate::parser::{parse_expr, parse_statement, ParseError};
use crate::plot3d::{Plot3dData, Plot3dFormat, Plot3dKind, View3d};
use crate::value::{EResult, Num, Value};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub const FILE_VERSION: u32 = 1;

/// Kind of a 2D plot: Cartesian X-Y or polar.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PlotKind {
    #[default]
    Xy,
    Polar,
}

impl PlotKind {
    pub fn is_xy(&self) -> bool {
        *self == PlotKind::Xy
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Worksheet {
    #[serde(default = "file_version")]
    pub version: u32,
    #[serde(default)]
    pub format: FormatOptions,
    /// Document-level default format of new plots («Использовать в качестве значений по умолчанию»).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plot_defaults: Option<PlotFormat>,
    /// Paper, orientation and margins for printing and PDF export.
    #[serde(default, skip_serializing_if = "PageSetup::is_default")]
    pub page: PageSetup,
    pub regions: Vec<Region>,
}

/// Paper sizes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Paper {
    #[default]
    A4,
    A3,
    A5,
    Letter,
    Legal,
}

impl Paper {
    pub const ALL: [Paper; 5] = [Paper::A4, Paper::A3, Paper::A5, Paper::Letter, Paper::Legal];

    /// Portrait size in millimetres (width, height).
    pub fn size_mm(self) -> (f32, f32) {
        match self {
            Paper::A4 => (210.0, 297.0),
            Paper::A3 => (297.0, 420.0),
            Paper::A5 => (148.0, 210.0),
            Paper::Letter => (215.9, 279.4),
            Paper::Legal => (215.9, 355.6),
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Paper::A4 => "A4",
            Paper::A3 => "A3",
            Paper::A5 => "A5",
            Paper::Letter => "Letter",
            Paper::Legal => "Legal",
        }
    }
}

/// Millimetres to worksheet pixels (96 dpi).
pub const MM_TO_PX: f32 = 96.0 / 25.4;

/// Page parameters (Файл → Параметры страницы).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PageSetup {
    pub paper: Paper,
    pub landscape: bool,
    /// Margins in millimetres: left, top, right, bottom.
    pub margins: [f32; 4],
}

impl Default for PageSetup {
    fn default() -> Self {
        // the printable area of A4 is then about 756 x 1039 worksheet pixels,
        // the size of the page the canvas has always shown
        PageSetup { paper: Paper::A4, landscape: false, margins: [5.0, 11.0, 5.0, 11.0] }
    }
}

impl PageSetup {
    pub fn is_default(&self) -> bool {
        *self == PageSetup::default()
    }

    /// Paper size in millimetres with the orientation applied.
    pub fn paper_mm(&self) -> (f32, f32) {
        let (w, h) = self.paper.size_mm();
        if self.landscape {
            (h, w)
        } else {
            (w, h)
        }
    }

    /// Size of the printable area in worksheet pixels (never smaller than 50 x 50).
    pub fn printable_px(&self) -> (f32, f32) {
        let (w, h) = self.paper_mm();
        let m = &self.margins;
        (((w - m[0] - m[2]) * MM_TO_PX).max(50.0), ((h - m[1] - m[3]) * MM_TO_PX).max(50.0))
    }
}

fn file_version() -> u32 {
    FILE_VERSION
}

impl Default for Worksheet {
    fn default() -> Self {
        Worksheet { version: FILE_VERSION, format: FormatOptions::default(), plot_defaults: None, page: PageSetup::default(), regions: Vec::new() }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Region {
    pub id: u64,
    pub x: f32,
    pub y: f32,
    /// Offset of the evaluation anchor (the baseline of the definition or
    /// evaluation operator) from the region's top-left corner. Mathcad orders
    /// regions by this point, not by the corner. `[0, 0]` for regions that do
    /// not know it (then the top-left corner is used).
    #[serde(default, skip_serializing_if = "anchor_is_zero")]
    pub anchor: [f32; 2],
    #[serde(flatten)]
    pub kind: RegionKind,
}

fn anchor_is_zero(a: &[f32; 2]) -> bool {
    a[0] == 0.0 && a[1] == 0.0
}

impl Region {
    pub fn new(id: u64, x: f32, y: f32, kind: RegionKind) -> Region {
        Region { id, x, y, anchor: [0.0, 0.0], kind }
    }

    /// The point regions are ordered by (see [`Worksheet::evaluation_order`]).
    pub fn anchor_pos(&self) -> (f32, f32) {
        (self.x + self.anchor[0], self.y + self.anchor[1])
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum RegionKind {
    Math {
        source: String,
    },
    Text {
        text: String,
        #[serde(default = "default_text_width")]
        width: f32,
        #[serde(default)]
        style: TextStyle,
        /// Wrap the text at `width` (imported Mathcad text regions); otherwise
        /// it wraps at the page margin.
        #[serde(default, skip_serializing_if = "is_false")]
        lock_width: bool,
    },
    Plot {
        /// Abscissa expression (usually a range variable).
        #[serde(rename = "x_expr")]
        x: String,
        /// Ordinate expressions.
        #[serde(rename = "y_exprs")]
        y: Vec<String>,
        #[serde(default = "default_plot_w")]
        width: f32,
        #[serde(default = "default_plot_h")]
        height: f32,
        /// Axis limits; `None` means autoscale.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        x_min: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        x_max: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        y_min: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        y_max: Option<f64>,
        /// Abscissa per trace when the traces have different ones (parallel to
        /// `y`); empty when `x` applies to all traces.
        #[serde(default, rename = "x_exprs", skip_serializing_if = "Vec::is_empty")]
        xs: Vec<String>,
        /// Grid, axis style, log scales and trace formatting.
        #[serde(default, skip_serializing_if = "PlotFormat::is_default")]
        fmt: PlotFormat,
        /// Cartesian (`x` = abscissa) or polar (`x` = angle θ, `y` = radii).
        #[serde(default, skip_serializing_if = "PlotKind::is_xy")]
        kind: PlotKind,
    },
    /// Surface, contour, 3D bar and 3D scatter plots: one placeholder with a
    /// matrix, a function of two variables, or `X, Y, Z` vectors.
    Plot3d {
        #[serde(default)]
        kind: Plot3dKind,
        expr: String,
        #[serde(default = "default_plot_w")]
        width: f32,
        #[serde(default = "default_plot_h")]
        height: f32,
        #[serde(default)]
        view: View3d,
        #[serde(default, skip_serializing_if = "Plot3dFormat::is_default")]
        fmt: Plot3dFormat,
    },
    /// Reference to another worksheet (Mathcad: Insert → Reference): that worksheet is
    /// evaluated first and its definitions are visible below this region. `href` is a path
    /// relative to the directory of this worksheet (`.xmath` or `.xmcd`).
    Reference { href: String },
    /// A picture (imported Mathcad pictures and plots that could not be
    /// decoded). Not editable.
    Image {
        /// Base64 of a PNG (or JPEG) file.
        png_base64: String,
        /// Size in worksheet pixels.
        width: f32,
        height: f32,
    },
}

fn is_false(b: &bool) -> bool {
    !*b
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AxisStyle {
    /// Axes cross at zero (Mathcad default).
    #[default]
    Crossed,
    /// Axes along the frame.
    Boxed,
    None,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LineStyle {
    #[default]
    Solid,
    Dot,
    Dash,
    DashDot,
    /// Markers only, no connecting line.
    Points,
}

/// Marker symbol of a trace.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Symbol {
    #[default]
    None,
    Triangle,
    Square,
    Circle,
    Diamond,
    Cross,
    Plus,
    OCircle,
    OSquare,
}

impl Symbol {
    pub const ALL: [Symbol; 9] = [
        Symbol::None,
        Symbol::Triangle,
        Symbol::Square,
        Symbol::Circle,
        Symbol::Diamond,
        Symbol::Cross,
        Symbol::Plus,
        Symbol::OCircle,
        Symbol::OSquare,
    ];
    pub fn glyph(self) -> &'static str {
        match self {
            Symbol::None => "",
            Symbol::Triangle => "▲",
            Symbol::Square => "■",
            Symbol::Circle => "●",
            Symbol::Diamond => "◆",
            Symbol::Cross => "×",
            Symbol::Plus => "+",
            Symbol::OCircle => "○",
            Symbol::OSquare => "□",
        }
    }
    fn is_none(&self) -> bool {
        *self == Symbol::None
    }
}

/// Mathcad's trace types.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TraceType {
    #[default]
    Lines,
    Points,
    Error,
    Bars,
    Step,
    Stem,
}

impl TraceType {
    pub const ALL: [TraceType; 6] = [TraceType::Lines, TraceType::Points, TraceType::Error, TraceType::Bars, TraceType::Step, TraceType::Stem];
    pub fn name(self) -> &'static str {
        match self {
            TraceType::Lines => "линии",
            TraceType::Points => "точки",
            TraceType::Error => "ошибки",
            TraceType::Bars => "столбцы",
            TraceType::Step => "ступенчатый",
            TraceType::Stem => "гистограмма",
        }
    }
    fn is_lines(&self) -> bool {
        *self == TraceType::Lines
    }
}

/// Overrides of the default look of one trace.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TraceFormat {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<[u8; 3]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<LineStyle>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<f32>,
    /// Legend label (empty: "кривая N").
    #[serde(skip_serializing_if = "String::is_empty")]
    pub legend: String,
    #[serde(skip_serializing_if = "Symbol::is_none")]
    pub symbol: Symbol,
    /// A symbol is drawn at every n-th point (0 is treated as 1).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol_freq: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol_width: Option<f32>,
    #[serde(skip_serializing_if = "TraceType::is_lines")]
    pub kind: TraceType,
    /// Plotted against the secondary Y axis.
    #[serde(skip_serializing_if = "is_false")]
    pub y2: bool,
}

/// Per-axis options of the format dialog.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AxisOpts {
    /// `None`: follows `PlotFormat::grid`.
    pub grid: Option<bool>,
    pub grid_color: [u8; 3],
    pub numbering: bool,
    /// Draw marker lines.
    pub markers: bool,
    pub marker_color: [u8; 3],
    pub autogrid: bool,
    /// Number of grid lines when `autogrid` is off.
    pub grids: u32,
    /// Values of the marker lines, e.g. `0.5, 2` (used when `markers` is on).
    pub marker_values: String,
}

impl Default for AxisOpts {
    fn default() -> Self {
        AxisOpts { grid: None, grid_color: [0, 200, 0], numbering: true, markers: false, marker_color: [255, 0, 0], autogrid: true, grids: 10, marker_values: String::new() }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LegendPos {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
    #[default]
    Bottom,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NumKind {
    #[default]
    General,
    Decimal,
    Scientific,
    Engineering,
    Fraction,
}

/// Number format of the axis numbers («Формат числа»).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct NumFormat {
    pub kind: NumKind,
    pub precision: u32,
    pub trailing_zeros: bool,
    pub eng_exponents: bool,
    pub exp_threshold: i32,
}

impl Default for NumFormat {
    fn default() -> Self {
        NumFormat { kind: NumKind::General, precision: 3, trailing_zeros: false, eng_exponents: false, exp_threshold: 3 }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AxisLabel {
    pub show: bool,
    pub text: String,
}

impl Default for AxisLabel {
    fn default() -> Self {
        AxisLabel { show: true, text: String::new() }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PlotFormat {
    /// Grid lines of the X axis (and the default for the Y axes).
    pub grid: bool,
    pub axes: AxisStyle,
    pub log_x: bool,
    pub log_y: bool,
    pub log_y2: bool,
    /// Show the legend even when no trace has its own label.
    pub legend: bool,
    pub hide_legend: bool,
    pub legend_pos: LegendPos,
    pub hide_args: bool,
    pub traces: Vec<TraceFormat>,
    /// Options of the X, main Y and secondary Y axes.
    pub axis: [AxisOpts; 3],
    /// Secondary Y axis enabled.
    pub y2_axis: bool,
    pub y2_min: Option<f64>,
    pub y2_max: Option<f64>,
    /// Equal scale on both axes.
    pub equal_scale: bool,
    pub number: NumFormat,
    pub title: String,
    pub title_bottom: bool,
    pub show_title: bool,
    /// Axis titles: X, Y, Y2.
    pub labels: [AxisLabel; 3],
}

impl Default for PlotFormat {
    fn default() -> Self {
        PlotFormat {
            grid: true,
            axes: AxisStyle::Crossed,
            log_x: false,
            log_y: false,
            log_y2: false,
            legend: false,
            hide_legend: false,
            legend_pos: LegendPos::Bottom,
            hide_args: false,
            traces: Vec::new(),
            axis: Default::default(),
            y2_axis: false,
            y2_min: None,
            y2_max: None,
            equal_scale: false,
            number: NumFormat::default(),
            title: String::new(),
            title_bottom: false,
            show_title: true,
            labels: Default::default(),
        }
    }
}

impl PlotFormat {
    /// The format of a freshly inserted plot: like the default, but with Mathcad's manual grid
    /// (about ten grid lines) instead of automatic steps. Saved files without a format keep `default()`.
    pub fn new_plot_default() -> PlotFormat {
        let mut f = PlotFormat::default();
        for a in &mut f.axis {
            a.autogrid = false;
        }
        f
    }

    pub fn is_default(&self) -> bool {
        *self == PlotFormat::default()
    }
    /// Grid of axis `k` (0 = X, 1 = Y, 2 = Y2).
    pub fn grid_of(&self, k: usize) -> bool {
        self.axis[k].grid.unwrap_or(self.grid)
    }
    /// Whether a legend is drawn.
    pub fn legend_visible(&self) -> bool {
        !self.hide_legend && (self.legend || self.traces.iter().any(|t| !t.legend.is_empty()))
    }
}

/// Character formatting of a text region.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TextStyle {
    /// Font size in points.
    pub size: f32,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    /// Typeface class (Arial, Times New Roman or Courier New and their look-alikes).
    pub font: TextFont,
}

/// Typeface class of a text region.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum TextFont {
    /// Arial and anything unknown.
    #[default]
    Sans,
    /// Times New Roman, Cambria, Georgia.
    Serif,
    /// Courier New.
    Mono,
}

impl TextFont {
    /// The class of a font family name.
    pub fn from_family(name: &str) -> TextFont {
        let n = name.to_lowercase();
        if n.contains("courier") || n.contains("consolas") || n.contains("mono") {
            TextFont::Mono
        } else if n.contains("times") || n.contains("cambria") || n.contains("georgia") || n.contains("serif") && !n.contains("sans") {
            TextFont::Serif
        } else {
            TextFont::Sans
        }
    }
}

impl Default for TextStyle {
    fn default() -> Self {
        TextStyle { size: 10.0, bold: false, italic: false, underline: false, font: TextFont::Sans }
    }
}

fn default_text_width() -> f32 {
    320.0
}
fn default_plot_w() -> f32 {
    360.0
}
fn default_plot_h() -> f32 {
    240.0
}

impl RegionKind {
    pub fn new_plot() -> RegionKind {
        RegionKind::Plot {
            x: String::new(),
            y: vec![String::new()],
            width: default_plot_w(),
            height: default_plot_h(),
            x_min: None,
            x_max: None,
            y_min: None,
            y_max: None,
            fmt: PlotFormat::default(),
            xs: Vec::new(),
            kind: PlotKind::Xy,
        }
    }

    pub fn new_polar_plot() -> RegionKind {
        let mut k = RegionKind::new_plot();
        if let RegionKind::Plot { kind, width, height, .. } = &mut k {
            *kind = PlotKind::Polar;
            *width = 300.0;
            *height = 280.0;
        }
        k
    }

    pub fn new_plot3d(kind: Plot3dKind) -> RegionKind {
        RegionKind::Plot3d { kind, expr: String::new(), width: 340.0, height: 280.0, view: View3d::default(), fmt: Plot3dFormat::default() }
    }

    pub fn new_text(text: impl Into<String>) -> RegionKind {
        RegionKind::Text { text: text.into(), width: default_text_width(), style: TextStyle::default(), lock_width: false }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Series {
    pub label: String,
    pub points: Vec<[f64; 2]>,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct PlotData {
    pub series: Vec<Series>,
    /// Errors per y-expression (or for the x-expression).
    pub errors: Vec<String>,
    /// Variable swept along the x-axis (QuickPlot detection), if any.
    pub x_var: Option<String>,
    /// Fixed axis limits from the region (`None`: automatic).
    pub x_min: Option<f64>,
    pub x_max: Option<f64>,
    pub y_min: Option<f64>,
    pub y_max: Option<f64>,
    pub kind: PlotKind,
    /// Display units of the swept variable and of the first ordinate (`""`: unitless).
    /// Values are plotted in SI base units, like Mathcad 15 does; the unit is
    /// shown next to the axis.
    pub x_unit: String,
    pub y_unit: String,
}

/// A plain copy of a computed value (for comparing results with Mathcad's
/// saved ones); only produced by [`Worksheet::evaluate_opts`] with snapshots on.
#[derive(Clone, Debug, PartialEq)]
pub struct Snapshot {
    pub rows: usize,
    pub cols: usize,
    /// Elements in row-major order, in SI units.
    pub data: Vec<Num>,
    /// Set for string values.
    pub text: Option<String>,
}

impl Snapshot {
    fn of(v: &Value) -> Option<Snapshot> {
        match v {
            Value::Num(n) => Some(Snapshot { rows: 1, cols: 1, data: vec![*n], text: None }),
            Value::Matrix(m) => Some(Snapshot { rows: m.rows, cols: m.cols, data: m.data.clone(), text: None }),
            Value::Range(r) => {
                let d: Vec<Num> = r.values().collect();
                Some(Snapshot { rows: d.len(), cols: 1, data: d, text: None })
            }
            Value::Str(s) => Some(Snapshot { rows: 1, cols: 1, data: Vec::new(), text: Some(s.to_string()) }),
            Value::Func(_) => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Output {
    /// Nothing to show (definitions, text regions).
    None,
    Value { view: ResultView, unit: Option<Expr> },
    Error(String),
    Plot(PlotData),
    Plot3d(Plot3dData),
}

#[derive(Clone, Debug, PartialEq)]
pub struct RegionOutput {
    pub stmt: Option<Result<Stmt, ParseError>>,
    pub output: Output,
    /// Computed value (only with snapshots enabled).
    pub value: Option<Snapshot>,
}

impl RegionOutput {
    pub fn error(&self) -> Option<String> {
        match (&self.stmt, &self.output) {
            (Some(Err(e)), _) => Some(e.msg.clone()),
            (_, Output::Error(e)) => Some(e.clone()),
            _ => None,
        }
    }
}

/// Anchors closer than this (in worksheet pixels) vertically are on one row.
pub const ROW_TOLERANCE: f32 = 0.5;

/// Default plot domain when the abscissa is an undefined variable (QuickPlot).
const QUICKPLOT_RANGE: (f64, f64, usize) = (-10.0, 10.0, 400);

impl Worksheet {
    /// Region indices in evaluation order like Mathcad: by the anchor point
    /// (baseline of the definition/evaluation operator), top-to-bottom, and
    /// left-to-right within a row. Anchors whose vertical positions differ by
    /// less than [`ROW_TOLERANCE`] are on the same row.
    pub fn evaluation_order(&self) -> Vec<usize> {
        let mut idx: Vec<usize> = (0..self.regions.len()).collect();
        idx.sort_by(|&a, &b| {
            let (pa, pb) = (self.regions[a].anchor_pos(), self.regions[b].anchor_pos());
            pa.1.total_cmp(&pb.1).then(pa.0.total_cmp(&pb.0))
        });
        // regroup rows: consecutive anchors closer than the tolerance are sorted by x
        let mut out = Vec::with_capacity(idx.len());
        let mut i = 0;
        while i < idx.len() {
            let mut j = i + 1;
            while j < idx.len()
                && self.regions[idx[j]].anchor_pos().1 - self.regions[idx[j - 1]].anchor_pos().1 < ROW_TOLERANCE
            {
                j += 1;
            }
            let mut row = idx[i..j].to_vec();
            row.sort_by(|&a, &b| self.regions[a].anchor_pos().0.total_cmp(&self.regions[b].anchor_pos().0));
            out.extend(row);
            i = j;
        }
        out
    }

    pub fn next_id(&self) -> u64 {
        self.regions.iter().map(|r| r.id).max().map_or(1, |m| m + 1)
    }

    pub fn to_json(&self) -> String {
        // (plot expressions are stored as `x_expr`/`y_exprs`, so they don't clash
        // with the region position `x`/`y`)
        serde_json::to_string_pretty(self).expect("worksheet serialization")
    }

    pub fn from_json(s: &str) -> Result<Worksheet, String> {
        serde_json::from_str(s).map_err(|e| e.to_string())
    }

    /// Builds a worksheet from plain text: one math region per line,
    /// lines starting with `#` become text regions.
    pub fn from_lines(src: &str) -> Worksheet {
        let mut ws = Worksheet::default();
        let mut y = 20.0;
        for line in src.lines() {
            let t = line.trim();
            if t.is_empty() {
                y += 12.0;
                continue;
            }
            let kind = if let Some(text) = t.strip_prefix('#') {
                RegionKind::new_text(text.trim())
            } else {
                RegionKind::Math { source: t.to_string() }
            };
            let id = ws.next_id();
            ws.regions.push(Region::new(id, 20.0, y, kind));
            y += 30.0;
        }
        ws
    }

    /// Evaluates the whole worksheet.
    pub fn evaluate(&self) -> HashMap<u64, RegionOutput> {
        self.evaluate_opts(false)
    }

    /// Evaluates the whole worksheet; with `snapshots` every computed value is
    /// also kept in [`RegionOutput::value`].
    pub fn evaluate_opts(&self, snapshots: bool) -> HashMap<u64, RegionOutput> {
        self.evaluate_with(snapshots, &EvalEnv::default())
    }

    /// Like [`evaluate_opts`](Self::evaluate_opts) in a worksheet directory `env`: relative file
    /// names of `READPRN`/`WRITEPRN`/`READEXCEL` and references are resolved there.
    pub fn evaluate_with(&self, snapshots: bool, env: &EvalEnv) -> HashMap<u64, RegionOutput> {
        let mut ev = Evaluator::new();
        ev.env = env.clone();
        self.run_regions(&mut ev, snapshots)
    }

    /// Evaluates the regions in `ev` (definitions stay in it afterwards: references use this).
    fn run_regions(&self, ev: &mut Evaluator, snapshots: bool) -> HashMap<u64, RegionOutput> {
        let mut out = HashMap::new();
        let order = self.evaluation_order();
        let parsed: Vec<Option<Result<Stmt, ParseError>>> = self
            .regions
            .iter()
            .map(|r| match &r.kind {
                RegionKind::Math { source } => Some(parse_statement(source)),
                _ => None,
            })
            .collect();

        // Pass 1: global definitions (≡) are visible everywhere.
        for &i in &order {
            if let Some(Ok(Stmt::Define { target, expr, global: true, show })) = &parsed[i] {
                let (output, shown) = self.define_output(ev, target, expr, show);
                let value = if snapshots { snapshot_var(ev, target).or(shown) } else { None };
                out.insert(self.regions[i].id, RegionOutput { stmt: parsed[i].clone(), output, value });
            }
        }

        // Pass 2: everything else in document order.
        let mut block: Option<SolveBlock> = None;
        for &i in &order {
            let region = &self.regions[i];
            let mut value = None;
            let output = match (&region.kind, &parsed[i]) {
                (_, Some(Ok(Stmt::Define { global: true, .. }))) => continue,
                (RegionKind::Math { .. }, Some(Err(_))) => Output::None,
                (RegionKind::Math { .. }, Some(Ok(stmt))) => {
                    self.eval_in_block(ev, stmt, region.id, &mut block, &mut out, snapshots.then_some(&mut value))
                }
                (RegionKind::Plot3d { kind, expr, .. }, _) => Output::Plot3d(plot3d(ev, *kind, expr)),
                (RegionKind::Plot { x, y, xs, x_min, x_max, y_min, y_max, kind, .. }, _) => {
                    let polar = *kind == PlotKind::Polar;
                    let mut d = if xs.len() == y.len() && xs.iter().any(|v| v != &xs[0]) {
                        let mut d = PlotData::default();
                        for (xi, yi) in xs.iter().zip(y) {
                            let part = plot(ev, xi, std::slice::from_ref(yi), polar);
                            d.series.extend(part.series);
                            d.errors.extend(part.errors);
                            d.x_var = d.x_var.or(part.x_var);
                        }
                        d
                    } else {
                        plot(ev, if x.is_empty() { xs.first().map_or("", |s| s.as_str()) } else { x }, y, polar)
                    };
                    d.kind = *kind;
                    (d.x_min, d.x_max, d.y_min, d.y_max) = (*x_min, *x_max, *y_min, *y_max);
                    Output::Plot(d)
                }
                (RegionKind::Reference { href }, _) => self.eval_reference(ev, href),
                _ => Output::None,
            };
            out.insert(region.id, RegionOutput { stmt: parsed[i].clone(), output, value });
        }
        if let Some(b) = block {
            flag_unclosed(&mut out, &b);
        }
        out
    }

    /// Evaluates the referenced worksheet in `ev`, so its definitions are visible below.
    fn eval_reference(&self, ev: &mut Evaluator, href: &str) -> Output {
        const MAX_DEPTH: usize = 16;
        let path = crate::fileio::resolve_read(&ev.env, href.trim());
        let canon = path.canonicalize().unwrap_or_else(|_| path.clone());
        if ev.env.stack.len() >= MAX_DEPTH {
            return Output::Error("слишком глубокая вложенность ссылок".into());
        }
        if ev.env.stack.contains(&canon) {
            return Output::Error(format!("циклическая ссылка: {href}"));
        }
        let loaded = (|| -> Result<Worksheet, String> {
            let bytes = std::fs::read(&path).map_err(|e| format!("не удалось прочитать {}: {e}", path.display()))?;
            let src = String::from_utf8_lossy(&bytes).into_owned();
            let head = src.trim_start_matches('\u{feff}').trim_start();
            if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("xmcd")) || head.starts_with("<?xml") {
                crate::xmcd::import_xmcd(&src).map(|r| r.worksheet)
            } else if head.starts_with('{') {
                Worksheet::from_json(&src)
            } else {
                Ok(Worksheet::from_lines(&src))
            }
        })();
        let sub = match loaded {
            Ok(w) => w,
            Err(e) => return Output::Error(e),
        };
        let saved = ev.env.clone();
        ev.env.dir = path.parent().map(|p| p.to_path_buf()).or(saved.dir.clone());
        ev.env.stack.push(canon);
        sub.run_regions(ev, false);
        ev.env = saved;
        Output::None
    }

    /// Handles solve blocks: `Given`, constraint regions and the closing call.
    fn eval_in_block(
        &self,
        ev: &mut Evaluator,
        stmt: &Stmt,
        id: u64,
        block: &mut Option<SolveBlock>,
        out: &mut HashMap<u64, RegionOutput>,
        snap: Option<&mut Option<Snapshot>>,
    ) -> Output {
        let expr_of = |stmt: &Stmt| match stmt {
            Stmt::Define { expr, .. } | Stmt::Eval { expr, .. } | Stmt::Show(expr) => expr.clone(),
        };
        if matches!(stmt, Stmt::Show(Expr::Var(n)) if n.eq_ignore_ascii_case("given")) {
            if let Some(old) = block.take() {
                flag_unclosed(out, &old);
            }
            *block = Some(SolveBlock { given: id, constraints: Vec::new() });
            return Output::None;
        }
        let expr = expr_of(stmt);
        if !solve::contains_solve(&expr) {
            let Some(b) = block.as_mut() else {
                return self.eval_stmt(ev, stmt, snap);
            };
            return match stmt {
                Stmt::Show(e) if solve::is_constraint(e) => {
                    b.constraints.push((id, e.clone()));
                    Output::None
                }
                _ => self.eval_stmt(ev, stmt, snap),
            };
        }
        // a closing call: Find / Minerr / Maximize / Minimize
        let b = block.take();
        let cs: Vec<Expr> = b.as_ref().map(|b| b.constraints.iter().map(|(_, e)| e.clone()).collect()).unwrap_or_default();
        if let Some(b) = &b {
            // constraint regions may only use defined names, the unknowns and function parameters
            let mut known = solve::solve_call_vars(&expr);
            if let Stmt::Define { target: Target::Func(_, params), .. } = stmt {
                known.extend(params.iter().cloned());
            }
            let sc = ev.top_scope();
            for (cid, ce) in &b.constraints {
                let mut names = Vec::new();
                crate::eval::free_vars(ce, &mut Vec::new(), &mut names);
                if let Some(n) = names.iter().find(|n| !known.contains(n) && ev.lookup(n, &sc).is_none()) {
                    if let Some(o) = out.get_mut(cid) {
                        o.output = Output::Error(format!("'{n}' не определена"));
                    }
                }
            }
        }
        if b.is_none() {
            return self.eval_stmt(ev, stmt, snap);
        }
        let injected = solve::inject(&expr, &cs);
        let new_stmt = match stmt {
            Stmt::Define { target, global, show, .. } => {
                Stmt::Define { target: target.clone(), expr: injected, global: *global, show: show.clone() }
            }
            Stmt::Eval { unit, .. } => Stmt::Eval { expr: injected, unit: unit.clone() },
            Stmt::Show(_) => Stmt::Show(injected),
        };
        self.eval_stmt(ev, &new_stmt, snap)
    }

    fn eval_stmt(&self, ev: &mut Evaluator, stmt: &Stmt, snap: Option<&mut Option<Snapshot>>) -> Output {
        let mut snap = snap;
        let result: EResult<Output> = (|| match stmt {
            Stmt::Define { target, expr, show, .. } => {
                let (o, shown) = self.define_output(ev, target, expr, show);
                if let Some(s) = snap.as_deref_mut() {
                    *s = snapshot_var(ev, target).or(shown);
                }
                return Ok(o);
            }
            Stmt::Eval { expr, unit } => {
                let sc = ev.top_scope();
                let v = ev.eval_iterated(expr, &sc)?;
                if let Some(s) = snap.as_deref_mut() {
                    *s = Snapshot::of(&v);
                }
                self.value_output(ev, &v, unit)
            }
            Stmt::Show(_) => Ok(Output::None),
        })();
        result.unwrap_or_else(Output::Error)
    }

    /// Runs a definition; with a displayed value (`d := expr =`) the output is that value.
    /// Also returns a snapshot of the displayed value (for indexed and vector targets, which
    /// have no variable of their own to snapshot).
    fn define_output(&self, ev: &mut Evaluator, target: &Target, expr: &Expr, show: &Option<Option<Expr>>) -> (Output, Option<Snapshot>) {
        let mut shown = None;
        let r = ev.define_value(target, expr).and_then(|v| match show {
            None => Ok(Output::None),
            Some(unit) => {
                // `v[i] := expr =` displays the assigned element(s), not the whole vector
                let v = match target {
                    Target::Indexed(name, idx) => {
                        let sc = ev.top_scope();
                        ev.eval_iterated(&Expr::Index(Box::new(Expr::Var(name.clone())), idx.clone()), &sc)?
                    }
                    _ => v,
                };
                shown = Snapshot::of(&v);
                self.value_output(ev, &v, unit)
            }
        });
        (r.unwrap_or_else(Output::Error), shown)
    }

    /// The displayed result of a value in the optional display unit.
    fn value_output(&self, ev: &Evaluator, v: &Value, unit: &Option<Expr>) -> EResult<Output> {
        let sc = ev.top_scope();
        let u = match unit {
            None => None,
            Some(u) => Some(ev.eval(u, &sc)?.as_num().map_err(|_| "единица отображения должна быть скаляром".to_string())?),
        };
        // `T = °C`: offset display unit (zero point in kelvin), unless shadowed
        let off = match (unit, u) {
            (Some(crate::Expr::Var(n)), Some(un)) => crate::units::offset_unit(n)
                .filter(|d| un.v.re == d.factor)
                .map_or(0.0, |d| d.offset_si()),
            _ => 0.0,
        };
        let view = ResultView::from_value_offset(v, u, off, &self.format)?;
        Ok(Output::Value { view, unit: unit.clone() })
    }
}

/// An open solve block: the `Given` region and the constraints collected so far.
struct SolveBlock {
    given: u64,
    constraints: Vec<(u64, Expr)>,
}

fn flag_unclosed(out: &mut HashMap<u64, RegionOutput>, b: &SolveBlock) {
    if let Some(o) = out.get_mut(&b.given) {
        o.output = Output::Error("Given без Find, Minerr, Maximize или Minimize: блок решения не закрыт".into());
    }
}

fn snapshot_var(ev: &Evaluator, target: &Target) -> Option<Snapshot> {
    match target {
        Target::Var(name) => ev.lookup(name, &ev.top_scope()).and_then(|v| Snapshot::of(&v)),
        _ => None,
    }
}

fn plot(ev: &Evaluator, x_src: &str, y_srcs: &[String], polar: bool) -> PlotData {
    let mut data = PlotData::default();
    let ys: Vec<(String, Expr)> = y_srcs
        .iter()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .filter_map(|s| match parse_expr(s) {
            Ok(e) => Some((s.to_string(), e)),
            Err(e) => {
                data.errors.push(format!("{s}: {}", e.msg));
                None
            }
        })
        .collect();
    if ys.is_empty() {
        return data;
    }
    let sc = ev.top_scope();
    let x_src = x_src.trim();

    // The variable swept along the x-axis, if any.
    let sweep_var: Option<String> = if x_src.is_empty() {
        let mut names = Vec::new();
        crate::eval::free_vars(&ys[0].1, &mut Vec::new(), &mut names);
        names
            .into_iter()
            .find(|n| matches!(ev.lookup(n, &sc), None | Some(Value::Range(_))))
    } else {
        match parse_expr(x_src) {
            Ok(Expr::Var(n)) if matches!(ev.lookup(&n, &sc), None | Some(Value::Range(_))) => Some(n),
            Ok(_) => None,
            Err(e) => {
                data.errors.push(format!("ось X: {}", e.msg));
                return data;
            }
        }
    };

    data.x_var = sweep_var.clone();
    match sweep_var {
        Some(name) => {
            let values: Vec<Num> = match ev.lookup(&name, &sc) {
                Some(Value::Range(r)) => r.values().collect(),
                _ => {
                    let (a, b, n) = if polar { (0.0, std::f64::consts::TAU, 360) } else { QUICKPLOT_RANGE };
                    (0..=n).map(|k| Num::real(a + (b - a) * k as f64 / n as f64)).collect()
                }
            };
            if let Some(t) = values.first().filter(|t| !t.dim.is_none()) {
                data.x_unit = crate::format::unit_text(t.dim).to_string();
            }
            for (label, e) in &ys {
                let mut points = Vec::with_capacity(values.len());
                let mut first_err = None;
                for t in &values {
                    match ev.eval(e, &sc.with(&name, Value::Num(*t))).and_then(|v| v.as_num()) {
                        Ok(y) if y.is_real() && y.v.re.is_finite() => {
                            if data.y_unit.is_empty() && !y.dim.is_none() {
                                data.y_unit = crate::format::unit_text(y.dim).to_string();
                            }
                            points.push([t.v.re, y.v.re])
                        }
                        Ok(_) => points.push([t.v.re, f64::NAN]),
                        Err(err) => {
                            first_err.get_or_insert(err);
                            points.push([t.v.re, f64::NAN]);
                        }
                    }
                }
                if points.iter().all(|p| p[1].is_nan()) {
                    if let Some(err) = first_err {
                        data.errors.push(format!("{label}: {err}"));
                        continue;
                    }
                }
                data.series.push(Series { label: label.clone(), points });
            }
        }
        None => {
            let xs = match parse_expr(x_src).map_err(|e| e.msg).and_then(|e| vector_of(ev, &e, &sc)) {
                Ok(v) => v,
                Err(e) => {
                    data.errors.push(format!("ось X: {e}"));
                    return data;
                }
            };
            if let Some(t) = xs.first().filter(|t| !t.dim.is_none()) {
                data.x_unit = crate::format::unit_text(t.dim).to_string();
            }
            for (label, e) in &ys {
                match vector_of(ev, e, &sc) {
                    Ok(yv) if yv.len() == xs.len() => {
                        if let Some(y) = yv.first().filter(|y| !y.dim.is_none()) {
                            if data.y_unit.is_empty() {
                                data.y_unit = crate::format::unit_text(y.dim).to_string();
                            }
                        }
                        data.series.push(Series {
                        label: label.clone(),
                        points: xs.iter().zip(&yv).map(|(x, y)| [x.v.re, y.v.re]).collect(),
                    })
                    }
                    Ok(yv) => data.errors.push(format!(
                        "{label}: {} значений, а по оси X — {}",
                        yv.len(),
                        xs.len()
                    )),
                    Err(err) => data.errors.push(format!("{label}: {err}")),
                }
            }
        }
    }
    data
}

fn vector_of(ev: &Evaluator, e: &Expr, sc: &Scope) -> EResult<Vec<Num>> {
    match ev.eval_iterated(e, sc)? {
        Value::Matrix(m) if m.is_vector() => Ok(m.data),
        Value::Num(n) => Ok(vec![n]),
        Value::Range(r) => Ok(r.values().collect()),
        other => Err(format!("ожидался вектор, найдено: {}", other.type_name())),
    }
}


const QUICKPLOT_3D: (f64, f64, usize) = (-5.0, 5.0, 25);

/// Computes the data of a 3D plot. `expr` is a matrix (surface/contour/bar over
/// the index grid), a function of two variables or an expression with up to two
/// undefined variables (QuickPlot over -5..5), and for scatter plots `X, Y, Z`
/// vectors or an n x 3 matrix.
fn plot3d(ev: &Evaluator, kind: Plot3dKind, src: &str) -> Plot3dData {
    let mut d = Plot3dData { kind, label: src.trim().to_string(), ..Default::default() };
    if src.trim().is_empty() {
        return d;
    }
    match plot3d_inner(ev, kind, src.trim(), &mut d) {
        Ok(()) => {}
        Err(e) => d.error = Some(e),
    }
    d
}

/// Splits `X, Y, Z` or `(X, Y, Z)` at top-level commas.
pub fn split_top_commas(s: &str) -> Vec<String> {
    let s = s.trim();
    let inner = s.strip_prefix('(').and_then(|r| r.strip_suffix(')')).filter(|r| {
        let mut depth = 0i32;
        r.chars().all(|c| {
            match c {
                '(' | '[' => depth += 1,
                ')' | ']' => depth -= 1,
                _ => {}
            }
            depth >= 0
        })
    });
    let s = inner.unwrap_or(s);
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut cur = String::new();
    for c in s.chars() {
        match c {
            '(' | '[' => depth += 1,
            ')' | ']' => depth -= 1,
            ',' if depth == 0 => {
                out.push(cur.trim().to_string());
                cur.clear();
                continue;
            }
            _ => {}
        }
        cur.push(c);
    }
    out.push(cur.trim().to_string());
    out
}

fn flat_numbers(v: Value) -> EResult<Vec<f64>> {
    match v {
        Value::Matrix(m) => Ok(m.data.iter().map(|n| n.v.re).collect()),
        Value::Num(n) => Ok(vec![n.v.re]),
        Value::Range(r) => Ok(r.values().map(|n| n.v.re).collect()),
        other => Err(format!("ожидался вектор или матрица, найдено: {}", other.type_name())),
    }
}

fn plot3d_inner(ev: &Evaluator, kind: Plot3dKind, src: &str, d: &mut Plot3dData) -> EResult<()> {
    let sc = ev.top_scope();
    if kind == Plot3dKind::Scatter {
        let parts = split_top_commas(src);
        let (x, y, z) = if parts.len() == 3 {
            let get = |s: &str| -> EResult<Vec<f64>> {
                let e = parse_expr(s).map_err(|e| format!("{s}: {}", e.msg))?;
                flat_numbers(ev.eval_iterated(&e, &sc)?)
            };
            (get(&parts[0])?, get(&parts[1])?, get(&parts[2])?)
        } else if parts.len() == 1 {
            let e = parse_expr(&parts[0]).map_err(|e| e.msg)?;
            match ev.eval_iterated(&e, &sc)? {
                Value::Matrix(m) if m.cols == 3 => (
                    (0..m.rows).map(|r| m.get(r, 0).v.re).collect(),
                    (0..m.rows).map(|r| m.get(r, 1).v.re).collect(),
                    (0..m.rows).map(|r| m.get(r, 2).v.re).collect(),
                ),
                _ => return Err("ожидались три вектора (X, Y, Z) или матрица с тремя столбцами".into()),
            }
        } else {
            return Err("ожидались три вектора (X, Y, Z)".into());
        };
        if x.len() != y.len() || y.len() != z.len() {
            return Err(format!("векторы разной длины: {}, {}, {}", x.len(), y.len(), z.len()));
        }
        d.pts = x.iter().zip(&y).zip(&z).map(|((a, b), c)| [*a, *b, *c]).collect();
        return Ok(());
    }
    let e = parse_expr(src).map_err(|e| e.msg)?;
    let mut names = Vec::new();
    crate::eval::free_vars(&e, &mut Vec::new(), &mut names);
    let mut undefined: Vec<String> = Vec::new();
    for n in names {
        if matches!(ev.lookup(&n, &sc), None | Some(Value::Range(_))) && !undefined.contains(&n) {
            undefined.push(n);
        }
    }
    let (a, b, n) = QUICKPLOT_3D;
    let axis = || -> Vec<f64> { (0..n).map(|k| a + (b - a) * k as f64 / (n - 1) as f64).collect() };
    let func_of = |f: &dyn Fn(f64, f64) -> EResult<Num>, d: &mut Plot3dData| {
        d.xs = axis();
        d.ys = axis();
        let mut first_err = None;
        for &x in &d.xs.clone() {
            for &y in &d.ys.clone() {
                match f(x, y) {
                    Ok(v) if v.is_real() && v.v.re.is_finite() => d.z.push(v.v.re),
                    Ok(_) => d.z.push(f64::NAN),
                    Err(e) => {
                        first_err.get_or_insert(e);
                        d.z.push(f64::NAN);
                    }
                }
            }
        }
        match first_err {
            Some(e) if d.z.iter().all(|v| v.is_nan()) => Err(e),
            _ => Ok(()),
        }
    };
    if undefined.is_empty() {
        // a matrix or a function name
        if let Expr::Var(name) = &e {
            if let Some(Value::Func(_)) = ev.lookup(name, &sc) {
                let call = Expr::Call(name.clone(), vec![Expr::Var("\u{1}x".into()), Expr::Var("\u{1}y".into())]);
                return func_of(
                    &|x, y| ev.eval(&call, &sc.with("\u{1}x", Value::Num(Num::real(x))).with("\u{1}y", Value::Num(Num::real(y)))).and_then(|v| v.as_num()),
                    d,
                );
            }
        }
        match ev.eval_iterated(&e, &sc)? {
            Value::Matrix(m) => {
                d.xs = (0..m.rows).map(|i| i as f64).collect();
                d.ys = (0..m.cols).map(|j| j as f64).collect();
                d.z = m.data.iter().map(|v| if v.is_real() { v.v.re } else { f64::NAN }).collect();
                Ok(())
            }
            other => Err(format!("ожидалась матрица или функция двух переменных, найдено: {}", other.type_name())),
        }
    } else {
        // expression with free variables: x and y (or the first two names)
        let vx = if undefined.iter().any(|n| n == "x") { "x".to_string() } else { undefined[0].clone() };
        let vy = if undefined.iter().any(|n| n == "y") && vx != "y" {
            Some("y".to_string())
        } else {
            undefined.iter().find(|n| **n != vx).cloned()
        };
        func_of(
            &|x, y| {
                let mut s2 = sc.with(&vx, Value::Num(Num::real(x)));
                if let Some(vy) = &vy {
                    s2 = s2.with(vy, Value::Num(Num::real(y)));
                }
                ev.eval(&e, &s2).and_then(|v| v.as_num())
            },
            d,
        )
    }
}
