use super::eval_text;
use crate::worksheet::{Output, Worksheet};

fn s(src: &str) -> String {
    eval_text(src).unwrap_or_else(|e| format!("ERR: {e}"))
}

fn eq(src: &str, expected: &str) {
    assert_eq!(s(src), expected, "{src}");
}

/// Numeric check: the result evaluated for the given values of the variables.
fn num(src: &str, vars: &[(&str, f64)]) -> f64 {
    use crate::eval::Evaluator;
    use crate::parser::parse_expr;
    let text = s(src);
    let e = parse_expr(&text).unwrap_or_else(|er| panic!("{text}: {}", er.msg));
    let mut ev = Evaluator::new();
    for (n, v) in vars {
        ev.define(&crate::ast::Target::Var(n.to_string()), &crate::ast::Expr::Num(*v)).unwrap();
    }
    let sc = ev.top_scope();
    ev.eval(&e, &sc).unwrap_or_else(|er| panic!("{text}: {er}")).as_num().unwrap().v.re
}

#[test]
fn syntax_round_trips() {
    for src in [
        "x^2 + 1 →",
        "x^2 + 1 → simplify",
        "(x + 1)^2 → expand → factor",
        "x^2 - 4 == 0 → solve, x",
        "[x + y == 3; x - y == 1] → solve, [x; y]",
        "x + y → substitute, x == 2",
        "pi → float, 5",
        "g := x^2 → expand",
        "g ≡ x^2 →",
        "sin(x) → series, x, 6",
        "a → assume, a > 0",
    ] {
        let st = crate::parser::parse_statement(src).unwrap_or_else(|e| panic!("{src}: {}", e.msg));
        let printed = st.to_string();
        assert_eq!(printed, src, "{src}");
        assert_eq!(crate::parser::parse_statement(&printed).unwrap(), st);
    }
    // ASCII arrows and `=` equations
    let a = crate::parser::parse_statement("x^2 = 4 -> solve, x").unwrap();
    assert_eq!(a.to_string(), "x^2 == 4 → solve, x");
    let b = crate::parser::parse_statement("x + y -> substitute, x = 2").unwrap();
    assert_eq!(b.to_string(), "x + y → substitute, x == 2");
    assert!(matches!(crate::parser::parse_statement("x^2 ->").unwrap(), crate::Stmt::Symbolic { ops, .. } if ops.is_empty()));
    // an ordinary evaluation with a unit is not an equation
    assert!(matches!(crate::parser::parse_statement("F = kN").unwrap(), crate::Stmt::Eval { .. }));
    assert!(crate::parser::parse_statement("x -> 5").is_err());
}

#[test]
fn rational_arithmetic() {
    eq("1/3 + 1/6 ->", "1/2");
    eq("2^100 ->", "1267650600228229401496703205376");
    eq("(1/2)^10 ->", "1/1024");
    eq("3/4*8 ->", "6");
    eq("-1/2 - 1/3 ->", "-5/6");
    eq("100! / 98! ->", "9900");
    eq("0.1 + 0.2 ->", "0.3");
    eq("0.5*x + 0.25*x ->", "0.75*x");
    eq("x/x ->", "1");
    eq("0*x ->", "0");
}

#[test]
fn radicals_and_constants() {
    eq("sqrt(8) ->", "2*sqrt(2)");
    eq("sqrt(12)*sqrt(3) ->", "6");
    eq("sqrt(2)*sqrt(3) ->", "sqrt(6)");
    eq("sqrt(1/2) ->", "sqrt(2)/2");
    eq("2^(-1/2) ->", "sqrt(2)/2");
    eq("8^(1/3) ->", "2");
    eq("sqrt(-4) ->", "2i");
    eq("pi*2 ->", "2*π");
    eq("exp(1) ->", "e");
    eq("exp(0) ->", "1");
    eq("ln(1) ->", "0");
    eq("exp(ln(x)) ->", "x");
}

