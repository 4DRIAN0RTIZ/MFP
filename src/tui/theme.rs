//! Semantic color roles for the whole TUI, including the visualizer.
//!
//! No widget or view code names a color: everything asks the active [`Theme`]
//! for a role or a ready-made [`Style`]. The theme is loaded once when the TUI
//! starts (see [`Theme::resolve`]) from a built-in preset or from a user file
//! in `<config_dir>/mfp/themes/<name>.toml`.
//!
//! `Color::Reset` is the "terminal native" value. The `default` preset uses it
//! for background and foreground and reproduces the original look, which has
//! no fixed background and works on dark and light terminals. Roles set to
//! `Reset` never set a color on the cell, and for the "soft" roles (`muted`,
//! `border`, `help_text`, `viz_dim`) `Reset` means "native color, dimmed".

use std::path::Path;
use std::str::FromStr;

use ratatui::style::{Color, Modifier, Style};
use serde::Deserialize;

use crate::config;

/// Built-in preset names, in display order.
#[cfg(test)]
pub const THEME_NAMES: [&str; 6] = [
    "default",
    "solarized",
    "high-contrast",
    "gruvbox",
    "nord",
    "dracula",
];

/// Zone of a level meter segment, from quiet to loud.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VuZone {
    /// Normal levels.
    Ok,
    /// Loud levels.
    Warn,
    /// Clipping levels.
    Clip,
}

/// Style with only the foreground set; `Reset` sets nothing so the cell keeps
/// the theme foreground (or the terminal default).
pub fn paint(color: Color) -> Style {
    if color == Color::Reset {
        Style::default()
    } else {
        Style::default().fg(color)
    }
}

/// Style for de-emphasized roles: an explicit color is used as is, `Reset`
/// falls back to the native foreground with the DIM modifier.
fn soft(color: Color) -> Style {
    if color == Color::Reset {
        Style::default().add_modifier(Modifier::DIM)
    } else {
        Style::default().fg(color)
    }
}

/// Every color decision of the TUI, one field per semantic role.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Theme {
    /// Fill of the whole screen. `Reset` paints nothing (terminal background).
    pub background: Color,
    /// Base text color of the whole screen. `Reset` keeps the terminal's own.
    pub foreground: Color,
    /// Titles, icons and other highlighted elements.
    pub accent: Color,
    /// Secondary text: durations, labels, hints, placeholders.
    pub muted: Color,
    /// Panel borders, dividers and separators.
    pub border: Color,
    /// Positive state (MPRIS connected).
    pub success: Color,
    /// Error messages.
    pub danger: Color,
    /// Background of the selected list row. With `selection_fg` also `Reset`
    /// the row is drawn with the REVERSED modifier (native look); otherwise
    /// each non-`Reset` value is applied explicitly.
    pub selection_bg: Color,
    /// Text color of the selected list row (see `selection_bg`).
    pub selection_fg: Color,
    /// The playing marker of the episode list.
    pub playing_marker: Color,
    /// The favorite star.
    pub favorite: Color,
    /// Elapsed part of the progress bar (and download gauge).
    pub progress_filled: Color,
    /// Remaining part of the progress bar.
    pub progress_empty: Color,
    /// Filled segments of the volume bar.
    pub volume_filled: Color,
    /// Empty segments of the volume bar.
    pub volume_empty: Color,
    /// Informational status messages.
    pub status_text: Color,
    /// Key names in the help bar.
    pub help_key: Color,
    /// Labels in the help bar.
    pub help_text: Color,
    /// Visualizer level gradient: lower third of a column.
    pub viz_low: Color,
    /// Visualizer level gradient: middle third of a column.
    pub viz_mid: Color,
    /// Visualizer level gradient: top third of a column.
    pub viz_high: Color,
    /// Peak-hold markers of the dots style.
    pub viz_peak: Color,
    /// Dots body and empty meter segments.
    pub viz_dim: Color,
    /// Oscilloscope line.
    pub viz_wave: Color,
    /// VU meter, normal zone (up to 60% of the scale).
    pub viz_vu_ok: Color,
    /// VU meter, loud zone (up to 85%).
    pub viz_vu_warn: Color,
    /// VU meter, clipping zone (above 85%).
    pub viz_vu_clip: Color,
    /// Peak-hold tick of the VU meter.
    pub viz_vu_hold: Color,
}

/// Color from a `0xRRGGBB` literal.
const fn hex(rgb: u32) -> Color {
    Color::Rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
}

