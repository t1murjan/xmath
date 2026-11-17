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

* length: `m km dm cm mm μm nm in ft yd mi`; mass: `kg gm mg tonne lb`; time: `s ms min hr day week yr`
* force: `N kN MN kgf tonnef lbf kip`; pressure/stress: `Pa kPa MPa GPa bar atm psi ksi mmHg`
* energy/power: `J kJ MJ cal kcal Wh kWh eV BTU`, `W kW MW hp`
* electric: `A mA kA V kV mV Ω kΩ MΩ S F μF nF pF H mH Wb T C`; frequency: `Hz kHz MHz rpm`
* temperature: `K` (differences `Δ°C`). Area/volume: `ha L mL liter gal`. Speed: `kph mph`.

## 4. Functions (call `list_functions` for the full list)

Trig (radians unless you pass `deg`): `sin cos tan cot sec csc asin acos atan atan2(x,y) sinh cosh tanh ...`
Analysis: `exp ln log sqrt nthroot(x,n) abs floor ceil round trunc mod(a,b) sign min max`
Statistics: `mean median stdev var`; vectors/matrices: `length rows cols last identity transpose det tr lsolve augment stack submatrix cross norme sort reverse`
Interpolation: `linterp(vx, vy, x)`; polynomials: `polyroots([c0; c1; c2])` (coefficients from the constant term up).
Special: `factorial gamma erf erfc dnorm pnorm`; conditional: `if(cond, a, b)`.
Linear algebra: `eigenvals eigenvecs eigenvec rank cholesky lu qr svd svds geninv rref norm1 norm2 normi cond2 diag csort rsort`.
Fitting: `lspline pspline cspline` + `interp(vs, vx, vy, x)`, `regress(vx, vy, k)` (also usable with `interp`), `slope intercept line corr`.
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
* Definitions are not allowed between `Given` and `Find` (error). `Given` without a closing call is flagged. Unknowns need guesses defined above (linear systems work from 0).

## 5. Vectors, matrices, ranges, tables

* Matrix literal: `[1, 2; 3, 4]` (commas separate columns, semicolons separate rows). Column vector `[1; 2; 3]`, row vector `[1, 2, 3]`.
* Indexing: `A[0, 1]` (row, column), `v[2]`; indices start at `ORIGIN` = 0.
* Operations: `A*B`, `A^-1`, `transpose(A)`, `det(A)`, `lsolve(A, b)`, `A*x`, elementwise through functions (`sqrt([1;4;9])`).
* Range variable: `i := 0..4` then `v[i] := i^2` builds the vector `[0;1;4;9;16]`; `v =` shows it.
  `x := 0, 0.25..1` gives `0, 0.25, 0.5, 0.75, 1`. An expression using a range variable is evaluated for every value:
  `x^2 =` shows a column of results (a "table").
* Range variables may appear in: indexed definitions (`v[i] := ...`), evaluations (`x^2 =`), and plots. They cannot be used as ordinary scalars in
  a plain definition (`y := x^2` with `x` a range is an error / ambiguous) — use `y[i] := ...` instead.

## 6. Plots

A plot region has an x expression (usually a range variable) and one or more y expressions.
Create it with `add_region` kind `plot`: `x_expr: "x"`, `y_exprs: ["sin(x)", "x^2"]`.
Define the range above the plot (`x := 0, 0.1..6.28`). If `x` is undefined, a quick-plot over -10..10 is used.
The server's result describes each series (label, point count, x/y range) — it cannot render images.

## 7. File format

`.xmath` is JSON: `{"version":1,"format":{...},"regions":[{"id":1,"x":20,"y":20,"type":"math","source":"L := 6 m"}, {"type":"text","text":"..."}, {"type":"plot","x":"x","y":["sin(x)"]}]}`.
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