#[test]
fn complex_numbers() {
    eq("(1+i)*(1-i) ->", "2");
    eq("(1+2i)*(3-4i) ->", "11 + 2i");
    eq("(3+4i)/(1-2i) ->", "-1 + 2i");
    eq("(1+2*1i)^2 ->", "-3 + 4i");
    eq("1i^2 ->", "-1");
    eq("exp(i*pi) ->", "-1");
    eq("sqrt(i) ->", "sqrt(2)/2 + sqrt(2)*1i/2");
}

#[test]
fn trig_values() {
    eq("sin(pi/6) ->", "1/2");
    eq("cos(pi/3) ->", "1/2");
    eq("sin(pi/4) ->", "sqrt(2)/2");
    eq("tan(pi/4) ->", "1");
    eq("cos(pi) ->", "-1");
    eq("sin(-x) ->", "-sin(x)");
    eq("cos(-x) ->", "cos(x)");
    eq("atan(1) ->", "π/4");
    eq("asin(1/2) ->", "π/6");
}

#[test]
fn simplify_cases() {
    eq("x^2 + 2*x + 1 ->", "x^2 + 2*x + 1");
    eq("(x^2-1)/(x-1) -> simplify", "x + 1");
    eq("(x^2 - 4)/(x + 2) -> simplify", "x - 2");
    eq("(a*b + a*c)/a -> simplify", "b + c");
    eq("(x^3 - 8)/(x - 2) -> simplify", "x^2 + 2*x + 4");
    eq("sin(x)^2 + cos(x)^2 -> simplify", "1");
    eq("1 - sin(x)^2 -> simplify", "cos(x)^2");
    eq("sin(x)/cos(x) -> simplify", "tan(x)");
    eq("exp(a)*exp(b) ->", "exp(a + b)");
    eq("x*x*x ->", "x^3");
    eq("x + x ->", "2*x");
    eq("x^2*x^3 ->", "x^5");
    eq("x^a*x^b ->", "x^(a + b)");
    eq("sqrt(x^2) -> assume, x > 0", "x");
}

#[test]
fn expand_cases() {
    eq("(x+1)^2 -> expand", "x^2 + 2*x + 1");
    eq("(x+y)^3 -> expand", "x^3 + 3*x^2*y + 3*x*y^2 + y^3");
    eq("(x-1)*(x+1) -> expand", "x^2 - 1");
    eq("(a+b)*(c+d) -> expand", "a*c + a*d + b*c + b*d");
    eq("2*(x+1)^2 - 2*x^2 -> expand", "4*x + 2");
    eq("sin(2*x) -> expand", "2*cos(x)*sin(x)");
    eq("cos(x+y) -> expand", "cos(x)*cos(y) - sin(x)*sin(y)");
    eq("exp(a+b) -> expand", "exp(a)*exp(b)");
    eq("(x+1)^2/(x-1) -> expand", "x^2/(x - 1) + 2*x/(x - 1) + 1/(x - 1)");
}

#[test]
fn factor_cases() {
    eq("x^2 - 1 -> factor", "(x - 1)*(x + 1)");
    eq("x^4 - 1 -> factor", "(x - 1)*(x + 1)*(x^2 + 1)");
    eq("x^2 + x + 1 -> factor", "x^2 + x + 1");
    eq("x^2 + 2*x + 1 -> factor", "(x + 1)^2");
    eq("2*x^2 + 4*x -> factor", "2*x*(x + 2)");
    eq("6*x^2 + 5*x + 1 -> factor", "(2*x + 1)*(3*x + 1)");
    eq("x^6 - 1 -> factor", "(x - 1)*(x + 1)*(x^2 + x + 1)*(x^2 - x + 1)");
    eq("x^4 + 4 -> factor", "(x^2 - 2*x + 2)*(x^2 + 2*x + 2)");
    eq("x^3 - 6*x^2 + 11*x - 6 -> factor", "(x - 1)*(x - 2)*(x - 3)");
    eq("x^2 - y^2 -> factor", "(x + y)*(x - y)");
    eq("x^2 - 2*x*y + y^2 -> factor", "(x - y)^2");
    eq("x^2*y + x*y^2 -> factor", "x*y*(x + y)");
    eq("1/x + 1/(x+1) -> factor", "(2*x + 1)/(x*(x + 1))");
    eq("sin(x)^2 - 1 -> factor", "(sin(x) - 1)*(sin(x) + 1)");
    eq("1.5*x^2 - 6 -> factor", "1.5*(x - 2)*(x + 2)");
}

