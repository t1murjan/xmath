//! Tests of the built-in functions and of the Russian error messages.

use num_complex::Complex64 as C64;
use xmath_core::eval::Evaluator;
use xmath_core::units::Dim;
use xmath_core::value::Value;
use xmath_core::{parse_expr, parse_statement, Stmt};

/// Runs definitions (all lines but the last) and evaluates the last line.
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

fn num(lines: &[&str]) -> f64 {
    match run(lines).unwrap() {
        Value::Num(n) => n.v.re,
        other => panic!("expected a scalar, got {other:?}"),
    }
}

fn mat(lines: &[&str]) -> Vec<Vec<f64>> {
    match run(lines).unwrap() {
        Value::Matrix(m) => (0..m.rows).map(|i| (0..m.cols).map(|j| m.get(i, j).v.re).collect()).collect(),
        other => panic!("expected a matrix, got {other:?}"),
    }
}

fn cmat(lines: &[&str]) -> Vec<Vec<(f64, f64)>> {
    match run(lines).unwrap() {
        Value::Matrix(m) => {
            (0..m.rows).map(|i| (0..m.cols).map(|j| (m.get(i, j).v.re, m.get(i, j).v.im)).collect()).collect()
        }
        other => panic!("expected a matrix, got {other:?}"),
    }
}

fn close(a: f64, b: f64, tol: f64) {
    assert!((a - b).abs() <= tol, "{a} != {b} (tol {tol})");
}

fn err(lines: &[&str]) -> String {
    run(lines).unwrap_err()
}

fn sub(m: &[Vec<f64>], c0: usize, c1: usize) -> Vec<Vec<f64>> {
    m.iter().map(|r| r[c0..c1].to_vec()).collect()
}

fn mul(a: &[Vec<f64>], b: &[Vec<f64>]) -> Vec<Vec<f64>> {
    (0..a.len())
        .map(|i| (0..b[0].len()).map(|j| (0..b.len()).map(|k| a[i][k] * b[k][j]).sum()).collect())
        .collect()
}

fn assert_mat(a: &[Vec<f64>], b: &[Vec<f64>], tol: f64) {
    assert_eq!((a.len(), a[0].len()), (b.len(), b[0].len()));
    for i in 0..a.len() {
        for j in 0..a[0].len() {
            close(a[i][j], b[i][j], tol);
        }
    }
}

// ---------------------------------------------------------------------------
// Linear algebra

#[test]
fn eigenvalues_symmetric_and_general() {
    let v = mat(&["M := [2, 1; 1, 2]", "eigenvals(M)"]);
    close(v[0][0], 1.0, 1e-12);
    close(v[1][0], 3.0, 1e-12);
    let v = mat(&["M := [4, 1; 2, 3]", "eigenvals(M)"]);
    close(v[0][0], 2.0, 1e-10);
    close(v[1][0], 5.0, 1e-10);
    // complex eigenvalues of a rotation
    let v = cmat(&["M := [0, -1; 1, 0]", "eigenvals(M)"]);
    close(v[0][0].0, 0.0, 1e-12);
    close(v[0][0].1, -1.0, 1e-12);
    close(v[1][0].1, 1.0, 1e-12);
    // complex matrix
    let v = cmat(&["M := [1, 2i; -2i, 1]", "eigenvals(M)"]);
    close(v[0][0].0, -1.0, 1e-10);
    close(v[1][0].0, 3.0, 1e-10);
    assert!(v[0][0].1 == 0.0 && v[1][0].1 == 0.0);
    // 4x4 non-symmetric with known spectrum (upper triangular, shuffled by similarity)
    let v = mat(&["M := [1, 2, 3, 4; 0, 2, 5, 6; 0, 0, 3, 7; 0, 0, 0, 4]", "eigenvals(M)"]);
    for (k, row) in v.iter().enumerate() {
        close(row[0], (k + 1) as f64, 1e-9);
    }
}

