//! Mathcad's data-analysis functions whose behaviour was recovered from the shipped worksheets:
//! `percentile`, `correl2d` and `walsh`.
//!
//! * `percentile(v, p)` uses the CDF = i/(n+1) plotting position, not Excel's `p(n-1)`:
//!   `pos = p(n+1) - 1`, clamped to `[0, n-1]`, with linear interpolation between neighbours.
//!   `location.xmcd` stores `percentile(0..10, 0.9) = 9.8`, which only this rule gives.
//! * `correl2d(M, K)` is plain correlation with a centred kernel, zero padding and an output the
//!   size of `M` (the kernel is *not* flipped, unlike convolution).
//! * `walsh(n, k)` is the k-th Walsh function of order `n`: 2ⁿ signs, the rows of the Sylvester
//!   Hadamard matrix in bit-reversed order — `walsh(2,1)` is `[+,+,-,-]`, not `[+,-,+,-]`.

use crate::value::{Matrix, Num, Value};

/// The numbers of a vector (a column or a row).
pub fn vector(v: &Value) -> Result<Vec<f64>, String> {
    match v {
        Value::Matrix(m) if m.cols == 1 || m.rows == 1 => Ok(m.data.iter().map(|n| n.v.re).collect()),
        Value::Matrix(_) => Err("ожидался вектор, а не матрица".into()),
        other => Err(format!("ожидался вектор, найдено: {}", other.type_name())),
    }
}

fn column(values: Vec<f64>) -> Value {
    Value::Matrix(Matrix::column(values.into_iter().map(Num::real).collect()))
}

/// Mathcad's `percentile(v, p)`: `p` between 0 and 1, the CDF = i/(n+1) plotting position.
pub fn percentile(v: &[f64], p: f64) -> Result<f64, String> {
    if !(0.0..=1.0).contains(&p) {
        return Err("percentile: процент должен быть между 0 и 1".into());
    }
    if v.is_empty() {
        return Err("percentile: пустой вектор".into());
    }
    let n = v.len();
    let mut sorted = v.to_vec();
    sorted.sort_by(|a, b| a.total_cmp(b));
    let pos = (p * (n as f64 + 1.0) - 1.0).clamp(0.0, (n - 1) as f64);
    let lo = pos.floor() as usize;
    let hi = pos.ceil() as usize;
    let f = pos - lo as f64;
    Ok(sorted[lo] + f * (sorted[hi] - sorted[lo]))
}

/// Mathcad's `correl2d(M, K)`: correlation of an image with a centred kernel, zero padded.
pub fn correl2d(m: &Matrix, k: &Matrix) -> Matrix {
    let (h, w) = (m.rows, m.cols);
    let (kh, kw) = (k.rows, k.cols);
    let (ch, cw) = ((kh / 2) as isize, (kw / 2) as isize);
    let mut out = Matrix::zeros(h, w);
    for i in 0..h {
        for j in 0..w {
            let mut sum = 0.0;
            for u in 0..kh {
                for v in 0..kw {
                    let ii = i as isize + u as isize - ch;
                    let jj = j as isize + v as isize - cw;
                    if ii >= 0 && jj >= 0 && (ii as usize) < h && (jj as usize) < w {
                        sum += m.data[ii as usize * w + jj as usize].v.re * k.data[u * kw + v].v.re;
                    }
                }
            }
            out.data[i * w + j] = Num::real(sum);
        }
    }
    out
}

/// Mathcad's `walsh(n, k)`: the k-th Walsh function of order `n` as a column of ±1.
pub fn walsh(n: i64, k: i64) -> Result<Vec<f64>, String> {
    if !(0..=12).contains(&n) {
        return Err("walsh: порядок должен быть от 0 до 12".into());
    }
    let len = 1i64 << n;
    if k < 0 || k >= len {
        return Err("walsh: индекс вне диапазона".into());
    }
    // w[n][k][j] = Π_t (-1)^(k_t · j_{n-1-t}): bit t of k pairs with the bit of j counted from the top
    Ok((0..len)
        .map(|j| {
            let mut sign = 1.0;
            for t in 0..n {
                if (k >> t) & 1 == 1 && (j >> (n - 1 - t)) & 1 == 1 {
                    sign = -sign;
                }
            }
            sign
        })
        .collect())
}

