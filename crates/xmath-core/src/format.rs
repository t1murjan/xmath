//! Result formatting in the Mathcad style (3 decimals, exponential threshold 3).

use crate::units::{self, Dim};
use crate::value::{Num, Value};
use num_complex::Complex64 as C64;
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FormatOptions {
    /// Number of decimal places shown.
    pub precision: usize,
    /// Numbers with |x| ≥ 10^t or |x| < 10^-t are shown in exponential form.
    pub exp_threshold: i32,
    /// Numbers with |x| < 10^-z are shown as 0.
    pub zero_threshold: i32,
    /// Unit system for automatic result units (default SI).
    pub unit_system: UnitSystem,
}

impl Default for FormatOptions {
    fn default() -> Self {
        FormatOptions { precision: 3, exp_threshold: 3, zero_threshold: 15, unit_system: UnitSystem::Si }
    }
}

/// A piece of a formatted number.
#[derive(Clone, Debug, PartialEq)]
pub enum Piece {
    Text(String),
    /// "× 10^n"
    Pow10(i32),
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct NumberText(pub Vec<Piece>);

impl fmt::Display for NumberText {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for p in &self.0 {
            match p {
                Piece::Text(t) => write!(f, "{t}")?,
                Piece::Pow10(e) => write!(f, "·10^{e}")?,
            }
        }
        Ok(())
    }
}

/// A unit raised to a rational power, e.g. `s^-2`.
#[derive(Clone, Debug, PartialEq)]
pub struct UnitPow {
    pub name: String,
    pub num: i32,
    pub den: i32,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct UnitText(pub Vec<UnitPow>);

impl UnitText {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

fn fmt_exp(f: &mut fmt::Formatter<'_>, num: i32, den: i32) -> fmt::Result {
    if den != 1 {
        write!(f, "^({num}/{den})")
    } else if num != 1 {
        write!(f, "^{num}")
    } else {
        Ok(())
    }
}

impl fmt::Display for UnitText {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let pos: Vec<_> = self.0.iter().filter(|u| u.num > 0).collect();
        let neg: Vec<_> = self.0.iter().filter(|u| u.num < 0).collect();
        if pos.is_empty() {
            write!(f, "1")?;
        }
        for (i, u) in pos.iter().enumerate() {
            if i > 0 {
                write!(f, "·")?;
            }
            write!(f, "{}", u.name)?;
            fmt_exp(f, u.num, u.den)?;
        }
        if !neg.is_empty() {
            write!(f, "/")?;
            if neg.len() > 1 {
                write!(f, "(")?;
            }
            for (i, u) in neg.iter().enumerate() {
                if i > 0 {
                    write!(f, "·")?;
                }
                write!(f, "{}", u.name)?;
                fmt_exp(f, -u.num, u.den)?;
            }
            if neg.len() > 1 {
                write!(f, ")")?;
            }
        }
        Ok(())
    }
}

/// Mathcad's "Unit System" worksheet option (Tools -> Worksheet Options).
/// It only changes how *automatic* result units are displayed; all
/// computation stays in SI.
///
/// * `Si`  - m, kg, s, A, K, mol, cd with N, Pa, J, W, ...
/// * `Mks` - same display as SI (Mathcad's MKS differs only in electrical
///   base conventions, which xmath does not model).
/// * `Cgs` - cm, gm, s with dyne and erg (pressure shows as `dyne/cm^2`).
/// * `Us`  - ft, lb, s, R with lbf, psi and hp.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UnitSystem {
    #[default]
    Si,
    Mks,
    Cgs,
    Us,
}

impl UnitSystem {
    pub const ALL: [UnitSystem; 4] = [UnitSystem::Si, UnitSystem::Mks, UnitSystem::Cgs, UnitSystem::Us];

    pub fn label(self) -> &'static str {
        match self {
            UnitSystem::Si => "SI",
            UnitSystem::Mks => "MKS",
            UnitSystem::Cgs => "CGS",
            UnitSystem::Us => "U.S.",
        }
    }
}

