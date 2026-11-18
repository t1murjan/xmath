---
name: xmath
description: Use for engineering and math calculations that need physical units, Mathcad-style worksheets, structural/mechanical/electrical formulas, matrices, integrals, derivatives, root finding and plots, and for creating or editing .xmath files. Drives the xmath calculation engine (a free Mathcad 15 clone) through its MCP server (tools evaluate, create_worksheet, add_region, open_worksheet) or the xmath-cli command line.
---

# xmath: unit-aware Mathcad-style calculations

xmath is a free, open-source Mathcad 15 clone written in Rust. A *worksheet* is a list of regions (math, text, plot)
evaluated top-to-bottom, then left-to-right. Math is written as plain linear text, units are first-class, and
incompatible units produce an error instead of a silent wrong number. Worksheets are saved as `.xmath` (JSON) files.

Prefer xmath over doing arithmetic in your head whenever units, many steps, matrices or numeric calculus are involved:
the result is computed exactly as Mathcad would and can be handed to the user as a reusable file.

Full syntax: [reference.md](reference.md) (also returned by the `syntax_help` MCP tool).

## Workflow with the MCP tools

Tools of the `xmath` MCP server (`claude mcp add xmath -- xmath-mcp`):

| Tool | Use |
|------|-----|
| `evaluate {lines}` | Run lines as a throw-away worksheet; returns `line -> result/error`. START HERE. |
| `create_worksheet {path, lines?, overwrite?}` | Create a `.xmath` file, one math region per line (`# text` lines become comments). |
| `add_region {path, kind, source/text/x_expr+y_exprs, after_id?}` | Append or insert a math / text / plot region. |
| `update_region {path, id, ...}`, `delete_region {path, id}` | Edit or remove a region by id. |
| `open_worksheet {path}` | Load and evaluate a file; lists all regions with ids, results, errors, plot summaries. |
| `list_functions`, `list_units`, `syntax_help` | Catalogs and the syntax reference. |

Recommended procedure:

1. Translate the problem into xmath lines. Define all inputs with units first, then formulas, then result lines (`name =`).
2. Test with `evaluate`. Fix errors reported per line (they do not stop the other lines).
3. When the user wants a deliverable, `create_worksheet` with the verified lines (add `# ...` comment lines for headings),
   then `add_region` for extras such as plots. Re-read with `open_worksheet` to confirm there are no errors.
4. Report results to the user with units and mention the file path.

Paths are relative to the server's working directory; writing is only allowed for `.xmath` files.

## Syntax in one screen

```
L := 6 m                      definition (local)
g0 ≡ 9.81 m/s^2               global definition (also ::=)
f(x) := x^2 + 1               function
f(3) =                        evaluate -> f(3) = 10
M = kN*m                      evaluate and display in a unit
i := 0..10    x := 0, 0.1..1  range variables (step 1 / step 0.1)
v[i] := i^2                   indexed definition -> vector
d := L/n =    d := L/n = mm   definition that also shows its value (optionally in a unit)
[a; b; c] := M*v              vector definition (RHS must be a vector of the same length)
M^<1>                         matrix column (from ORIGIN) -> column vector
vectorize(v*w)  vectorize(f(v))  element-wise evaluation (Mathcad arrow); plain v*w is a dot product
A := [1, 2; 3, 4]             matrix (commas: columns, semicolons: rows)
A[0, 1] =  det(A) =  A^-1 =   index from ORIGIN = 0
sum(k, 1, 10, k^2) =          385
integrate(exp(-x^2), x, -∞, ∞) =
diff(sin(x), x, 0) =          derivative at a point
root(x^2 - 2, x, 0, 2) =      1.414
lsolve(A, b) =                linear system
if(x > 0, 1, -1)
# any line starting with # is a comment (text region)
```

## Worked examples

### 1. Beam with a uniform load (units)

```
L := 6 m
q := 12 kN/m
M := q*L^2/8
M = kN*m               ->  M = 54 kN*m
V := q*L/2
V = kN                 ->  V = 36 kN
E := 210 GPa
I := 5000 cm^4
f := 5*q*L^4/(384*E*I)
f = mm                 ->  deflection in mm
```

Inputs carry units, so the final `= kN*m` / `= mm` just selects the display unit. If it complains about units, a
formula is dimensionally wrong: fix the formula, do not strip units.

### 2. Stress and unit conversion

```
F := 150 kN
A := 25 cm * 30 cm
sigma := F/A
sigma = MPa            ->  2 MPa
1 in = mm              ->  25.4 mm
10 kg * g = N          ->  98.067 N      (g is gravity)
```

### 3. Functions and tables (range variables)

```
x := 0, 0.5..2
f(x) := x^2 + 1
f(x) =                 ->  column of values (a table)
i := 0..4
v[i] := f(i)
v =                    ->  vector
sum(k, 0, 4, f(k)) =
```

A range variable can only be used in an indexed definition (`v[i] := ...`), in an evaluation (`f(x) =`) or in a plot.
`y := x^2` with a range `x` is an error: write `y[i] := ...`.

### 4. Matrices and linear systems

```
A := [2, 1; 1, 3]
b := [1; 2]
x := lsolve(A, b)
x =                    ->  [0.2; 0.6]
det(A) =               ->  5
A^-1 =
A*x =
polyroots([-6, 1, 1]) =   ->  [-3; 2]   (coefficients from the constant term up)
```

