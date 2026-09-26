//! Braille oscilloscope.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use super::{put, signed};
use crate::tui::theme::Theme;

/// Braille dot bit for pixel `(px, py)` inside a 2x4 cell.
fn dot_bit(px: usize, py: usize) -> u8 {
    match (px, py) {
        (0, 0) => 0x01,
        (0, 1) => 0x02,
        (0, 2) => 0x04,
        (0, _) => 0x40,
        (_, 0) => 0x08,
        (_, 1) => 0x10,
        (_, 2) => 0x20,
        _ => 0x80,
    }
}

/// Signed resample to `n` points: keeps the largest-magnitude sample of each
/// bin when decimating, interpolates linearly when there are fewer samples.
fn resample_signed(values: &[f32], n: usize) -> Vec<f32> {
    if values.is_empty() {
        return vec![0.0; n];
    }
    let len = values.len();
    if len >= n {
        return (0..n)
            .map(|i| {
                let start = i * len / n;
                let end = ((i + 1) * len / n).max(start + 1).min(len);
                values[start..end].iter().fold(0.0f32, |best, v| {
                    let v = signed(*v);
                    if v.abs() > best.abs() {
                        v
                    } else {
                        best
                    }
                })
            })
            .collect();
    }
    let last = (len - 1) as f32;
    (0..n)
        .map(|i| {
            let pos = last * i as f32 / (n - 1) as f32;
            let lo = (pos.floor() as usize).min(len - 1);
            let hi = (lo + 1).min(len - 1);
            let t = pos - lo as f32;
            signed(values[lo]) * (1.0 - t) + signed(values[hi]) * t
        })
        .collect()
}

/// Draws the waveform on a 2x4-dots-per-cell grid, `+1` at the top and `-1`
/// at the bottom of the area. Consecutive points are joined with Bresenham
/// segments; silence (or no data) draws a flat line through the center.
pub(super) fn render_wave(
    data: &super::VisualizerData,
    theme: &Theme,
    area: Rect,
    buf: &mut Buffer,
) {
    let w = area.width as usize;
    let h = area.height as usize;
    let (pw, ph) = (2 * w, 4 * h);
    let points = resample_signed(&data.waveform, pw);
    let mut cells = vec![0u8; w * h];

    let ys: Vec<i32> = points
        .iter()
        .map(|s| ((1.0 - (s + 1.0) / 2.0) * (ph - 1) as f32).round() as i32)
        .collect();

    let mut plot = |x: i32, y: i32| {
        if x < 0 || y < 0 || x as usize >= pw || y as usize >= ph {
            return;
        }
        let (x, y) = (x as usize, y as usize);
        cells[(y / 4) * w + x / 2] |= dot_bit(x % 2, y % 4);
    };
    for x in 0..pw {
        let y = ys[x];
        if x == 0 {
            plot(0, y);
            continue;
        }
        // Bresenham from the previous point to this one (dx is always 1, so
        // the line is stepped along y).
        let (mut cy, ty) = (ys[x - 1], y);
        let step = if ty >= cy { 1 } else { -1 };
        let span = (ty - cy).abs();
        for k in 0..=span {
            let px = if 2 * k <= span {
                x as i32 - 1
            } else {
                x as i32
            };
            plot(px, cy);
            cy += step;
        }
        plot(x as i32, ty);
    }

    for cy in 0..h {
        for cx in 0..w {
            let bits = cells[cy * w + cx];
            if bits != 0 {
                let ch = char::from_u32(0x2800 + bits as u32).unwrap_or(' ');
                put(
                    buf,
                    area.x + cx as u16,
                    area.y + cy as u16,
                    ch,
                    theme.viz_wave(),
                );
            }
        }
    }
}
