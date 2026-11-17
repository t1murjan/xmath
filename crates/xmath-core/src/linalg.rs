//! Dense linear algebra on plain `Vec<Vec<_>>` matrices: eigenvalue problems,
//! decompositions, least squares.

use crate::value::EResult;
use num_complex::Complex64 as C64;

pub type CMat = Vec<Vec<C64>>;
pub type RMat = Vec<Vec<f64>>;

fn czero() -> C64 {
    C64::new(0.0, 0.0)
}

pub fn max_abs(a: &CMat) -> f64 {
    a.iter().flatten().map(|z| z.norm()).fold(0.0, f64::max)
}

pub fn is_real(a: &CMat) -> bool {
    a.iter().flatten().all(|z| z.im == 0.0)
}

pub fn is_symmetric(a: &CMat) -> bool {
    let n = a.len();
    let tol = 1e-12 * max_abs(a);
    is_real(a) && (0..n).all(|i| (0..i).all(|j| (a[i][j] - a[j][i]).norm() <= tol))
}

pub fn is_hermitian(a: &CMat) -> bool {
    let n = a.len();
    let tol = 1e-12 * max_abs(a);
    (0..n).all(|i| (0..=i).all(|j| (a[i][j] - a[j][i].conj()).norm() <= tol))
}

fn real_identity(n: usize) -> RMat {
    (0..n).map(|i| (0..n).map(|j| if i == j { 1.0 } else { 0.0 }).collect()).collect()
}

// ---------------------------------------------------------------------------
// Eigenvalues

/// Cyclic Jacobi method for a real symmetric matrix: ascending eigenvalues and
/// the matching orthonormal eigenvectors (as columns of the returned matrix).
pub fn jacobi_symmetric(a: &RMat) -> (Vec<f64>, RMat) {
    let n = a.len();
    let mut a = a.clone();
    let mut v = real_identity(n);
    let total: f64 = a.iter().flatten().map(|x| x * x).sum();
    for _ in 0..100 {
        let mut off = 0.0;
        for i in 0..n {
            for j in i + 1..n {
                off += a[i][j] * a[i][j];
            }
        }
        if off <= 1e-32 * total || off == 0.0 {
            break;
        }
        for p in 0..n {
            for q in p + 1..n {
                if a[p][q].abs() < 1e-300 {
                    continue;
                }
                let theta = (a[q][q] - a[p][p]) / (2.0 * a[p][q]);
                let t = theta.signum() / (theta.abs() + (theta * theta + 1.0).sqrt());
                let c = 1.0 / (t * t + 1.0).sqrt();
                let s = t * c;
                for k in 0..n {
                    let (akp, akq) = (a[k][p], a[k][q]);
                    a[k][p] = c * akp - s * akq;
                    a[k][q] = s * akp + c * akq;
                }
                for k in 0..n {
                    let (apk, aqk) = (a[p][k], a[q][k]);
                    a[p][k] = c * apk - s * aqk;
                    a[q][k] = s * apk + c * aqk;
                }
                for row in v.iter_mut() {
                    let (vp, vq) = (row[p], row[q]);
                    row[p] = c * vp - s * vq;
                    row[q] = s * vp + c * vq;
                }
            }
        }
    }
    let mut idx: Vec<usize> = (0..n).collect();
    idx.sort_by(|&i, &j| a[i][i].total_cmp(&a[j][j]));
    let vals = idx.iter().map(|&i| a[i][i]).collect();
    let mut vecs = vec![vec![0.0; n]; n];
    for (newj, &oldj) in idx.iter().enumerate() {
        // sign convention: the largest component is positive
        let mut big = 0;
        for k in 0..n {
            if v[k][oldj].abs() > v[big][oldj].abs() * (1.0 + 1e-9) {
                big = k;
            }
        }
        let sgn = if v[big][oldj] < 0.0 { -1.0 } else { 1.0 };
        for k in 0..n {
            vecs[k][newj] = sgn * v[k][oldj];
        }
    }
    (vals, vecs)
}

