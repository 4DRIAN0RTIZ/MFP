//! Rendering of the compact player view.

use std::time::Instant;

use ratatui::{
    layout::{Constraint, Layout, Rect},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};

use super::app::{App, StatusKind};
use super::theme;
use super::widgets::{
    display_width, progress_bar, time_label, truncate_to_width, volume_bar, volume_percent,
};

/// Smallest terminal width the compact view supports.
pub const MIN_WIDTH: u16 = 40;
/// Smallest terminal height the compact view supports.
pub const MIN_HEIGHT: u16 = 9;

/// Columns before the episode title / progress bar (`"  ♪  "`).
const ICON_COLS: usize = 5;
/// Columns reserved at the right of the title row for the favorite star.
const STAR_COLS: usize = 3;
/// Width of the fixed parts of the volume row, excluding the bar.
const VOLUME_ROW_FIXED: usize = 33;
/// Bar segments at full size (one segment per 10% of volume).
const MAX_VOLUME_SEGMENTS: usize = 20;

/// Whether `area` is too small to draw the compact view.
pub fn is_too_small(area: Rect) -> bool {
    area.width < MIN_WIDTH || area.height < MIN_HEIGHT
}

/// Draws the whole UI for the current state.
pub fn draw(frame: &mut Frame, app: &App, now: Instant) {
    let area = frame.area();
    if is_too_small(area) {
        let msg = format!("Terminal too small (min {}x{})", MIN_WIDTH, MIN_HEIGHT);
        frame.render_widget(
            Paragraph::new(msg)
                .style(theme::error())
                .wrap(Wrap { trim: true }),
            area,
        );
        return;
    }
    draw_compact(frame, area, app, now);
}

fn draw_compact(frame: &mut Frame, area: Rect, app: &App, now: Instant) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme::border())
        .title(Span::styled(" mfp ", theme::title()));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let rows = Layout::vertical([
        Constraint::Length(1), // title
        Constraint::Length(1), // duration
        Constraint::Length(1), // progress
        Constraint::Length(1), // volume / shuffle / mpris
        Constraint::Length(1), // status
        Constraint::Length(1), // separator
        Constraint::Length(1), // help
        Constraint::Min(0),
    ])
    .split(inner);
    let width = inner.width as usize;

    frame.render_widget(Paragraph::new(title_line(app, width)), rows[0]);
    frame.render_widget(Paragraph::new(duration_line(app)), rows[1]);
    frame.render_widget(Paragraph::new(progress_line(app, width)), rows[2]);
    frame.render_widget(Paragraph::new(volume_line(app, width)), rows[3]);
    frame.render_widget(Paragraph::new(status_line(app, now, width)), rows[4]);
    draw_separator(frame, area, rows[5].y);
    frame.render_widget(Paragraph::new(help_line(width)), rows[6]);
}

/// Draws `├───┤` across the full outer width at row `y`.
fn draw_separator(frame: &mut Frame, outer: Rect, y: u16) {
    let line = format!("├{}┤", "─".repeat(outer.width.saturating_sub(2) as usize));
    frame
        .buffer_mut()
        .set_string(outer.x, y, line, theme::border());
}

fn title_line(app: &App, width: usize) -> Line<'static> {
    let title = app
        .episode
        .as_ref()
        .map(|e| e.title.as_str())
        .unwrap_or("-");
    let title = truncate_to_width(title, width.saturating_sub(ICON_COLS + STAR_COLS + 1));
    let pad = width.saturating_sub(ICON_COLS + display_width(&title) + STAR_COLS);
    let star = if app.favorite {
        Span::styled("★", theme::favorite())
    } else {
        Span::raw(" ")
    };
    Line::from(vec![
        Span::styled("  ♪  ", theme::accent()),
        Span::styled(title, theme::primary()),
        Span::raw(" ".repeat(pad)),
        star,
    ])
}

fn duration_line(app: &App) -> Line<'static> {
    let duration = app
        .episode
        .as_ref()
        .map(|e| e.duration.as_str())
        .unwrap_or("-");
    Line::from(Span::styled(
        format!("     Duración {}", duration),
        theme::dim(),
    ))
}