#[test]
fn eigenvectors() {
    // dense non-symmetric matrix (complex eigenpairs): A v = λ v, |v| = 1
    let m = [[2.0, 0.0, 1.0], [1.0, 3.0, 0.0], [0.0, 1.0, 4.0]];
    let defs = "M := [2, 0, 1; 1, 3, 0; 0, 1, 4]";
    let vals = cmat(&[defs, "eigenvals(M)"]);
    let vecs = cmat(&[defs, "eigenvecs(M)"]);
    for j in 0..3 {
        let lam = C64::new(vals[j][0].0, vals[j][0].1);
        let x: Vec<C64> = (0..3).map(|i| C64::new(vecs[i][j].0, vecs[i][j].1)).collect();
        close(x.iter().map(|z| z.norm_sqr()).sum::<f64>().sqrt(), 1.0, 1e-9);
        for i in 0..3 {
            let ax: C64 = (0..3).map(|k| x[k] * m[i][k]).sum();
            assert!((ax - lam * x[i]).norm() < 1e-8, "{ax} vs {}", lam * x[i]);
        }
    }
    // triangular matrix with real eigenvalues 2, 3, 5
    let v = mat(&["M := [2, 1, 0; 0, 3, 4; 0, 0, 5]", "eigenvecs(M)"]);
    close(v[0][0].abs(), 1.0, 1e-9);
    // symmetric: orthonormal columns
    let q = mat(&["M := [2, 1, 0; 1, 2, 1; 0, 1, 2]", "eigenvecs(M)"]);
    let qt: Vec<Vec<f64>> = (0..3).map(|i| (0..3).map(|j| q[j][i]).collect()).collect();
    assert_mat(&mul(&qt, &q), &[vec![1.0, 0.0, 0.0], vec![0.0, 1.0, 0.0], vec![0.0, 0.0, 1.0]], 1e-12);
    // a single eigenvector
    let v = mat(&["M := [4, 1; 2, 3]", "eigenvec(M, 5)"]);
    close(v[0][0] / v[1][0], 1.0, 1e-9);
    let v = mat(&["M := [4, 1; 2, 3]", "eigenvec(M, 2)"]);
    close(v[0][0] / v[1][0], -0.5, 1e-9);
    assert!(err(&["M := [4, 1; 2, 3]", "eigenvec(M, 7)"]).contains("собственным"));
    // repeated eigenvalue: independent vectors
    let q = mat(&["M := [2, 0; 0, 2]", "eigenvecs(M)"]);
    close((q[0][0] * q[1][1] - q[0][1] * q[1][0]).abs(), 1.0, 1e-9);
}

#[test]
fn eigenvalues_keep_units() {
    let Value::Matrix(m) = run(&["M := [4 m, 1 m; 1 m, 4 m]", "eigenvals(M)"]).unwrap() else { panic!() };
    assert_eq!(m.data[0].dim, xmath_core::units::LENGTH);
    close(m.data[1].v.re, 5.0, 1e-12);
    let Value::Matrix(m) = run(&["M := [4 m, 1 m; 1 m, 4 m]", "eigenvecs(M)"]).unwrap() else { panic!() };
    assert_eq!(m.data[0].dim, Dim::NONE);
    assert!(err(&["M := [4 m, 1 s; 1 m, 4 m]", "eigenvals(M)"]).contains("одинаковые единицы"));
}

#[test]
fn rank_and_rref() {
    close(num(&["rank([1, 2; 2, 4])"]), 1.0, 0.0);
    close(num(&["rank(identity(3))"]), 3.0, 0.0);
    close(num(&["rank([1, 2, 3; 4, 5, 6; 7, 8, 9])"]), 2.0, 0.0);
    close(num(&["rank([1, 2, 3; 4, 5, 6])"]), 2.0, 0.0);
    let r = mat(&["rref([1, 2, 3; 4, 5, 6; 7, 8, 10])"]);
    assert_mat(&r, &[vec![1.0, 0.0, 0.0], vec![0.0, 1.0, 0.0], vec![0.0, 0.0, 1.0]], 1e-12);
    let r = mat(&["rref([1, 2, 3; 2, 4, 6])"]);
    assert_mat(&r, &[vec![1.0, 2.0, 3.0], vec![0.0, 0.0, 0.0]], 1e-12);
}