/// Reduces `a` to upper Hessenberg form by Householder similarity transforms.
fn hessenberg(a: &mut CMat) {
    let n = a.len();
    for k in 0..n.saturating_sub(2) {
        let norm = (k + 1..n).map(|i| a[i][k].norm_sqr()).sum::<f64>().sqrt();
        if norm == 0.0 {
            continue;
        }
        let x0 = a[k + 1][k];
        let phase = if x0.norm() == 0.0 { C64::new(1.0, 0.0) } else { x0 / x0.norm() };
        let alpha = -phase * norm;
        let mut v: Vec<C64> = (k + 1..n).map(|i| a[i][k]).collect();
        v[0] -= alpha;
        let vn = v.iter().map(|z| z.norm_sqr()).sum::<f64>().sqrt();
        if vn == 0.0 {
            continue;
        }
        for z in &mut v {
            *z /= vn;
        }
        for j in 0..n {
            let mut s = czero();
            for (t, i) in (k + 1..n).enumerate() {
                s += v[t].conj() * a[i][j];
            }
            for (t, i) in (k + 1..n).enumerate() {
                a[i][j] -= 2.0 * v[t] * s;
            }
        }
        for row in a.iter_mut() {
            let mut s = czero();
            for (t, j) in (k + 1..n).enumerate() {
                s += row[j] * v[t];
            }
            for (t, j) in (k + 1..n).enumerate() {
                row[j] -= 2.0 * s * v[t].conj();
            }
        }
    }
}

/// All eigenvalues of a general complex matrix (Hessenberg reduction followed
/// by the shifted QR algorithm with deflation). The order is unspecified.
pub fn eigenvalues_general(a: &CMat) -> EResult<Vec<C64>> {
    let n = a.len();
    let mut h = a.clone();
    hessenberg(&mut h);
    let scale = max_abs(a).max(1e-300);
    let mut vals = vec![czero(); n];
    let mut hi = n;
    let mut iter = 0usize;
    let mut total = 0usize;
    while hi > 0 {
        let top = hi - 1;
        if top == 0 {
            vals[0] = h[0][0];
            break;
        }
        let mut l = top;
        while l > 0 {
            let s = h[l - 1][l - 1].norm() + h[l][l].norm();
            let s = if s == 0.0 { scale } else { s };
            if h[l][l - 1].norm() <= f64::EPSILON * s {
                h[l][l - 1] = czero();
                break;
            }
            l -= 1;
        }
        if l == top {
            vals[top] = h[top][top];
            hi -= 1;
            iter = 0;
            continue;
        }
        iter += 1;
        total += 1;
        if total > 200 * n + 1000 {
            return Err("не удалось найти собственные значения (QR-алгоритм не сошёлся)".into());
        }
        let mu = if iter % 10 == 0 {
            h[top][top] + C64::new(h[top][top - 1].norm(), 0.0)
        } else {
            let (a, b, c, d) = (h[top - 1][top - 1], h[top - 1][top], h[top][top - 1], h[top][top]);
            let half = (a + d) * 0.5;
            let disc = (half * half - (a * d - b * c)).sqrt();
            let (l1, l2) = (half + disc, half - disc);
            if (l1 - d).norm() < (l2 - d).norm() {
                l1
            } else {
                l2
            }
        };
        for i in l..=top {
            h[i][i] -= mu;
        }
        let mut rots = Vec::with_capacity(top - l);
        for j in l..top {
            let (x, y) = (h[j][j], h[j + 1][j]);
            let r = (x.norm_sqr() + y.norm_sqr()).sqrt();
            let (c, s) = if r == 0.0 { (C64::new(1.0, 0.0), czero()) } else { (x / r, y / r) };
            rots.push((c, s));
            for col in j..=top {
                let (p, q) = (h[j][col], h[j + 1][col]);
                h[j][col] = c.conj() * p + s.conj() * q;
                h[j + 1][col] = -s * p + c * q;
            }
        }
        for (idx, (c, s)) in rots.iter().enumerate() {
            let j = l + idx;
            for row in l..=j + 1 {
                let (p, q) = (h[row][j], h[row][j + 1]);
                h[row][j] = p * c + q * s;
                h[row][j + 1] = -p * s.conj() + q * c.conj();
            }
        }
        for i in l..=top {
            h[i][i] += mu;
        }
    }
    Ok(vals)
}

