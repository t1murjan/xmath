//! Static help: function descriptions, unit listing, syntax reference.

use xmath_core::units::{self, Dim};

/// The full syntax reference (shared with the Agent Skill).
pub const SYNTAX_HELP: &str = include_str!("../../../skills/xmath/reference.md");

/// One-line description of every built-in function (kept in sync with `builtins::NAMES` by a test).
pub const FUNCTION_DOCS: &[(&str, &str)] = &[
    ("sin", "sin(x): sine (x in radians or with deg)"),
    ("cos", "cos(x): cosine"),
    ("tan", "tan(x): tangent"),
    ("cot", "cot(x): cotangent"),
    ("sec", "sec(x): secant"),
    ("csc", "csc(x): cosecant"),
    ("asin", "asin(x): inverse sine (radians)"),
    ("acos", "acos(x): inverse cosine (radians)"),
    ("atan", "atan(x): inverse tangent (radians)"),
    ("acot", "acot(x): inverse cotangent"),
    ("atan2", "atan2(y, x): angle of the point (x, y), quadrant-aware"),
    ("sinh", "sinh(x): hyperbolic sine"),
    ("cosh", "cosh(x): hyperbolic cosine"),
    ("tanh", "tanh(x): hyperbolic tangent"),
    ("asinh", "asinh(x): inverse hyperbolic sine"),
    ("acosh", "acosh(x): inverse hyperbolic cosine"),
    ("atanh", "atanh(x): inverse hyperbolic tangent"),
    ("exp", "exp(x): e^x"),
    ("ln", "ln(x): natural logarithm"),
    ("log", "log(x) base-10 logarithm; log(x, b) logarithm to base b"),
    ("sqrt", "sqrt(x): square root (negative input gives a complex result); units are halved"),
    ("nthroot", "nthroot(x, n): n-th root"),
    ("abs", "abs(x): absolute value / complex modulus (also |x|)"),
    ("arg", "arg(z): argument (phase) of a complex number"),
    ("Re", "Re(z): real part"),
    ("Im", "Im(z): imaginary part"),
    ("conj", "conj(z): complex conjugate"),
    ("floor", "floor(x): round down to an integer"),
    ("ceil", "ceil(x): round up to an integer"),
    ("round", "round(x[, n]): round to the nearest integer (or n decimals)"),
    ("trunc", "trunc(x): drop the fractional part"),
    ("mod", "mod(a, b): remainder of a / b"),
    ("sign", "sign(x): -1, 0 or 1"),
    ("signum", "signum(x): sign function (Mathcad's signum)"),
    ("max", "max(a, b, ...) or max(vector/matrix): largest value"),
    ("min", "min(a, b, ...) or min(vector/matrix): smallest value"),
    ("mean", "mean(v): arithmetic mean of the elements"),
    ("median", "median(v): median of the elements"),
    ("stdev", "stdev(v): population standard deviation"),
    ("Stdev", "Stdev(v): sample standard deviation"),
    ("var", "var(v): population variance"),
    ("Var", "Var(v): sample variance"),
    ("sum", "sum(k, a, b, expr): sum of expr for k = a..b (also sums the elements of a vector)"),
    ("prod", "prod(k, a, b, expr): product of expr for k = a..b"),
    ("length", "length(v): number of elements of a vector"),
    ("rows", "rows(A): number of rows"),
    ("cols", "cols(A): number of columns"),
    ("last", "last(v): index of the last element (length - 1 with ORIGIN = 0)"),
    ("identity", "identity(n): n x n identity matrix"),
    ("transpose", "transpose(A): transposed matrix"),
    ("det", "det(A): determinant of a square matrix"),
    ("tr", "tr(A): trace of a square matrix"),
    ("lsolve", "lsolve(A, b): solves the linear system A*x = b"),
    ("augment", "augment(A, B): joins matrices side by side (same number of rows)"),
    ("stack", "stack(A, B): stacks matrices vertically (same number of columns)"),
    ("submatrix", "submatrix(A, ir, jr, ic, jc): block of A from row ir..jr and column ic..jc"),
    ("cross", "cross(a, b): cross product of two 3-vectors"),
    ("norme", "norme(A): Euclidean (Frobenius) norm"),
    ("sort", "sort(v): sorts a vector in ascending order"),
    ("reverse", "reverse(v): reverses a vector"),
    ("linterp", "linterp(vx, vy, x): linear interpolation of the points (vx, vy) at x"),
    ("polyroots", "polyroots(v): roots of the polynomial with coefficients v (constant term first)"),
    ("factorial", "factorial(n): n! (also written n!)"),
    ("gamma", "gamma(x): gamma function"),
    ("Γ", "Γ(x): gamma function"),
    ("erf", "erf(x): error function"),
    ("erfc", "erfc(x): complementary error function"),
    ("dnorm", "dnorm(x, mu, sigma): normal probability density"),
    ("pnorm", "pnorm(x, mu, sigma): normal cumulative distribution"),
    ("UnitsOf", "UnitsOf(x): the unit of x as a value (divide x by it to strip units)"),
    ("concat", "concat(s1, s2, ...): joins strings"),
    ("num2str", "num2str(x): number to string"),
    ("strlen", "strlen(s): length of a string"),
    ("if", "if(cond, a, b): a when cond is non-zero, otherwise b"),
    ("integrate", "integrate(expr, x, a, b): definite integral over x from a to b (infinite limits allowed)"),
    ("diff", "diff(expr, x[, x0]): numeric derivative of expr with respect to x (at x0)"),
    ("root", "root(expr, x) or root(expr, x, a, b): solves expr = 0 for x, starting at x's value or bracketed in [a, b]"),
    ("matrix", "matrix(m, n, f): m x n matrix with element (i, j) = f(i, j); f is the NAME of a user function defined above, e.g. f(i, j) := i + j"),
];