#[test]
fn decompositions() {
    let l = mat(&["cholesky([4, 2; 2, 3])"]);
    assert_mat(&l, &[vec![2.0, 0.0], vec![1.0, 2f64.sqrt()]], 1e-12);
    assert!(err(&["cholesky([1, 2; 2, 1])"]).contains("положительно определённой"));

    let a = vec![vec![1.0, 2.0, 3.0], vec![4.0, 5.0, 6.0], vec![7.0, 8.0, 10.0]];
    let plu = mat(&["lu([1, 2, 3; 4, 5, 6; 7, 8, 10])"]);
    let (p, l, u) = (sub(&plu, 0, 3), sub(&plu, 3, 6), sub(&plu, 6, 9));
    assert_mat(&mul(&p, &a), &mul(&l, &u), 1e-12);
    close(l[0][0], 1.0, 0.0);
    close(l[0][1], 0.0, 0.0);
    close(u[1][0], 0.0, 0.0);

    let a = vec![vec![1.0, 2.0], vec![3.0, 4.0], vec![5.0, 6.0]];
    let qr = mat(&["qr([1, 2; 3, 4; 5, 6])"]);
    let (q, r) = (sub(&qr, 0, 3), sub(&qr, 3, 5));
    assert_mat(&mul(&q, &r), &a, 1e-12);
    let qt: Vec<Vec<f64>> = (0..3).map(|i| (0..3).map(|j| q[j][i]).collect()).collect();
    assert_mat(&mul(&qt, &q), &[vec![1.0, 0.0, 0.0], vec![0.0, 1.0, 0.0], vec![0.0, 0.0, 1.0]], 1e-12);
    close(r[2][1], 0.0, 1e-13);
}

#[test]
fn singular_values() {
    let s = mat(&["svds([1, 2; 3, 4; 5, 6])"]);
    close(s[0][0], 9.525_518_091_565_104, 1e-10);
    close(s[1][0], 0.514_300_580_658_642, 1e-10);
    let a = vec![vec![1.0, 2.0], vec![3.0, 4.0], vec![5.0, 6.0]];
    let uv = mat(&["svd([1, 2; 3, 4; 5, 6])"]);
    assert_eq!((uv.len(), uv[0].len()), (5, 2));
    let (u, v) = (sub(&uv[..3], 0, 2), sub(&uv[3..], 0, 2));
    let mut us = u.clone();
    for row in &mut us {
        row[0] *= s[0][0];
        row[1] *= s[1][0];
    }
    let vt: Vec<Vec<f64>> = (0..2).map(|i| (0..2).map(|j| v[j][i]).collect()).collect();
    assert_mat(&mul(&us, &vt), &a, 1e-10);
    // wide matrix
    let s = mat(&["svds([1, 2, 3; 4, 5, 6])"]);
    close(s[0][0], 9.508_032_000_695_724, 1e-10);
    close(s[1][0], 0.772_869_635_673_484, 1e-10);
    close(num(&["cond2([1, 0; 0, 0.5])"]), 2.0, 1e-12);
    close(num(&["norm2([3, 0; 0, 4])"]), 4.0, 1e-12);
    assert!(err(&["cond2([1, 2; 2, 4])"]).contains("вырождена"));
}

#[test]
fn pseudo_inverse_and_norms() {
    let g = mat(&["A := [1, 2; 3, 4; 5, 6]", "geninv(A)*A"]);
    assert_mat(&g, &[vec![1.0, 0.0], vec![0.0, 1.0]], 1e-10);
    let g = mat(&["A := [1, 2; 3, 4]", "geninv(A)"]);
    assert_mat(&g, &[vec![-2.0, 1.0], vec![1.5, -0.5]], 1e-10);
    close(num(&["norm1([1, -2; 3, 4])"]), 6.0, 1e-14);
    close(num(&["normi([1, -2; 3, 4])"]), 7.0, 1e-14);
    let Value::Num(n) = run(&["normi([1 m, -2 m; 3 m, 4 m])"]).unwrap() else { panic!() };
    assert_eq!(n.dim, xmath_core::units::LENGTH);
}

#[test]
fn diag_and_sorting() {
    assert_mat(&mat(&["diag([1; 2; 3])"]), &[vec![1.0, 0.0, 0.0], vec![0.0, 2.0, 0.0], vec![0.0, 0.0, 3.0]], 0.0);
    let m = mat(&["csort([3, 1; 1, 2; 2, 3], 0)"]);
    assert_mat(&m, &[vec![1.0, 2.0], vec![2.0, 3.0], vec![3.0, 1.0]], 0.0);
    let m = mat(&["ORIGIN := 1", "csort([3, 1; 1, 2; 2, 3], 2)"]);
    assert_mat(&m, &[vec![3.0, 1.0], vec![1.0, 2.0], vec![2.0, 3.0]], 0.0);
    let m = mat(&["rsort([3, 1, 2; 4, 5, 6], 0)"]);
    assert_mat(&m, &[vec![1.0, 2.0, 3.0], vec![5.0, 6.0, 4.0]], 0.0);
}

