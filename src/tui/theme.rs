//! Colors and styles for the TUI, defined in one place.
//!
//! No background colors are set and the default foreground is left untouched
//! (`Color::Reset`), so the UI adapts to both dark and light terminals. Emphasis
//! comes from an accent color, bold and dim modifiers.

use ratatui::style::{Color, Modifier, Style};

/// Accent color for the playing marker, progress and volume bars.
const ACCENT: Color = Color::Blue;
/// Color of the favorite star.
const FAVORITE: Color = Color::Yellow;
/// Color of error messages.
const ERROR: Color = Color::Red;
/// Color of the "connected" MPRIS marker.
const OK: Color = Color::Green;

/// Panel border and title.
pub fn border() -> Style {
    Style::default().add_modifier(Modifier::DIM)
}

/// Panel title text.
pub fn title() -> Style {
    Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
}

/// Main text (episode title).
pub fn primary() -> Style {
    Style::default().add_modifier(Modifier::BOLD)
}

/// Secondary text (durations, labels, help line).
pub fn dim() -> Style {
    Style::default().add_modifier(Modifier::DIM)
}

/// Accent-colored elements (icons, filled bar segments).
pub fn accent() -> Style {
    Style::default().fg(ACCENT)
}

/// The favorite star.
pub fn favorite() -> Style {
    Style::default().fg(FAVORITE).add_modifier(Modifier::BOLD)
}

/// Error messages.
pub fn error() -> Style {
    Style::default().fg(ERROR).add_modifier(Modifier::BOLD)
}

/// Positive state (MPRIS connected).
pub fn ok() -> Style {
    Style::default().fg(OK)
}

/// Key names in the help line.
pub fn key() -> Style {
    Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
}