#[test]
fn solve_polynomials() {
    eq("x^2 - 5*x + 6 -> solve, x", "[2; 3]");
    eq("x^2 - 4 == 0 -> solve, x", "[-2; 2]");
    eq("x + 1 == 3 -> solve, x", "2");
    eq("x^2 - 2 -> solve, x", "[-sqrt(2); sqrt(2)]");
    eq("2*x^2 + 3*x - 2 -> solve, x", "[-2; 1/2]");
    eq("x^2 + 1 -> solve, x", "[-1i; 1i]");
    eq("x^2 + 2*x + 5 -> solve, x", "[-1 - 2i; -1 + 2i]");
    eq("x^3 - 6*x^2 + 11*x - 6 -> solve, x", "[1; 2; 3]");
    eq("x^4 - 5*x^2 + 4 -> solve, x", "[-2; -1; 1; 2]");
    eq("x^3 - 3*x + 2 -> solve, x", "[-2; 1]");
    eq("x^2 - 0.25 -> solve, x", "[-0.5; 0.5]");
    eq("a*x + b == 0 -> solve, x", "-b/a");
    eq("x^2 == a -> solve, x", "[sqrt(a); -sqrt(a)]");
}

#[test]
fn solve_cubic_quartic_exact() {
    // x^3 - 2: one real and two complex roots, checked numerically
    let r = s("x^3 - 2 -> solve, x");
    assert!(r.starts_with("[2^(1/3); "), "{r}");
    assert!(r.contains("sqrt(3)*1i"), "{r}");
    // irreducible quartics are solved with radicals
    let q = s("x^4 - 10*x^2 + 1 -> solve, x");
    assert_eq!(q, "[-sqrt(2*sqrt(6) + 5); -sqrt(-2*sqrt(6) + 5); sqrt(-2*sqrt(6) + 5); sqrt(2*sqrt(6) + 5)]");
    let u = s("x^4 + 1 -> solve, x");
    assert!(u.contains("sqrt(2)/2") && u.contains("1i"), "{u}");
    // x^3 + x + 1 has one real root 0.6823278...
    let c = s("x^3 + x + 1 -> solve, x");
    assert!(c.starts_with("[(sqrt(93)/18 - 1/2)^(1/3)"), "{c}");
}

#[test]
fn solve_rational_and_transcendental() {
    eq("(x^2-1)/(x-1) == 0 -> solve, x", "-1");
    eq("x/(x-1) == 2 -> solve, x", "2");
    eq("sqrt(x) == 3 -> solve, x", "9");
    eq("exp(x) == 2 -> solve, x", "ln(2)");
    eq("ln(x) == 2 -> solve, x", "exp(2)");
    eq("sin(x) == 1/2 -> solve, x", "π/6");
    eq("2^x == 8 -> solve, x", "3");
    // no closed form: numeric fall back with a note
    let st = crate::parser::parse_statement("cos(x) == x -> solve, x").unwrap();
    if let crate::Stmt::Symbolic { expr, ops, .. } = st {
        let defs = super::Defs::default();
        let ctx = super::Ctx { defs: &defs, numeric: None, value_of: None };
        match super::evaluate(&expr, &ops, &ctx).unwrap() {
            super::Outcome::Done(o) => {
                assert!(o.note.as_deref().unwrap_or("").contains("численно"));
                assert!(o.result.to_string().starts_with("0.7390851332"));
            }
            _ => panic!(),
        }
    }
}