impl Theme {
    /// The original look: terminal-native colors, blue accent. Every visualizer
    /// role is the accent, so the gradient is flat.
    pub const fn default_preset() -> Self {
        Self {
            background: Color::Reset,
            foreground: Color::Reset,
            accent: Color::Blue,
            muted: Color::Reset,
            border: Color::Reset,
            success: Color::Green,
            danger: Color::Red,
            selection_bg: Color::Reset,
            selection_fg: Color::Reset,
            playing_marker: Color::Blue,
            favorite: Color::Yellow,
            progress_filled: Color::Blue,
            progress_empty: Color::Blue,
            volume_filled: Color::Blue,
            volume_empty: Color::Blue,
            status_text: Color::Reset,
            help_key: Color::Blue,
            help_text: Color::Reset,
            viz_low: Color::Blue,
            viz_mid: Color::Blue,
            viz_high: Color::Blue,
            viz_peak: Color::Blue,
            viz_dim: Color::Reset,
            viz_wave: Color::Blue,
            viz_vu_ok: Color::Blue,
            viz_vu_warn: Color::Blue,
            viz_vu_clip: Color::Blue,
            viz_vu_hold: Color::Blue,
        }
    }

    fn solarized() -> Self {
        Self {
            background: hex(0x002b36),
            foreground: hex(0x839496),
            accent: hex(0x268bd2),
            muted: hex(0x586e75),
            border: hex(0x586e75),
            success: hex(0x859900),
            danger: hex(0xdc322f),
            selection_bg: hex(0x073642),
            selection_fg: hex(0x93a1a1),
            playing_marker: hex(0x2aa198),
            favorite: hex(0xb58900),
            progress_filled: hex(0x268bd2),
            progress_empty: hex(0x586e75),
            volume_filled: hex(0x2aa198),
            volume_empty: hex(0x586e75),
            status_text: hex(0x839496),
            help_key: hex(0x2aa198),
            help_text: hex(0x586e75),
            viz_low: hex(0x2aa198),
            viz_mid: hex(0xb58900),
            viz_high: hex(0xcb4b16),
            viz_peak: hex(0xeee8d5),
            viz_dim: hex(0x586e75),
            viz_wave: hex(0x2aa198),
            viz_vu_ok: hex(0x859900),
            viz_vu_warn: hex(0xb58900),
            viz_vu_clip: hex(0xdc322f),
            viz_vu_hold: hex(0xeee8d5),
        }
    }

    /// Named ANSI colors only, no grays that vanish on low-fidelity terminals.
    fn high_contrast() -> Self {
        Self {
            background: Color::Black,
            foreground: Color::White,
            accent: Color::LightCyan,
            muted: Color::White,
            border: Color::White,
            success: Color::LightGreen,
            danger: Color::LightRed,
            selection_bg: Color::White,
            selection_fg: Color::Black,
            playing_marker: Color::LightYellow,
            favorite: Color::LightYellow,
            progress_filled: Color::LightCyan,
            progress_empty: Color::White,
            volume_filled: Color::LightCyan,
            volume_empty: Color::White,
            status_text: Color::White,
            help_key: Color::LightYellow,
            help_text: Color::White,
            viz_low: Color::LightBlue,
            viz_mid: Color::LightYellow,
            viz_high: Color::LightRed,
            viz_peak: Color::White,
            viz_dim: Color::White,
            viz_wave: Color::LightCyan,
            viz_vu_ok: Color::LightGreen,
            viz_vu_warn: Color::LightYellow,
            viz_vu_clip: Color::LightRed,
            viz_vu_hold: Color::White,
        }
    }

    fn gruvbox() -> Self {
        Self {
            background: hex(0x282828),
            foreground: hex(0xebdbb2),
            accent: hex(0xfabd2f),
            muted: hex(0x928374),
            border: hex(0x665c54),
            success: hex(0xb8bb26),
            danger: hex(0xfb4934),
            selection_bg: hex(0x504945),
            selection_fg: hex(0xebdbb2),
            playing_marker: hex(0xb8bb26),
            favorite: hex(0xfabd2f),
            progress_filled: hex(0xfabd2f),
            progress_empty: hex(0x504945),
            volume_filled: hex(0x8ec07c),
            volume_empty: hex(0x504945),
            status_text: hex(0xebdbb2),
            help_key: hex(0xfe8019),
            help_text: hex(0xa89984),
            viz_low: hex(0x83a598),
            viz_mid: hex(0xfabd2f),
            viz_high: hex(0xfb4934),
            viz_peak: hex(0xebdbb2),
            viz_dim: hex(0x665c54),
            viz_wave: hex(0x8ec07c),
            viz_vu_ok: hex(0xb8bb26),
            viz_vu_warn: hex(0xfabd2f),
            viz_vu_clip: hex(0xfb4934),
            viz_vu_hold: hex(0xebdbb2),
        }
    }

