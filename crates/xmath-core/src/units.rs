//! Physical dimensions and the built-in unit table (SI based).

use std::fmt;

/// Exponents are stored in 1/12 steps, so square roots, cube roots and
/// fourth roots of units stay exact (e.g. `sqrt(m)` -> `m^(1/2)`).
pub const DIM_SCALE: i32 = 12;

/// Names of the seven SI base dimensions, in storage order.
pub const BASE_UNITS: [&str; 7] = ["m", "kg", "s", "A", "K", "mol", "cd"];

#[derive(Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Dim(pub [i32; 7]);

impl Dim {
    pub const NONE: Dim = Dim([0; 7]);

    pub const fn base(i: usize) -> Dim {
        let mut d = [0; 7];
        d[i] = DIM_SCALE;
        Dim(d)
    }

    /// Builds a dimension from integer exponents of (m, kg, s, A, K, mol, cd).
    pub const fn from_ints(e: [i32; 7]) -> Dim {
        let mut d = [0; 7];
        let mut i = 0;
        while i < 7 {
            d[i] = e[i] * DIM_SCALE;
            i += 1;
        }
        Dim(d)
    }

    pub fn is_none(&self) -> bool {
        self.0 == [0; 7]
    }

    pub fn mul(self, o: Dim) -> Dim {
        let mut d = self.0;
        for (a, b) in d.iter_mut().zip(o.0) {
            *a += b;
        }
        Dim(d)
    }

    pub fn div(self, o: Dim) -> Dim {
        let mut d = self.0;
        for (a, b) in d.iter_mut().zip(o.0) {
            *a -= b;
        }
        Dim(d)
    }

    pub fn inv(self) -> Dim {
        Dim::NONE.div(self)
    }

    /// Raises the dimension to a real power; fails if the result is not
    /// representable (e.g. `m^0.3`).
    pub fn powf(self, p: f64) -> Option<Dim> {
        if self.is_none() {
            return Some(self);
        }
        let mut d = [0; 7];
        for (i, e) in self.0.iter().enumerate() {
            let v = *e as f64 * p;
            let r = v.round();
            if (v - r).abs() > 1e-9 {
                return None;
            }
            d[i] = r as i32;
        }
        Some(Dim(d))
    }

    /// Exponent of base dimension `i` as a reduced fraction.
    pub fn exponent(&self, i: usize) -> (i32, i32) {
        reduce(self.0[i], DIM_SCALE)
    }
}

fn gcd(a: i32, b: i32) -> i32 {
    if b == 0 {
        a.abs()
    } else {
        gcd(b, a % b)
    }
}

pub fn reduce(n: i32, d: i32) -> (i32, i32) {
    let g = gcd(n, d).max(1);
    (n / g, d / g)
}

impl fmt::Debug for Dim {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_none() {
            return write!(f, "1");
        }
        let mut first = true;
        for i in 0..7 {
            let (n, d) = self.exponent(i);
            if n == 0 {
                continue;
            }
            if !first {
                write!(f, "·")?;
            }
            first = false;
            write!(f, "{}", BASE_UNITS[i])?;
            if d == 1 && n != 1 {
                write!(f, "^{n}")?;
            } else if d != 1 {
                write!(f, "^({n}/{d})")?;
            }
        }
        Ok(())
    }
}