fn progress_line(app: &App, width: usize) -> Line<'static> {
    let total = app.episode.as_ref().map(|e| e.total_seconds).unwrap_or(0);
    let elapsed = time_label(app.elapsed);
    let total_label = time_label(total);
    let icon = if app.paused { "⏸" } else { "▶" };
    // icon cols + elapsed + space + bar + gap + total + right margin
    let fixed = ICON_COLS + display_width(&elapsed) + 1 + 2 + display_width(&total_label) + 2;
    let bar = progress_bar(app.elapsed, total, width.saturating_sub(fixed));
    Line::from(vec![
        Span::styled(format!("  {}  ", icon), theme::accent()),
        Span::raw(elapsed),
        Span::raw(" "),
        Span::styled(bar, theme::accent()),
        Span::raw("  "),
        Span::styled(total_label, theme::dim()),
    ])
}

fn volume_line(app: &App, width: usize) -> Line<'static> {
    let segments = width
        .saturating_sub(VOLUME_ROW_FIXED)
        .clamp(4, MAX_VOLUME_SEGMENTS);
    let mpris = if app.mpris_connected {
        Span::styled("●", theme::ok())
    } else {
        Span::styled("○", theme::dim())
    };
    Line::from(vec![
        Span::styled("  Vol ", theme::dim()),
        Span::styled(volume_bar(app.volume, segments), theme::accent()),
        Span::raw(format!(" {:<4}", volume_percent(app.volume))),
        Span::styled(
            format!("  Shuffle {:<3}", if app.shuffle { "ON" } else { "OFF" }),
            theme::dim(),
        ),
        Span::styled("  MPRIS ", theme::dim()),
        mpris,
    ])
}

fn status_line(app: &App, now: Instant, width: usize) -> Line<'static> {
    match app.status_line(now) {
        Some((text, kind)) => {
            let style = match kind {
                StatusKind::Info => ratatui::style::Style::default(),
                StatusKind::Error => theme::error(),
            };
            Line::from(Span::styled(
                format!("  {}", truncate_to_width(&text, width.saturating_sub(3))),
                style,
            ))
        }
        None => Line::default(),
    }
}

/// Help bindings as `(key, label)`; the label is dropped in the shortest form.
const HELP_FULL: [(&str, &str); 10] = [
    ("n", "next"),
    ("b", "back"),
    ("p", "pause"),
    ("s", "shuffle"),
    ("f", "fav"),
    ("m", "mute"),
    ("+/-", "vol"),
    ("i", "info"),
    ("d", "download"),
    ("q", "quit"),
];
const HELP_MEDIUM: [(&str, &str); 8] = [
    ("n", "next"),
    ("b", "back"),
    ("p", "pause"),
    ("s", "shuffle"),
    ("f", "fav"),
    ("m", "mute"),
    ("+/-", "vol"),
    ("q", "quit"),
];
const HELP_SHORT: [(&str, &str); 10] = [
    ("n", ""),
    ("b", ""),
    ("p", ""),
    ("s", ""),
    ("f", ""),
    ("m", ""),
    ("+/-", ""),
    ("i", ""),
    ("d", ""),
    ("q", ""),
];

fn help_spans(items: &[(&str, &str)]) -> Vec<Span<'static>> {
    let mut spans = vec![Span::raw("  ")];
    for (i, (key, label)) in items.iter().enumerate() {
        if i > 0 {
            spans.push(Span::raw(if label.is_empty() { " " } else { "  " }));
        }
        spans.push(Span::styled(key.to_string(), theme::key()));
        if !label.is_empty() {
            spans.push(Span::styled(format!(" {}", label), theme::dim()));
        }
    }
    spans
}

