//! Mathcad 15 unit-system parity: unit factors, temperature, automatic units.

use xmath_core::eval::Evaluator;
use xmath_core::format::{auto_units, FormatOptions, UnitSystem};
use xmath_core::units::{self, Dim};
use xmath_core::value::Value;
use xmath_core::{parse_expr, parse_statement, Output, Region, RegionKind, Stmt, Worksheet};

fn run(lines: &[&str]) -> Result<Value, String> {
    let mut ev = Evaluator::new();
    let (last, defs) = lines.split_last().unwrap();
    for l in defs {
        match parse_statement(l).map_err(|e| e.msg)? {
            Stmt::Define { target, expr, .. } => ev.define(&target, &expr)?,
            _ => panic!("not a definition: {l}"),
        }
    }
    let e = parse_expr(last).map_err(|e| e.msg)?;
    let sc = ev.top_scope();
    ev.eval(&e, &sc)
}

fn si(lines: &[&str]) -> f64 {
    match run(lines).unwrap() {
        Value::Num(n) => n.v.re,
        o => panic!("{o:?}"),
    }
}

fn close(a: f64, b: f64) {
    assert!((a - b).abs() <= 1e-9 * b.abs().max(1e-300), "{a} != {b}");
}

/// Evaluates the worksheet and returns the display of every region as text.
fn show_with(sys: UnitSystem, lines: &[&str]) -> Vec<String> {
    let mut ws = Worksheet::default();
    ws.format = FormatOptions { unit_system: sys, ..FormatOptions::default() };
    for (i, l) in lines.iter().enumerate() {
        ws.regions.push(Region::new(i as u64, 0.0, i as f32 * 10.0, RegionKind::Math { source: l.to_string() }));
    }
    let out = ws.evaluate();
    (0..lines.len())
        .map(|i| match &out[&(i as u64)].output {
            Output::Value { view, .. } => view.to_string(),
            Output::Error(e) => format!("ERR {e}"),
            _ => String::new(),
        })
        .collect()
}

fn show(lines: &[&str]) -> Vec<String> {
    show_with(UnitSystem::Si, lines)
}

fn last(lines: &[&str]) -> String {
    show(lines).pop().unwrap()
}

#[test]
fn nist_factors() {
    // (unit, SI value) from NIST SP 811 / CODATA 2018
    let table: &[(&str, f64)] = &[
        ("in", 0.0254), ("ft", 0.3048), ("yd", 0.9144), ("mi", 1609.344), ("nmi", 1852.0),
        ("mil", 2.54e-5), ("Å", 1e-10), ("au", 1.495_978_707e11), ("ly", 9.460_730_472_580_8e15),
        ("pc", 3.085_677_581_491_367e16), ("lb", 0.453_592_37), ("oz", 0.028_349_523_125),
        ("ton", 907.184_74), ("slug", 14.593_902_937), ("amu", 1.660_539_066_6e-27),
        ("acre", 4046.856_422_4), ("ha", 1e4), ("gal", 3.785_411_784e-3), ("qt", 9.463_529_46e-4),
        ("pt", 4.731_764_73e-4), ("fl_oz", 2.957_352_956_25e-5), ("bbl", 0.158_987_294_928),
        ("L", 1e-3), ("kgf", 9.806_65), ("lbf", 4.448_221_615_260_5), ("kip", 4448.221_615_260_5),
        ("ozf", 0.278_013_850_953_8), ("tonf", 8896.443_230_521), ("dyne", 1e-5),
        ("atm", 101_325.0), ("at", 98_066.5), ("bar", 1e5), ("psi", 6894.757_293_168),
        ("torr", 133.322_368_421), ("mmHg", 133.322_387_415), ("inHg", 3386.388_640_341),
        ("mmH2O", 9.806_65), ("inH2O", 249.088_91), ("cal", 4.1868), ("BTU", 1055.055_852_62),
        ("erg", 1e-7), ("eV", 1.602_176_634e-19), ("kWh", 3.6e6), ("hp", 745.699_871_582),
        ("mph", 0.447_04), ("kph", 1.0 / 3.6), ("knot", 0.514_444_444_444), ("gauss", 1e-4),
        ("Ci", 3.7e10), ("P", 0.1), ("cSt", 1e-6), ("rev", std::f64::consts::TAU),
        ("arcmin", 2.908_882_086_657_216e-4), ("arcsec", 4.848_136_811_095_36e-6),
        ("yr", 31_557_600.0), ("hr", 3600.0), ("day", 86_400.0), ("wk", 604_800.0),
    ];
    for (name, f) in table {
        let v = si(&[name]);
        assert!((v - f).abs() <= 2e-9 * f.abs(), "{name}: {v} vs {f}");
    }
}

