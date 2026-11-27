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
pub const ILLUMINANCE: Dim = Dim::from_ints([-2, 0, 0, 0, 0, 0, 1]);
pub const DOSE: Dim = Dim::from_ints([2, 0, -2, 0, 0, 0, 0]);
pub const DYN_VISCOSITY: Dim = Dim::from_ints([-1, 1, -1, 0, 0, 0, 0]);
pub const KIN_VISCOSITY: Dim = Dim::from_ints([2, 0, -1, 0, 0, 0, 0]);

/// A named unit: `value_in_SI = (number + offset) * factor`.
///
/// # Temperature (Mathcad 15 semantics)
///
/// `K` and `R` (Rankine) are ordinary absolute units. `°C`/`degC` and
/// `°F`/`degF` are *offset* units (`offset != 0`); `Δ°C`/`ΔdegC`/`Δ°F`/`ΔdegF`
/// are plain difference units with `offset == 0`.
///
/// As in Mathcad, an offset unit is meaningful only (a) as the last factor of
/// a value, `T := 25 °C` (the evaluator turns `x °C` into `(x + 273.15) K`),
/// and (b) as a display unit, `T = °C` (shows `(T - 273.15 K) / K`). In every
/// other position (`°C^2`, `5 °C/s`, `a * °C` with a non-scalar `a`, a bare
/// `°C` value) the name behaves like `K` / `Δ°C`, i.e. `factor` only.
pub struct UnitDef {
    pub name: &'static str,
    pub factor: f64,
    pub dim: Dim,
    /// Zero point offset in the unit's own scale (273.15 for °C, 459.67 for °F).
    pub offset: f64,
}

impl UnitDef {
    /// Offset of the zero point expressed in SI kelvin (0 for non-offset units).
    pub fn offset_si(&self) -> f64 {
        self.offset * self.factor
    }
}

const fn u(name: &'static str, factor: f64, dim: Dim) -> UnitDef {
    UnitDef { name, factor, dim, offset: 0.0 }
}

const fn uo(name: &'static str, factor: f64, dim: Dim, offset: f64) -> UnitDef {
    UnitDef { name, factor, dim, offset }
}

const PI: f64 = std::f64::consts::PI;
/// Pound-mass in kg (exact).
pub const LB: f64 = 0.453_592_37;
/// Standard gravity, m/s^2 (exact).
pub const G0: f64 = 9.806_65;
/// Pound-force in N (exact: lb * g0).
pub const LBF: f64 = 4.448_221_615_260_5;
/// Pound per square inch in Pa.
pub const PSI: f64 = 6_894.757_293_168_361;