/// A named unit used by automatic display: (name, dimension, factor to SI).
type Named = (&'static str, Dim, f64);

const ELECTRIC: [Named; 8] = [
    ("C", units::CHARGE, 1.0),
    ("V", units::VOLTAGE, 1.0),
    ("Ω", units::RESISTANCE, 1.0),
    ("S", units::CONDUCTANCE, 1.0),
    ("F", units::CAPACITANCE, 1.0),
    ("H", units::INDUCTANCE, 1.0),
    ("Wb", units::MAG_FLUX, 1.0),
    ("T", units::MAG_FLUX_DENSITY, 1.0),
];

/// Base units (m, kg, s, A, K, mol, cd) and derived units of a system.
fn system_units(sys: UnitSystem) -> ([(&'static str, f64); 7], Vec<Named>) {
    let si = ["m", "kg", "s", "A", "K", "mol", "cd"].map(|n| (n, 1.0));
    match sys {
        UnitSystem::Si | UnitSystem::Mks => {
            let mut d: Vec<Named> = vec![
                // order matters for ties: W/(m·K) rather than N/(s·K)
                ("W", units::POWER, 1.0),
                ("N", units::FORCE, 1.0),
                ("Pa", units::PRESSURE, 1.0),
                ("J", units::ENERGY, 1.0),
            ];
            d.extend(ELECTRIC);
            (si, d)
        }
        UnitSystem::Cgs => {
            let mut b = si;
            b[0] = ("cm", 1e-2);
            b[1] = ("gm", 1e-3);
            let mut d: Vec<Named> = vec![("dyne", units::FORCE, 1e-5), ("erg", units::ENERGY, 1e-7)];
            d.extend(ELECTRIC);
            (b, d)
        }
        UnitSystem::Us => {
            let mut b = si;
            b[0] = ("ft", 0.3048);
            b[1] = ("lb", units::LB);
            b[4] = ("R", 5.0 / 9.0);
            let mut d: Vec<Named> = vec![
                ("lbf", units::FORCE, units::LBF),
                ("psi", units::PRESSURE, units::PSI),
                ("hp", units::POWER, 745.699_871_582_270_2),
            ];
            d.extend(ELECTRIC);
            (b, d)
        }
    }
}

/// Chooses how to display a dimension in the SI system.
pub fn unit_text(dim: Dim) -> UnitText {
    auto_units(dim, UnitSystem::Si).0
}

type Repr = Vec<(UnitPow, f64, usize)>;

/// Mathcad-style automatic units for `dim` in `sys`, plus the factor of the
/// displayed unit relative to SI (divide an SI value by it to get the number
/// to show).
///
/// Rules (documented in `docs/units.md`):
/// 1. A dimension that exactly matches a derived unit of the system is shown
///    with it (`N`, `Pa`, `J`, `W`, `C`, `V`, `Ω`, `S`, `F`, `H`, `Wb`, `T`).
///    Energy is always `J` (never `N·m`); frequency stays `1/s`.
/// 2. Otherwise the representation with the fewest unit factors wins, among
///    base units alone and one derived unit (to the power +1 or -1) times
///    base units: `Pa·s`, `W/(m·K)`, `V/m`, `kg/m^3`, `m^2/s`.
/// 3. On a tie base units win (`kg·m^2`, `m^2/s^2`), except that the
///    force, pressure and power units win when no mass remains (`N/m`, `W/m^2`).
/// 4. Order: derived unit, then kg (gm/lb), m, s, A, K, ...; negative
///    exponents after a `/`.
pub fn auto_units(dim: Dim, sys: UnitSystem) -> (UnitText, f64) {
    if dim.is_none() {
        return (UnitText::default(), 1.0);
    }
    let (bases, derived) = system_units(sys);
    let pow = |name: &str, num: i32, den: i32, factor: f64| {
        (UnitPow { name: name.to_string(), num, den }, factor.powf(num as f64 / den as f64))
    };
    let base_repr = |d: Dim| -> Repr {
        let mut out: Repr = Vec::new();
        for (i, (name, f)) in bases.iter().enumerate() {
            let (num, den) = d.exponent(i);
            if num != 0 {
                let (p, fac) = pow(name, num, den, *f);
                out.push((p, fac, i));
            }
        }
        // Mathcad lists the mass unit first, then length: kg·m/s^2
        out.sort_by_key(|(_, _, i)| match i {
            1 => 0,
            0 => 1,
            _ => 2,
        });
        out
    };
    if let Some((name, _, f)) = derived.iter().find(|(_, d, _)| *d == dim) {
        return (UnitText(vec![UnitPow { name: name.to_string(), num: 1, den: 1 }]), *f);
    }
    let mut best: (Repr, bool) = (base_repr(dim), false);
    for (name, d, f) in &derived {
        for k in [1, -1] {
            let residual = if k == 1 { dim.div(*d) } else { dim.mul(*d) };
            let (p, fac) = pow(name, k, 1, *f);
            let mut repr: Repr = vec![(p, fac, 0)];
            repr.extend(base_repr(residual));
            let wins_ties = k == 1 && residual.0[1] == 0 && residual.0.iter().all(|e| *e <= 0) && matches!(*name, "N" | "Pa" | "W" | "dyne" | "lbf" | "psi" | "hp");
            let better = repr.len() < best.0.len() || (repr.len() == best.0.len() && wins_ties && !best.1);
            if better {
                best = (repr, wins_ties);
            }
        }
    }
    let factor = best.0.iter().map(|(_, f, _)| *f).product();
    (UnitText(best.0.into_iter().map(|(p, _, _)| p).collect()), factor)
}

fn trim_zeros(s: String) -> String {
    if s.contains('.') {
        let t = s.trim_end_matches('0').trim_end_matches('.');
        t.to_string()
    } else {
        s
    }
}

fn format_real(x: f64, o: &FormatOptions) -> NumberText {
    if x.is_nan() {
        return NumberText(vec![Piece::Text("NaN".into())]);
    }
    if x.is_infinite() || x.abs() >= 1e307 {
        return NumberText(vec![Piece::Text(if x > 0.0 { "∞" } else { "-∞" }.into())]);
    }
    if x == 0.0 || x.abs() < 10f64.powi(-o.zero_threshold) {
        return NumberText(vec![Piece::Text("0".into())]);
    }
    let t = o.exp_threshold;
    let ax = x.abs();
    let mut e = ax.log10().floor() as i32;
    // Round half away from zero (1.2345 -> 1.235) like Mathcad, despite binary round-off.
    let fmt = |v: f64| trim_zeros(format!("{:.*}", o.precision, v * (1.0 + 4.0 * f64::EPSILON)));
    let fixed = fmt(x);
    // Numbers that would round to zero in fixed notation use the exponent form too.
    let use_exp = e >= t || e <= -t || fixed == "0" || fixed == "-0";
    if !use_exp {
        return NumberText(vec![Piece::Text(fixed)]);
    }
    let mut ms = fmt(x / 10f64.powi(e));
    if ms.trim_start_matches('-').starts_with("10") {
        e += 1;
        ms = fmt(x / 10f64.powi(e));
    }
    NumberText(vec![Piece::Text(ms), Piece::Pow10(e)])
}

pub fn format_complex(re: f64, im: f64, o: &FormatOptions) -> NumberText {
    let zero = 10f64.powi(-o.zero_threshold);
    let scale = re.abs().max(im.abs());
    let im_negligible = im.abs() < zero || im.abs() < 1e-15 * scale;
    let re_negligible = re.abs() < zero || re.abs() < 1e-15 * scale;
    if im_negligible {
        return format_real(re, o);
    }
    let mut pieces = Vec::new();
    if !re_negligible {
        pieces.extend(format_real(re, o).0);
        pieces.push(Piece::Text(if im < 0.0 { " - " } else { " + " }.into()));
        pieces.extend(format_real(im.abs(), o).0);
    } else {
        pieces.extend(format_real(im, o).0);
    }
    pieces.push(Piece::Text("i".into()));
    NumberText(pieces)
}

pub fn format_num(n: Num, o: &FormatOptions) -> NumberText {
    format_complex(n.v.re, n.v.im, o)
}

/// A displayable evaluation result.
#[derive(Clone, Debug, PartialEq)]
pub enum ResultView {
    Scalar {
        number: NumberText,
        units: UnitText,
    },
    Matrix {
        rows: usize,
        cols: usize,
        cells: Vec<NumberText>,
        /// Per-cell units when the elements have different units.
        cell_units: Option<Vec<UnitText>>,
        /// Common units shown after the matrix.
        units: UnitText,
    },
    Text(String),
    /// A table of strings (`PolyLookup(6)`).
    TextMatrix {
        rows: usize,
        cols: usize,
        cells: Vec<String>,
    },
    Function(String),
}

impl ResultView {
    /// Builds the view, optionally expressing the value in a user-chosen unit
    /// (`unit` is that unit's value in SI).
    pub fn from_value(v: &Value, unit: Option<Num>, o: &FormatOptions) -> Result<ResultView, String> {
        ResultView::from_value_offset(v, unit, 0.0, o)
    }

    /// Like [`from_value`](Self::from_value) for an offset display unit such
    /// as `°C`: `unit_offset` is the zero point of that scale in SI kelvin
    /// (273.15 for `°C`). It is subtracted from temperature values before
    /// scaling; values of other dimensions ignore it. Any dimension left over
    /// after dividing by the display unit is shown as automatic units
    /// (`5 N = kg` gives `5 m/s^2`, i.e. 5 m/s^2·kg).
    pub fn from_value_offset(
        v: &Value,
        unit: Option<Num>,
        unit_offset: f64,
        o: &FormatOptions,
    ) -> Result<ResultView, String> {
        // First divide by the user unit, then by the automatic unit factor.
        let scale = |n: Num| -> Result<(Num, UnitText), String> {
            let n = match unit {
                None => n,
                Some(u) => {
                    if u.v.norm() == 0.0 {
                        return Err("единица отображения равна нулю".into());
                    }
                    let shifted = if unit_offset != 0.0 && n.dim == u.dim {
                        C64::new(n.v.re - unit_offset, n.v.im)
                    } else {
                        n.v
                    };
                    Num::new(shifted / u.v, n.dim.div(u.dim))
                }
            };
            let (ut, f) = auto_units(n.dim, o.unit_system);
            Ok((Num::new(n.v / f, n.dim), ut))
        };
        match v {
            Value::Num(n) => {
                let (n, units) = scale(*n)?;
                Ok(ResultView::Scalar { number: format_num(n, o), units })
            }
            Value::Range(r) => {
                let m = crate::value::Matrix::column(r.values().collect());
                ResultView::from_value_offset(&Value::Matrix(m), unit, unit_offset, o)
            }
            Value::Matrix(m) => {
                let data = m.data.iter().map(|n| scale(*n)).collect::<Result<Vec<_>, _>>()?;
                let cells = data.iter().map(|(n, _)| format_num(*n, o)).collect();
                let d0 = data.first().map(|(n, _)| n.dim).unwrap_or(Dim::NONE);
                let uniform = data.iter().all(|(n, _)| n.dim == d0);
                let (cell_units, units) = if uniform {
                    (None, data.first().map(|(_, u)| u.clone()).unwrap_or_default())
                } else {
                    (Some(data.iter().map(|(_, u)| u.clone()).collect()), UnitText::default())
                };
                Ok(ResultView::Matrix { rows: m.rows, cols: m.cols, cells, cell_units, units })
            }
            Value::Str(s) => Ok(ResultView::Text(s.to_string())),
            Value::StrMat(t) => Ok(ResultView::TextMatrix {
                rows: t.rows,
                cols: t.cols,
                cells: t.data.iter().map(|s| s.to_string()).collect(),
            }),
            Value::Func(f) => Ok(ResultView::Function(format!("{}({})", f.name, f.params.join(", ")))),
        }
    }
}

impl fmt::Display for ResultView {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let units = |f: &mut fmt::Formatter<'_>, u: &UnitText| {
            if u.is_empty() {
                Ok(())
            } else {
                write!(f, " {u}")
            }
        };
        match self {
            ResultView::Scalar { number, units: u } => {
                write!(f, "{number}")?;
                units(f, u)
            }
            ResultView::Matrix { rows, cols, cells, cell_units, units: u } => {
                write!(f, "[")?;
                for r in 0..*rows {
                    if r > 0 {
                        write!(f, "; ")?;
                    }
                    for c in 0..*cols {
                        if c > 0 {
                            write!(f, ", ")?;
                        }
                        let k = r * cols + c;
                        write!(f, "{}", cells[k])?;
                        if let Some(cu) = cell_units {
                            units(f, &cu[k])?;
                        }
                    }
                }
                write!(f, "]")?;
                units(f, u)
            }
            ResultView::Text(s) => write!(f, "\"{s}\""),
            ResultView::TextMatrix { rows, cols, cells } => {
                write!(f, "[")?;
                for r in 0..*rows {
                    if r > 0 {
                        write!(f, "; ")?;
                    }
                    for c in 0..*cols {
                        if c > 0 {
                            write!(f, ", ")?;
                        }
                        write!(f, "\"{}\"", cells[r * cols + c])?;
                    }
                }
                write!(f, "]")
            }
            ResultView::Function(s) => write!(f, "{s}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(x: f64) -> String {
        format_real(x, &FormatOptions::default()).to_string()
    }

    #[test]
    fn numbers() {
        assert_eq!(f(0.0), "0");
        assert_eq!(f(1.0), "1");
        assert_eq!(f(3.14159), "3.142");
        assert_eq!(f(-2.5), "-2.5");
        assert_eq!(f(999.0), "999");
        assert_eq!(f(1000.0), "1·10^3");
        assert_eq!(f(12345.0), "1.235·10^4");
        assert_eq!(f(0.01), "0.01");
        assert_eq!(f(0.001), "1·10^-3");
        assert_eq!(f(0.0004), "4·10^-4");
        assert_eq!(f(9.9996e5), "1·10^6");
        assert_eq!(f(1e-20), "0");
        assert_eq!(format_complex(1.0, -2.0, &FormatOptions::default()).to_string(), "1 - 2i");
        assert_eq!(format_complex(0.0, 2.0, &FormatOptions::default()).to_string(), "2i");
    }

    #[test]
    fn units() {
        assert_eq!(unit_text(units::FORCE).to_string(), "N");
        assert_eq!(unit_text(units::VELOCITY).to_string(), "m/s");
        assert_eq!(unit_text(units::ACCELERATION).to_string(), "m/s^2");
        assert_eq!(unit_text(units::FREQUENCY).to_string(), "1/s");
        assert_eq!(unit_text(Dim::from_ints([-1, 1, -1, 0, 0, 0, 0])).to_string(), "Pa·s");
        assert_eq!(unit_text(units::LENGTH.powf(0.5).unwrap()).to_string(), "m^(1/2)");
    }
}
