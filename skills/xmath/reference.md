# xmath syntax reference

xmath is a free Mathcad 15 clone. A worksheet is a set of *regions* (math, text, plot) evaluated
top-to-bottom, then left-to-right (by `y`, then `x`). Every math region holds ONE statement written as
linear text. Everything defined above (smaller `y`) is visible below; `≡` (global) definitions are visible everywhere.

## 1. Statements

| Form | Meaning | Example |
|------|---------|---------|
| `name := expr` | local definition | `L := 6 m` |
| `name ≡ expr` or `name ::= expr` | global definition (visible in the whole sheet) | `g0 ≡ 9.81 m/s^2` |
| `f(a, b) := expr` | user function | `f(x) := x^2 + 1` |
| `expr =` | evaluate and show the result | `f(3) =` -> `f(3) = 10` |
| `expr = unit` | evaluate and show the result in a unit | `M = kN*m` |
| `name := expr =` / `name := expr = unit` | definition that also displays its value (Mathcad `d := a/b = 69.6 μm`) | `d := L/n = mm` |
| `[a; b; c] := expr` | vector definition: `expr` must be a vector of the same length; each name gets one element | `[x; y] := A*v` |
| `v[i] := expr` | indexed (element-wise) definition | `v[i] := i^2` |
| `i := a..b` | range variable, step 1 | `i := 0..10` |
| `x := a, a2..b` | range variable with step `a2 - a` | `x := 0, 0.1..1` |

* A line may NOT contain two statements. One line = one region.
* In evaluation, `:=` defines, `=` displays. There is no assignment with `=`.
* Redefinition is allowed; later regions see the latest definition above them.
* Lines starting with `#` in a `.txt` / `lines` input become text (comment) regions.

## 2. Numbers, operators, constants

* Numbers: `5`, `0.25`, `1.5e-3`, `2i` / `3 + 4i` (complex).
* `+ - * / ^`, unary minus, parentheses, factorial `n!`, absolute value / magnitude `|x|`, comparisons `< > <= >= ≠` (give 1 or 0).
* Implicit multiplication: `2 y`, `5 kN`, `2(x+1)`, `3 m`. Prefer an explicit `*` when in doubt.
* `^` is right-associative; `x^-1` works; `A^-1` is the matrix inverse.
* Constants / built-in variables: `π` (or `pi`), `e`, `i` (imaginary unit, only if you did not redefine it as a range variable), `∞`, `g` (gravity 9.807 m/s^2), `ORIGIN` (= 0, index of the first element), `TOL` (= 0.001).
* Mathcad caveat: `g` is GRAVITY. For grams use `gm`.
* Strings: `"text"`, functions `concat`, `num2str`, `strlen`.

## 3. Units

Write units right after the number: `5 kN`, `12 kN/m`, `210 GPa`, `30 deg`. Units are first-class:
`5 kN * 3 m` is a moment/energy (shown as `1.5·10^4 J` by default, `= kN*m` gives `15 kN*m`), `kN/m * m^2` is fine, `5 m + 3 s` is an error
("Единицы измерения не совпадают" — units mismatch). Dimensionless angle units: `rad`, `deg` (or `°`), `%`.
Use `expr = unit` to choose the display unit: `M = kN*m`, `sigma = MPa`, `L = mm`.
Call `list_units` for the exact table. Highlights:

* length: `m km dm cm mm μm nm pm Å in ft yd mi nmi mil au ly pc`; mass: `kg gm mg tonne mton lb oz ton slug amu` (`gm` = gram; `g` is gravity)
* time: `s sec ms μs ns ps min hr day wk yr`; area: `ha acre`; volume: `L mL dL cL hL liter gal qt pt fl_oz bbl`; speed: `kph mph knot`
* force: `N kN MN GN kgf tonnef dyne lbf kip ozf tonf`; pressure/stress: `Pa kPa MPa GPa hPa bar mbar atm at psi ksi torr mmHg inHg mmH2O inH2O`
* energy/power: `J mJ kJ MJ GJ cal kcal Wh kWh MWh eV keV MeV BTU erg`, `W mW kW MW GW hp`
* electric: `A mA μA kA V mV μV kV Ω mΩ kΩ MΩ S mS μS F mF μF nF pF H mH μH nH C mC μC Wb T mT gauss`
* frequency: `Hz kHz MHz GHz THz rpm`; radioactivity/dose: `Bq Ci Gy Sv`; light: `cd lm lx`; amount: `mol mmol kmol`
* viscosity: `P cP St cSt`; angle: `rad mrad deg ° rev arcmin arcsec`; `%`.