/// Help line using the longest variant that fits in `width`.
fn help_line(width: usize) -> Line<'static> {
    let variants: [&[(&str, &str)]; 3] = [&HELP_FULL, &HELP_MEDIUM, &HELP_SHORT];
    for items in variants {
        let line = Line::from(help_spans(items));
        if line.width() <= width {
            return line;
        }
    }
    Line::from(help_spans(&HELP_SHORT))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::app::{Activity, EpisodeView};
    use ratatui::{backend::TestBackend, Terminal};
    use std::time::Duration;

    fn playing_app() -> App {
        let mut app = App::new(true);
        app.begin_episode(
            EpisodeView {
                title: "Episode 75: Datassette".into(),
                duration: "01:32:53".into(),
                total_seconds: 5573,
            },
            false,
            false,
        );
        app.activity = Activity::Idle;
        app.elapsed = 51;
        app
    }

    fn render(app: &App, w: u16, h: u16) -> Vec<String> {
        let mut terminal = Terminal::new(TestBackend::new(w, h)).expect("test terminal");
        terminal
            .draw(|f| draw(f, app, Instant::now()))
            .expect("draw");
        let buf = terminal.backend().buffer().clone();
        (0..h)
            .map(|y| {
                (0..w)
                    .map(|x| buf[(x, y)].symbol().to_string())
                    .collect::<String>()
            })
            .collect()
    }

    fn screen(app: &App, w: u16, h: u16) -> String {
        render(app, w, h).join("\n")
    }

    #[test]
    fn compact_view_shows_episode_time_and_help() {
        let out = screen(&playing_app(), 100, 12);
        assert!(out.contains("mfp"), "{out}");
        assert!(out.contains("Episode 75: Datassette"), "{out}");
        assert!(out.contains("Duración 01:32:53"), "{out}");
        assert!(out.contains("00:51"), "{out}");
        assert!(out.contains("01:32:53"), "{out}");
        assert!(out.contains("100%"), "{out}");
        assert!(out.contains("Shuffle OFF"), "{out}");
        assert!(out.contains("MPRIS ●"), "{out}");
        assert!(out.contains("n next"), "{out}");
        assert!(out.contains("q quit"), "{out}");
        assert!(out.contains('▶') && !out.contains('⏸'), "{out}");
        assert!(!out.contains('★'), "{out}");
    }

    #[test]
    fn favorite_star_paused_icon_and_mpris_unavailable() {
        let mut app = playing_app();
        app.favorite = true;
        app.paused = true;
        app.shuffle = true;
        app.mpris_connected = false;
        let out = screen(&app, 80, 9);
        assert!(out.contains('★'), "{out}");
        assert!(out.contains('⏸') && !out.contains('▶'), "{out}");
        assert!(out.contains("Shuffle ON"), "{out}");
        assert!(out.contains("MPRIS ○"), "{out}");
    }

    #[test]
    fn status_message_is_shown_until_it_expires() {
        let mut app = playing_app();
        let now = Instant::now();
        app.set_status("Volume: 110%", now);
        let mut terminal = Terminal::new(TestBackend::new(80, 10)).expect("terminal");
        terminal.draw(|f| draw(f, &app, now)).expect("draw");
        let shown = format!("{:?}", terminal.backend().buffer());
        assert!(shown.contains("Volume: 110%"));
        terminal
            .draw(|f| draw(f, &app, now + Duration::from_secs(9)))
            .expect("draw");
        let shown = format!("{:?}", terminal.backend().buffer());
        assert!(!shown.contains("Volume: 110%"));
    }

    #[test]
    fn loading_and_connecting_texts() {
        let app = App::new(false);
        assert!(screen(&app, 80, 10).contains("Cargando feed..."));
        let mut app = playing_app();
        app.activity = Activity::Buffering;
        assert!(screen(&app, 80, 10).contains("Connecting... buffering..."));
    }

    #[test]
    fn too_small_terminal_shows_message_without_panicking() {
        assert!(is_too_small(Rect::new(0, 0, 39, 30)));
        assert!(is_too_small(Rect::new(0, 0, 100, 8)));
        assert!(!is_too_small(Rect::new(0, 0, 40, 9)));
        let out = screen(&playing_app(), 30, 6);
        assert!(out.contains("too small"), "{out}");
        // Degenerate sizes must not panic either.
        for (w, h) in [(1, 1), (0, 0), (5, 2), (200, 3)] {
            let mut terminal = Terminal::new(TestBackend::new(w, h)).expect("terminal");
            terminal
                .draw(|f| draw(f, &playing_app(), Instant::now()))
                .expect("draw");
        }
    }

    #[test]
    fn minimum_size_renders_every_row() {
        let out = screen(&playing_app(), 40, 9);
        assert!(out.contains("Episode 75: Datassette"), "{out}");
        assert!(out.contains("MPRIS ●"), "{out}");
        assert!(out.contains("├") && out.contains("┤"), "{out}");
    }

    #[test]
    fn long_titles_are_truncated_to_the_row() {
        let mut app = playing_app();
        if let Some(ep) = app.episode.as_mut() {
            ep.title = "Episode 1: ".to_string() + &"very long ".repeat(20);
        }
        let lines = render(&app, 50, 10);
        assert!(lines[1].contains('…'), "{}", lines[1]);
        assert!(lines[1].ends_with('│'), "{}", lines[1]);
    }

    #[test]
    fn help_line_adapts_to_width() {
        assert!(help_line(100).to_string().contains("d download"));
        let medium = help_line(70).to_string();
        assert!(medium.contains("s shuffle") && !medium.contains("download"));
        assert!(!help_line(30).to_string().contains("next"));
        assert!(help_line(30).width() <= 30);
    }
}
