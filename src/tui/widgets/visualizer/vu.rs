//! Two horizontal level meters.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Modifier;

use super::{put, unit};
use crate::tui::theme;

/// Width of the `"L "` label column; dropped when the area is too narrow.
const LABEL_WIDTH: u16 = 2;
/// Minimum area width for labels.
const LABEL_MIN_WIDTH: u16 = 6;

/// Draws one meter on row `y`: label, then `▮` segments up to `level`, `▯`
/// for the rest and a bold `▮` tick at the `hold` position when it is above
/// the level. The scale is linear: `level` 0..1 maps to the whole bar.
fn meter(buf: &mut Buffer, area: Rect, y: u16, label: char, level: f32, hold: f32) {
    let labelled = area.width >= LABEL_MIN_WIDTH;
    let offset = if labelled { LABEL_WIDTH } else { 0 };
    if labelled {
        put(buf, area.x, y, label, theme::dim());
    }
    let n = (area.width - offset) as usize;
    let filled = (unit(level) * n as f32).round() as usize;
    let tick = (unit(hold) * n as f32).round() as usize;
    for i in 0..n {
        let x = area.x + offset + i as u16;
        if i < filled {
            put(buf, x, y, '▮', theme::accent());
        } else if tick > filled && i + 1 == tick {
            put(buf, x, y, '▮', theme::accent().add_modifier(Modifier::BOLD));
        } else {
            put(buf, x, y, '▯', theme::dim());
        }
    }
}

/// Vu: L and R meters centered vertically (one blank row between them when
/// the height is at least 4). With a single row only one meter fits, labelled
/// `M`, showing the louder channel.
pub(super) fn render_vu(data: &super::VisualizerData, area: Rect, buf: &mut Buffer) {
    if area.height == 1 {
        meter(
            buf,
            area,
            area.y,
            'M',
            unit(data.level_l).max(unit(data.level_r)),
            unit(data.hold_l).max(unit(data.hold_r)),
        );
        return;
    }
    let gap = u16::from(area.height >= 4);
    let y0 = area.y + (area.height - (2 + gap)) / 2;
    meter(buf, area, y0, 'L', data.level_l, data.hold_l);
    meter(buf, area, y0 + 1 + gap, 'R', data.level_r, data.hold_r);
}