**Temperature (Mathcad 15 rules).** `K` and `R` (Rankine) are absolute units; `°C` (`degC`) and `°F` (`degF`) are *offset* units;
`Δ°C`, `Δ°F` are differences. `T := 25 °C` stores 298.15 K (`x °C` = `(x + 273.15) K`, `x °F` = `(x + 459.67)·5/9 K`) and
`T = °C` shows 25 (`T = °F` 77, `T = K` 298.15). The offset applies only when `°C`/`°F` is the last factor of a scalar
number (`25 °C`, `x °C`) and as a display unit; anywhere else (`5 °C/s`, `°C^2`, a bare `°C`) it behaves as `K` / `Δ°C`.
Add differences with `Δ°C`: `T + 10 Δ°C`. Subtracting two temperatures gives kelvin. A user definition (`R := 8.314`) shadows a unit name.

**Result units.** Results are shown in SI automatically: a derived unit when the dimension matches exactly (`N Pa J W C V Ω S F H Wb T`;
energy is `J`, never `N·m`; frequency is `1/s`), else the shortest combination (`Pa·s`, `W/(m·K)`, `N/m`, `kg/m^3`, `m^2/s`).
The worksheet option `unit_system` (`si`, `mks`, `cgs`, `us` in `format`) changes only the display: CGS shows `cm gm dyne erg`,
US shows `ft lb lbf psi hp R`. If the display unit has another dimension the rest is appended: `5 N = kg` gives `5 m/s^2` (i.e. 5 m/s^2·kg).

## 4. Functions (call `list_functions` for the full list)

Trig (radians unless you pass `deg`): `sin cos tan cot sec csc asin acos atan atan2(x,y) sinh cosh tanh ...`
Analysis: `exp ln log sqrt nthroot(x,n) abs floor ceil round trunc mod(a,b) sign min max`
Statistics: `mean median stdev var`; vectors/matrices: `length rows cols last identity transpose det tr lsolve augment stack submatrix cross norme sort reverse`
Interpolation: `linterp(vx, vy, x)`; polynomials: `polyroots([c0; c1; c2])` (coefficients from the constant term up).
Special: `factorial gamma erf erfc dnorm pnorm`; conditional: `if(cond, a, b)`.
Linear algebra: `eigenvals eigenvecs eigenvec rank cholesky lu qr svd svds geninv rref norm1 norm2 normi cond2 diag csort rsort`.
Fitting: `lspline pspline cspline` + `interp(vs, vx, vy, x)`, `regress(vx, vy, k)` (also usable with `interp`), `slope intercept line corr`.
Curve fits (return a column vector of parameters; `vg` is an initial guess): `expfit(vx, vy[, vg])` a*e^(b*x)+c; `lgsfit(vx, vy, vg)` a/(1+b*e^(-c*x)); `logfit(vx, vy, vg)` a*ln(x+b)+c; `lnfit(vx, vy)` a*ln(x)+b; `pwrfit(vx, vy, vg)` a*x^b+c; `sinfit(vx, vy, vg)` a*sin(x+b)+c; `medfit(vx, vy)`; `linfit(vx, vy, F)` with `F(x)` returning a vector of basis functions; `genfit(vx, vy, vg, F)` with `F(x, u)` returning `f` or `[f; df/du0; ...]` (Levenberg-Marquardt).
Smoothing: `loess(vx, vy, span)` (use as `interp(loess(vx, vy, span), vx, vy, x)`), `ksmooth(vx, vy, b)`, `medsmooth(vy, n)`, `supsmooth(vx, vy)`; linear prediction `predict(v, m, n)` (Burg; needs at least 2m points).
Random numbers (fixed seed, reproducible): `rnd(x)`, and vector generators `runif(m, a, b) rnorm(m, mu, sigma) rexp(m, r) rpois(m, lambda) rbinom(m, n, p) rgamma rchisq rbeta rt rlnorm rweibull rcauchy rlogis rgeom`; discrete distributions `dbinom pbinom dpois ppois`.
A plain `0` is compatible with any units (`max(0, 3 m)`, `integrate(f, t, 0, 1 m)`, ranges `0 .. 1 m`). Negative numbers take the principal branch: `(-8)^(1/3) = 1 + 1.732i`; use `nthroot(-8, 3) = -2` for the real root.
More statistics: `cvar skew kurt hist gmean hmean mode qnorm dunif punif dexp pexp`; integers: `gcd lcm`; `Φ` (Heaviside), `δ` (Kronecker), `sinc csgn`.
Bessel: `J0 J1 Jn Y0 Y1 Yn I0 I1 In K0 K1 Kn`. Strings: `concat num2str str2num substr search strlen vec2str str2vec`. Tables: `lookup match vlookup hlookup`.
ODEs: `rkfixed(y0, x1, x2, n, D)` / `Rkadapt(...)` where `D(x, y)` is a user function returning the derivative vector; the result matrix has columns `[x, y0, y1, ...]`.
Error messages from the engine are in Russian (e.g. `Единицы измерения не совпадают` = units mismatch, `'x' не определена` = undefined variable).