pub const LENGTH: Dim = Dim::from_ints([1, 0, 0, 0, 0, 0, 0]);
pub const MASS: Dim = Dim::from_ints([0, 1, 0, 0, 0, 0, 0]);
pub const TIME: Dim = Dim::from_ints([0, 0, 1, 0, 0, 0, 0]);
pub const CURRENT: Dim = Dim::from_ints([0, 0, 0, 1, 0, 0, 0]);
pub const TEMPERATURE: Dim = Dim::from_ints([0, 0, 0, 0, 1, 0, 0]);
pub const AMOUNT: Dim = Dim::from_ints([0, 0, 0, 0, 0, 1, 0]);
pub const LUMINOSITY: Dim = Dim::from_ints([0, 0, 0, 0, 0, 0, 1]);
pub const AREA: Dim = Dim::from_ints([2, 0, 0, 0, 0, 0, 0]);
pub const VOLUME: Dim = Dim::from_ints([3, 0, 0, 0, 0, 0, 0]);
pub const FREQUENCY: Dim = Dim::from_ints([0, 0, -1, 0, 0, 0, 0]);
pub const VELOCITY: Dim = Dim::from_ints([1, 0, -1, 0, 0, 0, 0]);
pub const ACCELERATION: Dim = Dim::from_ints([1, 0, -2, 0, 0, 0, 0]);
pub const FORCE: Dim = Dim::from_ints([1, 1, -2, 0, 0, 0, 0]);
pub const PRESSURE: Dim = Dim::from_ints([-1, 1, -2, 0, 0, 0, 0]);
pub const ENERGY: Dim = Dim::from_ints([2, 1, -2, 0, 0, 0, 0]);
pub const POWER: Dim = Dim::from_ints([2, 1, -3, 0, 0, 0, 0]);
pub const CHARGE: Dim = Dim::from_ints([0, 0, 1, 1, 0, 0, 0]);
pub const VOLTAGE: Dim = Dim::from_ints([2, 1, -3, -1, 0, 0, 0]);
pub const RESISTANCE: Dim = Dim::from_ints([2, 1, -3, -2, 0, 0, 0]);
pub const CONDUCTANCE: Dim = Dim::from_ints([-2, -1, 3, 2, 0, 0, 0]);
pub const CAPACITANCE: Dim = Dim::from_ints([-2, -1, 4, 2, 0, 0, 0]);
pub const INDUCTANCE: Dim = Dim::from_ints([2, 1, -2, -2, 0, 0, 0]);
pub const MAG_FLUX: Dim = Dim::from_ints([2, 1, -2, -1, 0, 0, 0]);
pub const MAG_FLUX_DENSITY: Dim = Dim::from_ints([0, 1, -2, -1, 0, 0, 0]);

/// A named unit: `value_in_SI = number * factor`.
pub struct UnitDef {
    pub name: &'static str,
    pub factor: f64,
    pub dim: Dim,
}

const fn u(name: &'static str, factor: f64, dim: Dim) -> UnitDef {
    UnitDef { name, factor, dim }
}

const PI: f64 = std::f64::consts::PI;
const LBF: f64 = 4.448_221_615_260_5;

