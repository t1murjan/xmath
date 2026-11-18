//! Symbolic matrices: sums, products, powers, transpose, determinant, inverse.

use super::algebra::{cancel, expand, simplify};
use super::sx::*;
use num_traits::Signed;

pub fn dims(m: &[Vec<Sx>]) -> (usize, usize) {
    (m.len(), m.first().map_or(0, |r| r.len()))
}

fn bad(msg: &str) -> Sx {
    flag_error(msg);
    Sx::Fun("undefined".into(), vec![])
}

pub fn mat_sum(terms: Vec<Sx>) -> Sx {
    let mut acc: Option<Vec<Vec<Sx>>> = None;
    let mut scalars = Vec::new();
    for t in terms {
        match t {
            Sx::Mat(m) => match &mut acc {
                None => acc = Some(m),
                Some(a) => {
                    if dims(a) != dims(&m) {
                        return bad("сложение матриц разного размера");
                    }
                    for (ra, rm) in a.iter_mut().zip(m) {
                        for (x, y) in ra.iter_mut().zip(rm) {
                            *x = add(x.clone(), y);
                        }
                    }
                }
            },
            other => scalars.push(other),
        }
    }
    let mut a = acc.unwrap();
    if !scalars.is_empty() {
        let s = sum(scalars);
        for r in a.iter_mut() {
            for x in r.iter_mut() {
                *x = add(x.clone(), s.clone());
            }
        }
    }
    Sx::Mat(a)
}

pub fn matmul(a: &[Vec<Sx>], b: &[Vec<Sx>]) -> Option<Vec<Vec<Sx>>> {
    let (ra, ca) = dims(a);
    let (rb, cb) = dims(b);
    if ca != rb {
        return None;
    }
    let mut out = vec![vec![zero(); cb]; ra];
    for i in 0..ra {
        for j in 0..cb {
            out[i][j] = sum((0..ca).map(|k| mul(a[i][k].clone(), b[k][j].clone())).collect());
        }
    }
    Some(out)
}

pub fn mat_product(factors: Vec<Sx>) -> Sx {
    let mut scalars = Vec::new();
    let mut acc: Option<Vec<Vec<Sx>>> = None;
    for f in factors {
        match f {
            Sx::Mat(m) => {
                acc = Some(match acc {
                    None => m,
                    Some(a) => match matmul(&a, &m) {
                        Some(r) => r,
                        None => return bad("умножение матриц несовместимых размеров"),
                    },
                });
            }
            other => scalars.push(other),
        }
    }
    let mut a = acc.unwrap();
    if !scalars.is_empty() {
        let s = product(scalars);
        for r in a.iter_mut() {
            for x in r.iter_mut() {
                *x = mul(s.clone(), x.clone());
            }
        }
    }
    Sx::Mat(a)
}

pub fn identity(n: usize) -> Vec<Vec<Sx>> {
    (0..n).map(|i| (0..n).map(|j| if i == j { one() } else { zero() }).collect()).collect()
}

pub fn mat_pow(m: Sx, e: Sx) -> Sx {
    let Sx::Mat(rows) = &m else { return m };
    let Some(n) = e.as_int() else {
        return Sx::Pow(Box::new(m), Box::new(e));
    };
    let (r, c) = dims(rows);
    if r != c {
        return bad("возведение в степень неквадратной матрицы");
    }
    if n == 0 {
        return Sx::Mat(identity(r));
    }
    let base = if n < 0 {
        match inverse(rows) {
            Some(i) => i,
            None => return bad("матрица вырождена (определитель равен нулю)"),
        }
    } else {
        rows.clone()
    };
    let mut result = identity(r);
    for _ in 0..n.unsigned_abs().min(1000) {
        result = matmul(&result, &base).unwrap();
        for row in result.iter_mut() {
            for x in row.iter_mut() {
                *x = simplify_entry(x);
            }
        }
    }
    Sx::Mat(result)
}

fn simplify_entry(x: &Sx) -> Sx {
    if x.size() > 4 {
        let c = cancel(&expand(x));
        if c.size() < x.size() {
            return c;
        }
    }
    x.clone()
}

pub fn transpose(m: &[Vec<Sx>]) -> Vec<Vec<Sx>> {
    let (r, c) = dims(m);
    (0..c).map(|j| (0..r).map(|i| m[i][j].clone()).collect()).collect()
}

pub fn trace(m: &[Vec<Sx>]) -> Sx {
    sum((0..m.len().min(m.first().map_or(0, |r| r.len()))).map(|i| m[i][i].clone()).collect())
}

pub fn det(m: &[Vec<Sx>]) -> Option<Sx> {
    let (r, c) = dims(m);
    if r != c {
        return None;
    }
    Some(match r {
        0 => one(),
        1 => m[0][0].clone(),
        2 => sub(mul(m[0][0].clone(), m[1][1].clone()), mul(m[0][1].clone(), m[1][0].clone())),
        _ => {
            if r > 7 && m.iter().flatten().any(|x| !matches!(x, Sx::Num(_))) {
                return None;
            }
            // Laplace expansion along the row with most zeros
            let ri = (0..r).max_by_key(|&i| m[i].iter().filter(|x| x.is_zero()).count()).unwrap();
            let mut terms = Vec::new();
            for j in 0..c {
                if m[ri][j].is_zero() {
                    continue;
                }
                let minor: Vec<Vec<Sx>> = (0..r).filter(|&i| i != ri).map(|i| (0..c).filter(|&k| k != j).map(|k| m[i][k].clone()).collect()).collect();
                let d = det(&minor)?;
                let sign = if (ri + j) % 2 == 0 { 1 } else { -1 };
                terms.push(product(vec![int(sign), m[ri][j].clone(), d]));
            }
            sum(terms)
        }
    })
}

pub fn inverse(m: &[Vec<Sx>]) -> Option<Vec<Vec<Sx>>> {
    let (r, c) = dims(m);
    if r != c {
        return None;
    }
    let d = simplify_entry(&expand(&det(m)?));
    if d.is_zero() {
        return None;
    }
    if r == 1 {
        return Some(vec![vec![pow(d, int(-1))]]);
    }
    let mut out = vec![vec![zero(); r]; r];
    for i in 0..r {
        for j in 0..r {
            let minor: Vec<Vec<Sx>> = (0..r).filter(|&k| k != j).map(|k| (0..r).filter(|&l| l != i).map(|l| m[k][l].clone()).collect()).collect();
            let cof = det(&minor)?;
            let sign = if (i + j) % 2 == 0 { 1 } else { -1 };
            out[i][j] = simplify_entry(&expand_if_small(&div(mul(int(sign), cof), d.clone())));
        }
    }
    Some(out)
}

fn expand_if_small(x: &Sx) -> Sx {
    // keep a common denominator but normalise the numerator
    let c = cancel(x);
    if c.size() <= x.size() * 2 {
        c
    } else {
        x.clone()
    }
}

pub fn is_neg_rat(x: &Sx) -> bool {
    matches!(x, Sx::Num(r) if r.is_negative())
}
