//! How many Mathcad data components of a corpus directory decode (`cargo run --example components_scan -- DIR`).
use std::io::{Cursor, Read};

fn main() {
    let dir = std::env::args().nth(1).expect("corpus directory");
    let (mut ok, mut fail, mut files) = (0, 0, 0);
    let mut by: std::collections::BTreeMap<String, (usize, usize)> = Default::default();
    let mut entries: Vec<_> = std::fs::read_dir(&dir).unwrap().filter_map(|e| e.ok()).map(|e| e.path()).collect();
    entries.sort();
    for p in entries {
        if p.extension().and_then(|e| e.to_str()) != Some("xmcd") {
            continue;
        }
        let text = std::fs::read_to_string(&p).unwrap_or_default();
        let Ok(doc) = roxmltree::Document::parse(&text) else { continue };
        files += 1;
        // item-id -> payload
        let mut items = std::collections::HashMap::new();
        for el in doc.descendants().filter(|n| n.is_element() && n.tag_name().name() == "item") {
            let Some(id) = el.attribute("item-id") else { continue };
            let raw: String = el.text().unwrap_or("").chars().filter(|c| !c.is_whitespace()).collect();
            if let Ok(bytes) = base64_decode(&raw) {
                let data = if el.attribute("content-encoding") == Some("gzip") {
                    match gunzip(&bytes) {
                        Some(d) => d,
                        None => continue,
                    }
                } else {
                    bytes
                };
                items.insert(id.to_string(), data);
            }
        }
        for comp in doc.descendants().filter(|n| n.is_element() && n.tag_name().name() == "component") {
            let Some(id) = comp.attribute("item-idref") else { continue };
            let Some(data) = items.get(id) else { continue };
            let buddy = comp.attribute("clsid-buddy").unwrap_or("(none)").to_lowercase();
            let entry = by.entry(buddy).or_default();
            if xmath_core::component::table(data).is_some() {
                ok += 1;
                entry.0 += 1;
            } else {
                entry.1 += 1;
                fail += 1;
                if std::env::var("VERBOSE").is_ok() && fail < 40 {
                    let names = stream_names(data);
                    println!("FAIL {} component {} buddy {:?} streams {:?}", p.file_name().unwrap().to_string_lossy(), id, comp.attribute("clsid-buddy"), names);
                }
            }
        }
    }
    println!("{files} files: {ok} components decoded, {fail} not");
    for (k, (a, b)) in &by {
        println!("  {k}: {a} decoded, {b} not");
    }
}

fn stream_names(data: &[u8]) -> Vec<String> {
    let magic: [u8; 8] = [0xd0, 0xcf, 0x11, 0xe0, 0xa1, 0xb1, 0x1a, 0xe1];
    let Some(at) = data.windows(8).position(|w| w == magic) else { return vec!["(no CFB)".into()] };
    let Ok(mut file) = cfb::CompoundFile::open(Cursor::new(&data[at..])) else { return vec!["(cfb error)".into()] };
    let mut out = Vec::new();
    let paths: Vec<String> = file.walk().map(|e| e.path().to_string_lossy().into_owned()).collect();
    for path in paths {
        if path == "/" {
            continue;
        }
        if let Ok(mut s) = file.open_stream(&path) {
            let mut buf = Vec::new();
            let _ = s.read_to_end(&mut buf);
            out.push(format!("{path}:{}", buf.len()));
        } else {
            out.push(format!("{path}/"));
        }
    }
    out
}

fn base64_decode(s: &str) -> Result<Vec<u8>, ()> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.decode(s).map_err(|_| ())
}

fn gunzip(b: &[u8]) -> Option<Vec<u8>> {
    use std::io::Read;
    let mut out = Vec::new();
    flate2::read::GzDecoder::new(b).read_to_end(&mut out).ok()?;
    Some(out)
}
