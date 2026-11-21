---
name: xmath
description: Use for engineering and math calculations that need physical units, symbolic algebra (simplify, expand, factor, solve, symbolic derivatives and integrals, limits, series), Mathcad-style worksheets, structural/mechanical/electrical formulas, matrices, integrals, derivatives, root finding and plots, and for creating or editing .xmath files. Drives the xmath calculation engine (a free Mathcad 15 clone) through its MCP server (tools evaluate, create_worksheet, add_region, open_worksheet) or the xmath-cli command line.
---

# xmath: unit-aware Mathcad-style calculations

xmath is a free, open-source Mathcad 15 clone written in Rust. A *worksheet* is a list of regions (math, text, plot)
evaluated top-to-bottom, then left-to-right. Math is written as plain linear text, units are first-class, and
incompatible units produce an error instead of a silent wrong number. Worksheets are saved as Mathcad 15 `.xmcd` files (the default format of the program) or xmath's own `.xmath` (JSON) files; every tool below that takes a worksheet path accepts both.

Prefer xmath over doing arithmetic in your head whenever units, many steps, matrices or numeric calculus are involved:
the result is computed exactly as Mathcad would and can be handed to the user as a reusable file.

Full syntax: [reference.md](reference.md) (also returned by the `syntax_help` MCP tool).

## Workflow with the MCP tools

Tools of the `xmath` MCP server (`claude mcp add xmath -- xmath-mcp`):

| Tool | Use |
|------|-----|
| `evaluate {lines}` | Run lines as a throw-away worksheet; returns `line -> result/error`. START HERE. |
| `create_worksheet {path, lines?, overwrite?}` | Create a `.xmcd`/`.xmath` file, one region per line in reading order: math lines, `# text` headings, `plot: f(x), g(x) vs x` plots. |
| `add_region {path, kind, source/text/x_expr+y_exprs, after_id? / beside_id?}` | Insert a math / text / plot region below (`after_id`) or to the right of (`beside_id`) an existing one. |
| `update_region {path, id, ...}`, `delete_region {path, id}` | Edit or remove a region by id. |
| `open_worksheet {path}` | Load and evaluate a file; lists all regions with ids, results, errors, plot summaries. |
| `list_open_windows` | Lists open xmath documents and their `target`. Use the selected target for the whole task. |
| `list_functions`, `list_units`, `syntax_help` | Catalogs and the syntax reference. |

**The user's open xmath window.** If the user has the xmath program open and asks you to work "in the open
window / in the open file / в открытом окне / в программе", do NOT start xmath, do NOT create or open a file:
call `list_open_windows`, select the requested document by title/path (the first is the most recently focused), and use
its `target` as the `path` for every call in this task. Read it with `open_worksheet`. Pass the returned
`revision` as `expected_revision` to the next edit, and use the new revision from each successful edit.
For `create_worksheet` on a non-empty window, use `overwrite: true` only when asked to replace the whole sheet. The changes appear in that window immediately, also in an unsaved document, and the user can undo
each of your edits with Ctrl+Z there. Saving stays the user's decision (Ctrl+S in the window).
If a write reports a conflict, read the same window again, compare the user's changes, and apply only the edits still needed.
If the window has closed, report that to the user.

Recommended procedure:

1. Translate the problem into xmath lines. Define all inputs with units first, then formulas, then result lines (`name =`).
2. Test with `evaluate`. Fix errors reported per line (they do not stop the other lines).
3. When the user wants a deliverable, write ONE `create_worksheet` call whose `lines` are already in the final reading
   order (see "Layout" below): headings, the inputs and formulas of a step, its plot (`plot: ...` line) right after
   them, then the results that interpret the plot. Re-read with `open_worksheet` to confirm there are no errors.
4. Report results to the user with units and mention the file path.

## Layout: a worksheet a teacher can read

The worksheet is read like a Mathcad sheet handed in by a student, top to bottom. The server places regions and
separates any that would overlap using their real drawn size (tall fractions, matrices, programs, matrix results,
plots) and keeps the evaluation order, so you never compute coordinates yourself. Your job is the ORDER:

* Group the sheet into steps. Each step: a `# heading` line, its inputs, its formulas, then (if useful) its plot,
  then the evaluations `name =` that read values off it. Never put all formulas first and all plots at the end:
  a plot belongs directly below (or beside) the definitions it shows, so it is clear which formulas it illustrates.
* In `create_worksheet`, write a plot as a line `plot: y1(x), y2(x) vs x` at that place (the range variable `x` must
  be defined on an earlier line). Several traces: separate them with commas.
* To put a plot or a result next to its formulas instead of below, use `add_region` with `beside_id` = the id of the
  region it belongs to (e.g. the plot beside the function definition, `f(2) =` beside the plot). Use `after_id` to
  insert below a given region; the regions after it move down automatically.
* Do not pass explicit `x`/`y` unless the user asks for a specific placement; one statement per region (line).
* Keep a heading text before every step (`# 2. Прогиб балки`), so the sheet reads like a report.

Paths are relative to the server's working directory; writing is only allowed for `.xmath` and `.xmcd` files. Editing an `.xmcd` re-imports it and saves the whole file again (plots as Mathcad plots with pictures, stored results), so you can work directly on Mathcad files; polar and 3D plots are written as pictures for Mathcad and restored by xmath.

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
x^2 - 4 = 0 -> solve, x       symbolic: [-2; 2] (also -> simplify / expand / factor / series, x, 6 ...)
diff(sin(x)*x, x) ->          symbolic derivative; integrate(x*exp(x), x) -> symbolic antiderivative
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

`create_worksheet` with the plot in its place:

```
# 1. Колебания
x := 0, 0.1..6.28
y(x) := sin(x)*exp(-x/5)
plot: y(x), cos(x) vs x
y(1) =
```

Or add one later: `add_region {kind: "plot", x_expr: "x", y_exprs: ["y(x)"], after_id: <id of y(x) :=>}` (below the
definition) or `beside_id` (to its right). The result lists each series: label, number of points, x/y range. (The
server returns data, not an image; the GUI renders it.)

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
`stmt if cond`, Mathcad's line form `stmt otherwise` (after an `if` line), `break`, `continue`, `return e`,
`e1 on error e2`. The value is the last EXECUTED statement (or `return`).
Mathcad semantics: every `if` line whose condition holds runs, so `{ 0 if x < 0; x^2 if x < 1; 2*x otherwise }` gives
`x^2` for x = -1, not 0. For exclusive branches use `return` (`{ return 0 if x < 0; return x^2 if x < 1; 2*x }`) or
non-overlapping conditions. Details in reference.md ("Programs").

## Export, PDF and files

* MCP `export_pdf {path, out}` renders a worksheet to PDF (A4 by default); `export_xmcd {path, out}` writes a Mathcad 15 file (X-Y plots as Mathcad plots, polar and 3D plots as pictures). CLI: `xmath-cli sheet.xmath --pdf out.pdf` / `--export-xmcd out.xmcd`.
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
- Transpose: `A^T` or `transpose(A)` (T stays a power only if you define a number `T`).
- `diff` / `integrate` / `root` take an expression and the variable name: `diff(x^2, x, 3)`.
- An error in one region never stops the others; read each line's `error` field.

## Fallback without MCP: the CLI

```
cargo install --path crates/xmath-cli      # or: cargo build --release  ->  target/release/xmath-cli
printf 'L := 6 m\nq := 12 kN/m\nM := q*L^2/8\nM = kN*m\n' > calc.txt
xmath-cli calc.txt                         # prints every line with its result; exit code 1 if any error
xmath-cli sheet.xmath                      # same for a saved worksheet
```

A `.txt` file is read as one region per non-blank line (`#` starts a text region). To deliver a worksheet,
run `xmath-cli calc.txt --convert calc.xmath`, then use `xmath-cli calc.xmath` to check the saved results.