#[test]
fn solve_systems() {
    eq("[x + y == 3; x - y == 1] -> solve, [x; y]", "[2; 1]");
    eq("[x + y = 3; x - y = 1] -> solve, [x; y]", "[2; 1]");
    eq("[2*x + y == 5; x - y == 1] -> solve, [x; y]", "[2; 1]");
    eq("[x^2 + y^2 == 25; x - y == 1] -> solve, [x; y]", "[-3, -4; 4, 3]");
    eq("[a*x + b*y == e; c*x + d*y == f] -> solve, [x; y]", "[(-b*f + d*e)/(a*d - b*c); (a*f - c*e)/(a*d - b*c)]");
}

#[test]
fn solve_errors() {
    assert!(s("x + 1 == x -> solve, x").starts_with("ERR"));
    assert!(s("x^2 + 1 -> solve").starts_with("ERR") == false); // a single symbol: allowed
    assert!(s("x*y -> solve").starts_with("ERR: solve: укажите переменную"));
    assert!(s("x -> frobnicate").contains("не поддерживается"));
}

#[test]
fn differentiation() {
    eq("diff(x^3 + sin(x), x) ->", "3*x^2 + cos(x)");
    eq("diff(exp(x^2), x) ->", "2*x*exp(x^2)");
    eq("diff(x^x, x) ->", "x^x*(ln(x) + 1)");
    eq("diff(atan(x), x) ->", "1/(x^2 + 1)");
    eq("diff(sqrt(1+x^2), x) ->", "x/sqrt(x^2 + 1)");
    eq("diff(sin(x)*cos(x), x, 2) ->", "-4*cos(x)*sin(x)");
    eq("diff(ln(x)/x, x) -> simplify", "(-ln(x) + 1)/x^2");
    eq("diff(a*x^2 + b*x + c, x) ->", "2*a*x + b");
    eq("diff(tan(x), x) ->", "1/cos(x)^2");
    eq("diff(y, x) ->", "0");
    eq("diff(x^5, x, 3) ->", "60*x^2");
}

#[test]
fn integration() {
    eq("integrate(x^2, x) ->", "x^3/3");
    eq("integrate(1/x, x) ->", "ln(x)");
    eq("integrate(1/(x^2+1), x) ->", "atan(x)");
    eq("integrate(x*exp(x), x) ->", "x*exp(x) - exp(x)");
    eq("integrate(x*sin(x), x) ->", "-x*cos(x) + sin(x)");
    eq("integrate(ln(x), x) ->", "x*ln(x) - x");
    eq("integrate(1/(x^2+4), x) ->", "atan(x/2)/2");
    eq("integrate(x/(x^2+1), x) ->", "ln(x^2 + 1)/2");
    eq("integrate(cos(x)*sin(x), x) ->", "-cos(x)^2/2");
    eq("integrate(1/(x^2+2*x+5), x) ->", "atan((x + 1)/2)/2");
    eq("integrate(tan(x), x) ->", "-ln(cos(x))");
    eq("integrate(1/sqrt(1-x^2), x) ->", "asin(x)");
    eq("integrate(1/(x*ln(x)), x) ->", "ln(ln(x))");
    eq("integrate(exp(2*x), x) ->", "exp(2*x)/2");
    eq("integrate(sin(x)^2, x) ->", "(x - cos(x)*sin(x))/2");
    eq("integrate(a*x + b, x) ->", "a*x^2/2 + b*x");
    eq("integrate(1/(x^2-1), x) ->", "ln(x - 1)/2 - ln(x + 1)/2");
    eq("integrate(1/(x^3-x), x) ->", "ln(x - 1)/2 + ln(x + 1)/2 - ln(x)");
    eq("integrate(sqrt(x), x) ->", "2*x^(3/2)/3");
    // unknown integrands stay unevaluated
    eq("integrate(exp(x^2), x) ->", "integrate(exp(x^2), x)");
}