// ---------------------------------------------------------------------------
// Interpolation and regression

#[test]
fn splines() {
    let xs = "vx := [0; 1; 2; 3; 4]";
    // cubic end conditions reproduce a cubic exactly
    let ys = "vy := [0; 1; 8; 27; 64]";
    close(num(&[xs, ys, "interp(cspline(vx, vy), vx, vy, 2.5)"]), 15.625, 1e-10);
    close(num(&[xs, ys, "interp(cspline(vx, vy), vx, vy, 3.5)"]), 42.875, 1e-10);
    // parabolic ends reproduce a parabola exactly
    let ys = "vy := [0; 1; 4; 9; 16]";
    close(num(&[xs, ys, "interp(pspline(vx, vy), vx, vy, 1.5)"]), 2.25, 1e-10);
    close(num(&[xs, ys, "interp(pspline(vx, vy), vx, vy, -1)"]), 1.0, 1e-9);
    // all splines pass through the knots; lspline is linear beyond the ends
    for f in ["lspline", "pspline", "cspline"] {
        for (x, y) in [(0.0, 0.0), (2.0, 4.0), (4.0, 16.0)] {
            close(num(&[xs, ys, &format!("interp({f}(vx, vy), vx, vy, {x})")]), y, 1e-10);
        }
    }
    let a = num(&[xs, ys, "interp(lspline(vx, vy), vx, vy, 5)"]);
    let b = num(&[xs, ys, "interp(lspline(vx, vy), vx, vy, 6)"]);
    let c = num(&[xs, ys, "interp(lspline(vx, vy), vx, vy, 7)"]);
    close(b - a, c - b, 1e-10);
    // natural spline of a smooth function is accurate
    close(num(&["vx := [0; 1; 2; 3]", "vy := [0; 1; 0; -1]", "interp(lspline(vx, vy), vx, vy, 0.5)"]), 0.7, 1e-10);
    // layout of the coefficient vector
    let v = mat(&[xs, ys, "lspline(vx, vy)"]);
    assert_eq!(v.len(), 8);
    close(v[0][0], 1.0, 0.0);
    close(v[1][0], 5.0, 0.0);
    close(v[3][0], 0.0, 1e-14);
    // two points: a straight line
    close(num(&["interp(cspline([0; 2], [1; 5]), [0; 2], [1; 5], 1)"]), 3.0, 1e-12);
    // units
    let Value::Num(n) = run(&["vx := [0 s; 1 s; 2 s]", "vy := [0 m; 1 m; 4 m]", "interp(cspline(vx, vy), vx, vy, 1.5 s)"])
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(n.dim, xmath_core::units::LENGTH);
    close(n.v.re, 2.25, 1e-10);
    assert!(err(&["vx := [0 s; 1 s; 2 s]", "vy := [0 m; 1 m; 4 m]", "interp(cspline(vx, vy), vx, vy, 1.5 m)"])
        .contains("Единицы"));
}

#[test]
fn regression() {
    let d = ["vx := [0; 1; 2; 3; 4]", "vy := [1; 6; 17; 34; 57]"]; // 1 + 2x + 3x^2
    let c = mat(&[d[0], d[1], "regress(vx, vy, 2)"]);
    assert_eq!(c.len(), 6);
    close(c[0][0], 4.0, 0.0);
    close(c[3][0], 1.0, 1e-10);
    close(c[4][0], 2.0, 1e-10);
    close(c[5][0], 3.0, 1e-10);
    close(num(&[d[0], d[1], "interp(regress(vx, vy, 2), vx, vy, 1.5)"]), 10.75, 1e-9);
    // linear data
    let l = ["x := [1; 2; 3; 4]", "y := [3; 5; 7; 9]"];
    close(num(&[l[0], l[1], "slope(x, y)"]), 2.0, 1e-12);
    close(num(&[l[0], l[1], "intercept(x, y)"]), 1.0, 1e-12);
    let v = mat(&[l[0], l[1], "line(x, y)"]);
    close(v[0][0], 1.0, 1e-12);
    close(v[1][0], 2.0, 1e-12);
    close(num(&[l[0], l[1], "corr(x, y)"]), 1.0, 1e-12);
    close(num(&["corr([1; 2; 3; 4], [9; 7; 5; 3])"]), -1.0, 1e-12);
    close(num(&["corr([1; 2; 3; 4; 5], [2; 1; 4; 3; 5])"]), 0.8, 1e-12);
    let Value::Num(s) = run(&["slope([1 s; 2 s; 3 s], [2 m; 4 m; 6 m])"]).unwrap() else { panic!() };
    assert_eq!(s.dim, xmath_core::units::VELOCITY);
    assert!(err(&["regress([1; 2], [1; 2], 2)"]).contains("степень"));
}