/// LU factorisation with partial pivoting. Returns (packed LU, row permutation,
/// permutation sign, singular?). With `fix_tiny` zero pivots are replaced by a
/// tiny number (used for inverse iteration).
pub fn lu_factor(a: &CMat, fix_tiny: bool) -> (CMat, Vec<usize>, f64, bool) {
    let n = a.len();
    let mut m = a.clone();
    let mut perm: Vec<usize> = (0..n).collect();
    let mut sign = 1.0;
    let mut singular = false;
    let scale = max_abs(a).max(1e-300);
    for k in 0..n {
        let mut p = k;
        for i in k + 1..n {
            if m[i][k].norm() > m[p][k].norm() {
                p = i;
            }
        }
        if m[p][k].norm() <= 1e-15 * scale * 1e-3 {
            singular = true;
            if fix_tiny {
                m[p][k] = C64::new(1e-14 * scale, 0.0);
            } else if m[p][k].norm() == 0.0 {
                continue;
            }
        }
        if p != k {
            m.swap(k, p);
            perm.swap(k, p);
            sign = -sign;
        }
        let pivot = m[k][k];
        for i in k + 1..n {
            let f = m[i][k] / pivot;
            m[i][k] = f;
            for j in k + 1..n {
                let t = m[k][j];
                m[i][j] -= f * t;
            }
        }
    }
    (m, perm, sign, singular)
}

fn lu_solve(lu: &CMat, perm: &[usize], b: &[C64]) -> Vec<C64> {
    let n = lu.len();
    let mut x: Vec<C64> = perm.iter().map(|&p| b[p]).collect();
    for i in 0..n {
        for k in 0..i {
            let t = lu[i][k] * x[k];
            x[i] -= t;
        }
    }
    for i in (0..n).rev() {
        for k in i + 1..n {
            let t = lu[i][k] * x[k];
            x[i] -= t;
        }
        x[i] /= lu[i][i];
    }
    x
}

fn norm2(v: &[C64]) -> f64 {
    v.iter().map(|z| z.norm_sqr()).sum::<f64>().sqrt()
}

/// Unit-norm eigenvector of `a` for the eigenvalue `lambda` by inverse
/// iteration. Vectors in `prev` (belonging to the same eigenvalue cluster) are
/// projected out so that repeated eigenvalues give independent vectors.
pub fn eigenvector(a: &CMat, lambda: C64, prev: &[Vec<C64>], seed: usize) -> Vec<C64> {
    let n = a.len();
    let scale = max_abs(a).max(1e-300);
    let mut m = a.clone();
    let shifted = lambda + C64::new(1e-10 * scale, 0.0);
    for (i, row) in m.iter_mut().enumerate() {
        row[i] -= shifted;
    }
    let (lu, perm, _, _) = lu_factor(&m, true);
    let mut x: Vec<C64> = (0..n)
        .map(|i| C64::new(((i + 1) as f64 * 1.7 + seed as f64 * 0.9).sin() + 0.3, 0.0))
        .collect();
    for _ in 0..4 {
        for p in prev {
            let dot: C64 = p.iter().zip(&x).map(|(u, w)| u.conj() * w).sum();
            for (xi, pi) in x.iter_mut().zip(p) {
                *xi -= dot * pi;
            }
        }
        let nrm = norm2(&x);
        if nrm == 0.0 {
            break;
        }
        for z in &mut x {
            *z /= nrm;
        }
        x = lu_solve(&lu, &perm, &x);
    }
    for p in prev {
        let dot: C64 = p.iter().zip(&x).map(|(u, w)| u.conj() * w).sum();
        for (xi, pi) in x.iter_mut().zip(p) {
            *xi -= dot * pi;
        }
    }
    let nrm = norm2(&x).max(1e-300);
    for z in &mut x {
        *z /= nrm;
    }
    // phase: the largest component is real and positive
    let mut big = 0;
    for i in 0..n {
        if x[i].norm() > x[big].norm() * (1.0 + 1e-9) {
            big = i;
        }
    }
    let ph = x[big].conj() / x[big].norm().max(1e-300);
    for z in &mut x {
        *z *= ph;
    }
    x
}