#[test]
fn definite_integrals() {
    eq("integrate(x^2, x, 0, 3) ->", "9");
    eq("integrate(sin(x), x, 0, pi) ->", "2");
    eq("integrate(exp(-x), x, 0, inf) ->", "1");
    eq("integrate(1/x^2, x, 1, inf) ->", "1");
    eq("integrate(1/(1+x^2), x, -inf, inf) ->", "π");
    eq("integrate(x, x, a, b) ->", "-a^2/2 + b^2/2");
    // a pole inside the interval: not evaluated symbolically
    eq("integrate(1/x^2, x, -1, 1) ->", "integrate(1/x^2, x, -1, 1)");
}

#[test]
fn limits() {
    eq("lim(sin(x)/x, x, 0) ->", "1");
    eq("lim((x^2-1)/(x-1), x, 1) ->", "2");
    eq("lim(1/x, x, 0, 1) ->", "∞");
    eq("lim(1/x, x, 0, -1) ->", "-∞");
    eq("lim((1+1/x)^x, x, inf) ->", "e");
    eq("lim(x*exp(-x), x, inf) ->", "0");
    eq("lim((x^2+1)/(2*x^2-3), x, inf) ->", "1/2");
    eq("lim((exp(x)-1)/x, x, 0) ->", "1");
    eq("lim((1-cos(x))/x^2, x, 0) ->", "1/2");
    eq("lim(x*ln(x), x, 0, 1) ->", "0");
    eq("lim(x^x, x, 0, 1) ->", "1");
    eq("lim(atan(x), x, inf) ->", "π/2");
    eq("lim(ln(x)/x, x, inf) ->", "0");
    eq("lim(exp(x)/x^2, x, inf) ->", "∞");
    eq("lim(sqrt(x+1)-sqrt(x), x, inf) ->", "0");
    // two-sided limit of 1/x does not exist
    eq("lim(1/x, x, 0) ->", "lim(1/x, x, 0)");
}

#[test]
fn series_and_parfrac() {
    eq("sin(x) -> series, x, 6", "x^5/120 - x^3/6 + x");
    eq("exp(x) -> series, x, 5", "x^4/24 + x^3/6 + x^2/2 + x + 1");
    eq("1/(1-x) -> series, x, 4", "x^3 + x^2 + x + 1");
    eq("cos(x)/x^2 -> series, x, 4", "x^2/24 + 1/x^2 - 1/2");
    eq("ln(1+x) -> series, x, 4", "x^3/3 - x^2/2 + x");
    eq("exp(x) -> series, x = 1, 3", "e*(x - 1)^2/2 + e*(x - 1) + e");
    eq("(x^2+1)/(x*(x-1)) -> parfrac, x", "2/(x - 1) - 1/x + 1");
    eq("1/(x^2-1) -> parfrac, x", "1/(2*(x - 1)) - 1/(2*(x + 1))");
    eq("x^2/(x^2+x-2) -> parfrac, x", "1/(3*(x - 1)) - 4/(3*(x + 2)) + 1");
    eq("1/(x*(x+1)^2) -> parfrac, x", "1/x - 1/(x + 1) - 1/(x + 1)^2");
}

#[test]
fn collect_coeffs_substitute_float() {
    eq("a*x + b*x + c -> collect, x", "c + x*(a + b)");
    eq("a*x^2 + b*x + c -> coeffs, x", "[c; b; a]");
    eq("(x+1)^3 -> coeffs, x", "[1; 3; 3; 1]");
    eq("x^2 + y -> substitute, x = 2", "y + 4");
    eq("x^2 + y -> substitute, x = a+b", "y + (a + b)^2");
    eq("x + y -> substitute, x = y, y = x", "x + y");
    eq("sin(x) -> substitute, x = pi/2", "1");
    eq("pi -> float, 5", "3.1416");
    eq("sqrt(2)+1/3 -> float, 10", "1.747546896");
    eq("exp(1) -> float, 6", "2.71828");
    eq("1/3 -> float, 3", "0.333");
}