### Calculus and solving (the variable name is the second argument)

* `sum(k, a, b, expr)`, `prod(k, a, b, expr)` — e.g. `sum(k, 1, 10, k^2) =` -> `385`
* `integrate(expr, x, a, b)` — `∞` / `-∞` allowed: `integrate(exp(-x^2), x, -∞, ∞) =` -> `1.772`
* `diff(expr, x)` (needs `x` defined), `diff(expr, x, x0)`: numeric derivative at the point `x0` (`x` need not be defined when `x0` is given). Write the EXPRESSION, not a function name:
  `diff(sin(x), x, 0) =` -> `1`, `diff(x^2, x, 3) =` -> `6`
* `root(expr, x)` (uses the current guess for `x` if defined) or bracketed `root(expr, x, a, b)`: `root(x^2-2, x, 0, 2) =` -> `1.414`
* Pass expressions, not bare function names, to `integrate` / `diff` / `root` (`integrate(f, x, 0, 3)` with a user function `f` fails: write `integrate(f(x), x, 0, 3)`).

### Solve blocks (Given / Find / Minerr / Minimize / Maximize)

One region (line) each, in this order: guess values for the unknowns, a line `Given`, constraint lines (Boolean `==`, `<`, `>`, `<=`, `>=`, several allowed; units fine), then the closing call:

```
x := 1
y := 1
Given
x^2 + y^2 == 5
x + y == 3
Find(x, y) =
```

* `Find(x)` returns a scalar, `Find(x, y)` a column vector; units of the guesses are kept. Error `Решение не найдено…` if no solution; use `Minerr(x, y)` for the best least-squares fit of inconsistent constraints.
* `Minimize(f, x, y)` / `Maximize(f, x, y)`: `f` is a user function name; optimizes from the current `x`, `y`, honouring constraints of a preceding `Given` (works without `Given` too).
* `sol := Find(x, y)` stores the vector; `[x; y] := Find(x, y)` assigns each unknown.
* A function of a solve block: `r(a) := Find(x)` (constraints may use `a`); `r(9) =` re-solves.
* Keep definitions out of the block (they run as ordinary definitions; a guess may even follow `Given`). TOL/CTOL definitions are respected. `Given` without a closing call is flagged. Unknowns need guesses defined above (linear systems work from 0).

## 5. Vectors, matrices, ranges, tables

