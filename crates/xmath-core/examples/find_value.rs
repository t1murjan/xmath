//! Debug helper: find a little-endian f64 inside a component's `Contents` stream.
use std::io::{Cursor, Read};

fn main() {
    let path = std::env::args().nth(1).expect("payload");
    let want: f64 = std::env::args().nth(2).expect("value").parse().expect("number");
    let data = std::fs::read(&path).expect("read");
    let magic: [u8; 8] = [0xd0, 0xcf, 0x11, 0xe0, 0xa1, 0xb1, 0x1a, 0xe1];
    let at = data.windows(8).position(|w| w == magic).expect("no CFB");
    let mut file = cfb::CompoundFile::open(Cursor::new(&data[at..])).expect("open");
    let mut s = file.open_stream("/Contents").expect("stream");
    let mut buf = Vec::new();
    let _ = s.read_to_end(&mut buf);
    let pat = want.to_le_bytes();
    let mut hits = Vec::new();
    for i in 0..buf.len().saturating_sub(8) {
        if buf[i..i + 8] == pat {
            hits.push(i);
        }
    }
    println!("{want}: {hits:x?} (stream {} bytes)", buf.len());
}