#[test]
fn no_duplicate_unit_names() {
    let mut names: Vec<_> = units::UNITS.iter().map(|u| u.name).collect();
    let n = names.len();
    names.sort();
    names.dedup();
    assert_eq!(n, names.len());
}

#[test]
fn units_of_dimensions() {
    assert_eq!(units::lookup("rpm").unwrap().dim, units::FREQUENCY);
    assert_eq!(units::lookup("lx").unwrap().dim, units::ILLUMINANCE);
    assert_eq!(units::lookup("Gy").unwrap().dim, units::DOSE);
    assert_eq!(units::lookup("gm").unwrap().factor, 1e-3);
    // `g` is gravity, not gram
    close(si(&["g"]), 9.80665);
}

#[test]
fn temperature_offsets() {
    close(si(&["T := 25 °C", "T"]), 298.15);
    close(si(&["T := 25 degC", "T"]), 298.15);
    close(si(&["T := 77 °F", "T"]), 298.15);
    close(si(&["T := 0 °C", "T"]), 273.15);
    close(si(&["T := -40 °F", "T"]), 233.15);
    close(si(&["T := 536.67 R", "T"]), 298.15);
    // arithmetic around the offset literal
    close(si(&["25 °C + 10 Δ°C"]), 308.15);
    close(si(&["T := 25 °C", "ΔT := 10 Δ°C", "T + ΔT"]), 308.15);
    close(si(&["10 Δ°F"]), 10.0 * 5.0 / 9.0);
    // implicit multiplication by a scalar variable
    close(si(&["x := 30", "x °C"]), 303.15);
    // elsewhere °C is just K
    close(si(&["5 °C/s"]), 5.0);
    close(si(&["2 °C^2"]), 2.0);
    close(si(&["°C"]), 1.0);
    // user definitions shadow the unit
    close(si(&["°C := 7", "3 °C"]), 21.0);
}

#[test]
fn temperature_display() {
    assert_eq!(show(&["T := 25 °C", "T = K"])[1], "298.15");
    assert_eq!(show(&["T := 25 °C", "T = °C"])[1], "25");
    assert_eq!(show(&["T := 25 °C", "T = °F"])[1], "77");
    assert_eq!(show(&["T := 25 °C", "T = degC"])[1], "25");
    assert_eq!(show(&["T := 25 °C", "T = R"])[1], "536.67");
    assert_eq!(show(&["T := 25 °C", "T ="])[1], "298.15 K");
    assert_eq!(show(&["ΔT := 10 Δ°C", "ΔT = Δ°C"])[1], "10");
    assert_eq!(show(&["ΔT := 10 Δ°C", "ΔT = Δ°F"])[1], "18");
    assert_eq!(show(&["T := 100 °C", "T = °F"])[1], "212");
    // non-temperature values ignore the offset
    assert_eq!(show(&["5 m = °C"])[0], "5 m/K");
}

#[test]
fn display_units() {
    assert_eq!(last(&["M := 12 kN*m", "M = kN*m"]), "12");
    assert_eq!(last(&["s := 250 MPa", "s = MPa"]), "250");
    assert_eq!(last(&["v := 72 km/hr", "v ="]), "20 m/s");
    assert_eq!(last(&["v := 72 km/hr", "v = km/hr"]), "72");
    assert_eq!(last(&["r := 7850 kg/m^3", "r = kg/m^3"]), "7.85·10^3");
    assert_eq!(last(&["p := 2 kgf/cm^2", "p = kgf/cm^2"]), "2");
    assert_eq!(last(&["p := 2 kgf/cm^2", "p = Pa"]), "1.961·10^5");
    assert_eq!(last(&["F := 3000 N", "F = kN"]), "3");
    // different dimension: remaining units are appended (Mathcad: 5 m/s^2 · kg)
    assert_eq!(last(&["5 N = kg"]), "5 m/s^2");
    assert_eq!(last(&["5 N = m"]), "5 N/m");
}

