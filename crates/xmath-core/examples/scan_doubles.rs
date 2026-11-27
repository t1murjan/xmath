//! Debug helper: candidate double arrays inside a component's `Contents` stream.
use std::io::{Cursor, Read};

fn main() {
    let path = std::env::args().nth(1).expect("payload");
    let data = std::fs::read(&path).expect("read");
    let magic: [u8; 8] = [0xd0, 0xcf, 0x11, 0xe0, 0xa1, 0xb1, 0x1a, 0xe1];
    let at = data.windows(8).position(|w| w == magic).expect("no CFB");
    let mut file = cfb::CompoundFile::open(Cursor::new(&data[at..])).expect("open");
    let mut s = file.open_stream("/Contents").expect("stream");
    let mut buf = Vec::new();
    let _ = s.read_to_end(&mut buf);
    for off in 0..buf.len().saturating_sub(8) {
        let n = 6.min((buf.len() - off) / 8);
        let mut vals = Vec::new();
        for k in 0..n {
            let mut w = [0u8; 8];
            w.copy_from_slice(&buf[off + 8 * k..off + 8 * k + 8]);
            vals.push(f64::from_le_bytes(w));
        }
        if vals.iter().all(|v| v.is_finite() && v.abs() > 1e-12 && v.abs() < 1e6) {
            println!("{off:#06x}: {:?}", vals);
        }
    }
}
