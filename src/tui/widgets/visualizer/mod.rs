//! Audio visualizer widgets: six interchangeable styles drawn from a plain
//! [`VisualizerData`] snapshot. Pure drawing code: no I/O, no audio, no state.
//!
//! Common guarantees for every style:
//! - any `Rect` is accepted (it is clipped to the buffer; empty areas draw
//!   nothing) and any data length, including empty slices;
//! - non-finite values count as 0 and everything is clamped to `0..=1`
//!   (`-1..=1` for the waveform);
//! - only single-width characters are used and no background color is set;
//!   the look comes from the viz roles of the [`Theme`] plus bold/dim modifiers.
//!
//! Widgets take the whole `&Theme` (it is `Copy` and cheap) instead of a
//! separate palette struct, and only ask it for styles, never for `Color`s.

mod bars;
mod vu;
mod wave;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::widgets::Widget;

use crate::audio_tap::Levels;
use crate::operations::spectrum::SpectrumAnalyzer;
use crate::tui::theme::Theme;

/// Eighth-height blocks, index `n - 1` fills `n/8` of a cell from the bottom.
pub(super) const BLOCKS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

/// Upper bound of analyzer bands any style asks for.
pub const MAX_BANDS: usize = 128;
/// Bands requested by styles that are not band based (Wave, Vu).
const SMALL_BANDS: usize = 8;
/// Minimum area width for bars to be separated by a one-column gap.
const GAP_MIN_WIDTH: u16 = 6;

/// Snapshot of everything the visualizer can draw.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct VisualizerData {
    /// Band levels, `0..=1`, low to high frequency.
    pub bands: Vec<f32>,
    /// Peak-hold level per band, `0..=1` (animated by the caller). May be
    /// empty, in which case the styles that show peaks use `bands`.
    pub peaks: Vec<f32>,
    /// Recent mono samples, `-1..=1`, any length.
    pub waveform: Vec<f32>,
    /// Left channel level, `0..=1`.
    pub level_l: f32,
    /// Right channel level, `0..=1`.
    pub level_r: f32,
    /// Peak-hold tick of the left meter (Vu); 0 shows no tick.
    pub hold_l: f32,
    /// Peak-hold tick of the right meter (Vu); 0 shows no tick.
    pub hold_r: f32,
}

impl VisualizerData {
    /// Builds a snapshot from the analyzer state, recent mono samples and the
    /// tap levels (peak amplitude per channel).
    pub fn from_analysis(analyzer: &SpectrumAnalyzer, waveform: Vec<f32>, levels: Levels) -> Self {
        Self {
            bands: analyzer.bands().to_vec(),
            peaks: analyzer.peaks().to_vec(),
            waveform,
            level_l: levels.left_peak,
            level_r: levels.right_peak,
            hold_l: 0.0,
            hold_r: 0.0,
        }
    }
}

/// Visual style of the visualizer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VisualStyle {
    /// Vertical bars with eighth-cell steps.
    #[default]
    Bars,
    /// Bars grown symmetrically from the horizontal center line.
    Mirror,
    /// Braille oscilloscope of the waveform.
    Wave,
    /// Columns of dots with a peak marker.
    Dots,
    /// Smooth filled silhouette of the spectrum.
    Area,
    /// Two horizontal level meters (L and R).
    Vu,
}

impl VisualStyle {
    /// Every style, in rotation order.
    pub const ALL: [VisualStyle; 6] = [
        VisualStyle::Bars,
        VisualStyle::Mirror,
        VisualStyle::Wave,
        VisualStyle::Dots,
        VisualStyle::Area,
        VisualStyle::Vu,
    ];

    fn index(self) -> usize {
        Self::ALL.iter().position(|s| *s == self).unwrap_or(0)
    }

    /// Next style, wrapping around.
    pub fn next(self) -> Self {
        Self::ALL[(self.index() + 1) % Self::ALL.len()]
    }

    /// Previous style, wrapping around.
    #[cfg(test)]
    pub fn prev(self) -> Self {
        Self::ALL[(self.index() + Self::ALL.len() - 1) % Self::ALL.len()]
    }

    /// Lowercase English name, used in the UI and the config file.
    pub fn name(self) -> &'static str {
        match self {
            VisualStyle::Bars => "bars",
            VisualStyle::Mirror => "mirror",
            VisualStyle::Wave => "wave",
            VisualStyle::Dots => "dots",
            VisualStyle::Area => "area",
            VisualStyle::Vu => "vu",
        }
    }

    /// Parses a style name, ignoring case and surrounding whitespace.
    pub fn from_name(name: &str) -> Option<Self> {
        let name = name.trim();
        Self::ALL
            .into_iter()
            .find(|s| s.name().eq_ignore_ascii_case(name))
    }
}