// ---------------------------------------------------------------------------
// Statistics and misc scalar functions

#[test]
fn statistics() {
    close(num(&["cvar([1; 2; 3; 4], [2; 4; 6; 8])"]), 2.5, 1e-12);
    close(num(&["skew([1; 2; 3; 4; 10])"]), 1.697_056_274_847_714, 1e-12);
    close(num(&["kurt([1; 2; 3; 4; 10])"]), 3.152, 1e-12);
    close(num(&["gmean([1; 2; 3; 4; 10])"]), 2.992_555_739_477_689_6, 1e-12);
    close(num(&["hmean([1; 2; 3; 4; 10])"]), 2.290_076_335_877_863, 1e-12);
    close(num(&["gmean(2, 8)"]), 4.0, 1e-12);
    close(num(&["mode([1; 2; 2; 3; 3; 3])"]), 3.0, 0.0);
    assert!(err(&["mode([1; 2; 3])"]).contains("моды"));
    let Value::Num(g) = run(&["gmean(2 m, 8 m)"]).unwrap() else { panic!() };
    assert_eq!(g.dim, xmath_core::units::LENGTH);
    assert!(err(&["gmean(1, -1)"]).contains("положительными"));
    let h = mat(&["hist([0; 1; 2; 3], [0.5; 1.5; 1.5; 2.5; 3])"]);
    assert_mat(&h, &[vec![1.0], vec![2.0], vec![1.0]], 0.0);
    let h = mat(&["hist(2, [1; 2; 3; 4])"]);
    assert_mat(&h, &[vec![2.0], vec![2.0]], 0.0);
}

#[test]
fn scalar_helpers() {
    close(num(&["gcd(12, 18)"]), 6.0, 0.0);
    close(num(&["gcd(12, 18, 8)"]), 2.0, 0.0);
    close(num(&["lcm(4, 6)"]), 12.0, 0.0);
    close(num(&["lcm(4, 6, 10)"]), 60.0, 0.0);
    close(num(&["Φ(-1)"]), 0.0, 0.0);
    close(num(&["Φ(0)"]), 1.0, 0.0);
    close(num(&["heaviside(3 m)"]), 1.0, 0.0);
    close(num(&["δ(2, 2)"]), 1.0, 0.0);
    close(num(&["kronecker(1, 2)"]), 0.0, 0.0);
    close(num(&["sinc(0)"]), 1.0, 0.0);
    close(num(&["sinc(π/2)"]), 2.0 / std::f64::consts::PI, 1e-14);
    close(num(&["csgn(-2)"]), -1.0, 0.0);
    close(num(&["csgn(2i)"]), 1.0, 0.0);
    close(num(&["csgn(0)"]), 0.0, 0.0);
}

#[test]
fn distributions() {
    close(num(&["qnorm(0.975, 0, 1)"]), 1.959_963_984_540_054, 1e-12);
    close(num(&["qnorm(0.5, 3, 2)"]), 3.0, 1e-14);
    close(num(&["pnorm(qnorm(0.123, 1, 2), 1, 2)"]), 0.123, 1e-13);
    assert!(err(&["qnorm(1.5, 0, 1)"]).contains("между 0 и 1"));
    close(num(&["dunif(0.5, 0, 2)"]), 0.5, 1e-15);
    close(num(&["dunif(3, 0, 2)"]), 0.0, 1e-15);
    close(num(&["punif(0.5, 0, 2)"]), 0.25, 1e-15);
    close(num(&["punif(5, 0, 2)"]), 1.0, 1e-15);
    close(num(&["dexp(1, 2)"]), 2.0 * (-2f64).exp(), 1e-15);
    close(num(&["pexp(1, 2)"]), 1.0 - (-2f64).exp(), 1e-15);
    close(num(&["pexp(-1, 2)"]), 0.0, 1e-15);
}

