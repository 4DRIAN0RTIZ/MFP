//! Rendering of the compact player view and the full view (episode list,
//! player and status panels).

use std::time::Instant;

use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::Modifier,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};

use super::app::{Activity, App, InputMode, LayoutMode, StatusKind};
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
///
/// Takes `&mut App` because the renderer reports back the terminal size and
/// list height, which selection paging and layout toggling depend on.
pub fn draw(frame: &mut Frame, app: &mut App, now: Instant) {
    let area = frame.area();
    app.set_viewport(area.width, area.height);
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
    match app.layout() {
        LayoutMode::Compact => draw_compact(frame, area, app, now),
        LayoutMode::Full => draw_full(frame, area, app, now),
    }
}

/// Widest the episode list panel gets, in columns.
const LIST_MAX_WIDTH: u16 = 44;
/// Narrowest the episode list panel gets, in columns.
const LIST_MIN_WIDTH: u16 = 24;
/// Columns before a list row's title (`"▶ "`).
const MARKER_COLS: usize = 2;
/// Columns after a list row's title (`" ★"`).
const ROW_STAR_COLS: usize = 2;

fn draw_full(frame: &mut Frame, area: Rect, app: &mut App, now: Instant) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme::border())
        .title(Span::styled(" mfp ", theme::title()));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let rows = Layout::vertical([
        Constraint::Min(1),    // body
        Constraint::Length(1), // separator
        Constraint::Length(1), // help
    ])
    .split(inner);
    let body = rows[0];
    let list_width = (inner.width * 2 / 5).clamp(LIST_MIN_WIDTH, LIST_MAX_WIDTH);
    let cols = Layout::horizontal([
        Constraint::Length(list_width),
        Constraint::Length(1),
        Constraint::Min(0),
    ])
    .split(body);
    let divider_x = cols[1].x;

    // Vertical divider with junctions on the outer border and the separator.
    for y in body.y..body.y + body.height {
        frame
            .buffer_mut()
            .set_string(divider_x, y, "│", theme::border());
    }
    frame
        .buffer_mut()
        .set_string(divider_x, area.y, "┬", theme::border());
    draw_separator(frame, area, rows[1].y, Some(divider_x));

    draw_list(frame, inset(cols[0]), app);
    draw_side(frame, inset(cols[2]), app, now);
    let width = inner.width as usize;
    let help = match app.effective_input_mode() {
        InputMode::List => help_line_for(&LIST_HELP, width),
        InputMode::Search => help_line_for(&SEARCH_HELP, width),
    };
    frame.render_widget(Paragraph::new(help), rows[2]);
}

/// `area` with one blank column on each side, so panel contents never touch
/// the borders or the divider.
fn inset(area: Rect) -> Rect {
    Rect {
        x: area.x.saturating_add(1),
        width: area.width.saturating_sub(2),
        ..area
    }
}

/// Header, rows and empty states of the episode list.
fn draw_list(frame: &mut Frame, area: Rect, app: &mut App) {
    if area.height == 0 || area.width == 0 {
        return;
    }
    let width = area.width as usize;
    let parts = Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).split(area);
    frame.render_widget(Paragraph::new(list_header(app, width)), parts[0]);

    let height = parts[1].height as usize;
    let (offset, window) = app.list_window(height);
    let selected = app.selected();
    let mut lines: Vec<Line> = Vec::with_capacity(window.len());
    for (i, &episode) in window.iter().enumerate() {
        let is_selected = offset + i == selected;
        lines.push(episode_row(app, episode, width, is_selected));
    }
    if window.is_empty() {
        let text = if app.episodes.is_empty() {
            match app.activity {
                Activity::Loading => "Loading...",
                _ => "No episodes",
            }
        } else {
            "no matches"
        };
        lines.push(Line::from(Span::styled(
            format!("  {}", text),
            theme::dim(),
        )));
    }
    frame.render_widget(Paragraph::new(lines), parts[1]);
}

/// `Episodes (75)`, or `/query_ (3/75)` while a filter is active.
fn list_header(app: &App, width: usize) -> Line<'static> {
    let total = app.episodes.len();
    let text = if app.input_mode == InputMode::Search || !app.query.is_empty() {
        let cursor = if app.input_mode == InputMode::Search {
            "_"
        } else {
            ""
        };
        format!(
            "/{}{} ({}/{})",
            app.query,
            cursor,
            app.visible().len(),
            total
        )
    } else {
        format!("Episodes ({})", total)
    };
    Line::from(Span::styled(
        truncate_to_width(&text, width),
        theme::primary(),
    ))
}

