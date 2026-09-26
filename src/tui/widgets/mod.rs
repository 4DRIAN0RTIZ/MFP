//! Small pure formatting helpers used by the views, plus the visualizer widgets.

pub mod visualizer;

use ratatui::text::Span;

use crate::player::format_duration;

/// Number of volume-bar segments per 100% of volume is `segments / 2`
/// (the player caps volume at 200%).
const MAX_VOLUME: f32 = 2.0;

/// Progress bar of exactly `width` cells, e.g. `━━━━╸─────`.
///
/// The head `╸` marks the current position; a finished (or over-full) track
/// fills the whole bar. An unknown total (0) shows the head at the start.
pub fn progress_bar(elapsed: u64, total: u64, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    let filled = if total == 0 {
        0
    } else {
        ((elapsed.min(total) as u128 * width as u128) / total as u128) as usize
    };
    if filled >= width {
        return "━".repeat(width);
    }
    format!("{}╸{}", "━".repeat(filled), "─".repeat(width - filled - 1))
}

/// Volume bar of `segments` cells (`▮` filled, `▯` empty) for a 0.0-2.0 volume.
pub fn volume_bar(volume: f32, segments: usize) -> String {
    let ratio = (volume / MAX_VOLUME).clamp(0.0, 1.0);
    let filled = ((ratio * segments as f32).round() as usize).min(segments);
    format!("{}{}", "▮".repeat(filled), "▯".repeat(segments - filled))
}

/// Time label formatted like the plain CLI (`MM:SS` or `HH:MM:SS`).
pub fn time_label(seconds: u64) -> String {
    format_duration(seconds)
}

/// Volume as a whole percentage, e.g. `110%`.
pub fn volume_percent(volume: f32) -> String {
    format!("{:.0}%", volume * 100.0)
}

/// Terminal display width of `text`.
pub fn display_width(text: &str) -> usize {
    Span::raw(text).width()
}

/// Truncates `text` to at most `max` display cells, ending with `…` if cut.
pub fn truncate_to_width(text: &str, max: usize) -> String {
    if display_width(text) <= max {
        return text.to_string();
    }
    if max == 0 {
        return String::new();
    }
    let mut out = String::new();
    let mut used = 0;
    for c in text.chars() {
        let w = display_width(c.encode_utf8(&mut [0; 4]));
        if used + w + 1 > max {
            break;
        }
        out.push(c);
        used += w;
    }
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_bar_has_exact_width() {
        for (e, t) in [
            (0, 100),
            (50, 100),
            (99, 100),
            (100, 100),
            (500, 100),
            (5, 0),
        ] {
            assert_eq!(progress_bar(e, t, 20).chars().count(), 20, "{e}/{t}");
        }
        assert_eq!(progress_bar(1, 1, 0), "");
    }

    #[test]
    fn progress_bar_positions() {
        assert_eq!(progress_bar(0, 100, 10), "╸─────────");
        assert_eq!(progress_bar(50, 100, 10), "━━━━━╸────");
        assert_eq!(progress_bar(100, 100, 10), "━━━━━━━━━━");
        assert_eq!(progress_bar(500, 100, 10), "━━━━━━━━━━");
        assert_eq!(progress_bar(5, 0, 4), "╸───");
    }

    #[test]
    fn volume_bar_scales_to_200_percent() {
        assert_eq!(volume_bar(0.0, 20), "▯".repeat(20));
        assert_eq!(
            volume_bar(1.0, 20),
            format!("{}{}", "▮".repeat(10), "▯".repeat(10))
        );
        assert_eq!(volume_bar(2.0, 20), "▮".repeat(20));
        assert_eq!(volume_bar(3.5, 20), "▮".repeat(20));
        assert_eq!(volume_bar(-1.0, 4), "▯".repeat(4));
    }

    #[test]
    fn labels_match_cli_format() {
        assert_eq!(time_label(51), "00:51");
        assert_eq!(time_label(5573), "01:32:53");
        assert_eq!(volume_percent(1.1), "110%");
        assert_eq!(volume_percent(0.0), "0%");
    }

    #[test]
    fn truncation_respects_display_width() {
        assert_eq!(truncate_to_width("hello", 10), "hello");
        assert_eq!(truncate_to_width("hello world", 6), "hello…");
        assert_eq!(truncate_to_width("hello", 0), "");
        // Wide (2-cell) characters are never split.
        let cut = truncate_to_width("日本語日本語", 5);
        assert!(display_width(&cut) <= 5, "{cut}");
        assert!(cut.ends_with('…'));
    }
}