#[test]
fn bessel_functions() {
    close(num(&["J0(1)"]), 0.765_197_686_557_966_6, 1e-14);
    close(num(&["J1(1)"]), 0.440_050_585_744_933_5, 1e-14);
    close(num(&["Jn(2, 1)"]), 0.114_903_484_931_900_5, 1e-14);
    close(num(&["Y0(1)"]), 0.088_256_964_215_676_96, 1e-14);
    close(num(&["Y1(1)"]), -0.781_212_821_300_288_7, 1e-13);
    close(num(&["Yn(2, 1)"]), -1.650_682_606_816_254_6, 1e-13);
    close(num(&["I0(1)"]), 1.266_065_877_752_008_4, 1e-14);
    close(num(&["I1(1)"]), 0.565_159_103_992_485, 1e-14);
    close(num(&["In(2, 2)"]), 0.688_948_447_698_738_2, 1e-13);
    close(num(&["K0(1)"]), 0.421_024_438_240_708_3, 1e-13);
    close(num(&["K1(1)"]), 0.601_907_230_197_234_6, 1e-13);
    close(num(&["Kn(2, 3)"]), 0.061_510_458_471_742_04, 1e-13);
    close(num(&["J0(0)"]), 1.0, 0.0);
    assert!(err(&["Y0(-1)"]).contains("положительным"));
    assert!(err(&["J0(1 m)"]).contains("безразмерным"));
    // elementwise on vectors
    let v = mat(&["J0([0; 1])"]);
    close(v[1][0], 0.765_197_686_557_966_6, 1e-14);
}

// ---------------------------------------------------------------------------
// Strings and lookup

#[test]
fn strings() {
    let Value::Str(s) = run(&["substr(\"hello world\", 6, 5)"]).unwrap() else { panic!() };
    assert_eq!(&*s, "world");
    let Value::Str(s) = run(&["ORIGIN := 1", "substr(\"hello\", 1, 2)"]).unwrap() else { panic!() };
    assert_eq!(&*s, "he");
    close(num(&["search(\"hello\", \"l\", 0)"]), 2.0, 0.0);
    close(num(&["search(\"hello\", \"l\", 3)"]), 3.0, 0.0);
    close(num(&["search(\"hello\", \"lo\", 0)"]), 3.0, 0.0);
    close(num(&["search(\"hello\", \"z\", 0)"]), -1.0, 0.0);
    close(num(&["ORIGIN := 1", "search(\"hello\", \"h\", 1)"]), 1.0, 0.0);
    close(num(&["str2num(\"3.5\")"]), 3.5, 0.0);
    close(num(&["str2num(\"2 + 3\")"]), 5.0, 0.0);
    assert!(err(&["str2num(\"abc +\")"]).contains("не удалось"));
    let v = mat(&["str2vec(\"AB\")"]);
    assert_mat(&v, &[vec![65.0], vec![66.0]], 0.0);
    let Value::Str(s) = run(&["vec2str(str2vec(\"Привет\"))"]).unwrap() else { panic!() };
    assert_eq!(&*s, "Привет");
}

#[test]
fn lookups() {
    let a = "A := [1, 10; 2, 20; 3, 30]";
    close(num(&["lookup(2, [1; 2; 3], [10; 20; 30])"]), 20.0, 0.0);
    close(num(&["lookup(2.9, [1; 2; 3], [10; 20; 30])"]), 30.0, 0.0);
    close(num(&[a, "vlookup(2, A, 1)"]), 20.0, 0.0);
    close(num(&["B := [1, 2, 3; 4, 5, 6]", "hlookup(2, B, 1)"]), 5.0, 0.0);
    close(num(&["ORIGIN := 1", a, "vlookup(3, A, 2)"]), 30.0, 0.0);
    close(num(&["match(5, [3; 5; 7])"]), 1.0, 0.0);
    close(num(&["ORIGIN := 1", "match(5, [3; 5; 7])"]), 2.0, 0.0);
    let v = mat(&["match(2, [2; 1; 2])"]);
    assert_mat(&v, &[vec![0.0], vec![2.0]], 0.0);
    let v = mat(&["match(4, [1, 2; 3, 4])"]);
    assert_mat(&v, &[vec![1.0, 1.0]], 0.0);
    assert!(err(&["match(9, [1; 2])"]).contains("не найдено"));
    assert!(err(&["vlookup(1, [1, 2; 3, 4], 5)"]).contains("индекс"));
}