pub static UNITS: &[UnitDef] = &[
    // length
    u("m", 1.0, LENGTH),
    u("km", 1e3, LENGTH),
    u("dm", 1e-1, LENGTH),
    u("cm", 1e-2, LENGTH),
    u("mm", 1e-3, LENGTH),
    u("μm", 1e-6, LENGTH),
    u("nm", 1e-9, LENGTH),
    u("pm", 1e-12, LENGTH),
    u("um", 1e-6, LENGTH),
    u("fm", 1e-15, LENGTH),
    u("Å", 1e-10, LENGTH),
    u("in", 0.0254, LENGTH),
    u("mil", 2.54e-5, LENGTH),
    u("ft", 0.3048, LENGTH),
    u("yd", 0.9144, LENGTH),
    u("mi", 1609.344, LENGTH),
    u("nmi", 1852.0, LENGTH),
    u("au", 1.495_978_707e11, LENGTH),
    u("ly", 9.460_730_472_580_8e15, LENGTH),
    u("pc", 3.085_677_581_491_367e16, LENGTH),
    // mass (Mathcad: `gm` is the gram, `g` is gravity)
    u("kg", 1.0, MASS),
    u("gm", 1e-3, MASS),
    u("mg", 1e-6, MASS),
    u("μg", 1e-9, MASS),
    u("tonne", 1e3, MASS),
    u("mton", 1e3, MASS),
    u("lb", LB, MASS),
    u("lbm", LB, MASS),
    u("oz", LB / 16.0, MASS),
    u("ton", LB * 2000.0, MASS),
    u("slug", 14.593_902_937_2, MASS),
    u("amu", 1.660_539_066_60e-27, MASS),
    // time
    u("s", 1.0, TIME),
    u("sec", 1.0, TIME),
    u("ms", 1e-3, TIME),
    u("μs", 1e-6, TIME),
    u("ns", 1e-9, TIME),
    u("ps", 1e-12, TIME),
    u("us", 1e-6, TIME),
    u("min", 60.0, TIME),
    u("hr", 3600.0, TIME),
    u("day", 86400.0, TIME),
    u("wk", 604800.0, TIME),
    u("week", 604800.0, TIME),
    u("yr", 31_557_600.0, TIME),
    // electric current, amount, luminosity
    u("A", 1.0, CURRENT),
    u("mA", 1e-3, CURRENT),
    u("μA", 1e-6, CURRENT),
    u("kA", 1e3, CURRENT),
    u("mol", 1.0, AMOUNT),
    u("mmol", 1e-3, AMOUNT),
    u("kmol", 1e3, AMOUNT),
    u("cd", 1.0, LUMINOSITY),
    u("lm", 1.0, LUMINOSITY),
    u("lx", 1.0, ILLUMINANCE),
    // temperature: K, Rankine, offset scales °C / °F and difference units (see module docs)
    u("K", 1.0, TEMPERATURE),
    u("R", 5.0 / 9.0, TEMPERATURE),
    uo("°C", 1.0, TEMPERATURE, 273.15),
    uo("degC", 1.0, TEMPERATURE, 273.15),
    uo("°F", 5.0 / 9.0, TEMPERATURE, 459.67),
    uo("degF", 5.0 / 9.0, TEMPERATURE, 459.67),
    u("Δ°C", 1.0, TEMPERATURE),
    u("ΔdegC", 1.0, TEMPERATURE),
    u("Δ°F", 5.0 / 9.0, TEMPERATURE),
    u("ΔdegF", 5.0 / 9.0, TEMPERATURE),
    // area and volume
    u("ha", 1e4, AREA),
    u("acre", 4_046.856_422_4, AREA),
    u("L", 1e-3, VOLUME),
    u("liter", 1e-3, VOLUME),
    u("mL", 1e-6, VOLUME),
    u("cL", 1e-5, VOLUME),
    u("dL", 1e-4, VOLUME),
    u("hL", 0.1, VOLUME),
    u("gal", 3.785_411_784e-3, VOLUME),
    u("qt", 9.463_529_46e-4, VOLUME),
    u("pt", 4.731_764_73e-4, VOLUME),
    u("fl_oz", 2.957_352_956_25e-5, VOLUME),
    u("floz", 2.957_352_956_25e-5, VOLUME),
    u("bbl", 0.158_987_294_928, VOLUME),
    // frequency
    u("Hz", 1.0, FREQUENCY),
    u("kHz", 1e3, FREQUENCY),
    u("MHz", 1e6, FREQUENCY),
    u("GHz", 1e9, FREQUENCY),
    u("THz", 1e12, FREQUENCY),
    u("rpm", 1.0 / 60.0, FREQUENCY),
    // velocity
    u("kph", 1.0 / 3.6, VELOCITY),
    u("mph", 0.447_04, VELOCITY),
    u("knot", 1852.0 / 3600.0, VELOCITY),
    // force
    u("N", 1.0, FORCE),
    u("kN", 1e3, FORCE),
    u("MN", 1e6, FORCE),
    u("GN", 1e9, FORCE),
    u("dyne", 1e-5, FORCE),
    u("kgf", 9.806_65, FORCE),
    u("tonnef", 9.806_65e3, FORCE),
    u("lbf", LBF, FORCE),
    u("kip", LBF * 1e3, FORCE),
    u("ozf", LBF / 16.0, FORCE),
    u("tonf", LBF * 2000.0, FORCE),
    // pressure
    u("Pa", 1.0, PRESSURE),
    u("kPa", 1e3, PRESSURE),
    u("MPa", 1e6, PRESSURE),
    u("GPa", 1e9, PRESSURE),
    u("hPa", 1e2, PRESSURE),
    u("bar", 1e5, PRESSURE),
    u("mbar", 1e2, PRESSURE),
    u("atm", 101_325.0, PRESSURE),
    u("at", 98_066.5, PRESSURE),
    u("psi", PSI, PRESSURE),
    u("ksi", PSI * 1e3, PRESSURE),
    u("torr", 101_325.0 / 760.0, PRESSURE),
    u("mmHg", 133.322_387_415, PRESSURE),
    u("inHg", 3_386.388_640_341, PRESSURE),
    u("mmH2O", 9.806_65, PRESSURE),
    u("inH2O", 249.088_91, PRESSURE),
    // energy
    u("J", 1.0, ENERGY),
    u("mJ", 1e-3, ENERGY),
    u("kJ", 1e3, ENERGY),
    u("MJ", 1e6, ENERGY),
    u("GJ", 1e9, ENERGY),
    u("erg", 1e-7, ENERGY),
    u("cal", 4.1868, ENERGY),
    u("kcal", 4186.8, ENERGY),
    u("Wh", 3600.0, ENERGY),
    u("kWh", 3.6e6, ENERGY),
    u("MWh", 3.6e9, ENERGY),
    u("eV", 1.602_176_634e-19, ENERGY),
    u("keV", 1.602_176_634e-16, ENERGY),
    u("MeV", 1.602_176_634e-13, ENERGY),
    u("BTU", 1055.055_852_62, ENERGY),
    // power
    u("W", 1.0, POWER),
    u("mW", 1e-3, POWER),
    u("kW", 1e3, POWER),
    u("MW", 1e6, POWER),
    u("GW", 1e9, POWER),
    u("hp", 745.699_871_582_270_2, POWER),
    // electromagnetism
    u("C", 1.0, CHARGE),
    u("mC", 1e-3, CHARGE),
    u("μC", 1e-6, CHARGE),
    u("V", 1.0, VOLTAGE),
    u("mV", 1e-3, VOLTAGE),
    u("μV", 1e-6, VOLTAGE),
    u("kV", 1e3, VOLTAGE),
    u("Ω", 1.0, RESISTANCE),
    u("ohm", 1.0, RESISTANCE),
    u("mΩ", 1e-3, RESISTANCE),
    u("kΩ", 1e3, RESISTANCE),
    u("MΩ", 1e6, RESISTANCE),
    u("S", 1.0, CONDUCTANCE),
    u("mS", 1e-3, CONDUCTANCE),
    u("μS", 1e-6, CONDUCTANCE),
    u("F", 1.0, CAPACITANCE),
    u("mF", 1e-3, CAPACITANCE),
    u("μF", 1e-6, CAPACITANCE),
    u("nF", 1e-9, CAPACITANCE),
    u("pF", 1e-12, CAPACITANCE),
    u("H", 1.0, INDUCTANCE),
    u("mH", 1e-3, INDUCTANCE),
    u("μH", 1e-6, INDUCTANCE),
    u("nH", 1e-9, INDUCTANCE),
    u("Wb", 1.0, MAG_FLUX),
    u("mWb", 1e-3, MAG_FLUX),
    u("T", 1.0, MAG_FLUX_DENSITY),
    u("mT", 1e-3, MAG_FLUX_DENSITY),
    u("μT", 1e-6, MAG_FLUX_DENSITY),
    u("gauss", 1e-4, MAG_FLUX_DENSITY),
    // radioactivity and dose
    u("Bq", 1.0, FREQUENCY),
    u("Ci", 3.7e10, FREQUENCY),
    u("Gy", 1.0, DOSE),
    u("Sv", 1.0, DOSE),
    // viscosity
    u("P", 0.1, DYN_VISCOSITY),
    u("cP", 1e-3, DYN_VISCOSITY),
    u("St", 1e-4, KIN_VISCOSITY),
    u("cSt", 1e-6, KIN_VISCOSITY),
    // Mathcad accepts the full SI names as well as the symbols
    u("volt", 1.0, VOLTAGE),
    u("ampere", 1.0, CURRENT),
    u("amp", 1.0, CURRENT),
    u("coulomb", 1.0, CHARGE),
    u("farad", 1.0, CAPACITANCE),
    u("henry", 1.0, INDUCTANCE),
    u("siemens", 1.0, CONDUCTANCE),
    u("weber", 1.0, MAG_FLUX),
    u("tesla", 1.0, MAG_FLUX_DENSITY),
    u("newton", 1.0, FORCE),
    u("pascal", 1.0, PRESSURE),
    u("joule", 1.0, ENERGY),
    u("watt", 1.0, POWER),
    u("hertz", 1.0, FREQUENCY),
    u("lumen", 1.0, LUMINOSITY),
    u("lux", 1.0, ILLUMINANCE),
    u("becquerel", 1.0, FREQUENCY),
    u("gray", 1.0, DOSE),
    u("sievert", 1.0, DOSE),
    u("kelvin", 1.0, TEMPERATURE),
    u("mole", 1.0, AMOUNT),
    u("candela", 1.0, LUMINOSITY),
    u("meter", 1.0, LENGTH),
    u("metre", 1.0, LENGTH),
    u("second", 1.0, TIME),
    u("minute", 60.0, TIME),
    u("hour", 3600.0, TIME),
    u("gram", 1e-3, MASS),
    u("litre", 1e-3, VOLUME),
    u("radian", 1.0, Dim::NONE),
    u("steradian", 1.0, Dim::NONE),
    u("degree", PI / 180.0, Dim::NONE),
    // angles (dimensionless)
    u("rad", 1.0, Dim::NONE),
    u("mrad", 1e-3, Dim::NONE),
    u("deg", PI / 180.0, Dim::NONE),
    u("°", PI / 180.0, Dim::NONE),
    u("rev", 2.0 * PI, Dim::NONE),
    u("arcmin", PI / 10800.0, Dim::NONE),
    u("arcsec", PI / 648000.0, Dim::NONE),
    u("%", 0.01, Dim::NONE),
];

pub fn lookup(name: &str) -> Option<&'static UnitDef> {
    UNITS.iter().find(|u| u.name == name)
}

/// The offset temperature unit (`°C`, `°F`, ...) called `name`, if it is one.
pub fn offset_unit(name: &str) -> Option<&'static UnitDef> {
    lookup(name).filter(|u| u.offset != 0.0)
}

/// Units that are preferred when showing a result in "automatic" units
/// (SI system). `Hz`/`Bq` are deliberately absent: like Mathcad, a frequency
/// is shown as `1/s`.
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
