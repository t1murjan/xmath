//! Mathcad's Fourier transforms: `cfft`, `icfft`, `FFT`, `IFFT`.
//!
//! Mathcad normalizes the forward transforms by 1/N and leaves the inverse ones unscaled, so
//! `icfft(cfft(v))` and `ifft(fft(v))` give `v` back. `FFT`/`IFFT` work on a real signal and
//! carry the first N/2+1 (packed) bins of the spectrum, as Mathcad does.

use crate::value::{Matrix, Num, Value};
use num_complex::Complex64 as C64;

/// The discrete Fourier transform of `v`; `inverse` uses the opposite sign and the caller's
/// normalization. Radix-2 when the length allows it, a direct sum otherwise (the samples in the
/// shipped Mathcad worksheets are small and are not always powers of two).
pub fn transform(v: &[C64], inverse: bool) -> Vec<C64> {
    let n = v.len();
    if n == 0 {
        return Vec::new();
    }
    if n.is_power_of_two() {
        radix2(v, inverse)
    } else {
        direct(v, inverse)
    }
}

fn direct(v: &[C64], inverse: bool) -> Vec<C64> {
    let n = v.len();
    let sign = if inverse { 1.0 } else { -1.0 };
    (0..n)
        .map(|k| {
            let mut acc = C64::new(0.0, 0.0);
            for (j, x) in v.iter().enumerate() {
                let ang = sign * 2.0 * std::f64::consts::PI * (j * k) as f64 / n as f64;
                acc += x * C64::new(ang.cos(), ang.sin());
            }
            acc
        })
        .collect()
}

fn radix2(v: &[C64], inverse: bool) -> Vec<C64> {
    let n = v.len();
    let mut a = v.to_vec();
    // bit reversal
    let mut j = 0usize;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            a.swap(i, j);
        }
    }
    let sign = if inverse { 1.0 } else { -1.0 };
    let mut len = 2usize;
    while len <= n {
        let ang = sign * 2.0 * std::f64::consts::PI / len as f64;
        let wl = C64::new(ang.cos(), ang.sin());
        for i in (0..n).step_by(len) {
            let mut w = C64::new(1.0, 0.0);
            for k in 0..len / 2 {
                let u = a[i + k];
                let t = w * a[i + k + len / 2];
                a[i + k] = u + t;
                a[i + k + len / 2] = u - t;
                w *= wl;
            }
        }
        len <<= 1;
    }
    a
}

/// The vector behind a value (a row or a column); numbers are not vectors.
pub fn as_vector(v: &Value) -> Result<Vec<C64>, String> {
    match v {
        Value::Matrix(m) if m.cols == 1 || m.rows == 1 => Ok(m.data.iter().map(|n| n.v).collect()),
        Value::Matrix(_) => Err("ожидался вектор, а не матрица".into()),
        other => Err(format!("ожидался вектор, найдено: {}", other.type_name())),
    }
}

/// A column vector of complex numbers.
pub fn to_column(data: Vec<C64>) -> Value {
    Value::Matrix(Matrix::column(data.into_iter().map(|z| Num::new(z, crate::units::Dim::NONE)).collect()))
}

/// Forward transform of a complex vector, normalized by 1/N (Mathcad's `cfft`).
pub fn cfft(v: &[C64]) -> Vec<C64> {
    let n = v.len().max(1);
    transform(v, false).into_iter().map(|z| z / n as f64).collect()
}

/// Inverse of [`cfft`] (Mathcad's `icfft`: no further normalization).
pub fn icfft(v: &[C64]) -> Vec<C64> {
    transform(v, true)
}

/// Mathcad's `FFT`: the packed spectrum of a real signal, N/2+1 values.
pub fn fft(v: &[C64]) -> Vec<C64> {
    let n = v.len().max(2);
    cfft(v).into_iter().take(n / 2 + 1).collect()
}

/// Mathcad's `IFFT`: the real signal behind a packed spectrum.
pub fn ifft(u: &[C64]) -> Vec<C64> {
    let m = u.len();
    if m < 2 {
        return vec![u.first().copied().unwrap_or_default()];
    }
    let n = 2 * (m - 1);
    let mut full = vec![C64::new(0.0, 0.0); n];
    full[..m].copy_from_slice(u);
    // the second half of the spectrum is the conjugate mirror of the first
    for k in 1..m - 1 {
        full[n - k] = u[k].conj();
    }
    icfft(&full)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(xs: &[f64]) -> Vec<C64> {
        xs.iter().map(|x| C64::new(*x, 0.0)).collect()
    }

    fn close(a: &[C64], b: &[C64], tol: f64) -> bool {
        a.len() == b.len() && a.iter().zip(b).all(|(x, y)| (x - y).norm() <= tol)
    }

    #[test]
    fn normalizations_match_mathcad() {
        // cfft of a constant signal is a single spike at 0 with the mean as its height
        let c = cfft(&v(&[3.0, 3.0, 3.0, 3.0]));
        assert!((c[0] - C64::new(3.0, 0.0)).norm() < 1e-12);
        for z in &c[1..] {
            assert!(z.norm() < 1e-12);
        }
        // and the inverse gives the signal back
        assert!(close(&icfft(&c), &v(&[3.0; 4]), 1e-12));
        assert!(close(&cfft(&v(&[1.0, 0.0, 0.0, 0.0])), &v(&[0.25; 4]), 1e-12));
    }

    #[test]
    fn round_trips_for_any_length() {
        for n in [7usize, 8, 16, 50, 64, 101] {
            let signal: Vec<C64> = (0..n).map(|k| C64::new((k as f64 * 0.37).sin(), (k as f64 * 0.11).cos())).collect();
            assert!(close(&icfft(&cfft(&signal)), &signal, 1e-9), "cfft round trip, n={n}");
        }
        // the packed FFT/IFFT pair needs an even length: the second half of the spectrum is a
        // conjugate mirror of the first (Mathcad requires a power of two, which is even)
        for n in [8usize, 16, 50, 64] {
            let real: Vec<C64> = (0..n).map(|k| C64::new((k as f64 * 0.29).sin(), 0.0)).collect();
            let packed = fft(&real);
            assert_eq!(packed.len(), n / 2 + 1);
            assert!(close(&ifft(&packed), &real, 1e-9), "fft round trip, n={n}");
        }
    }

    #[test]
    fn a_sine_gives_two_bins() {
        // sin(2π·3·k/16) has energy only in bins 3 and 13
        let n = 16;
        let sig: Vec<C64> = (0..n).map(|k| C64::new((2.0 * std::f64::consts::PI * 3.0 * k as f64 / n as f64).sin(), 0.0)).collect();
        let s = cfft(&sig);
        for (k, z) in s.iter().enumerate() {
            let expect = if k == 3 || k == 13 { 0.5 } else { 0.0 };
            assert!((z.norm() - expect).abs() < 1e-12, "bin {k}: {}", z.norm());
        }
    }
}
