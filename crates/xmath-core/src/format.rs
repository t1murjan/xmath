//! Result formatting in the Mathcad style (3 decimals, exponential threshold 3).

use crate::units::{self, Dim};
use crate::value::{Num, Value};
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
}

impl Default for FormatOptions {
    fn default() -> Self {
        FormatOptions { precision: 3, exp_threshold: 3, zero_threshold: 15 }
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

/// Chooses how to display a dimension: a named derived unit if one matches
/// exactly, otherwise a product of SI base units.
pub fn unit_text(dim: Dim) -> UnitText {
    if dim.is_none() {
        return UnitText::default();
    }
    if let Some((name, _)) = units::DERIVED_DISPLAY.iter().find(|(_, d)| *d == dim) {
        return UnitText(vec![UnitPow { name: name.to_string(), num: 1, den: 1 }]);
    }
    // try a derived unit times a single base unit power (e.g. N·m, Pa·s)
    let mut out = Vec::new();
    for i in 0..7 {
        let (num, den) = dim.exponent(i);
        if num != 0 {
            out.push(UnitPow { name: units::BASE_UNITS[i].to_string(), num, den });
        }
    }
    // Mathcad lists kg first: kg·m/s²
    out.sort_by_key(|u| match u.name.as_str() {
        "kg" => 0,
        "m" => 1,
        _ => 2,
    });
    UnitText(out)
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
    Function(String),
}

impl ResultView {
    /// Builds the view, optionally expressing the value in a user-chosen unit
    /// (`unit` is that unit's value in SI).
    pub fn from_value(v: &Value, unit: Option<Num>, o: &FormatOptions) -> Result<ResultView, String> {
        let scale = |n: Num| -> Result<Num, String> {
            match unit {
                None => Ok(n),
                Some(u) => {
                    if u.v.norm() == 0.0 {
                        return Err("the display unit is zero".into());
                    }
                    Ok(Num::new(n.v / u.v, n.dim.div(u.dim)))
                }
            }
        };
        match v {
            Value::Num(n) => {
                let n = scale(*n)?;
                Ok(ResultView::Scalar { number: format_num(n, o), units: unit_text(n.dim) })
            }
            Value::Range(r) => {
                let m = crate::value::Matrix::column(r.values().collect());
                ResultView::from_value(&Value::Matrix(m), unit, o)
            }
            Value::Matrix(m) => {
                let data = m.data.iter().map(|n| scale(*n)).collect::<Result<Vec<_>, _>>()?;
                let cells = data.iter().map(|n| format_num(*n, o)).collect();
                let d0 = data.first().map(|n| n.dim).unwrap_or(Dim::NONE);
                let uniform = data.iter().all(|n| n.dim == d0);
                let (cell_units, units) = if uniform {
                    (None, unit_text(d0))
                } else {
                    (Some(data.iter().map(|n| unit_text(n.dim)).collect()), UnitText::default())
                };
                Ok(ResultView::Matrix { rows: m.rows, cols: m.cols, cells, cell_units, units })
            }
            Value::Str(s) => Ok(ResultView::Text(s.to_string())),
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
        assert_eq!(unit_text(Dim::from_ints([-1, 1, -1, 0, 0, 0, 0])).to_string(), "kg/(m·s)");
        assert_eq!(unit_text(units::LENGTH.powf(0.5).unwrap()).to_string(), "m^(1/2)");
    }
}