/// Physical dimensions used to group the unit table.
fn dim_groups() -> Vec<(&'static str, Dim)> {
    vec![
        ("dimensionless (angles, percent)", Dim::NONE),
        ("length", units::LENGTH),
        ("mass", units::MASS),
        ("time", units::TIME),
        ("electric current", units::CURRENT),
        ("temperature", units::TEMPERATURE),
        ("amount of substance", units::AMOUNT),
        ("luminous intensity", units::LUMINOSITY),
        ("area", units::AREA),
        ("volume", units::VOLUME),
        ("frequency", units::FREQUENCY),
        ("velocity", units::VELOCITY),
        ("force", units::FORCE),
        ("pressure / stress", units::PRESSURE),
        ("energy / work", units::ENERGY),
        ("power", units::POWER),
        ("electric charge", units::CHARGE),
        ("voltage", units::VOLTAGE),
        ("resistance", units::RESISTANCE),
        ("conductance", units::CONDUCTANCE),
        ("capacitance", units::CAPACITANCE),
        ("inductance", units::INDUCTANCE),
        ("magnetic flux", units::MAG_FLUX),
        ("magnetic flux density", units::MAG_FLUX_DENSITY),
    ]
}

/// Units grouped by dimension: `(group title, [(unit name, factor to SI)])`.
pub fn units_by_dimension() -> Vec<(String, Vec<(&'static str, f64)>)> {
    type Group = (String, Dim, Vec<(&'static str, f64)>);
    let mut groups: Vec<Group> =
        dim_groups().into_iter().map(|(n, d)| (n.to_string(), d, Vec::new())).collect();
    for u in units::UNITS {
        match groups.iter_mut().find(|g| g.1 == u.dim) {
            Some(g) => g.2.push((u.name, u.factor)),
            None => groups.push((format!("{:?}", u.dim), u.dim, vec![(u.name, u.factor)])),
        }
    }
    groups.into_iter().filter(|g| !g.2.is_empty()).map(|(n, _, v)| (n, v)).collect()
}
