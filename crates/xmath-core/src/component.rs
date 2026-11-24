//! Mathcad's data components (Data Table, Input Table): an `<component>` region carries an
//! embedded OLE object whose `Contents` stream holds the table values as `f64`. The worksheet
//! then refers to the table by the name in the component's `<outputs>`.
//!
//! Only the value array is decoded here. Layout of `Contents` as observed in the shipped
//! samples: a header, then two `u32` — `n - 1` and `n` — followed by `n` little-endian doubles.
//! A table with several columns is stored as several components, one per output name.

use std::io::{Cursor, Read};

const CFB_MAGIC: [u8; 8] = [0xd0, 0xcf, 0x11, 0xe0, 0xa1, 0xb1, 0x1a, 0xe1];

/// The table values of a component payload, if it holds a decodable `Contents` stream.
pub fn table_values(payload: &[u8]) -> Option<Vec<f64>> {
    let at = payload.windows(CFB_MAGIC.len()).position(|w| w == CFB_MAGIC)?;
    let mut file = cfb::CompoundFile::open(Cursor::new(&payload[at..])).ok()?;
    let mut stream = file.open_stream("Contents").ok()?;
    let mut contents = Vec::new();
    stream.read_to_end(&mut contents).ok()?;
    values_of(&contents)
}

/// The array behind a `Contents` stream: the `(n-1, n)` pair with the largest `n` whose `n`
/// doubles are finite. Header words can form small false pairs (`(0, 1)`), so a lone value is
/// not enough — every table in the shipped samples holds several.
pub fn values_of(c: &[u8]) -> Option<Vec<f64>> {
    let mut found: Option<Vec<f64>> = None;
    let mut p = 4;
    while p + 4 + 8 <= c.len() {
        let n = u32::from_le_bytes([c[p], c[p + 1], c[p + 2], c[p + 3]]) as usize;
        let prev = u32::from_le_bytes([c[p - 4], c[p - 3], c[p - 2], c[p - 1]]) as usize;
        if n >= 2 && n <= 100_000 && prev + 1 == n && p + 4 + 8 * n <= c.len() {
            let mut vals = Vec::with_capacity(n);
            let mut ok = true;
            for k in 0..n {
                let at = p + 4 + 8 * k;
                let v = f64::from_le_bytes(c[at..at + 8].try_into().ok()?);
                if !v.is_finite() || v.abs() > 1e12 {
                    ok = false;
                    break;
                }
                vals.push(v);
            }
            if ok && found.as_ref().is_none_or(|f: &Vec<f64>| vals.len() > f.len()) {
                found = Some(vals);
            }
        }
        p += 1;
    }
    found
}

/// The values as a Mathcad matrix literal `[a, b, ...]`, or `None` when empty.
pub fn literal(values: &[f64]) -> Option<String> {
    if values.is_empty() {
        return None;
    }
    let mut s = String::from("[");
    for (i, v) in values.iter().enumerate() {
        if i > 0 {
            s.push_str(", ");
        }
        s.push_str(&format!("{v:?}"));
    }
    s.push(']');
    Some(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A header, the `(n-1, n)` pair and the values, as the samples store them.
    fn contents(values: &[f64]) -> Vec<u8> {
        let mut c = vec![0u8; 12];
        c.extend_from_slice(&((values.len() - 1) as u32).to_le_bytes());
        c.extend_from_slice(&(values.len() as u32).to_le_bytes());
        for v in values {
            c.extend_from_slice(&v.to_le_bytes());
        }
        c
    }

    #[test]
    fn finds_the_value_array() {
        let vals = [22.0, 48.0, 88.0, 35.0, 7.0, 98.0, 57.0, 39.0, 76.0, 81.0];
        assert_eq!(values_of(&contents(&vals)), Some(vals.to_vec()));
        assert_eq!(literal(&vals).unwrap(), "[22.0, 48.0, 88.0, 35.0, 7.0, 98.0, 57.0, 39.0, 76.0, 81.0]");
        // a lone value cannot be told apart from the header words, so it is not claimed
        assert_eq!(values_of(&contents(&[3.5])), None);
        assert_eq!(values_of(&[]), None);
    }

    #[test]
    fn ignores_a_broken_header() {
        let mut c = contents(&[1.0, 2.0]);
        c[12] = 0xff; // the n - 1 word no longer matches
        assert_eq!(values_of(&c), None);
    }
}
