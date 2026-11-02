//! Media helpers: artwork loading and time formatting.

use std::time::Duration;

use image::DynamicImage;
use ratatui::style::Color;
use tokio::sync::mpsc::UnboundedSender;

use crate::app::Event;

/// Largest artwork download accepted; covers are normally well under 1 MiB.
const MAX_ART_BYTES: u64 = 16 * 1024 * 1024;

/// Loads `mpris:artUrl` (file://, http(s):// or a bare path) off the async runtime and
/// reports back with `Event::Art`. `None` means the artwork could not be loaded.
pub fn load_art(url: String, tx: UnboundedSender<Event>) {
    tokio::task::spawn_blocking(move || {
        let img = read_bytes(&url).and_then(|b| image::load_from_memory(&b).map_err(|e| e.to_string())).ok();
        let accent = img.as_ref().and_then(accent_color);
        let _ = tx.send(Event::Art { url, img, accent });
    });
}

/// Picks a vivid, readable accent from the artwork: the most colorful hue family,
/// averaged, then lifted so it reads well on a dark background.
pub fn accent_color(img: &DynamicImage) -> Option<Color> {
    let thumb = img.thumbnail(32, 32).to_rgb8();
    // 12 hue buckets of (weight, r, g, b).
    let mut buckets = [(0f32, 0f32, 0f32, 0f32); 12];
    for p in thumb.pixels() {
        let [r, g, b] = p.0.map(|c| c as f32 / 255.0);
        let (h, s, v) = rgb_to_hsv(r, g, b);
        let w = s * s * v;
        if v < 0.2 || s < 0.15 {
            continue;
        }
        let bucket = &mut buckets[((h / 30.0) as usize).min(11)];
        *bucket = (bucket.0 + w, bucket.1 + r * w, bucket.2 + g * w, bucket.3 + b * w);
    }
    let (w, r, g, b) = buckets.into_iter().max_by(|a, b| a.0.total_cmp(&b.0))?;
    if w < 0.5 {
        return None; // greyscale artwork: keep the theme accent
    }
    let (h, s, v) = rgb_to_hsv(r / w, g / w, b / w);
    let (r, g, b) = hsv_to_rgb(h, s.clamp(0.35, 0.65), v.max(0.85));
    Some(Color::Rgb((r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8))
}

#[expect(clippy::float_cmp, reason = "`max` is bitwise equal to one of the channels")]
fn rgb_to_hsv(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let max = r.max(g).max(b);
    let d = max - r.min(g).min(b);
    let h = if d == 0.0 {
        0.0
    } else if max == r {
        60.0 * ((g - b) / d).rem_euclid(6.0)
    } else if max == g {
        60.0 * ((b - r) / d + 2.0)
    } else {
        60.0 * ((r - g) / d + 4.0)
    };
    (h, if max == 0.0 { 0.0 } else { d / max }, max)
}

fn hsv_to_rgb(h: f32, s: f32, v: f32) -> (f32, f32, f32) {
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0).rem_euclid(2.0) - 1.0).abs());
    let (r, g, b) = match (h / 60.0) as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    (r + v - c, g + v - c, b + v - c)
}

fn read_bytes(url: &str) -> Result<Vec<u8>, String> {
    if url.starts_with("http://") || url.starts_with("https://") {
        let mut res = ureq::get(url).call().map_err(|e| e.to_string())?;
        return res.body_mut().with_config().limit(MAX_ART_BYTES).read_to_vec().map_err(|e| e.to_string());
    }
    let path = url.strip_prefix("file://").unwrap_or(url);
    std::fs::read(percent_decode(path)).map_err(|e| e.to_string())
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = bytes.get(i + 1..i + 3).and_then(|h| std::str::from_utf8(h).ok());
        match (bytes[i], hex.and_then(|h| u8::from_str_radix(h, 16).ok())) {
            (b'%', Some(b)) => {
                out.push(b);
                i += 3;
            }
            (b, _) => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// `m:ss`, or `h:mm:ss` past an hour.
pub fn fmt_time(d: Duration) -> String {
    let s = d.as_secs();
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_and_format() {
        assert_eq!(percent_decode("/tmp/a%20b%2"), "/tmp/a b%2");
        assert_eq!(fmt_time(Duration::from_secs(75)), "1:15");
        assert_eq!(fmt_time(Duration::from_secs(3725)), "1:02:05");
    }

    #[test]
    fn accent_from_image() {
        let red = DynamicImage::ImageRgb8(image::RgbImage::from_pixel(8, 8, image::Rgb([200, 20, 20])));
        let Some(Color::Rgb(r, g, b)) = accent_color(&red) else { panic!("no accent") };
        assert!(r > 200 && g < r && b < r);
        let grey = DynamicImage::ImageRgb8(image::RgbImage::from_pixel(8, 8, image::Rgb([120, 120, 120])));
        assert_eq!(accent_color(&grey), None);
    }
}