    fn nord() -> Self {
        Self {
            background: hex(0x2e3440),
            foreground: hex(0xd8dee9),
            accent: hex(0x88c0d0),
            muted: hex(0x616e88),
            border: hex(0x4c566a),
            success: hex(0xa3be8c),
            danger: hex(0xbf616a),
            selection_bg: hex(0x434c5e),
            selection_fg: hex(0xeceff4),
            playing_marker: hex(0x88c0d0),
            favorite: hex(0xebcb8b),
            progress_filled: hex(0x88c0d0),
            progress_empty: hex(0x434c5e),
            volume_filled: hex(0x81a1c1),
            volume_empty: hex(0x434c5e),
            status_text: hex(0xd8dee9),
            help_key: hex(0x88c0d0),
            help_text: hex(0x616e88),
            viz_low: hex(0x5e81ac),
            viz_mid: hex(0x88c0d0),
            viz_high: hex(0xebcb8b),
            viz_peak: hex(0xeceff4),
            viz_dim: hex(0x4c566a),
            viz_wave: hex(0x88c0d0),
            viz_vu_ok: hex(0xa3be8c),
            viz_vu_warn: hex(0xebcb8b),
            viz_vu_clip: hex(0xbf616a),
            viz_vu_hold: hex(0xeceff4),
        }
    }

    fn dracula() -> Self {
        Self {
            background: hex(0x282a36),
            foreground: hex(0xf8f8f2),
            accent: hex(0xbd93f9),
            muted: hex(0x6272a4),
            border: hex(0x44475a),
            success: hex(0x50fa7b),
            danger: hex(0xff5555),
            selection_bg: hex(0x44475a),
            selection_fg: hex(0xf8f8f2),
            playing_marker: hex(0x50fa7b),
            favorite: hex(0xf1fa8c),
            progress_filled: hex(0xbd93f9),
            progress_empty: hex(0x44475a),
            volume_filled: hex(0xff79c6),
            volume_empty: hex(0x44475a),
            status_text: hex(0xf8f8f2),
            help_key: hex(0xff79c6),
            help_text: hex(0x6272a4),
            viz_low: hex(0x8be9fd),
            viz_mid: hex(0xbd93f9),
            viz_high: hex(0xff79c6),
            viz_peak: hex(0xf8f8f2),
            viz_dim: hex(0x44475a),
            viz_wave: hex(0x8be9fd),
            viz_vu_ok: hex(0x50fa7b),
            viz_vu_warn: hex(0xf1fa8c),
            viz_vu_clip: hex(0xff5555),
            viz_vu_hold: hex(0xf8f8f2),
        }
    }

    /// Built-in preset by name (one of the built-in names).
    pub fn preset(name: &str) -> Option<Self> {
        Some(match name {
            "default" => Self::default_preset(),
            "solarized" => Self::solarized(),
            "high-contrast" => Self::high_contrast(),
            "gruvbox" => Self::gruvbox(),
            "nord" => Self::nord(),
            "dracula" => Self::dracula(),
            _ => return None,
        })
    }

    /// Resolves the theme called `name`: a built-in preset first, otherwise
    /// the user file `<config_dir>/mfp/themes/<name>.toml`. Any problem falls
    /// back to the `default` preset and is written to `mfp.log`. Never fails.
    pub fn resolve(name: &str) -> Self {
        let dir = config::themes_dir().ok();
        let (theme, problem) = Self::resolve_in(name, dir.as_deref());
        if let Some(problem) = problem {
            crate::logging::log(&problem);
        }
        theme
    }

    /// Like [`Theme::resolve`] with an explicit themes directory and no
    /// logging: the problem, if any, is returned for the caller to report.
    pub fn resolve_in(name: &str, themes_dir: Option<&Path>) -> (Self, Option<String>) {
        if let Some(theme) = Self::preset(name) {
            return (theme, None);
        }
        match Self::load_custom(name, themes_dir) {
            Ok(theme) => (theme, None),
            Err(reason) => (
                Self::default_preset(),
                Some(format!(
                    "theme: cannot load '{}': {}; using 'default'",
                    name, reason
                )),
            ),
        }
    }