pub static UNITS: &[UnitDef] = &[
    // length
    u("m", 1.0, LENGTH),
    u("km", 1e3, LENGTH),
    u("dm", 0.1, LENGTH),
    u("cm", 1e-2, LENGTH),
    u("mm", 1e-3, LENGTH),
    u("μm", 1e-6, LENGTH),
    u("um", 1e-6, LENGTH),
    u("nm", 1e-9, LENGTH),
    u("in", 0.0254, LENGTH),
    u("ft", 0.3048, LENGTH),
    u("yd", 0.9144, LENGTH),
    u("mi", 1609.344, LENGTH),
    // mass (Mathcad convention: `gm` is gram, `g` is gravity)
    u("kg", 1.0, MASS),
    u("gm", 1e-3, MASS),
    u("mg", 1e-6, MASS),
    u("tonne", 1e3, MASS),
    u("lb", 0.453_592_37, MASS),
    // time
    u("s", 1.0, TIME),
    u("ms", 1e-3, TIME),
    u("μs", 1e-6, TIME),
    u("us", 1e-6, TIME),
    u("ns", 1e-9, TIME),
    u("min", 60.0, TIME),
    u("hr", 3600.0, TIME),
    u("day", 86400.0, TIME),
    u("week", 604800.0, TIME),
    u("yr", 31_557_600.0, TIME),
    // electric current, temperature, amount, luminosity
    u("A", 1.0, CURRENT),
    u("mA", 1e-3, CURRENT),
    u("kA", 1e3, CURRENT),
    u("K", 1.0, TEMPERATURE),
    u("Δ°C", 1.0, TEMPERATURE),
    u("mol", 1.0, AMOUNT),
    u("cd", 1.0, LUMINOSITY),
    // area and volume
    u("ha", 1e4, AREA),
    u("L", 1e-3, VOLUME),
    u("mL", 1e-6, VOLUME),
    u("liter", 1e-3, VOLUME),
    u("gal", 3.785_411_784e-3, VOLUME),
    // frequency
    u("Hz", 1.0, FREQUENCY),
    u("kHz", 1e3, FREQUENCY),
    u("MHz", 1e6, FREQUENCY),
    u("GHz", 1e9, FREQUENCY),
    u("rpm", 1.0 / 60.0, FREQUENCY),
    // speed
    u("kph", 1.0 / 3.6, VELOCITY),
    u("mph", 0.447_04, VELOCITY),
    // force
    u("N", 1.0, FORCE),
    u("kN", 1e3, FORCE),
    u("MN", 1e6, FORCE),
    u("dyne", 1e-5, FORCE),
    u("kgf", 9.806_65, FORCE),
    u("tonnef", 9.806_65e3, FORCE),
    u("lbf", LBF, FORCE),
    u("kip", LBF * 1e3, FORCE),
    // pressure
    u("Pa", 1.0, PRESSURE),
    u("kPa", 1e3, PRESSURE),
    u("MPa", 1e6, PRESSURE),
    u("GPa", 1e9, PRESSURE),
    u("bar", 1e5, PRESSURE),
    u("atm", 101_325.0, PRESSURE),
    u("psi", LBF / 0.0254 / 0.0254, PRESSURE),
    u("ksi", LBF / 0.0254 / 0.0254 * 1e3, PRESSURE),
    u("mmHg", 133.322_387_415, PRESSURE),
    u("torr", 101_325.0 / 760.0, PRESSURE),
    // energy
    u("J", 1.0, ENERGY),
    u("kJ", 1e3, ENERGY),
    u("MJ", 1e6, ENERGY),
    u("cal", 4.1868, ENERGY),
    u("kcal", 4186.8, ENERGY),
    u("Wh", 3600.0, ENERGY),
    u("kWh", 3.6e6, ENERGY),
    u("eV", 1.602_176_634e-19, ENERGY),
    u("BTU", 1055.055_852_62, ENERGY),
    // power
    u("W", 1.0, POWER),
    u("mW", 1e-3, POWER),
    u("kW", 1e3, POWER),
    u("MW", 1e6, POWER),
    u("hp", 745.699_871_582_270_2, POWER),
    // electromagnetism
    u("C", 1.0, CHARGE),
    u("V", 1.0, VOLTAGE),
    u("mV", 1e-3, VOLTAGE),
    u("kV", 1e3, VOLTAGE),
    u("Ω", 1.0, RESISTANCE),
    u("ohm", 1.0, RESISTANCE),
    u("kΩ", 1e3, RESISTANCE),
    u("MΩ", 1e6, RESISTANCE),
    u("S", 1.0, CONDUCTANCE),
    u("F", 1.0, CAPACITANCE),
    u("mF", 1e-3, CAPACITANCE),
    u("μF", 1e-6, CAPACITANCE),
    u("nF", 1e-9, CAPACITANCE),
    u("pF", 1e-12, CAPACITANCE),
    u("H", 1.0, INDUCTANCE),
    u("mH", 1e-3, INDUCTANCE),
    u("Wb", 1.0, MAG_FLUX),
    u("T", 1.0, MAG_FLUX_DENSITY),
    // angles (dimensionless)
    u("rad", 1.0, Dim::NONE),
    u("deg", PI / 180.0, Dim::NONE),
    u("°", PI / 180.0, Dim::NONE),
    u("%", 0.01, Dim::NONE),
];

pub fn lookup(name: &str) -> Option<&'static UnitDef> {
    UNITS.iter().find(|u| u.name == name)
}

/// Units that are preferred when showing a result in "automatic" units.
pub static DERIVED_DISPLAY: &[(&str, Dim)] = &[
    ("N", FORCE),
    ("Pa", PRESSURE),
    ("J", ENERGY),
    ("W", POWER),
    ("C", CHARGE),
    ("V", VOLTAGE),
    ("Ω", RESISTANCE),
    ("S", CONDUCTANCE),
    ("F", CAPACITANCE),
    ("H", INDUCTANCE),
    ("Wb", MAG_FLUX),
    ("T", MAG_FLUX_DENSITY),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dim_arith() {
        assert_eq!(LENGTH.div(TIME).div(TIME).mul(MASS), FORCE);
        assert_eq!(AREA.powf(0.5), Some(LENGTH));
        assert_eq!(LENGTH.powf(0.5).unwrap().powf(2.0), Some(LENGTH));
        assert_eq!(LENGTH.powf(0.3), None);
        assert_eq!(format!("{:?}", FORCE), "m·kg·s^-2");
    }
}
