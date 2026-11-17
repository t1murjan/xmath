//! Picture regions (imported Mathcad pictures and plots that could not be
//! decoded): PNG/JPEG data decoded once into an egui texture per region.

use base64::Engine;
use egui::{Color32, ColorImage, Context, Painter, Pos2, Rect, TextureHandle, TextureOptions};
use std::collections::HashMap;

/// Textures of the picture regions, keyed by region id.
#[derive(Default)]
pub struct ImageCache {
    /// id -> (fingerprint of the data, texture; `None` when decoding failed)
    map: HashMap<u64, (u64, Option<TextureHandle>)>,
}

fn fingerprint(b64: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    b64.len().hash(&mut h);
    b64.as_bytes().iter().take(96).for_each(|b| b.hash(&mut h));
    b64.as_bytes().iter().rev().take(96).for_each(|b| b.hash(&mut h));
    h.finish()
}

/// Decodes base64 image data (PNG) into an egui image.
pub fn decode(b64: &str) -> Option<ColorImage> {
    let bytes = base64::engine::general_purpose::STANDARD.decode(b64.as_bytes()).ok()?;
    let img = ::image::load_from_memory(&bytes).ok()?.to_rgba8();
    let size = [img.width() as usize, img.height() as usize];
    Some(ColorImage::from_rgba_unmultiplied(size, img.as_raw()))
}

impl ImageCache {
    /// The texture of region `id`, decoded on first use (and again when the data changes).
    pub fn texture(&mut self, ctx: &Context, id: u64, b64: &str) -> Option<&TextureHandle> {
        let fp = fingerprint(b64);
        let stale = self.map.get(&id).map_or(true, |(f, _)| *f != fp);
        if stale {
            let tex = decode(b64).map(|img| ctx.load_texture(format!("image-{id}"), img, TextureOptions::LINEAR));
            self.map.insert(id, (fp, tex));
        }
        self.map.get(&id).and_then(|(_, t)| t.as_ref())
    }

    /// Forgets textures of regions that no longer exist.
    pub fn retain(&mut self, alive: impl Fn(u64) -> bool) {
        self.map.retain(|id, _| alive(*id));
    }
}

/// Draws the picture (or a frame with a note when it cannot be decoded).
pub fn paint(painter: &Painter, rect: Rect, tex: Option<&TextureHandle>) {
    match tex {
        Some(t) => {
            painter.image(t.id(), rect, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
        }
        None => {
            painter.rect_stroke(rect, 0.0, egui::Stroke::new(1.0_f32, Color32::GRAY), egui::StrokeKind::Inside);
            painter.text(rect.center(), egui::Align2::CENTER_CENTER, "[рисунок]", egui::FontId::proportional(13.0), Color32::GRAY);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_a_png() {
        let img = ::image::RgbaImage::from_pixel(3, 2, ::image::Rgba([255, 0, 0, 255]));
        let mut bytes = std::io::Cursor::new(Vec::new());
        img.write_to(&mut bytes, ::image::ImageFormat::Png).unwrap();
        let b64 = base64::engine::general_purpose::STANDARD.encode(bytes.into_inner());
        let c = decode(&b64).expect("png");
        assert_eq!(c.size, [3, 2]);
        assert!(decode("not base64!").is_none());
        assert!(decode("AAAA").is_none());
    }
}
