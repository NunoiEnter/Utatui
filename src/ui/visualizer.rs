//! Visualizer widget: draws spectrum values in one of several styles.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier};
use ratatui::widgets::Widget;

use crate::config::Mode;

const EIGHTHS: [char; 9] = [' ', '▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
const ASCII_LEVELS: [char; 9] = [' ', '.', '.', ':', ':', '|', '|', '#', '#'];

pub struct Spectrum<'a> {
    pub values: &'a [f32],
    pub mode: Mode,
    pub spacing: u16,
    pub gradient: &'a [Color],
    pub ascii: bool,
}

impl Widget for Spectrum<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 || self.values.is_empty() {
            return;
        }
        match self.mode {
            Mode::Wave => self.wave(area, buf),
            Mode::Minimal => {
                let row = Rect { y: area.bottom() - 1, height: 1, ..area };
                self.bars(row, buf, false);
            }
            Mode::Mirror => self.mirror(area, buf),
            Mode::Bars => self.bars(area, buf, false),
            Mode::Blocks => self.bars(area, buf, true),
        }
    }
}

impl Spectrum<'_> {
    /// Color for a row at `t` (0 = bottom, 1 = top), interpolating between gradient stops.
    fn color(&self, t: f32) -> Color {
        let g = self.gradient;
        match g.len() {
            0 => Color::Reset,
            1 => g[0],
            n => {
                let x = t.clamp(0.0, 1.0) * (n - 1) as f32;
                let i = (x as usize).min(n - 2);
                lerp(g[i], g[i + 1], x - i as f32)
            }
        }
    }

    /// Bar columns that fit `width`: (x offset, bar width, resampled values).
    fn layout(&self, width: u16) -> (u16, u16, Vec<f32>) {
        let s = self.spacing;
        let fit = ((width + s) / (1 + s)).max(1) as usize;
        let values = resample(self.values, fit.min(self.values.len()));
        let n = values.len() as u16;
        let bw = ((width.saturating_sub(s * (n - 1))) / n).max(1);
        let total = n * bw + s * (n - 1);
        ((width.saturating_sub(total)) / 2, bw, values)
    }

    fn bars(&self, area: Rect, buf: &mut Buffer, blocks: bool) {
        let (off, bw, values) = self.layout(area.width);
        let h = area.height as f32;
        for (i, v) in values.iter().enumerate() {
            let x0 = area.x + off + i as u16 * (bw + self.spacing);
            let level = (v.clamp(0.0, 1.0) * h * 8.0).round() as u32;
            for row in 0..area.height {
                let y = area.bottom() - 1 - row;
                let fill = level.saturating_sub(row as u32 * 8).min(8) as usize;
                let ch = if blocks {
                    if fill >= 4 { if self.ascii { '#' } else { '■' } } else { ' ' }
                } else if self.ascii {
                    ASCII_LEVELS[fill]
                } else {
                    EIGHTHS[fill]
                };
                // A dim baseline under silent bars, so silence doesn't look like a broken widget.
                let (ch, modifier) = match ch {
                    ' ' if row == 0 => (if self.ascii { '_' } else { '▁' }, Modifier::DIM),
                    ' ' => continue,
                    ch => (ch, Modifier::empty()),
                };
                let fg = self.color(row as f32 / (h - 1.0).max(1.0));
                for x in x0..(x0 + bw).min(area.right()) {
                    buf[(x, y)].set_char(ch).set_fg(fg).set_style(modifier);
                }
            }
        }
    }

    /// Bars growing up and down from a center line, at half-cell resolution below it.
    fn mirror(&self, area: Rect, buf: &mut Buffer) {
        let top_h = area.height.div_ceil(2);
        let top = Rect { height: top_h, ..area };
        self.bars(top, buf, false);
        let bottom_h = area.height - top_h;
        let (off, bw, values) = self.layout(area.width);
        for (i, v) in values.iter().enumerate() {
            let x0 = area.x + off + i as u16 * (bw + self.spacing);
            let level = (v.clamp(0.0, 1.0) * bottom_h as f32 * 2.0).round() as u16;
            for row in 0..bottom_h {
                let fill = level.saturating_sub(row * 2).min(2);
                let ch = match (fill, self.ascii) {
                    (0, _) => continue,
                    (1, false) => '▀',
                    (_, false) => '█',
                    (_, true) => ':',
                };
                // Reflection fades toward the bottom gradient stop.
                let fg = self.color(row as f32 / (top_h as f32).max(1.0));
                for x in x0..(x0 + bw).min(area.right()) {
                    buf[(x, area.y + top_h + row)].set_char(ch).set_fg(fg);
                }
            }
        }
    }

    /// A continuous line tracing the spectrum, drawn with braille dots (2×4 per cell).
    fn wave(&self, area: Rect, buf: &mut Buffer) {
        if self.ascii {
            return self.bars(area, buf, true);
        }
        let px_w = area.width as usize * 2;
        let px_h = area.height as usize * 4;
        let values = resample_smooth(self.values, px_w);
        let mut prev: Option<usize> = None;
        let mut cells = vec![0u8; area.width as usize * area.height as usize];
        for (x, v) in values.iter().enumerate() {
            let y = px_h - 1 - (v.clamp(0.0, 1.0) * (px_h - 1) as f32).round() as usize;
            // Fill vertically to the previous point so the line stays connected.
            let (a, b) = prev.map_or((y, y), |p| (p.min(y), p.max(y)));
            for yy in a..=b {
                let (cx, cy) = (x / 2, yy / 4);
                cells[cy * area.width as usize + cx] |= braille_bit(x % 2, yy % 4);
            }
            prev = Some(y);
        }
        for (i, bits) in cells.iter().enumerate().filter(|(_, b)| **b != 0) {
            let (cx, cy) = (i % area.width as usize, i / area.width as usize);
            let ch = char::from_u32(0x2800 + *bits as u32).unwrap_or(' ');
            let t = 1.0 - cy as f32 / (area.height as f32 - 1.0).max(1.0);
            buf[(area.x + cx as u16, area.y + cy as u16)].set_char(ch).set_fg(self.color(t));
        }
    }
}