/// Bar count and column stride for `width` columns: one-column gaps when the
/// area is wide enough, otherwise adjacent bars.
pub(super) fn bar_layout(width: u16) -> (usize, usize) {
    if width == 0 {
        (0, 1)
    } else if width >= GAP_MIN_WIDTH {
        ((width as usize).div_ceil(2), 2)
    } else {
        (width as usize, 1)
    }
}

/// How many analyzer bands `style` wants for an area `area_width` wide.
/// Band-based styles ask for one band per bar (or per column for Area), capped
/// at [`MAX_BANDS`]; Wave and Vu only need a few.
pub fn bands_wanted(style: VisualStyle, area_width: u16) -> usize {
    if area_width == 0 {
        return 0;
    }
    let n = match style {
        VisualStyle::Bars | VisualStyle::Mirror | VisualStyle::Dots => bar_layout(area_width).0,
        VisualStyle::Area => area_width as usize,
        VisualStyle::Wave | VisualStyle::Vu => SMALL_BANDS,
    };
    n.min(MAX_BANDS)
}

/// Draws `style` into `area` of `buf`.
pub fn render(
    style: VisualStyle,
    data: &VisualizerData,
    theme: &Theme,
    area: Rect,
    buf: &mut Buffer,
) {
    let area = area.intersection(buf.area);
    if area.width == 0 || area.height == 0 {
        return;
    }
    match style {
        VisualStyle::Bars => bars::render_bars(data, theme, area, buf),
        VisualStyle::Mirror => bars::render_mirror(data, theme, area, buf),
        VisualStyle::Dots => bars::render_dots(data, theme, area, buf),
        VisualStyle::Area => bars::render_area(data, theme, area, buf),
        VisualStyle::Wave => wave::render_wave(data, theme, area, buf),
        VisualStyle::Vu => vu::render_vu(data, theme, area, buf),
    }
}

/// Widget wrapper around [`render`].
pub struct Visualizer<'a> {
    /// Style to draw.
    pub style: VisualStyle,
    /// Data to draw.
    pub data: &'a VisualizerData,
    /// Colors to draw with.
    pub theme: &'a Theme,
}

impl Widget for Visualizer<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        render(self.style, self.data, self.theme, area, buf);
    }
}

/// Clamps to `0..=1`, mapping non-finite values to 0.
pub(super) fn unit(v: f32) -> f32 {
    if v.is_finite() {
        v.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

/// Clamps to `-1..=1`, mapping non-finite values to 0.
pub(super) fn signed(v: f32) -> f32 {
    if v.is_finite() {
        v.clamp(-1.0, 1.0)
    } else {
        0.0
    }
}

/// Resamples `values` (sanitized to `0..=1`) to exactly `n` points: max-pool
/// when there are more values than points, linear interpolation when fewer.
/// Empty input yields silence.
pub(super) fn resample(values: &[f32], n: usize) -> Vec<f32> {
    if n == 0 {
        return Vec::new();
    }
    if values.is_empty() {
        return vec![0.0; n];
    }
    let len = values.len();
    if len >= n {
        return (0..n)
            .map(|i| {
                let start = i * len / n;
                let end = ((i + 1) * len / n).max(start + 1).min(len);
                values[start..end]
                    .iter()
                    .fold(0.0f32, |m, v| m.max(unit(*v)))
            })
            .collect();
    }
    // len < n, so n >= 2 and len - 1 >= 0.
    let last = (len - 1) as f32;
    (0..n)
        .map(|i| {
            let pos = last * i as f32 / (n - 1) as f32;
            let lo = (pos.floor() as usize).min(len - 1);
            let hi = (lo + 1).min(len - 1);
            let t = pos - lo as f32;
            unit(values[lo]) * (1.0 - t) + unit(values[hi]) * t
        })
        .collect()
}

/// Writes one cell if it lies inside the buffer.
pub(super) fn put(buf: &mut Buffer, x: u16, y: u16, ch: char, style: Style) {
    if let Some(cell) = buf.cell_mut((x, y)) {
        cell.set_char(ch).set_style(style);
    }
}

/// Gradient style of a column cell. Color follows `color_frac` (height of the
/// cell center, `0` bottom to `1` top); the upper third of a column
/// (`bold_frac > 0.66`) is bold for a subtle intensity ramp that works on any
/// terminal background.
pub(super) fn ramp(theme: &Theme, bold_frac: f32, color_frac: f32) -> Style {
    let style = theme.level_style(color_frac);
    if bold_frac > 0.66 {
        style.add_modifier(Modifier::BOLD)
    } else {
        style
    }
}

#[cfg(test)]
mod tests;