#[test]
fn automatic_units_table() {
    let d = |e: [i32; 7]| Dim::from_ints(e);
    let t = |dim: Dim| auto_units(dim, UnitSystem::Si).0.to_string();
    assert_eq!(t(units::FORCE), "N");
    assert_eq!(t(units::PRESSURE), "Pa");
    assert_eq!(t(units::ENERGY), "J");
    assert_eq!(t(units::POWER), "W");
    assert_eq!(t(units::VOLTAGE), "V");
    assert_eq!(t(units::RESISTANCE), "Ω");
    assert_eq!(t(units::FREQUENCY), "1/s");
    assert_eq!(t(units::VELOCITY), "m/s");
    assert_eq!(t(units::KIN_VISCOSITY), "m^2/s");
    assert_eq!(t(units::DYN_VISCOSITY), "Pa·s");
    assert_eq!(t(d([-3, 1, 0, 0, 0, 0, 0])), "kg/m^3");
    assert_eq!(t(d([1, 1, -3, 0, -1, 0, 0])), "W/(m·K)");
    assert_eq!(t(d([0, 1, -2, 0, 0, 0, 0])), "N/m");
    assert_eq!(t(d([1, 1, -3, -1, 0, 0, 0])), "V/m");
    assert_eq!(t(d([2, 1, 0, 0, 0, 0, 0])), "kg·m^2");
    assert_eq!(t(d([3, 1, -3, -2, 0, 0, 0])), "Ω·m");
    assert_eq!(t(d([0, 1, -3, 0, 0, 0, 0])), "W/m^2");
}

#[test]
fn unit_systems() {
    let out = show_with(UnitSystem::Cgs, &["F := 1 N", "F =", "p := 1 Pa", "p =", "L := 1 m", "L =", "m := 1 kg", "m =", "E := 1 J", "E ="]);
    assert_eq!(out[1], "1·10^5 dyne");
    assert_eq!(out[3], "10 dyne/cm^2");
    assert_eq!(out[5], "100 cm");
    assert_eq!(out[7], "1·10^3 gm");
    assert_eq!(out[9], "1·10^7 erg");
    let us = show_with(UnitSystem::Us, &["F := 1 lbf", "F =", "L := 12 in", "L =", "p := 1 psi", "p =", "m := 1 lb", "m ="]);
    assert_eq!(us[1], "1 lbf");
    assert_eq!(us[3], "1 ft");
    assert_eq!(us[5], "1 psi");
    assert_eq!(us[7], "1 lb");
    let mks = show_with(UnitSystem::Mks, &["1 N ="]);
    assert_eq!(mks[0], "1 N");
    // temperature in US system is Rankine
    assert_eq!(show_with(UnitSystem::Us, &["T := 0 °F", "T ="])[1], "459.67 R");
    // computation is unchanged: the display unit still divides correctly
    assert_eq!(show_with(UnitSystem::Cgs, &["F := 1 N", "F = N"])[1], "1");
}

#[test]
fn format_options_serde_compat() {
    let old = r#"{"precision":5,"exp_threshold":3,"zero_threshold":15}"#;
    let o: FormatOptions = serde_json::from_str(old).unwrap();
    assert_eq!(o.precision, 5);
    assert_eq!(o.unit_system, UnitSystem::Si);
    let empty: FormatOptions = serde_json::from_str("{}").unwrap();
    assert_eq!(empty, FormatOptions::default());
    let cgs = FormatOptions { unit_system: UnitSystem::Cgs, ..FormatOptions::default() };
    let s = serde_json::to_string(&cgs).unwrap();
    assert!(s.contains("\"cgs\""));
    assert_eq!(serde_json::from_str::<FormatOptions>(&s).unwrap(), cgs);
}