/// Eigenvalues (sorted: real part, then imaginary part) and optionally
/// eigenvectors (`vecs[j]` belongs to `vals[j]`). Real symmetric and Hermitian
/// matrices give real ascending eigenvalues.
pub fn eigen_system(a: &CMat, want_vectors: bool) -> EResult<(Vec<C64>, Vec<Vec<C64>>)> {
    let n = a.len();
    if is_symmetric(a) {
        let r: RMat = a.iter().map(|row| row.iter().map(|z| z.re).collect()).collect();
        let (vals, vecs) = jacobi_symmetric(&r);
        let cv = (0..n).map(|j| (0..n).map(|i| C64::new(vecs[i][j], 0.0)).collect()).collect();
        return Ok((vals.into_iter().map(|x| C64::new(x, 0.0)).collect(), cv));
    }
    let scale = max_abs(a).max(1e-300);
    let hermitian = is_hermitian(a);
    let real = is_real(a);
    let mut vals = eigenvalues_general(a)?;
    for z in &mut vals {
        if hermitian || (real && z.im.abs() <= 1e-10 * scale) {
            z.im = 0.0;
        }
    }
    vals.sort_by(|x, y| x.re.total_cmp(&y.re).then(x.im.total_cmp(&y.im)));
    let mut vecs: Vec<Vec<C64>> = Vec::new();
    if want_vectors {
        let mut cluster: Vec<Vec<C64>> = Vec::new();
        for (j, &lam) in vals.iter().enumerate() {
            if j == 0 || (lam - vals[j - 1]).norm() > 1e-8 * scale {
                cluster.clear();
            }
            let mut v = eigenvector(a, lam, &cluster, cluster.len());
            if real && lam.im == 0.0 {
                for z in &mut v {
                    if z.im.abs() < 1e-10 {
                        z.im = 0.0;
                    }
                }
            }
            cluster.push(v.clone());
            vecs.push(v);
        }
    }
    Ok((vals, vecs))
}

// ---------------------------------------------------------------------------
// Decompositions

/// Cholesky factor `L` (lower triangular, `L·L^H = A`); only the upper triangle is read.
pub fn cholesky(a: &CMat) -> EResult<CMat> {
    let n = a.len();
    let mut l = vec![vec![czero(); n]; n];
    for i in 0..n {
        for j in 0..=i {
            let mut sum = a[j][i].conj();
            for k in 0..j {
                sum -= l[i][k] * l[j][k].conj();
            }
            if i == j {
                if sum.re <= 0.0 || !sum.re.is_finite() {
                    return Err("матрица не является положительно определённой".into());
                }
                l[i][i] = C64::new(sum.re.sqrt(), 0.0);
            } else {
                l[i][j] = sum / l[j][j];
            }
        }
    }
    Ok(l)
}

/// `P·A = L·U` with a permutation matrix `P`; returns (P, L, U).
pub fn lu_decompose(a: &CMat) -> EResult<(CMat, CMat, CMat)> {
    let n = a.len();
    let (lu, perm, _, _) = lu_factor(a, false);
    let mut p = vec![vec![czero(); n]; n];
    let mut l = vec![vec![czero(); n]; n];
    let mut u = vec![vec![czero(); n]; n];
    for i in 0..n {
        p[i][perm[i]] = C64::new(1.0, 0.0);
        for j in 0..n {
            if j < i {
                l[i][j] = lu[i][j];
            } else {
                u[i][j] = lu[i][j];
            }
        }
        l[i][i] = C64::new(1.0, 0.0);
    }
    Ok((p, l, u))
}

/// Reduced row echelon form; returns the matrix and the number of pivots.
pub fn rref(a: &CMat) -> (CMat, usize) {
    let rows = a.len();
    let cols = a.first().map_or(0, |r| r.len());
    let mut m = a.clone();
    let tol = (rows.max(cols) as f64) * f64::EPSILON * max_abs(a) * 10.0;
    let mut r = 0;
    for c in 0..cols {
        if r >= rows {
            break;
        }
        let mut p = r;
        for i in r + 1..rows {
            if m[i][c].norm() > m[p][c].norm() {
                p = i;
            }
        }
        if m[p][c].norm() <= tol {
            for row in m.iter_mut().skip(r) {
                row[c] = czero();
            }
            continue;
        }
        m.swap(r, p);
        let pivot = m[r][c];
        for j in c..cols {
            m[r][j] /= pivot;
        }
        for i in 0..rows {
            if i != r {
                let f = m[i][c];
                if f != czero() {
                    for j in c..cols {
                        let t = m[r][j];
                        m[i][j] -= f * t;
                    }
                }
            }
        }
        r += 1;
    }
    (m, r)
}