    fn load_custom(name: &str, themes_dir: Option<&Path>) -> Result<Self, String> {
        let dir = themes_dir.ok_or("config directory not found")?;
        let path = config::theme_file_in(dir, name).ok_or("invalid theme name")?;
        let content = std::fs::read_to_string(&path)
            .map_err(|e| format!("cannot read {}: {}", path.display(), e))?;
        Self::parse_custom(&content).map_err(|e| format!("{}: {}", path.display(), e))
    }

    /// Builds a theme from the text of a user theme file (see [`ThemeFile`]).
    fn parse_custom(content: &str) -> Result<Self, String> {
        let file: ThemeFile =
            toml::from_str(content).map_err(|e| format!("invalid TOML: {}", e))?;
        let mut theme = match file.base.as_deref() {
            None => Self::default_preset(),
            Some(base) => Self::preset(base).ok_or_else(|| {
                format!(
                    "unknown base '{}' (only built-in presets are supported)",
                    base
                )
            })?,
        };
        file.apply(&mut theme)?;
        Ok(theme)
    }

    // ----- ready-made styles -------------------------------------------

    /// Style painting the whole screen, or `None` when both background and
    /// foreground are `Reset` (nothing to paint).
    pub fn base_style(&self) -> Option<Style> {
        if self.background == Color::Reset && self.foreground == Color::Reset {
            return None;
        }
        let mut style = paint(self.foreground);
        if self.background != Color::Reset {
            style = style.bg(self.background);
        }
        Some(style)
    }

    /// Panel borders and separators.
    pub fn border(&self) -> Style {
        soft(self.border)
    }

    /// Panel title.
    pub fn title(&self) -> Style {
        paint(self.accent).add_modifier(Modifier::BOLD)
    }

    /// Main text (episode title, list header).
    pub fn primary(&self) -> Style {
        Style::default().add_modifier(Modifier::BOLD)
    }

    /// Secondary text.
    pub fn dim(&self) -> Style {
        soft(self.muted)
    }

    /// Accent-colored elements (icons).
    pub fn accent(&self) -> Style {
        paint(self.accent)
    }

    /// Playing marker of the list.
    pub fn playing_marker(&self) -> Style {
        paint(self.playing_marker)
    }

    /// The favorite star.
    pub fn favorite(&self) -> Style {
        paint(self.favorite).add_modifier(Modifier::BOLD)
    }

    /// Error messages.
    pub fn error(&self) -> Style {
        paint(self.danger).add_modifier(Modifier::BOLD)
    }

    /// Positive state.
    pub fn ok(&self) -> Style {
        paint(self.success)
    }

    /// Informational status messages.
    pub fn status(&self) -> Style {
        paint(self.status_text)
    }

    /// Style patched over the selected list row: REVERSED when both selection
    /// colors are `Reset`, explicit colors otherwise.
    pub fn selection(&self) -> Style {
        if self.selection_bg == Color::Reset && self.selection_fg == Color::Reset {
            return Style::default().add_modifier(Modifier::REVERSED);
        }
        let mut style = paint(self.selection_fg);
        if self.selection_bg != Color::Reset {
            style = style.bg(self.selection_bg);
        }
        style
    }

    /// Elapsed part of the progress bar.
    pub fn progress_filled(&self) -> Style {
        paint(self.progress_filled)
    }

    /// Remaining part of the progress bar.
    pub fn progress_empty(&self) -> Style {
        paint(self.progress_empty)
    }

    /// Filled volume segments.
    pub fn volume_filled(&self) -> Style {
        paint(self.volume_filled)
    }

    /// Empty volume segments.
    pub fn volume_empty(&self) -> Style {
        paint(self.volume_empty)
    }

    /// Key names of the help bar.
    pub fn key(&self) -> Style {
        paint(self.help_key).add_modifier(Modifier::BOLD)
    }

    /// Labels of the help bar.
    pub fn help_text(&self) -> Style {
        soft(self.help_text)
    }

    /// Visualizer gradient color at height fraction `t` (`0` bottom, `1` top).
    /// See [`gradient`].
    pub fn level_color(&self, t: f32) -> Color {
        gradient(self.viz_low, self.viz_mid, self.viz_high, t)
    }

    /// Foreground style of the visualizer gradient at height fraction `t`.
    pub fn level_style(&self, t: f32) -> Style {
        paint(self.level_color(t))
    }

    /// Dots body and empty meter segments.
    pub fn viz_dim(&self) -> Style {
        soft(self.viz_dim)
    }