#[test]
fn sums_and_products() {
    eq("sum(k, 1, n, k) ->", "n^2/2 + n/2");
    eq("sum(k, 1, n, k^2) ->", "n^3/3 + n^2/2 + n/6");
    eq("sum(k, 1, 10, k^2) ->", "385");
    eq("sum(k, 0, n, 2^k) ->", "2^(n + 1) - 1");
    eq("sum(k, 1, inf, 1/2^k) ->", "1");
    eq("prod(k, 1, n, k) ->", "n!");
    eq("prod(k, 1, 5, k) ->", "120");
}

#[test]
fn matrices() {
    eq("[1, 2; 3, 4]^-1 ->", "[-2, 1; 3/2, -1/2]");
    eq("[a, b; c, d]^-1 ->", "[d/(a*d - b*c), -b/(a*d - b*c); -c/(a*d - b*c), a/(a*d - b*c)]");
    eq("transpose([1, 2; 3, 4]) ->", "[1, 3; 2, 4]");
    eq("|[1, 2; 3, 4]| ->", "-2");
    eq("det([a, b; c, d]) ->", "a*d - b*c");
    eq("[1, 2; 3, 4]*[x; y] ->", "[x + 2*y; 3*x + 4*y]");
    eq("[1, 2; 3, 4] + [x, 0; 0, x] ->", "[x + 1, 2; 3, x + 4]");
    eq("2*[x, y] ->", "[2*x, 2*y]");
    eq("[1, 1; 0, 1]^3 ->", "[1, 3; 0, 1]");
    eq("tr([a, b; c, d]) ->", "a + d");
    eq("det([1, 2, 3; 4, 5, 6; 7, 8, 10]) ->", "-3");
    assert!(s("[1, 2; 3, 4]*[1, 2, 3] ->").starts_with("ERR"));
    assert!(s("[1, 2; 2, 4]^-1 ->").starts_with("ERR"));
}

#[test]
fn units_stay_symbols() {
    eq("5*m + 3*m ->", "8*m");
    eq("2*m*3*s ->", "6*m*s");
    eq("5*m/s*2*s ->", "10*m");
    eq("5*kg*9.81*m/s^2 ->", "49.05*kg*m/s^2");
}

#[test]
fn assumptions() {
    eq("sqrt(x^2) ->", "sqrt(x^2)");
    eq("sqrt(x^2) -> assume, x > 0", "x");
    eq("abs(x) -> assume, x > 0", "x");
    eq("abs(x) -> assume, x < 0", "-x");
}

#[test]
fn numeric_correctness_of_results() {
    // identities checked numerically at sample points
    let x = [("x", 0.73)];
    assert!((num("(x+1)^5 -> expand", &x) - 1.73f64.powi(5)).abs() < 1e-9);
    assert!((num("x^4 - 1 -> factor", &x) - (0.73f64.powi(4) - 1.0)).abs() < 1e-9);
    assert!((num("integrate(x^2*exp(x), x) ->", &x) - (0.73f64.powi(2) * 0.73f64.exp() - 2.0 * 0.73 * 0.73f64.exp() + 2.0 * 0.73f64.exp())).abs() < 1e-9);
    assert!((num("diff(sin(x)^3, x) ->", &x) - 3.0 * 0.73f64.sin().powi(2) * 0.73f64.cos()).abs() < 1e-9);
    assert!((num("1/(x^2+3*x+2) -> parfrac, x", &x) - 1.0 / (0.73f64.powi(2) + 3.0 * 0.73 + 2.0)).abs() < 1e-9);
}

// ------------------------------------------------------------ worksheet level

fn ws(lines: &str) -> (Worksheet, std::collections::HashMap<u64, crate::worksheet::RegionOutput>) {
    let w = Worksheet::from_lines(lines);
    let out = crate::evaluate_worksheet(&w);
    (w, out)
}

