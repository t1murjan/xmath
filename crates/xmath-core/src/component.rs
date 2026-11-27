//! Mathcad's data components (Data Table, Input Table): an `<component>` region carries an
//! embedded OLE object whose `Contents` stream holds the table values as `f64`. The worksheet
//! then refers to the table by the name in the component's `<outputs>`.
//!
//! Only the value array is decoded here. Layout of `Contents` as observed in the shipped
//! samples: a header, then two `u32` — `n - 1` and `n` — followed by `n` little-endian doubles.
//! A table with several columns is stored as several components, one per output name.

use std::io::{Cursor, Read};

const CFB_MAGIC: [u8; 8] = [0xd0, 0xcf, 0x11, 0xe0, 0xa1, 0xb1, 0x1a, 0xe1];

/// A decoded data table: the number of rows and columns and the values row by row.
pub struct Table {
    pub rows: usize,
    pub cols: usize,
    pub values: Vec<f64>,
}

/// The table of a component payload, if it holds a decodable `Contents` stream.
pub fn table(payload: &[u8]) -> Option<Table> {
    let at = payload.windows(CFB_MAGIC.len()).position(|w| w == CFB_MAGIC)?;
    let mut file = cfb::CompoundFile::open(Cursor::new(&payload[at..])).ok()?;
    let mut stream = file.open_stream("Contents").ok()?;
    let mut contents = Vec::new();
    stream.read_to_end(&mut contents).ok()?;
    // a "read from file" component holds the file name, not a table
    if has_file_name(&contents) {
        return None;
    }
    table_of(&contents)
}

/// Whether a `Contents` stream names a data file (`bigdata.prn`, `data.txt`, …): such a
/// component reads its values from that file, and the file is usually not shipped.
fn has_file_name(c: &[u8]) -> bool {
    for ext in ["prn", "dat", "txt", "csv", "xls", "xlsx"] {
        // UTF-16LE text of ".ext": the dot and the first letter share one 16-bit word
        let mut pat: Vec<u8> = Vec::new();
        pat.push(b'.');
        pat.push(0);
        for ch in ext.chars() {
            pat.push(ch as u8);
            pat.push(0);
        }
        if c.windows(pat.len()).any(|w| w == pat.as_slice()) {
            return true;
        }
    }
    false
}

/// The array behind a `Contents` stream: the `(n-1, n)` pair with the largest `n` whose `n`
/// doubles are finite. Header words can form small false pairs (`(0, 1)`), so a lone value is
/// not enough — every table in the shipped samples holds several.
pub fn table_of(c: &[u8]) -> Option<Table> {
    // prefer a candidate whose numbers look like data (no denormal padding) and is longer
    let mut found: Option<(Table, bool)> = None;
    let mut p = 4;
    while p + 4 + 8 <= c.len() {
        let n = u32::from_le_bytes([c[p], c[p + 1], c[p + 2], c[p + 3]]) as usize;
        let rows = u32::from_le_bytes([c[p - 4], c[p - 3], c[p - 2], c[p - 1]]) as usize + 1;
        if n >= 2 && n <= 100_000 && rows >= 1 && n % rows == 0 && p + 4 + 8 * n <= c.len() {
            let mut vals = Vec::with_capacity(n);
            let mut ok = true;
            let mut clean = true;
            for k in 0..n {
                let at = p + 4 + 8 * k;
                let Ok(bytes) = c[at..at + 8].try_into() else {
                    ok = false;
                    break;
                };
                let v = f64::from_le_bytes(bytes);
                if !v.is_finite() || v.abs() > 1e12 {
                    ok = false;
                    break;
                }
                // denormals are the padding Mathcad writes after the array, never data
                if v != 0.0 && v.abs() < 1e-300 {
                    clean = false;
                }
                vals.push(v);
            }
            if ok {
                let better = match &found {
                    None => true,
                    Some((f, f_clean)) => match (*f_clean, clean) {
                        (false, true) => true,
                        (true, false) => false,
                        _ => vals.len() > f.values.len(),
                    },
                };
                if better {
                    found = Some((Table { rows, cols: n / rows, values: vals }, clean));
                }
            }
        }
        p += 1;
    }
    found.map(|(t, _)| t)
}

/// The table as a Mathcad matrix literal `[a, b; c, d]`, or `None` when empty. The stored
/// values are column by column, like every matrix in Mathcad's XML.
pub fn literal(t: &Table) -> Option<String> {
    if t.values.is_empty() || t.rows == 0 || t.cols == 0 {
        return None;
    }
    let mut s = String::from("[");
    for r in 0..t.rows {
        if r > 0 {
            s.push_str("; ");
        }
        for c in 0..t.cols {
            if c > 0 {
                s.push_str(", ");
            }
            s.push_str(&format!("{:?}", t.values[c * t.rows + r]));
        }
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
        let t = table_of(&contents(&vals)).expect("table");
        assert_eq!((t.rows, t.cols), (10, 1));
        assert_eq!(t.values, vals.to_vec());
        // one column: a column vector, so the rows are separated by ';'
        assert_eq!(literal(&t).unwrap(), "[22.0; 48.0; 88.0; 35.0; 7.0; 98.0; 57.0; 39.0; 76.0; 81.0]");
        // a lone value cannot be told apart from the header words, so it is not claimed
        assert!(table_of(&contents(&[3.5])).is_none());
        assert!(table_of(&[]).is_none());
    }

    #[test]
    fn a_two_column_table_is_a_matrix() {
        // linregre.xmcd: 14 rows, two columns, the header pair is (rows-1, rows*cols)
        let vals: Vec<f64> = (0..28).map(|i| i as f64).collect();
        let mut c = vec![0u8; 12];
        c.extend_from_slice(&13u32.to_le_bytes());
        c.extend_from_slice(&28u32.to_le_bytes());
        for v in &vals {
            c.extend_from_slice(&v.to_le_bytes());
        }
        let t = table_of(&c).expect("table");
        assert_eq!((t.rows, t.cols), (14, 2));
        // column by column in the stream: the first 14 values are the first column
        let lit = literal(&t).unwrap();
        assert!(lit.starts_with("[0.0, 14.0; 1.0, 15.0;"), "{lit}");
        assert!(lit.ends_with("13.0, 27.0]"), "{lit}");
    }

    #[test]
    fn ignores_a_broken_header() {
        let mut c = contents(&[1.0, 2.0]);
        c[12] = 0xff; // the rows word no longer divides the count
        assert!(table_of(&c).is_none());
    }
}
