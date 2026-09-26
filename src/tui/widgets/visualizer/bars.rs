//! Column-based styles: Bars, Mirror, Dots and Area.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Modifier;

use super::{bar_layout, put, ramp, resample, unit, BLOCKS};
use crate::tui::theme;

/// Character filling `eighths` (1..=8) of a cell from the bottom.
fn block(eighths: usize) -> char {
    BLOCKS[eighths.clamp(1, 8) - 1]
}

/// Number of eighth-steps for `level` over `rows` cells (`8 * rows` total).
fn steps(level: f32, rows: usize) -> usize {
    (unit(level) * (8 * rows) as f32).round() as usize
}

/// Bars: 1-column bars (with a 1-column gap when wide enough) using eighth
/// blocks. Silence draws nothing (blank cells).
pub(super) fn render_bars(data: &super::VisualizerData, area: Rect, buf: &mut Buffer) {
    let (count, stride) = bar_layout(area.width);
    let levels = resample(&data.bands, count);
    let h = area.height as usize;
    for (i, level) in levels.iter().enumerate() {
        let x = area.x + (i * stride) as u16;
        let total = steps(*level, h);
        for r in 0..h {
            let cell = total.saturating_sub(8 * r).min(8);
            if cell == 0 {
                break;
            }
            let y = area.bottom() - 1 - r as u16;
            put(buf, x, y, block(cell), ramp((r + 1) as f32 / h as f32));
        }
    }
}

/// Mirror: bars grow up and down from the horizontal center line.
///
/// Upper half: `▁..█` fill the cell from its bottom (next to the center).
/// Lower half: the fill must hang from the cell's top, which no block glyph
/// offers below `▀`. Technique: draw the complementary lower block
/// (`8 - n` eighths) with `Modifier::REVERSED`, which swaps foreground and
/// the terminal's default background, so the *top* `n/8` shows in the accent
/// color. This uses the same glyphs as the upper half, so it renders in every
/// font that has `▁..█`; no explicit background color is ever set.
///
/// With an odd height the middle row is lit (`█`) whenever the bar is
/// non-zero; with height 1 the row shows the bottom block for the level.
pub(super) fn render_mirror(data: &super::VisualizerData, area: Rect, buf: &mut Buffer) {
    let (count, stride) = bar_layout(area.width);
    let levels = resample(&data.bands, count);
    let h = area.height as usize;
    let half = h / 2;
    let odd = h % 2 == 1;
    for (i, level) in levels.iter().enumerate() {
        let x = area.x + (i * stride) as u16;
        if half == 0 {
            let s = steps(*level, 1);
            if s > 0 {
                put(buf, x, area.y, block(s), ramp(1.0));
            }
            continue;
        }
        let total = steps(*level, half);
        if odd && total > 0 {
            put(buf, x, area.y + half as u16, '█', ramp(0.0));
        }
        for r in 0..half {
            let cell = total.saturating_sub(8 * r).min(8);
            if cell == 0 {
                break;
            }
            let style = ramp((r + 1) as f32 / half as f32);
            let up_y = area.y + (half - 1 - r) as u16;
            let down_y = area.y + (half + usize::from(odd) + r) as u16;
            put(buf, x, up_y, block(cell), style);
            if cell == 8 {
                put(buf, x, down_y, '█', style);
            } else {
                put(
                    buf,
                    x,
                    down_y,
                    block(8 - cell),
                    style.add_modifier(Modifier::REVERSED),
                );
            }
        }
    }
}

/// Dots: a column of dim `·` up to each band's level (brighter head dot) and a
/// `▪` peak marker at the `peaks` height (`bands` when `peaks` is empty).
pub(super) fn render_dots(data: &super::VisualizerData, area: Rect, buf: &mut Buffer) {
    let (count, stride) = bar_layout(area.width);
    let levels = resample(&data.bands, count);
    let peak_src = if data.peaks.is_empty() {
        &data.bands
    } else {
        &data.peaks
    };
    let peaks = resample(peak_src, count);
    let h = area.height as usize;
    for i in 0..count {
        let x = area.x + (i * stride) as u16;
        let rows = (unit(levels[i]) * h as f32).round() as usize;
        for r in 0..rows.min(h) {
            let style = if r + 1 == rows {
                theme::accent()
            } else {
                theme::dim()
            };
            put(buf, x, area.bottom() - 1 - r as u16, '·', style);
        }
        let peak_rows = ((unit(peaks[i]) * h as f32).round() as usize).min(h);
        if peak_rows >= 1 {
            let y = area.bottom() - peak_rows as u16;
            put(buf, x, y, '▪', theme::accent().add_modifier(Modifier::BOLD));
        }
    }
}

/// Area: one column per band sample (linear interpolation), top edge in
/// eighth blocks, dim full blocks below it.
pub(super) fn render_area(data: &super::VisualizerData, area: Rect, buf: &mut Buffer) {
    let levels = resample(&data.bands, area.width as usize);
    let h = area.height as usize;
    for (i, level) in levels.iter().enumerate() {
        let x = area.x + i as u16;
        let total = steps(*level, h);
        for r in 0..h {
            let cell = total.saturating_sub(8 * r).min(8);
            if cell == 0 {
                break;
            }
            let top = total <= 8 * (r + 1);
            let style = if top {
                theme::accent().add_modifier(Modifier::BOLD)
            } else {
                theme::accent().add_modifier(Modifier::DIM)
            };
            put(buf, x, area.bottom() - 1 - r as u16, block(cell), style);
        }
    }
}