// ---------------------------------------------------------------------------
// ODE

#[test]
fn ode_solvers() {
    let sho = ["D(x, y) := [y[1]; -y[0]]", "y0 := [1; 0]"];
    let r = mat(&[sho[0], sho[1], "rkfixed(y0, 0, π, 200, D)"]);
    assert_eq!((r.len(), r[0].len()), (201, 3));
    close(r[0][0], 0.0, 0.0);
    close(r[200][0], std::f64::consts::PI, 1e-12);
    close(r[200][1], -1.0, 1e-8);
    close(r[100][1], 0.0, 1e-8);
    close(r[100][2], -1.0, 1e-8);
    let r = mat(&[sho[0], sho[1], "Rkadapt(y0, 0, π, 10, D)"]);
    assert_eq!(r.len(), 11);
    close(r[10][1], -1.0, 1e-6);
    close(r[5][2], -1.0, 1e-6);
    // scalar equation y' = y
    close(mat(&["f(x, y) := y", "rkfixed(1, 0, 1, 100, f)"])[100][1], std::f64::consts::E, 1e-8);
    close(mat(&["f(x, y) := y", "Rkadapt(1, 0, 1, 4, f)"])[4][1], std::f64::consts::E, 1e-6);
    assert!(err(&["D(x, y) := 1", "rkfixed([1; 2], 0, 1, 10, D)"]).contains("вектор"));
    assert!(err(&["rkfixed([1], 0, 1, 10, 5)"]).contains("функцией"));
}

// ---------------------------------------------------------------------------
// Russian messages

#[test]
fn russian_errors() {
    assert_eq!(err(&["x + 1"]), "'x' не определена");
    assert_eq!(err(&["1 m + 1 s"]), "Единицы измерения не совпадают");
    assert_eq!(err(&["1 / 0"]), "Деление на ноль");
    assert_eq!(err(&["[1, 2; 2, 4]^-1"]), "Матрица вырождена");
    assert_eq!(err(&["lsolve([1, 2; 2, 4], [1; 2])"]), "lsolve: Матрица вырождена");
    assert!(err(&["foo(1)"]).contains("не определена"));
    assert!(err(&["f(x) := x", "f(1, 2)"]).contains("нужно аргументов: 1, передано: 2"));
    assert!(err(&["sin(1 m)"]).contains("безразмерным"));
    assert!(err(&["[1, 2; 3, 4] + [1, 2]"]).contains("нельзя сложить"));
    assert!(err(&["v := [1; 2]", "v[5]"]).contains("вне допустимых границ"));
    assert!(err(&["root(x^2 + 1, x, 0, 1)"]).contains("одинаковый знак"));
    let e = parse_statement("x := ■").unwrap_err();
    assert_eq!(e.msg, "Заполните поле ввода ■");
    assert!(parse_statement("x := (1 + ").unwrap_err().msg.contains("конец выражения")
        || parse_statement("x := (1 + ").unwrap_err().msg.contains("незаконченное"));
    assert!(parse_statement("x := 1 +* 2").unwrap_err().msg.contains("неожиданно"));
    assert!(parse_statement("[1, 2; 3]").unwrap_err().msg.contains("разную длину"));
}

#[test]
fn all_new_names_are_registered() {
    for name in [
        "eigenvals", "eigenvecs", "eigenvec", "rank", "cholesky", "lu", "qr", "svd", "svds", "geninv", "rref",
        "norm1", "norm2", "normi", "cond2", "diag", "csort", "rsort", "lspline", "pspline", "cspline", "interp",
        "slope", "intercept", "line", "corr", "regress", "cvar", "skew", "kurt", "hist", "gmean", "hmean", "mode",
        "gcd", "lcm", "Φ", "heaviside", "δ", "kronecker", "sinc", "csgn", "qnorm", "dunif", "punif", "dexp",
        "pexp", "J0", "J1", "Jn", "Y0", "Y1", "Yn", "I0", "I1", "In", "K0", "K1", "Kn", "substr", "search",
        "str2num", "vec2str", "str2vec", "lookup", "match", "vlookup", "hlookup", "rkfixed", "Rkadapt",
    ] {
        assert!(xmath_core::builtins::NAMES.contains(&name), "{name} is missing from NAMES");
    }
}