* Matrix literal: `[1, 2; 3, 4]` (commas separate columns, semicolons separate rows). Column vector `[1; 2; 3]`, row vector `[1, 2, 3]`.
* Indexing: `A[0, 1]` (row, column), `v[2]`; indices start at `ORIGIN` = 0.
* Operations: `A*B`, `A^-1`, `transpose(A)`, `det(A)`, `lsolve(A, b)`, `A*x`, elementwise through functions (`sqrt([1;4;9])`).
* Mathcad applies some operations element by element **automatically** (no `vectorize` needed), and xmath follows these rules:
  `v^n` (any scalar `n`) for a non-square array/vector; `scalar/v`; `v/w` for arrays of equal shape when the divisor is non-square; `scalar^v`;
  `v + scalar`, `v - w`, comparisons and scalar functions (`sqrt(v)`) are element-wise. Unchanged: `v*w` of two column vectors is the dot product
  (a scalar), `A*B` is the matrix product, and for SQUARE matrices `A^n` is the matrix power and `x/A`, `B/A` are `A^-1` products.
  Use `vectorize(...)` to force element-wise behaviour for `v*w`, square matrices, or user functions.
* Matrix column (Mathcad `M⟨j⟩`, Ctrl+6): `M^<j>` gives column `j` as a column vector (index from `ORIGIN`), e.g. `X := M^<0>`, `(transpose(M))^<1>`. The error "столбец … вне допустимых границ" is raised for a bad index.
* Vectorize (Mathcad arrow over an expression, Ctrl+-): `vectorize(expr)` applies everything inside **element by element** over all vector/matrix operands of the same size
  (scalars are broadcast): `vectorize(v*w)` is the element-wise product (not the dot product), `vectorize(v/w)` element-wise quotient, `vectorize(f(v))` calls a user function or
  built-in per element, `vectorize(v^2 + 1)`. Operands of different sizes give an error. Typical use: `S := vectorize(f(φ))`, `k := vectorize(a/b)`.
* Range sum `sum(i, expr)` / `prod(i, expr)` (Mathcad Σ with only an index under it) runs over the range variable `i` defined above.
* Range variable: `i := 0..4` then `v[i] := i^2` builds the vector `[0;1;4;9;16]`; `v =` shows it.
  `x := 0, 0.25..1` gives `0, 0.25, 0.5, 0.75, 1`. An expression using a range variable is evaluated for every value:
  `x^2 =` shows a column of results (a "table").
* Range variables may appear in: indexed definitions (`v[i] := ...`), evaluations (`x^2 =`), and plots. They cannot be used as ordinary scalars in
  a plain definition (`y := x^2` with `x` a range is an error / ambiguous) — use `y[i] := ...` instead.

### Programs (Mathcad programming blocks)

A program is an expression in braces; statements are separated by `;` (a trailing `;` is allowed, and `;` is optional after a `}`).
It can be the right-hand side of `:=` for variables and functions, or be used inside any expression.

```
f(n) := { s ← 0; for i ∈ 1..n { s ← s + i^2 }; s }
g(x) := { return -1 if x < 0; sqrt(x) }
h(x) := { if x < 0 { "neg" } else if x == 0 { "zero" } otherwise { "pos" } }
k(x) := { y ← 1/x on error 0; y }
w(a) := { i ← 0; while i < a { i ← i + 1; if i == 5 { break } }; i }
```

* Statements: `x ← e` / `v[i] ← e` (local assignment; ASCII `<-`), `for i ∈ range { ... }` (ASCII `in`; range `1..n`, `0, 2..10`, or a vector),
  `while cond { ... }`, `if cond { ... } otherwise { ... }` (`else` and `else if` also accepted), postfix `stmt if cond`,
  `break`, `continue`, `return e`, and `e1 on error e2` (value of `e2` if `e1` fails; usable on any expression).
* Consecutive `if` statements of a block form a chain: an `otherwise` runs only if its own condition and the conditions of all preceding `if`s of the chain were false, e.g.
  `{ a if x < 0; if x >= 0 and x < 5 { b } otherwise { c } }` returns `a`, `b` or `c` (Mathcad semantics).