/// Householder QR of a real `m×n` matrix: `Q` (m×m, orthogonal) and `R` (m×n, upper triangular).
pub fn qr_real(a: &RMat) -> (RMat, RMat) {
    let m = a.len();
    let n = a.first().map_or(0, |r| r.len());
    let mut r = a.clone();
    let mut q = real_identity(m);
    for k in 0..n.min(m.saturating_sub(1)) {
        let norm = (k..m).map(|i| r[i][k] * r[i][k]).sum::<f64>().sqrt();
        if norm == 0.0 {
            continue;
        }
        let alpha = if r[k][k] >= 0.0 { -norm } else { norm };
        let mut v: Vec<f64> = (k..m).map(|i| r[i][k]).collect();
        v[0] -= alpha;
        let vn = v.iter().map(|x| x * x).sum::<f64>().sqrt();
        if vn == 0.0 {
            continue;
        }
        for x in &mut v {
            *x /= vn;
        }
        for j in 0..n {
            let s: f64 = (k..m).map(|i| v[i - k] * r[i][j]).sum();
            for i in k..m {
                r[i][j] -= 2.0 * v[i - k] * s;
            }
        }
        for row in q.iter_mut() {
            let s: f64 = (k..m).map(|j| row[j] * v[j - k]).sum();
            for j in k..m {
                row[j] -= 2.0 * s * v[j - k];
            }
        }
        for i in k + 1..m {
            r[i][k] = 0.0;
        }
    }
    (q, r)
}

/// Least-squares solution of `A x ≈ b` (A is `m×n`, `m >= n`, full column rank).
pub fn lstsq(a: &RMat, b: &[f64]) -> EResult<Vec<f64>> {
    let m = a.len();
    let n = a.first().map_or(0, |r| r.len());
    let (q, r) = qr_real(a);
    let qtb: Vec<f64> = (0..n).map(|j| (0..m).map(|i| q[i][j] * b[i]).sum()).collect();
    let tol = 1e-13 * (0..n).map(|i| r[i][i].abs()).fold(0.0, f64::max);
    let mut x = vec![0.0; n];
    for i in (0..n).rev() {
        if r[i][i].abs() <= tol {
            return Err("данные не позволяют определить решение (вырожденная система)".into());
        }
        let s: f64 = (i + 1..n).map(|j| r[i][j] * x[j]).sum();
        x[i] = (qtb[i] - s) / r[i][i];
    }
    Ok(x)
}