    /// Peak markers of the dots style.
    pub fn viz_peak(&self) -> Style {
        paint(self.viz_peak).add_modifier(Modifier::BOLD)
    }

    /// Oscilloscope line.
    pub fn viz_wave(&self) -> Style {
        paint(self.viz_wave)
    }

    /// Filled VU segment in `zone`.
    pub fn vu_style(&self, zone: VuZone) -> Style {
        paint(match zone {
            VuZone::Ok => self.viz_vu_ok,
            VuZone::Warn => self.viz_vu_warn,
            VuZone::Clip => self.viz_vu_clip,
        })
    }

    /// Peak-hold tick of the VU meter.
    pub fn vu_hold(&self) -> Style {
        paint(self.viz_vu_hold).add_modifier(Modifier::BOLD)
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::default_preset()
    }
}

/// Color at `t` (`0..=1`, non-finite counts as 0) of a three-stop gradient.
/// When all stops are `Color::Rgb` the result is interpolated linearly
/// (`low`->`mid` over the first half, `mid`->`high` over the second);
/// otherwise it steps at one third and two thirds.
pub fn gradient(low: Color, mid: Color, high: Color, t: f32) -> Color {
    let t = if t.is_finite() {
        t.clamp(0.0, 1.0)
    } else {
        0.0
    };
    if let (Color::Rgb(lr, lg, lb), Color::Rgb(mr, mg, mb), Color::Rgb(hr, hg, hb)) =
        (low, mid, high)
    {
        let (a, b, u) = if t < 0.5 {
            ([lr, lg, lb], [mr, mg, mb], t * 2.0)
        } else {
            ([mr, mg, mb], [hr, hg, hb], (t - 0.5) * 2.0)
        };
        let mix = |i: usize| (a[i] as f32 + (b[i] as f32 - a[i] as f32) * u).round() as u8;
        return Color::Rgb(mix(0), mix(1), mix(2));
    }
    if t < 1.0 / 3.0 {
        low
    } else if t < 2.0 / 3.0 {
        mid
    } else {
        high
    }
}

/// Parses a color string (`"cyan"`, `"light-blue"`, `"#268bd2"`, `"166"`,
/// `"reset"`), naming the field on failure.
fn parse_color(field: &str, value: &str) -> Result<Color, String> {
    Color::from_str(value.trim()).map_err(|_| format!("invalid color for '{}': '{}'", field, value))
}

/// Declares [`ThemeFile`] with one optional string per theme role and the
/// method that applies the present ones over a base theme.
macro_rules! theme_file {
    ($(#[$meta:meta])* $($field:ident),* $(,)?) => {
        $(#[$meta])*
        #[derive(Debug, Default, Deserialize)]
        #[serde(default, deny_unknown_fields)]
        pub struct ThemeFile {
            /// Built-in preset to inherit from; `default` when omitted.
            pub base: Option<String>,
            $(
                #[allow(missing_docs)]
                pub $field: Option<String>,
            )*
        }

        impl ThemeFile {
            /// Overwrites the roles present in the file, failing on the first
            /// invalid color.
            fn apply(&self, theme: &mut Theme) -> Result<(), String> {
                $(
                    if let Some(value) = &self.$field {
                        theme.$field = parse_color(stringify!($field), value)?;
                    }
                )*
                Ok(())
            }
        }
    };
}

theme_file! {
    /// A user theme, `<config_dir>/mfp/themes/<name>.toml`. Every field is an
    /// optional color string with the name of a [`Theme`] role; a missing role
    /// inherits from `base` (a built-in preset name, `default` when omitted;
    /// basing a theme on another custom theme is not supported). Colors are
    /// names (`"cyan"`, `"light-blue"`), hex (`"#268bd2"`), ANSI indexes
    /// (`"166"`) or `"reset"` (terminal native). Unknown keys or invalid
    /// colors make the whole theme fail to load, which falls back to `default`
    /// and logs the reason.
    ///
    /// ```toml
    /// base = "dracula"
    /// accent = "#ff79c6"
    /// viz_high = "light-red"
    /// selection_bg = "166"
    /// ```
    background, foreground, accent, muted, border, success, danger,
    selection_bg, selection_fg, playing_marker, favorite,
    progress_filled, progress_empty, volume_filled, volume_empty,
    status_text, help_key, help_text,
    viz_low, viz_mid, viz_high, viz_peak, viz_dim, viz_wave,
    viz_vu_ok, viz_vu_warn, viz_vu_clip, viz_vu_hold,
}

#[cfg(test)]
mod tests;