pub fn values(values: &[f64]) -> Value {
    column(values.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentile_matches_the_stored_values() {
        let x: Vec<f64> = (0..=10).map(|i| i as f64).collect();
        assert_eq!(percentile(&x, 0.50).unwrap(), 5.0);
        assert_eq!(percentile(&x, 0.25).unwrap(), 2.0);
        assert!((percentile(&x, 0.90).unwrap() - 9.8).abs() < 1e-12);
        // location.xmcd: mfg = [79, 85, 55.2, 86, 81]
        let mfg = [79.0, 85.0, 55.2, 86.0, 81.0];
        assert!((percentile(&mfg, 0.25).unwrap() - 67.1).abs() < 1e-9);
        assert!((percentile(&mfg, 0.50).unwrap() - 81.0).abs() < 1e-9);
        assert!((percentile(&mfg, 0.75).unwrap() - 85.5).abs() < 1e-9);
        assert!(percentile(&x, 1.5).is_err());
    }

    #[test]
    fn correlation_of_the_worked_example() {
        // correl2d.xmcd: the 5x5 image, the 3x3 template, and the 25 values Mathcad stored
        // the image and the template as the worksheet displays them (row by row)
        let m = Matrix::new(5, 5, vec![
            3.0, 1.0, 2.0, -1.0, 0.0,
            0.0, 1.0, 1.0, 3.0, 1.0,
            1.0, 1.0, 2.0, 1.0, 2.0,
            -1.0, 1.0, 1.0, 2.0, -2.0,
            3.0, -1.0, 1.0, 3.0, -2.0,
        ].into_iter().map(Num::real).collect());
        let k = Matrix::new(3, 3, vec![1.0, 1.0, 3.0, 1.0, 2.0, 1.0, 1.0, 1.0, 2.0].into_iter().map(Num::real).collect());
        let out = correl2d(&m, &k);
        let want = [
            9.0, 10.0, 12.0, 6.0, 3.0,
            10.0, 19.0, 11.0, 16.0, 7.0,
            7.0, 11.0, 23.0, 12.0, 9.0,
            4.0, 14.0, 17.0, 12.0, 2.0,
            7.0, 5.0, 12.0, 2.0, -1.0,
        ];
        for (i, w) in want.iter().enumerate() {
            assert!((out.data[i].v.re - w).abs() < 1e-12, "cell {i}: {} != {w}", out.data[i].v.re);
        }
    }

    #[test]
    fn walsh_rows_are_bit_reversed_hadamard() {
        assert_eq!(walsh(2, 0).unwrap(), vec![1.0, 1.0, 1.0, 1.0]);
        assert_eq!(walsh(2, 1).unwrap(), vec![1.0, 1.0, -1.0, -1.0]);
        assert_eq!(walsh(2, 2).unwrap(), vec![1.0, -1.0, 1.0, -1.0]);
        assert_eq!(walsh(2, 3).unwrap(), vec![1.0, -1.0, -1.0, 1.0]);
        assert!(walsh(2, 4).is_err());
        // Walsh.xmcd stores the dot product of two order-6 functions as zero
        let a = walsh(6, 3).unwrap();
        let b = walsh(6, 9).unwrap();
        assert_eq!(a.iter().zip(&b).map(|(x, y)| x * y).sum::<f64>(), 0.0);
        // every pair of rows is orthogonal, and each row has length 2^n in norm squared
        for i in 0..8 {
            for j in 0..8 {
                let s: f64 = walsh(3, i).unwrap().iter().zip(walsh(3, j).unwrap()).map(|(x, y)| x * y).sum();
                assert_eq!(s, if i == j { 8.0 } else { 0.0 }, "{i},{j}");
            }
        }
    }
}