/// Singular value decomposition of a real `m×n` matrix by one-sided Jacobi.
/// Returns `(U, s, V)` with `U` m×k, `V` n×k, `k = min(m, n)`, `s` descending,
/// and `A = U·diag(s)·Vᵀ`.
pub fn svd_real(a: &RMat) -> (RMat, Vec<f64>, RMat) {
    let m = a.len();
    let n = a.first().map_or(0, |r| r.len());
    if m < n {
        let at: RMat = (0..n).map(|j| (0..m).map(|i| a[i][j]).collect()).collect();
        let (u, s, v) = svd_real(&at);
        return (v, s, u);
    }
    let mut u = a.clone();
    let mut v = real_identity(n);
    for _ in 0..60 {
        let mut rotated = false;
        for p in 0..n.saturating_sub(1) {
            for q in p + 1..n {
                let (mut alpha, mut beta, mut gamma) = (0.0, 0.0, 0.0);
                for row in u.iter() {
                    alpha += row[p] * row[p];
                    beta += row[q] * row[q];
                    gamma += row[p] * row[q];
                }
                if gamma == 0.0 || gamma.abs() <= 1e-15 * (alpha * beta).sqrt() {
                    continue;
                }
                rotated = true;
                let zeta = (beta - alpha) / (2.0 * gamma);
                let t = zeta.signum() / (zeta.abs() + (1.0 + zeta * zeta).sqrt());
                let c = 1.0 / (1.0 + t * t).sqrt();
                let s = c * t;
                for row in u.iter_mut().chain(v.iter_mut()) {
                    let (x, y) = (row[p], row[q]);
                    row[p] = c * x - s * y;
                    row[q] = s * x + c * y;
                }
            }
        }
        if !rotated {
            break;
        }
    }
    let mut sv: Vec<f64> = (0..n).map(|j| (0..m).map(|i| u[i][j] * u[i][j]).sum::<f64>().sqrt()).collect();
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&i, &j| sv[j].total_cmp(&sv[i]));
    let smax = sv.iter().cloned().fold(0.0, f64::max);
    let mut uo = vec![vec![0.0; n]; m];
    let mut vo = vec![vec![0.0; n]; n];
    let mut so = vec![0.0; n];
    let mut zero_cols = Vec::new();
    for (nj, &oj) in order.iter().enumerate() {
        so[nj] = sv[oj];
        for i in 0..n {
            vo[i][nj] = v[i][oj];
        }
        if sv[oj] > 1e-14 * smax && sv[oj] > 0.0 {
            for i in 0..m {
                uo[i][nj] = u[i][oj] / sv[oj];
            }
        } else {
            so[nj] = if sv[oj] > 0.0 { sv[oj] } else { 0.0 };
            zero_cols.push(nj);
        }
    }
    sv.clear();
    // complete U with orthonormal columns for (numerically) zero singular values
    for &c in &zero_cols {
        for cand in 0..m {
            let mut e = vec![0.0; m];
            e[cand] = 1.0;
            for j in 0..n {
                if j == c || (zero_cols.contains(&j) && j > c) {
                    continue;
                }
                let dot: f64 = (0..m).map(|i| uo[i][j] * e[i]).sum();
                for i in 0..m {
                    e[i] -= dot * uo[i][j];
                }
            }
            let nrm = e.iter().map(|x| x * x).sum::<f64>().sqrt();
            if nrm > 1e-8 {
                for i in 0..m {
                    uo[i][c] = e[i] / nrm;
                }
                break;
            }
        }
    }
    (uo, so, vo)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(x: f64) -> C64 {
        C64::new(x, 0.0)
    }

    fn cm(rows: &[&[f64]]) -> CMat {
        rows.iter().map(|r| r.iter().map(|&x| c(x)).collect()).collect()
    }

    #[test]
    fn symmetric_eigen() {
        let a = cm(&[&[2.0, 1.0], &[1.0, 2.0]]);
        let (vals, vecs) = eigen_system(&a, true).unwrap();
        assert!((vals[0].re - 1.0).abs() < 1e-12 && (vals[1].re - 3.0).abs() < 1e-12);
        assert!(vals.iter().all(|z| z.im == 0.0));
        assert!((vecs[1][0].re - vecs[1][1].re).abs() < 1e-12);
    }

    #[test]
    fn general_eigen() {
        // rotation by 90 degrees: ±i
        let a = cm(&[&[0.0, -1.0], &[1.0, 0.0]]);
        let (vals, vecs) = eigen_system(&a, true).unwrap();
        assert!((vals[0] - C64::new(0.0, -1.0)).norm() < 1e-12);
        assert!((vals[1] - C64::new(0.0, 1.0)).norm() < 1e-12);
        for (lam, v) in vals.iter().zip(&vecs) {
            for i in 0..2 {
                let av: C64 = (0..2).map(|j| a[i][j] * v[j]).sum();
                assert!((av - lam * v[i]).norm() < 1e-9);
            }
        }
        // non-symmetric with real eigenvalues 1, 2, 3
        let a = cm(&[&[2.0, 1.0, 0.0], &[0.0, 3.0, 4.0], &[0.0, 0.0, 1.0]]);
        let (vals, vecs) = eigen_system(&a, true).unwrap();
        for (z, e) in vals.iter().zip([1.0, 2.0, 3.0]) {
            assert!((z.re - e).abs() < 1e-10 && z.im == 0.0, "{vals:?}");
        }
        for (lam, v) in vals.iter().zip(&vecs) {
            for i in 0..3 {
                let av: C64 = (0..3).map(|j| a[i][j] * v[j]).sum();
                assert!((av - lam * v[i]).norm() < 1e-8);
            }
        }
    }

    #[test]
    fn svd_and_qr() {
        let a: RMat = vec![vec![1.0, 2.0], vec![3.0, 4.0], vec![5.0, 6.0]];
        let (u, s, v) = svd_real(&a);
        assert!((s[0] - 9.525_518_091_565_104).abs() < 1e-10);
        assert!((s[1] - 0.514_300_580_658_642).abs() < 1e-10);
        for i in 0..3 {
            for j in 0..2 {
                let x: f64 = (0..2).map(|k| u[i][k] * s[k] * v[j][k]).sum();
                assert!((x - a[i][j]).abs() < 1e-10);
            }
        }
        let (q, r) = qr_real(&a);
        for i in 0..3 {
            for j in 0..2 {
                let x: f64 = (0..3).map(|k| q[i][k] * r[k][j]).sum();
                assert!((x - a[i][j]).abs() < 1e-12);
            }
        }
        assert!(r[1][0] == 0.0 && r[2][0] == 0.0 && r[2][1].abs() < 1e-12);
    }
}
