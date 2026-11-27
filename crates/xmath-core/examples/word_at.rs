//! Debug helper: f64 words of a component's `Contents` from a byte offset.
use std::io::{Cursor, Read};

fn main() {
    let path = std::env::args().nth(1).expect("payload");
    let off: usize = std::env::args().nth(2).and_then(|s| s.parse().ok()).unwrap_or(0);
    let count: usize = std::env::args().nth(3).and_then(|s| s.parse().ok()).unwrap_or(16);
    let data = std::fs::read(&path).expect("read");
    let magic: [u8; 8] = [0xd0, 0xcf, 0x11, 0xe0, 0xa1, 0xb1, 0x1a, 0xe1];
    let at = data.windows(8).position(|w| w == magic).expect("no CFB");
    let mut file = cfb::CompoundFile::open(Cursor::new(&data[at..])).expect("open");
    let mut s = file.open_stream("/Contents").expect("stream");
    let mut buf = Vec::new();
    let _ = s.read_to_end(&mut buf);
    println!("len {}", buf.len());
    for k in 0..count {
        let o = off + k * 8;
        if o + 8 > buf.len() { break }
        let mut w = [0u8; 8];
        w.copy_from_slice(&buf[o..o + 8]);
        println!("{o:#06x}: {:?}", f64::from_le_bytes(w));
    }
}
