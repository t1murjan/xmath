//! Debug helper: hex dump of the `Contents` stream of a component payload.
use std::io::{Cursor, Read};

fn main() {
    let path = std::env::args().nth(1).expect("payload path");
    let data = std::fs::read(&path).expect("read");
    let magic: [u8; 8] = [0xd0, 0xcf, 0x11, 0xe0, 0xa1, 0xb1, 0x1a, 0xe1];
    let at = data.windows(8).position(|w| w == magic).expect("no CFB");
    let mut file = cfb::CompoundFile::open(Cursor::new(&data[at..])).expect("open");
    let mut s = file.open_stream("/Contents").expect("stream");
    let mut buf = Vec::new();
    let _ = s.read_to_end(&mut buf);
    for (i, chunk) in buf.chunks(16).enumerate() {
        print!("{:04x}: ", i * 16);
        for b in chunk { print!("{b:02x} "); }
        for _ in chunk.len()..16 { print!("   "); }
        print!(" |");
        for b in chunk {
            let c = *b as char;
            print!("{}", if c.is_ascii_graphic() { c } else { '.' });
        }
        println!("|");
    }
}