### 5. Calculus and roots

```
integrate(x^2, x, 0, 3) =           9
integrate(exp(-x^2), x, -∞, ∞) =    1.772
diff(x^3, x, 2) =                   12
root(cos(x) - x, x, 0, 1) =         0.739
```

Always pass an expression in `x` (`f(x)`), not a bare function name `f`, to `integrate`, `diff` and `root`.

### Solve blocks

Systems of equations: guesses, `Given` on its own line, constraints, then `Find`:

```
x := 1
y := 1
Given
x^2 + y^2 == 5
x + y == 3
Find(x, y) =
```

`Minerr(x, y)` gives a least-squares answer for inconsistent systems; `Minimize(f, x)` / `Maximize(f, x)` optimize a user function `f(x)`; `r(a) := Find(x)` defines a function of a solve block. Put guesses above `Given`, one region (line) per constraint, `Given` alone on its line; vector unknowns (`Q := [1; 1]`, `Find(Q)`) work too.

### 6. Plot (via MCP)

1. `create_worksheet` with lines `["x := 0, 0.1..6.28"]`.
2. `add_region` `{kind: "plot", x_expr: "x", y_exprs: ["sin(x)", "cos(x)"]}`.
3. The result lists each series: label, number of points, x/y range. (The server returns data, not an image; the GUI renders it.)

### 7. Complex numbers, electrical

```
Z := 3 + 4i
abs(Z) =            5
arg(Z) =            0.927 (radians)
R := 100 Ω
C := 10 μF
```
One line = one statement: never put `R := 100 Ω   C := 10 μF` on a single line.

### 8. Programs (loops, conditions)

```
sumsq(n) := { s ← 0; for i ∈ 1..n { s ← s + i^2 }; s }
sumsq(10) =                          385
g(x) := { return -1 if x < 0; sqrt(x) }
newton(a) := { x ← a; while abs(x^2 - a) > 1e-12 { x ← (x + a/x)/2 }; x }
safe(x) := { y ← 1/x on error 0; y }
h(x) := { if x < 0 { "neg" } else if x == 0 { "zero" } otherwise { "pos" } }
```

Programs are `{ stmt; stmt; ... }` expressions: `←` assigns a local, `for i ∈ a..b { }`, `while c { }`, `if c { } otherwise { }`,
`stmt if cond`, `break`, `continue`, `return e`, `e1 on error e2`. The value is the last statement (or `return`).
Details in reference.md ("Programs").

## Export, PDF and files

* MCP `export_pdf {path, out}` renders a worksheet to PDF (A4 by default); `export_xmcd {path, out}` writes a Mathcad 15 file (plots become text notes). CLI: `xmath-cli sheet.xmath --pdf out.pdf` / `--export-xmcd out.xmcd`.
* File functions (paths relative to the worksheet folder): `READPRN("a.prn")`, `READFILE("a.csv","delimited")`, `READCSV`, `READEXCEL("a.xlsx","Sheet1!A1:C10")`; writing is a special definition: `WRITEPRN("out.prn") := M`, `APPENDPRN(...)`, `WRITECSV(...)`. Writes never leave the worksheet folder (no `..`); the MCP server never writes while evaluating.
* A reference region (Insert -> Reference) evaluates another .xmath/.xmcd first and exposes its definitions below it.
* Details: docs/export-print.md.

## Common pitfalls

- `g` is gravity (9.807 m/s^2); grams are `gm`. `10 gm = 0.01 kg`.
- Unit mismatch (`5 m + 3 s`, `sin(5 m)`) is an error. Angles: `sin(30 deg) = 0.5`; without `deg` the argument is radians.
- Implicit multiplication works (`5 kN`, `2 y`) but prefer an explicit `*` between variables: `a*b`.
- Evaluation order is by position: a definition must come BEFORE its use (create/append in logical order).
- Indices start at 0 (`ORIGIN = 0`); `A[0, 0]` is the first element.
- Results show 3 decimals; request another unit with `expr = unit`. Large/small numbers print as `a·10^n`.
- Use `^` for powers (not `**`), `:=` for definitions, `=` only to display. No LaTeX, no `$`.
- `i` is the imaginary unit; do not use it as a range variable together with complex numbers (use `j`, `k`, `n`).
- `diff` / `integrate` / `root` take an expression and the variable name: `diff(x^2, x, 3)`.
- An error in one region never stops the others; read each line's `error` field.

## Fallback without MCP: the CLI

```
cargo install --path crates/xmath-cli      # or: cargo build --release  ->  target/release/xmath-cli
printf 'L := 6 m\nq := 12 kN/m\nM := q*L^2/8\nM = kN*m\n' > calc.txt
xmath-cli calc.txt                         # prints every line with its result; exit code 1 if any error
xmath-cli sheet.xmath                      # same for a saved worksheet
```

A `.txt` file is read as one math region per line (`#` lines are comments). To produce a `.xmath` file without MCP, write
JSON by hand: `{"version":1,"regions":[{"id":1,"x":20,"y":20,"type":"math","source":"L := 6 m"}, ...]}` with increasing `y`
(about 30 per line) and unique `id`s.