* The value of a program is the value of the last executed statement, or of `return`. A program that ends with no value is an error.
* `←` always creates/updates a **local**; locals shadow worksheet variables and never change them. Assignments in nested blocks update the enclosing program's local.
  `v[i] ← e` builds a local vector (indices start at `ORIGIN`).
* Loop iteration limit: 10 000 000 per program run (error in Russian).
* Beware: `for i ∈ 2..n` with `n < 2` counts down (2, 1) — guard with `return 1 if n < 2`. Write `a < -1` with a space (`a<-1` is an assignment arrow).

## 6. Plots

A plot region has an x expression (usually a range variable) and one or more y expressions.
Create it with `add_region` kind `plot`: `x_expr: "x"`, `y_exprs: ["sin(x)", "x^2"]`.
Define the range above the plot (`x := 0, 0.1..6.28`). If `x` is undefined, a quick-plot over -10..10 is used.
The server's result describes each series (label, point count, x/y range) — it cannot render images.

Other plot kinds (all through `add_region`; `open_worksheet` reports them with `type` and data ranges):

| `kind` | arguments | what is drawn |
|--------|-----------|---------------|
| `polar_plot` | `x_expr` = angle θ (range variable; empty: 0..2π), `y_exprs` = radii | polar curves, angles in degrees, radius rings; negative radius is drawn on the opposite side |
| `surface_plot` | `expr` | surface of a matrix `M` (rows = x index, columns = y index), of a function name `f` of two variables, or of an expression in `x`, `y` (QuickPlot over -5..5, 25x25 grid) |
| `contour_plot` | `expr` (same as surface) | filled contours with 10 labelled level lines (marching squares) |
| `bar_plot` | `expr` (matrix) | 3D bars |
| `scatter_plot` | `expr` = `X, Y, Z` (three vectors) or an n×3 matrix | 3D points |

`open_worksheet` regions of these kinds have `type` `polar_plot`, `surface_plot`, `contour_plot`, `bar_plot`
or `scatter_plot`; 3D ones carry `plot: {grid: [nx, ny], points, x_range, y_range, z_range}` (or `error`).
Axis numbers follow the worksheet result format: with the default exponential threshold 3, `0.001` is
shown as `1×10⁻³`. Values of quantities with units are plotted in SI base units (like Mathcad 15) and the
unit is shown in grey next to the axis (`[m]`) unless the axis has its own title.
The GUI edits all plot options (secondary Y axis, symbols, trace types, legend, titles, number format,
3D view angles, colour maps, contour levels) in the format dialogs; they are stored in the region's `fmt` / `view`.

## 7. File format

`.xmath` is JSON: `{"version":1,"format":{...},"regions":[{"id":1,"x":20,"y":20,"type":"math","source":"L := 6 m"}, {"type":"text","text":"..."}, {"type":"plot","x_expr":"x","y_exprs":["sin(x)"]}, {"type":"plot","kind":"polar","x_expr":"t","y_exprs":["1+cos(t)"]}, {"type":"plot3d","kind":"surface","expr":"sin(x)*cos(y)","view":{"azimuth":35,"elevation":30,"zoom":1}}]}`.
Use the MCP tools or `xmath-cli` instead of hand-editing it.

## 8. Common pitfalls

1. `g` is gravity, grams are `gm`. `10 gm` is 0.01 kg.
2. Unit mismatch: `5 m + 3 s`, `sqrt(5 m)` is fine but `sin(5 m)` is not (argument must be dimensionless).
3. Forces: `kN` is a unit; `kN*m` is a moment; `kN/m` a line load; both become SI when printed unless you request `= kN*m`.
4. Implicit multiplication can surprise: `a b` is `a*b`, `f (x)` may be a call. Use `*`.
5. Order matters: a region only sees definitions with a smaller `y` (or same `y` and smaller `x`).
6. Indices start at 0 (`ORIGIN = 0`).
7. Results default to 3 decimals; large/small numbers use `a·10^n`. Use `expr = unit` for control.
8. Do not use `$`, `\`, LaTeX or `==` — only the linear syntax above. Use `^` not `**`.
9. `1/0`, undefined names and unit mismatches return an error string for that region only; the others are still evaluated.