fn sym_of(lines: &str) -> Vec<String> {
    let (w, out) = ws(lines);
    w.regions
        .iter()
        .map(|r| match &out[&r.id].output {
            Output::Symbolic { result, .. } => result.to_string(),
            Output::Error(e) => format!("ERR: {e}"),
            Output::Value { view, .. } => format!("VALUE {view}"),
            _ => String::new(),
        })
        .collect()
}

#[test]
fn definitions_are_substituted() {
    let r = sym_of("a := 3\nb := a + 1\nx^2 + a*x + b ->");
    assert_eq!(r[2], "x^2 + 3*x + 4");
    // functions are expanded
    let r = sym_of("f(x) := x^2 + 1\nf(a + 1) -> expand");
    assert_eq!(r[1], "a^2 + 2*a + 2");
    // undefined names stay symbolic, defined ones are used
    let r = sym_of("k := 2\nk*y + z ->");
    assert_eq!(r[1], "2*y + z");
    // the variable of the keyword is never substituted
    let r = sym_of("x := 5\nx^2 - 4 -> solve, x");
    assert_eq!(r[1], "[-2; 2]");
    // later definitions override earlier ones
    let r = sym_of("a := 1\na ->\na := 2\na ->");
    assert_eq!((r[1].as_str(), r[3].as_str()), ("1", "2"));
    // derivative of a defined function
    let r = sym_of("g(x) := x^3\ndiff(g(x), x) ->");
    assert_eq!(r[1], "3*x^2");
}

#[test]
fn symbolic_definition_with_result() {
    let (w, out) = ws("h := 2\ng := h*x^2 -> expand\ng ->");
    let r1 = &out[&w.regions[1].id];
    assert!(matches!(&r1.output, Output::Symbolic { result, .. } if result.to_string() == "2*x^2"));
    // the symbolic definition is available below
    let r2 = &out[&w.regions[2].id];
    assert!(matches!(&r2.output, Output::Symbolic { result, .. } if result.to_string() == "2*x^2"));
}

#[test]
fn numeric_fallbacks() {
    // matrices and built-in functions with closed arguments are evaluated numerically
    let r = sym_of("v := [3; 4]\nlength(v) ->");
    assert_eq!(r[1], "2");
    let r = sym_of("A := [1, 2; 3, 4]\nA^-1 ->");
    assert_eq!(r[1], "[-2, 1; 3/2, -1/2]");
    let r = sym_of("A := [1, 2; 3, 4]\nA[1, 1] ->");
    assert_eq!(r[1], "4");
    // programs cannot be symbolic: shown as a value
    let r = sym_of("{ s <- 0; for i in 1..3 { s <- s + i }; s } ->");
    assert!(r[0].starts_with("VALUE 6") || r[0] == "6", "{r:?}");
}

#[test]
fn errors_are_russian() {
    let r = sym_of("1/0 ->");
    assert!(r[0].contains("деление на ноль"), "{r:?}");
    let r = sym_of("x -> collect");
    assert!(r[0].contains("требует аргумент"), "{r:?}");
    let r = sym_of("x -> series, 2");
    assert!(r[0].starts_with("ERR"), "{r:?}");
    let r = sym_of("x + 1 -> coeffs, y");
    assert_eq!(r[0], "[x + 1]");
}

#[test]
fn chained_keywords() {
    eq("(x+1)^2 - (x-1)^2 -> expand -> factor", "4*x");
    eq("(x^2-1)^2 -> expand -> factor", "(x - 1)^2*(x + 1)^2");
    eq("(x+1)^3 -> expand -> collect, x", "x^3 + 3*x^2 + 3*x + 1");
}

/// Development helper: prints the results of the lines in $XMATH_EXPLORE (a text file).
#[test]
fn explore() {
    let Ok(path) = std::env::var("XMATH_EXPLORE") else { return };
    let inputs = std::fs::read_to_string(path).unwrap_or_default();
    for line in inputs.lines().filter(|l| !l.trim().is_empty()) {
        println!("{line}   ==>   {}", s(line));
    }
}