/// One list row: playing marker, title, favorite star. The selected row is
/// drawn reversed (no fixed colors, so it works on dark and light terminals).
fn episode_row(app: &App, episode: usize, width: usize, selected: bool) -> Line<'static> {
    let title = app.episodes.get(episode).map(String::as_str).unwrap_or("");
    let room = width.saturating_sub(MARKER_COLS + ROW_STAR_COLS);
    let shown = truncate_to_width(title, room);
    let pad = room.saturating_sub(display_width(&shown));
    let playing = app.playing == Some(episode);
    let favorite = app.favorites.contains(title);
    let marker = if playing {
        Span::styled("▶ ", theme::accent())
    } else {
        Span::raw("  ")
    };
    let star = if favorite {
        Span::styled(" ★", theme::favorite())
    } else {
        Span::raw("  ")
    };
    let title_style = if playing {
        theme::primary()
    } else {
        ratatui::style::Style::default()
    };
    let mut line = Line::from(vec![
        marker,
        Span::styled(shown, title_style),
        Span::raw(" ".repeat(pad)),
        star,
    ]);
    if selected {
        line = line.patch_style(ratatui::style::Style::default().add_modifier(Modifier::REVERSED));
    }
    line
}

/// Right side of the full view: player info and the bordered status panel.
fn draw_side(frame: &mut Frame, area: Rect, app: &App, now: Instant) {
    let rows = Layout::vertical([
        Constraint::Length(1), // title
        Constraint::Length(1), // duration
        Constraint::Length(1), // blank
        Constraint::Length(1), // progress
        Constraint::Length(1), // volume / shuffle / mpris
        Constraint::Min(0),    // status panel
    ])
    .split(area);
    let width = area.width as usize;
    frame.render_widget(Paragraph::new(title_line(app, width)), rows[0]);
    frame.render_widget(Paragraph::new(duration_line(app)), rows[1]);
    frame.render_widget(Paragraph::new(progress_line(app, width)), rows[3]);
    frame.render_widget(Paragraph::new(volume_line(app, width)), rows[4]);

    let panel = rows[5];
    if panel.height < 3 || panel.width < 4 {
        return;
    }
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme::border())
        .title(Span::styled(" Status ", theme::dim()));
    let inner = block.inner(panel);
    frame.render_widget(block, panel);
    frame.render_widget(
        Paragraph::new(status_panel_lines(app, now, inner.width as usize)),
        inner,
    );
}

/// Lines of the status panel: message or activity, download text, download bar.
fn status_panel_lines(app: &App, now: Instant, width: usize) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    if let Some((text, kind)) = app.message_line(now) {
        let style = match kind {
            StatusKind::Info => ratatui::style::Style::default(),
            StatusKind::Error => theme::error(),
        };
        lines.push(Line::from(Span::styled(
            format!(" {}", truncate_to_width(&text, width.saturating_sub(2))),
            style,
        )));
    }
    if let Some(text) = &app.download {
        lines.push(Line::from(Span::raw(format!(
            " {}",
            truncate_to_width(text, width.saturating_sub(2))
        ))));
        if let Some((done, total)) = app.download_bytes {
            let bar = progress_bar(done, total, width.saturating_sub(2));
            lines.push(Line::from(Span::styled(
                format!(" {}", bar),
                theme::accent(),
            )));
        }
    }
    lines
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
    draw_separator(frame, area, rows[5].y, None);
    frame.render_widget(Paragraph::new(help_line(width)), rows[6]);
}

