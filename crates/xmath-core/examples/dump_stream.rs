//! Debug helper: dump a stream of a Mathcad component payload as f64 words.
use std::io::{Cursor, Read};

fn main() {
    let path = std::env::args().nth(1).expect("payload path");
    let data = std::fs::read(&path).expect("read");
    let magic: [u8; 8] = [0xd0, 0xcf, 0x11, 0xe0, 0xa1, 0xb1, 0x1a, 0xe1];
    let at = data.windows(8).position(|w| w == magic).expect("no CFB");
    let mut file = cfb::CompoundFile::open(Cursor::new(&data[at..])).expect("open");
    let paths: Vec<String> = file.walk().map(|e| e.path().to_string_lossy().into_owned()).collect();
    for p in paths {
        if p == "/" { continue }
        let Ok(mut s) = file.open_stream(&p) else { continue };
        let mut buf = Vec::new();
        let _ = s.read_to_end(&mut buf);
        println!("{p}: {} bytes", buf.len());
        if buf.len() > 4000 { continue }
        for (k, chunk) in buf.chunks(8).enumerate().take(120) {
            let mut w = [0u8; 8];
            w.copy_from_slice(chunk);
            let f = f64::from_le_bytes(w);
            let i = u64::from_le_bytes(w);
            println!("  {k:4} @{:#06x} u32={:>12} {:>10} f64={f:.6e}", k * 8, (i & 0xffff_ffff) as u32, i);
        }
    }
}