fn braille_bit(dx: usize, dy: usize) -> u8 {
    const BITS: [[u8; 4]; 2] = [[0x01, 0x02, 0x04, 0x40], [0x08, 0x10, 0x20, 0x80]];
    BITS[dx][dy]
}

/// Averages `v` into `n` buckets (n <= `v.len()`).
fn resample(v: &[f32], n: usize) -> Vec<f32> {
    if n == 0 || n >= v.len() {
        return v.to_vec();
    }
    (0..n)
        .map(|i| {
            let (a, b) = (i * v.len() / n, ((i + 1) * v.len() / n).max(i * v.len() / n + 1));
            v[a..b].iter().sum::<f32>() / (b - a) as f32
        })
        .collect()
}

/// Linear interpolation of `v` onto `n` evenly spaced points.
fn resample_smooth(v: &[f32], n: usize) -> Vec<f32> {
    if v.len() < 2 || n < 2 {
        return vec![v.first().copied().unwrap_or(0.0); n];
    }
    (0..n)
        .map(|i| {
            let x = i as f32 * (v.len() - 1) as f32 / (n - 1) as f32;
            let j = (x as usize).min(v.len() - 2);
            v[j] + (v[j + 1] - v[j]) * (x - j as f32)
        })
        .collect()
}

pub fn lerp(a: Color, b: Color, t: f32) -> Color {
    match (a, b) {
        (Color::Rgb(r1, g1, b1), Color::Rgb(r2, g2, b2)) => {
            let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
            Color::Rgb(m(r1, r2), m(g1, g2), m(b1, b2))
        }
        _ if t < 0.5 => a,
        _ => b,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resampling_and_layout() {
        assert_eq!(resample(&[1.0, 3.0, 5.0, 7.0], 2), vec![2.0, 6.0]);
        assert_eq!(resample_smooth(&[0.0, 1.0], 3), vec![0.0, 0.5, 1.0]);
        let s = Spectrum { values: &[0.5; 10], mode: Mode::Bars, spacing: 1, gradient: &[], ascii: false };
        // 10 bars, width 1, spacing 1 = 19 cols, centered in 21.
        assert_eq!(s.layout(21).0, 1);
        assert_eq!(s.layout(5).2.len(), 3); // too narrow: bars merge
        let mut buf = Buffer::empty(Rect::new(0, 0, 30, 6));
        for mode in Mode::ALL {
            Spectrum { mode, ..s }.render(buf.area, &mut buf);
        }
    }

    #[test]
    fn silence_draws_baseline() {
        let s = Spectrum { values: &[0.0; 8], mode: Mode::Bars, spacing: 1, gradient: &[], ascii: false };
        let mut buf = Buffer::empty(Rect::new(0, 0, 20, 4));
        s.render(buf.area, &mut buf);
        // 8 bars, width 1, spacing 1: 15 columns centered in 20, so bars sit at x = 2, 4, ...
        assert_eq!(buf[(2, 3)].symbol(), "▁");
        assert_eq!(buf[(3, 3)].symbol(), " ");
        assert_eq!(buf[(2, 2)].symbol(), " ");
    }

    #[test]
    fn gradient() {
        assert_eq!(lerp(Color::Rgb(0, 0, 0), Color::Rgb(100, 200, 50), 0.5), Color::Rgb(50, 100, 25));
    }
}