/// Draws `├───┤` across the full outer width at row `y`, with a `┴` junction
/// at column `junction_x` when a vertical divider ends there.
fn draw_separator(frame: &mut Frame, outer: Rect, y: u16, junction_x: Option<u16>) {
    let line = format!("├{}┤", "─".repeat(outer.width.saturating_sub(2) as usize));
    frame
        .buffer_mut()
        .set_string(outer.x, y, line, theme::border());
    if let Some(x) = junction_x {
        frame.buffer_mut().set_string(x, y, "┴", theme::border());
    }
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
const HELP_FULL: [(&str, &str); 11] = [
    ("n", "next"),
    ("b", "back"),
    ("p", "pause"),
    ("s", "shuffle"),
    ("f", "fav"),
    ("m", "mute"),
    ("+/-", "vol"),
    ("i", "info"),
    ("d", "download"),
    ("v", "vista"),
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
const HELP_SHORT: [(&str, &str); 11] = [
    ("n", ""),
    ("b", ""),
    ("p", ""),
    ("s", ""),
    ("f", ""),
    ("m", ""),
    ("+/-", ""),
    ("i", ""),
    ("d", ""),
    ("v", ""),
    ("q", ""),
];

/// Full view, list mode.
const LIST_HELP: [&[(&str, &str)]; 3] = [
    &[
        ("↑↓", "mover"),
        ("Enter", "play"),
        ("/", "buscar"),
        ("n", "next"),
        ("b", "back"),
        ("p", "pause"),
        ("s", "shuffle"),
        ("f", "fav"),
        ("d", "download"),
        ("v", "vista"),
        ("q", "quit"),
    ],
    &[
        ("↑↓", "mover"),
        ("Enter", "play"),
        ("/", "buscar"),
        ("n", "next"),
        ("p", "pause"),
        ("v", "vista"),
        ("q", "quit"),
    ],
    &[
        ("↑↓", ""),
        ("Enter", ""),
        ("/", ""),
        ("n", ""),
        ("p", ""),
        ("v", ""),
        ("q", ""),
    ],
];

/// Full view, search mode.
const SEARCH_HELP: [&[(&str, &str)]; 2] = [
    &[("Enter", "aceptar"), ("Esc", "cancelar"), ("↑↓", "mover")],
    &[("Enter", ""), ("Esc", "")],
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

/// Help line of the compact view.
fn help_line(width: usize) -> Line<'static> {
    let variants: [&[(&str, &str)]; 3] = [&HELP_FULL, &HELP_MEDIUM, &HELP_SHORT];
    help_line_for(&variants, width)
}

/// Longest of `variants` (ordered long to short) that fits in `width`; the
/// shortest one when none fits.
fn help_line_for(variants: &[&[(&str, &str)]], width: usize) -> Line<'static> {
    for items in variants {
        let line = Line::from(help_spans(items));
        if line.width() <= width {
            return line;
        }
    }
    match variants.last() {
        Some(items) => Line::from(help_spans(items)),
        None => Line::default(),
    }
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
        let mut app = app.clone();
        let mut terminal = Terminal::new(TestBackend::new(w, h)).expect("test terminal");
        terminal
            .draw(|f| draw(f, &mut app, Instant::now()))
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
        terminal.draw(|f| draw(f, &mut app, now)).expect("draw");
        let shown = format!("{:?}", terminal.backend().buffer());
        assert!(shown.contains("Volume: 110%"));
        terminal
            .draw(|f| draw(f, &mut app, now + Duration::from_secs(9)))
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
                .draw(|f| draw(f, &mut playing_app(), Instant::now()))
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

    fn full_app() -> App {
        let mut app = playing_app();
        app.set_episodes(vec![
            "Episode 78: Ben Frost".into(),
            "Episode 77: Anonymous".into(),
            "Episode 76: Mordant Music".into(),
            "Episode 75: Datassette".into(),
            "Episode 74: NCW".into(),
        ]);
        app.playing = Some(3);
        app.select_playing();
        app.favorites.insert("Episode 75: Datassette".into());
        app.favorite = true;
        app
    }

    #[test]
    fn full_view_shows_list_markers_and_player() {
        let lines = render(&full_app(), 110, 30);
        let out = lines.join("\n");
        assert!(out.contains("Episodes (5)"), "{out}");
        assert!(out.contains("Episode 78: Ben Frost"), "{out}");
        assert!(out.contains("Episode 74: NCW"), "{out}");
        let playing = lines
            .iter()
            .find(|l| l.contains("▶ Episode 75: Datassette"))
            .unwrap_or_else(|| panic!("no playing row\n{out}"));
        assert!(playing.contains('★'), "{playing}");
        // Only the favorite row has a star in the list column (title row has its own).
        let other = lines
            .iter()
            .find(|l| l.contains("Episode 77: Anonymous"))
            .unwrap_or_else(|| panic!("no row\n{out}"));
        assert!(!other.contains('★'), "{other}");
        assert!(out.contains("Duración 01:32:53"), "{out}");
        assert!(out.contains("00:51"), "{out}");
        assert!(out.contains("Shuffle OFF"), "{out}");
        assert!(out.contains(" Status "), "{out}");
        assert!(out.contains("┬") && out.contains("┴"), "{out}");
        assert!(
            out.contains("Enter play") && out.contains("v vista"),
            "{out}"
        );
    }

    #[test]
    fn full_view_marks_the_selected_row_with_reverse_video() {
        let mut app = full_app();
        app.move_selection(crate::tui::list::ListMove::Home);
        let mut terminal = Terminal::new(TestBackend::new(110, 30)).expect("terminal");
        terminal
            .draw(|f| draw(f, &mut app, Instant::now()))
            .expect("draw");
        let buf = terminal.backend().buffer().clone();
        let (mut reversed_rows, mut normal_rows) = (Vec::new(), Vec::new());
        for y in 0..30u16 {
            let text: String = (1..40u16)
                .map(|x| buf[(x, y)].symbol().to_string())
                .collect();
            if !text.contains("Episode 7") {
                continue;
            }
            if buf[(4, y)].modifier.contains(Modifier::REVERSED) {
                reversed_rows.push(text);
            } else {
                normal_rows.push(text);
            }
        }
        assert_eq!(reversed_rows.len(), 1, "{reversed_rows:?}");
        assert!(reversed_rows[0].contains("Episode 78: Ben Frost"));
        assert_eq!(normal_rows.len(), 4);
    }

    #[test]
    fn full_view_shows_search_query_and_match_count() {
        let mut app = full_app();
        app.start_search();
        for c in "datas".chars() {
            app.push_query(c);
        }
        let out = screen(&app, 110, 30);
        assert!(out.contains("/datas_ (1/5)"), "{out}");
        assert!(out.contains("Episode 75: Datassette"), "{out}");
        assert!(!out.contains("Episode 78: Ben Frost"), "{out}");
        assert!(out.contains("Esc cancelar"), "{out}");
        // After accepting, the query stays but the cursor goes away.
        app.accept_search();
        let out = screen(&app, 110, 30);
        assert!(out.contains("/datas (1/5)"), "{out}");
        assert!(out.contains("n next"), "{out}");
    }

    #[test]
    fn full_view_shows_no_matches_and_loading_rows() {
        let mut app = full_app();
        for c in "zzz".chars() {
            app.push_query(c);
        }
        assert!(screen(&app, 110, 30).contains("no matches"));
        let mut loading = App::new(false);
        loading.set_viewport(110, 30);
        assert!(screen(&loading, 110, 30).contains("Loading..."));
    }

    #[test]
    fn full_view_scrolls_the_list_to_keep_the_selection_visible() {
        let mut app = App::new(false);
        app.set_episodes(
            (0..100)
                .map(|i| format!("Episode {}: t", 200 - i))
                .collect(),
        );
        app.move_selection(crate::tui::list::ListMove::End);
        let out = screen(&app, 100, 20);
        assert!(out.contains("Episode 101: t"), "{out}");
        assert!(!out.contains("Episode 200: t"), "{out}");
        assert!(out.contains("Episodes (100)"), "{out}");
    }

    #[test]
    fn full_view_status_panel_shows_messages_and_download_progress() {
        let mut app = full_app();
        let now = Instant::now();
        app.set_status("Paused", now);
        app.apply_download_event(crate::operations::downloads::DownloadEvent::Progress {
            downloaded: 12 * 1_048_576,
            total: 84 * 1_048_576,
        });
        let mut terminal = Terminal::new(TestBackend::new(110, 30)).expect("terminal");
        terminal.draw(|f| draw(f, &mut app, now)).expect("draw");
        let shown = format!("{:?}", terminal.backend().buffer());
        assert!(shown.contains("Paused"), "{shown}");
        assert!(shown.contains("Progress: 14.3% (12.0/84.0 MB)"), "{shown}");
        assert!(shown.contains('━'), "{shown}");
    }

    #[test]
    fn narrow_terminals_fall_back_to_compact_and_v_can_force_full() {
        let mut app = full_app();
        let out = screen(&app, 70, 20);
        assert!(!out.contains("Episodes ("), "{out}");
        assert!(out.contains("Episode 75: Datassette"), "{out}");
        app.layout_pref = crate::tui::app::LayoutPref::Full;
        assert!(screen(&app, 70, 20).contains("Episodes (5)"));
        app.layout_pref = crate::tui::app::LayoutPref::Compact;
        assert!(!screen(&app, 140, 40).contains("Episodes ("));
    }

    #[test]
    fn full_view_does_not_panic_on_degenerate_sizes() {
        for pref in [
            crate::tui::app::LayoutPref::Auto,
            crate::tui::app::LayoutPref::Full,
        ] {
            for w in [0u16, 1, 10, 39, 40, 63, 64, 65, 80, 200] {
                for h in [0u16, 1, 5, 8, 9, 11, 12, 13, 16, 60] {
                    let mut app = full_app();
                    app.layout_pref = pref;
                    app.start_search();
                    app.download = Some("x".into());
                    app.download_bytes = Some((1, 0));
                    let mut terminal = Terminal::new(TestBackend::new(w, h)).expect("terminal");
                    terminal
                        .draw(|f| draw(f, &mut app, Instant::now()))
                        .expect("draw");
                }
            }
        }
    }

    #[test]
    fn full_help_line_adapts_to_width() {
        assert!(help_line_for(&LIST_HELP, 110)
            .to_string()
            .contains("d download"));
        let medium = help_line_for(&LIST_HELP, 70).to_string();
        assert!(medium.contains("Enter play") && !medium.contains("download"));
        assert!(help_line_for(&LIST_HELP, 30).width() <= 30);
        assert!(help_line_for(&SEARCH_HELP, 70)
            .to_string()
            .contains("Esc cancelar"));
    }
}
