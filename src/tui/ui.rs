//! Rendering of the compact player view and the full view (episode list,
//! player info, audio visualizer and a one-line status strip).

use std::time::Instant;

use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};

use super::app::{Activity, App, InputMode, LayoutMode, StatusKind};
use super::theme::Theme;
use super::widgets::visualizer::Visualizer;
use super::widgets::{
    display_width, progress_bar, time_label, truncate_to_width, volume_bar, volume_percent,
};

/// Smallest terminal width the compact view supports.
pub const MIN_WIDTH: u16 = 40;
/// Smallest terminal height the compact view supports.
pub const MIN_HEIGHT: u16 = 9;

/// Smallest terminal height where the compact view spends its spare rows on
/// the visualizer; shorter terminals keep the plain compact sketch.
pub const COMPACT_VIZ_MIN_HEIGHT: u16 = 13;
/// Fewest rows the visualizer needs to be worth drawing.
const VIZ_MIN_ROWS: u16 = 2;
/// Free rows in the right pane from which one blank row is kept between the
/// player info and the visualizer.
const VIZ_SPACER_MIN_ROWS: u16 = 4;
/// Room a download text needs on the status strip before it is shown next to
/// a message.
const STRIP_DOWNLOAD_MIN_COLS: usize = 12;
/// Narrowest gauge worth drawing on the status strip.
const STRIP_GAUGE_MIN_COLS: usize = 6;

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
    let theme = app.theme;
    if let Some(base) = theme.base_style() {
        frame.render_widget(Block::default().style(base), area);
    }
    app.set_viewport(area.width, area.height);
    app.set_viz_size(0, 0);
    if is_too_small(area) {
        let msg = format!("Terminal too small (min {}x{})", MIN_WIDTH, MIN_HEIGHT);
        frame.render_widget(
            Paragraph::new(msg)
                .style(theme.error())
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
    let theme = app.theme;
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme.border())
        .title(Span::styled(" mfp ", theme.title()));
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
            .set_string(divider_x, y, "│", theme.border());
    }
    frame
        .buffer_mut()
        .set_string(divider_x, area.y, "┬", theme.border());
    draw_separator(frame, &theme, area, rows[1].y, Some(divider_x));

    draw_list(frame, inset(cols[0]), app);
    draw_side(frame, inset(cols[2]), app, now);
    let width = inner.width as usize;
    let help = match app.effective_input_mode() {
        InputMode::List => help_line_for(&theme, &LIST_HELP, width),
        InputMode::Search => help_line_for(&theme, &SEARCH_HELP, width),
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
    let theme = app.theme;
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
        lines.push(Line::from(Span::styled(format!("  {}", text), theme.dim())));
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
        app.theme.primary(),
    ))
}

/// One list row: playing marker, title, favorite star. The selected row is
/// drawn with the theme's selection style (reversed video in the default theme).
fn episode_row(app: &App, episode: usize, width: usize, selected: bool) -> Line<'static> {
    let title = app.episodes.get(episode).map(String::as_str).unwrap_or("");
    let room = width.saturating_sub(MARKER_COLS + ROW_STAR_COLS);
    let shown = truncate_to_width(title, room);
    let pad = room.saturating_sub(display_width(&shown));
    let theme = app.theme;
    let playing = app.playing == Some(episode);
    let favorite = app.favorites.contains(title);
    let marker = if playing {
        Span::styled("▶ ", theme.playing_marker())
    } else {
        Span::raw("  ")
    };
    let star = if favorite {
        Span::styled(" ★", theme.favorite())
    } else {
        Span::raw("  ")
    };
    let title_style = if playing {
        theme.primary()
    } else {
        Style::default()
    };
    let mut line = Line::from(vec![
        marker,
        Span::styled(shown, title_style),
        Span::raw(" ".repeat(pad)),
        star,
    ]);
    if selected {
        line = line.patch_style(theme.selection());
    }
    line
}

/// Areas of the right pane of the full view, top to bottom.
struct SideLayout {
    /// Title, duration, blank, progress and volume rows.
    info: [Rect; 5],
    /// Visualizer area; empty when the pane is too short for it.
    viz: Rect,
    /// One-line strip for messages and download progress.
    status: Rect,
}

/// Splits the right pane: fixed info rows on top, a one-line status strip at
/// the bottom, and everything left in between for the visualizer.
fn side_layout(area: Rect) -> SideLayout {
    let rows = Layout::vertical([
        Constraint::Length(1), // title
        Constraint::Length(1), // duration
        Constraint::Length(1), // blank
        Constraint::Length(1), // progress
        Constraint::Length(1), // volume / shuffle / mpris
        Constraint::Min(0),    // visualizer
        Constraint::Length(1), // status strip
    ])
    .split(area);
    let rest = rows[5];
    let spacer = u16::from(rest.height >= VIZ_SPACER_MIN_ROWS);
    let viz = Rect {
        y: rest.y.saturating_add(spacer),
        height: rest.height.saturating_sub(spacer),
        ..rest
    };
    SideLayout {
        info: [rows[0], rows[1], rows[2], rows[3], rows[4]],
        viz: if viz.height >= VIZ_MIN_ROWS && viz.width > 0 {
            viz
        } else {
            Rect::default()
        },
        status: rows[6],
    }
}

/// Right side of the full view: player info, visualizer and status strip.
fn draw_side(frame: &mut Frame, area: Rect, app: &mut App, now: Instant) {
    let layout = side_layout(area);
    let width = area.width as usize;
    frame.render_widget(Paragraph::new(title_line(app, width)), layout.info[0]);
    frame.render_widget(Paragraph::new(duration_line(app)), layout.info[1]);
    frame.render_widget(Paragraph::new(progress_line(app, width)), layout.info[3]);
    frame.render_widget(Paragraph::new(volume_line(app, width)), layout.info[4]);
    draw_visualizer(frame, layout.viz, app);
    frame.render_widget(Paragraph::new(strip_line(app, now, width)), layout.status);
}

/// Draws the visualizer into `area` (when on and non-empty) and reports its
/// size so the event loop can size the analysis.
fn draw_visualizer(frame: &mut Frame, area: Rect, app: &mut App) {
    if !app.viz_enabled || area.width == 0 || area.height == 0 {
        return;
    }
    app.set_viz_size(area.width, area.height);
    frame.render_widget(
        Visualizer {
            style: app.viz_style,
            data: &app.viz_data,
            theme: &app.theme,
        },
        area,
    );
}

/// The one-line status strip: transient message or activity, then the
/// download text and, if there is room, a compact gauge.
fn strip_line(app: &App, now: Instant, width: usize) -> Line<'static> {
    let theme = app.theme;
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut used = 0;
    if let Some((text, kind)) = app.message_line(now) {
        let limit = if app.download.is_some() {
            width
                .saturating_sub(STRIP_DOWNLOAD_MIN_COLS + 2)
                .max(width / 2)
        } else {
            width
        };
        let text = truncate_to_width(&text, limit);
        used = display_width(&text);
        let style = match kind {
            StatusKind::Info => theme.status(),
            StatusKind::Error => theme.error(),
        };
        spans.push(Span::styled(text, style));
    }
    if let Some(download) = &app.download {
        let sep = if used > 0 { 2 } else { 0 };
        let room = width.saturating_sub(used + sep);
        if room >= STRIP_DOWNLOAD_MIN_COLS || used == 0 {
            let text = truncate_to_width(download, room);
            used += sep + display_width(&text);
            spans.push(Span::raw(" ".repeat(sep)));
            spans.push(Span::raw(text));
            if let Some((done, total)) = app.download_bytes {
                let gauge = width.saturating_sub(used + 1);
                if gauge >= STRIP_GAUGE_MIN_COLS {
                    spans.push(Span::raw(" "));
                    spans.extend(bar_spans(
                        progress_bar(done, total, gauge),
                        theme.progress_filled(),
                        theme.progress_empty(),
                    ));
                }
            }
        }
    }
    Line::from(spans)
}

fn draw_compact(frame: &mut Frame, area: Rect, app: &mut App, now: Instant) {
    let theme = app.theme;
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme.border())
        .title(Span::styled(" mfp ", theme.title()));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Spare rows go to the visualizer on tall terminals; otherwise they stay
    // blank at the bottom, as in the plain compact sketch.
    let tall = area.height >= COMPACT_VIZ_MIN_HEIGHT;
    let rows = Layout::vertical([
        Constraint::Length(1), // title
        Constraint::Length(1), // duration
        Constraint::Length(1), // progress
        Constraint::Length(1), // volume / shuffle / mpris
        Constraint::Length(1), // status
        if tall {
            Constraint::Min(0) // visualizer
        } else {
            Constraint::Length(0)
        },
        Constraint::Length(1), // separator
        Constraint::Length(1), // help
        if tall {
            Constraint::Length(0)
        } else {
            Constraint::Min(0)
        },
    ])
    .split(inner);
    let width = inner.width as usize;

    frame.render_widget(Paragraph::new(title_line(app, width)), rows[0]);
    frame.render_widget(Paragraph::new(duration_line(app)), rows[1]);
    frame.render_widget(Paragraph::new(progress_line(app, width)), rows[2]);
    frame.render_widget(Paragraph::new(volume_line(app, width)), rows[3]);
    frame.render_widget(Paragraph::new(status_line(app, now, width)), rows[4]);
    if tall && rows[5].height >= VIZ_MIN_ROWS {
        draw_visualizer(frame, inset(rows[5]), app);
    }
    draw_separator(frame, &theme, area, rows[6].y, None);
    frame.render_widget(Paragraph::new(help_line(&theme, width)), rows[7]);
}

/// Draws `├───┤` across the full outer width at row `y`, with a `┴` junction
/// at column `junction_x` when a vertical divider ends there.
fn draw_separator(frame: &mut Frame, theme: &Theme, outer: Rect, y: u16, junction_x: Option<u16>) {
    let line = format!("├{}┤", "─".repeat(outer.width.saturating_sub(2) as usize));
    frame
        .buffer_mut()
        .set_string(outer.x, y, line, theme.border());
    if let Some(x) = junction_x {
        frame.buffer_mut().set_string(x, y, "┴", theme.border());
    }
}

fn title_line(app: &App, width: usize) -> Line<'static> {
    let theme = app.theme;
    let title = app
        .episode
        .as_ref()
        .map(|e| e.title.as_str())
        .unwrap_or("-");
    let title = truncate_to_width(title, width.saturating_sub(ICON_COLS + STAR_COLS + 1));
    let pad = width.saturating_sub(ICON_COLS + display_width(&title) + STAR_COLS);
    let star = if app.favorite {
        Span::styled("★", theme.favorite())
    } else {
        Span::raw(" ")
    };
    Line::from(vec![
        Span::styled("  ♪  ", theme.accent()),
        Span::styled(title, theme.primary()),
        Span::raw(" ".repeat(pad)),
        star,
    ])
}

fn duration_line(app: &App) -> Line<'static> {
    let theme = app.theme;
    let duration = app
        .episode
        .as_ref()
        .map(|e| e.duration.as_str())
        .unwrap_or("-");
    Line::from(Span::styled(
        format!("     Duración {}", duration),
        theme.dim(),
    ))
}

fn progress_line(app: &App, width: usize) -> Line<'static> {
    let theme = app.theme;
    let total = app.episode.as_ref().map(|e| e.total_seconds).unwrap_or(0);
    let elapsed = time_label(app.elapsed);
    let total_label = time_label(total);
    let icon = if app.paused { "⏸" } else { "▶" };
    // icon cols + elapsed + space + bar + gap + total + right margin
    let fixed = ICON_COLS + display_width(&elapsed) + 1 + 2 + display_width(&total_label) + 2;
    let bar = progress_bar(app.elapsed, total, width.saturating_sub(fixed));
    let mut spans = vec![
        Span::styled(format!("  {}  ", icon), theme.accent()),
        Span::raw(elapsed),
        Span::raw(" "),
    ];
    spans.extend(bar_spans(
        bar,
        theme.progress_filled(),
        theme.progress_empty(),
    ));
    spans.push(Span::raw("  "));
    spans.push(Span::styled(total_label, theme.dim()));
    Line::from(spans)
}

fn volume_line(app: &App, width: usize) -> Line<'static> {
    let theme = app.theme;
    let segments = width
        .saturating_sub(VOLUME_ROW_FIXED)
        .clamp(4, MAX_VOLUME_SEGMENTS);
    let mpris = if app.mpris_connected {
        Span::styled("●", theme.ok())
    } else {
        Span::styled("○", theme.dim())
    };
    let mut spans = vec![Span::styled("  Vol ", theme.dim())];
    spans.extend(bar_spans(
        volume_bar(app.volume, segments),
        theme.volume_filled(),
        theme.volume_empty(),
    ));
    spans.push(Span::raw(format!(" {:<4}", volume_percent(app.volume))));
    spans.push(Span::styled(
        format!("  Shuffle {:<3}", if app.shuffle { "ON" } else { "OFF" }),
        theme.dim(),
    ));
    spans.push(Span::styled("  MPRIS ", theme.dim()));
    spans.push(mpris);
    Line::from(spans)
}

fn status_line(app: &App, now: Instant, width: usize) -> Line<'static> {
    let theme = app.theme;
    match app.status_line(now) {
        Some((text, kind)) => {
            let style = match kind {
                StatusKind::Info => theme.status(),
                StatusKind::Error => theme.error(),
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
    ("d", "download"),
    ("h", "list"),
    ("v", "viz"),
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
const HELP_SHORT: [(&str, &str); 12] = [
    ("n", ""),
    ("b", ""),
    ("p", ""),
    ("s", ""),
    ("f", ""),
    ("m", ""),
    ("+/-", ""),
    ("i", ""),
    ("d", ""),
    ("h", ""),
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
        ("p", "pause"),
        ("s", "shuffle"),
        ("f", "fav"),
        ("d", "download"),
        ("h", "list"),
        ("v", "viz"),
        ("q", "quit"),
    ],
    &[
        ("↑↓", "mover"),
        ("Enter", "play"),
        ("/", "buscar"),
        ("n", "next"),
        ("p", "pause"),
        ("h", "list"),
        ("q", "quit"),
    ],
    &[
        ("↑↓", ""),
        ("Enter", ""),
        ("/", ""),
        ("n", ""),
        ("p", ""),
        ("h", ""),
        ("v", ""),
        ("q", ""),
    ],
];

/// Full view, search mode.
const SEARCH_HELP: [&[(&str, &str)]; 2] = [
    &[("Enter", "aceptar"), ("Esc", "cancelar"), ("↑↓", "mover")],
    &[("Enter", ""), ("Esc", "")],
];

/// Splits a progress or volume bar into its filled part (up to and including
/// the `╸` head) and its empty part (`─` / `▯`) and styles each.
fn bar_spans(bar: String, filled: Style, empty: Style) -> Vec<Span<'static>> {
    let split = bar.find(['─', '▯']).unwrap_or(bar.len());
    let (done, rest) = bar.split_at(split);
    let mut spans = Vec::with_capacity(2);
    if !done.is_empty() {
        spans.push(Span::styled(done.to_string(), filled));
    }
    if !rest.is_empty() {
        spans.push(Span::styled(rest.to_string(), empty));
    }
    spans
}

fn help_spans(theme: &Theme, items: &[(&str, &str)]) -> Vec<Span<'static>> {
    let mut spans = vec![Span::raw("  ")];
    for (i, (key, label)) in items.iter().enumerate() {
        if i > 0 {
            spans.push(Span::raw(if label.is_empty() { " " } else { "  " }));
        }
        spans.push(Span::styled(key.to_string(), theme.key()));
        if !label.is_empty() {
            spans.push(Span::styled(format!(" {}", label), theme.help_text()));
        }
    }
    spans
}

/// Help line of the compact view.
fn help_line(theme: &Theme, width: usize) -> Line<'static> {
    let variants: [&[(&str, &str)]; 3] = [&HELP_FULL, &HELP_MEDIUM, &HELP_SHORT];
    help_line_for(theme, &variants, width)
}

/// Longest of `variants` (ordered long to short) that fits in `width`; the
/// shortest one when none fits.
fn help_line_for(theme: &Theme, variants: &[&[(&str, &str)]], width: usize) -> Line<'static> {
    for items in variants {
        let line = Line::from(help_spans(theme, items));
        if line.width() <= width {
            return line;
        }
    }
    match variants.last() {
        Some(items) => Line::from(help_spans(theme, items)),
        None => Line::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::app::{Activity, EpisodeView};
    use ratatui::{backend::TestBackend, style::Modifier, Terminal};
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
        assert!(help_line(&Theme::default(), 100)
            .to_string()
            .contains("d download"));
        let medium = help_line(&Theme::default(), 70).to_string();
        assert!(medium.contains("s shuffle") && !medium.contains("download"));
        assert!(!help_line(&Theme::default(), 30)
            .to_string()
            .contains("next"));
        assert!(help_line(&Theme::default(), 30).width() <= 30);
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
        assert!(!out.contains(" Status "), "{out}");
        assert!(out.contains("┬") && out.contains("┴"), "{out}");
        assert!(
            out.contains("Enter play") && out.contains("h list"),
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
    fn narrow_terminals_fall_back_to_compact_and_h_can_force_full() {
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
    fn help_variants_stay_within_their_width_budgets() {
        // Compact: long form fits a 100-column terminal (98 inside the
        // border), medium fits 70 (68), short fits the 40-column minimum.
        let long = help_line(&Theme::default(), 98);
        assert!(long.width() <= 98 && long.to_string().contains("v viz"));
        assert!(long.to_string().contains("h list"));
        assert!(long.to_string().contains("d download"));
        let medium = help_line(&Theme::default(), 68);
        assert!(medium.width() <= 68 && !medium.to_string().contains("viz"));
        let short = help_line(&Theme::default(), 38);
        assert!(short.width() <= 38 && short.to_string().contains(" h v "));
        // Full view: long form fits 110 columns (108 inside the border), the
        // short one fits the 64-column minimum (62).
        let long = help_line_for(&Theme::default(), &LIST_HELP, 108);
        assert!(long.width() <= 108 && long.to_string().contains("v viz"));
        assert!(long.to_string().contains("h list"));
        assert!(help_line_for(&Theme::default(), &LIST_HELP, 62).width() <= 62);
        assert!(help_line_for(&Theme::default(), &LIST_HELP, 22)
            .to_string()
            .contains(" h v "));
    }

    #[test]
    fn full_help_line_adapts_to_width() {
        assert!(help_line_for(&Theme::default(), &LIST_HELP, 110)
            .to_string()
            .contains("d download"));
        let medium = help_line_for(&Theme::default(), &LIST_HELP, 70).to_string();
        assert!(medium.contains("Enter play") && !medium.contains("download"));
        assert!(help_line_for(&Theme::default(), &LIST_HELP, 30).width() <= 30);
        assert!(help_line_for(&Theme::default(), &SEARCH_HELP, 70)
            .to_string()
            .contains("Esc cancelar"));
    }

    fn loud() -> crate::tui::widgets::visualizer::VisualizerData {
        crate::tui::widgets::visualizer::VisualizerData {
            bands: vec![1.0; 64],
            peaks: vec![1.0; 64],
            waveform: (0..512).map(|i| (i as f32 / 10.0).sin()).collect(),
            level_l: 0.8,
            level_r: 0.8,
            hold_l: 0.9,
            hold_r: 0.9,
        }
    }

    /// Draws once and returns the screen lines plus the size the visualizer
    /// reported.
    fn render_viz(app: &App, w: u16, h: u16) -> (Vec<String>, (u16, u16)) {
        let mut app = app.clone();
        app.viz_data = loud();
        let mut terminal = Terminal::new(TestBackend::new(w, h)).expect("terminal");
        terminal
            .draw(|f| draw(f, &mut app, Instant::now()))
            .expect("draw");
        let buf = terminal.backend().buffer().clone();
        let lines = (0..h)
            .map(|y| {
                (0..w)
                    .map(|x| buf[(x, y)].symbol().to_string())
                    .collect::<String>()
            })
            .collect();
        (lines, app.viz_size())
    }

    fn has_viz_glyph(line: &str) -> bool {
        line.chars().any(|c| matches!(c, '\u{2581}'..='\u{2588}'))
    }

    #[test]
    fn side_layout_gives_the_leftover_rows_to_the_visualizer() {
        let l = side_layout(Rect::new(10, 2, 60, 20));
        assert_eq!(l.info[0].y, 2);
        assert_eq!(l.info[4].y, 6);
        // 20 - 5 info - 1 strip = 14 free rows; one is a spacer.
        assert_eq!(l.viz, Rect::new(10, 8, 60, 13));
        assert_eq!(l.status, Rect::new(10, 21, 60, 1));
        // Exactly VIZ_MIN_ROWS free rows: no spacer, still drawn.
        let l = side_layout(Rect::new(0, 0, 60, 8));
        assert_eq!(l.viz, Rect::new(0, 5, 60, 2));
        assert_eq!(l.status.y, 7);
        // Too short: no visualizer, strip still there.
        let l = side_layout(Rect::new(0, 0, 60, 7));
        assert_eq!(l.viz, Rect::default());
        assert_eq!(l.status.height, 1);
        assert_eq!(side_layout(Rect::new(0, 0, 0, 20)).viz, Rect::default());
    }

    #[test]
    fn full_view_draws_the_visualizer_between_info_and_status_strip() {
        let mut app = full_app();
        app.set_status("Visualizer: bars", Instant::now());
        let (lines, size) = render_viz(&app, 110, 30);
        assert!(size.0 > 20 && size.1 >= 10, "{size:?}");
        let first = lines
            .iter()
            .position(|l| has_viz_glyph(l))
            .expect("viz rows");
        let volume = lines
            .iter()
            .position(|l| l.contains("MPRIS"))
            .expect("volume row");
        assert!(first > volume, "viz below the info block");
        // Strip: the last row of the body, right above the separator.
        let sep = lines
            .iter()
            .rposition(|l| l.starts_with('├'))
            .expect("separator");
        assert!(
            lines[sep - 1].contains("Visualizer: bars"),
            "{}",
            lines[sep - 1]
        );
        assert!(!has_viz_glyph(&lines[sep - 1]));
        // The list panel is not touched by the visualizer.
        assert!(lines[first].contains('│'));
        assert!(!lines.join("\n").contains(" Status "));
    }

    #[test]
    fn full_view_visualizer_off_reports_no_area_and_draws_nothing() {
        let mut app = full_app();
        app.viz_enabled = false;
        let (lines, size) = render_viz(&app, 110, 30);
        assert_eq!(size, (0, 0));
        assert!(!lines.iter().any(|l| has_viz_glyph(l)));
    }

    #[test]
    fn strip_shows_message_and_download_progress_on_one_line() {
        let mut app = full_app();
        let now = Instant::now();
        app.set_status("Paused", now);
        app.apply_download_event(crate::operations::downloads::DownloadEvent::Progress {
            downloaded: 12 * 1_048_576,
            total: 84 * 1_048_576,
        });
        let line = strip_line(&app, now, 62).to_string();
        assert!(line.contains("Paused"), "{line}");
        assert!(line.contains("Progress: 14.3% (12.0/84.0 MB)"), "{line}");
        assert!(line.contains('━'), "gauge fits at 62 columns: {line}");
        assert!(display_width(&line) <= 62);
        // Narrow strip: still one line within its width, download text first.
        for w in [0usize, 1, 5, 12, 20, 30, 45] {
            assert!(
                display_width(&strip_line(&app, now, w).to_string()) <= w,
                "{w}"
            );
        }
        app.download = None;
        assert_eq!(strip_line(&app, now, 62).to_string(), "Paused");
        let idle = strip_line(&full_app(), now, 62).to_string();
        assert_eq!(idle, "");
    }

    #[test]
    fn strip_in_the_full_view_keeps_the_download_visible_with_the_visualizer_on() {
        let mut app = full_app();
        let now = Instant::now();
        app.set_status("Paused", now);
        app.apply_download_event(crate::operations::downloads::DownloadEvent::Progress {
            downloaded: 12 * 1_048_576,
            total: 84 * 1_048_576,
        });
        let (lines, _) = render_viz(&app, 110, 30);
        let strip = lines
            .iter()
            .find(|l| l.contains("Progress: 14.3%"))
            .expect("progress text");
        assert!(strip.contains("Paused") && strip.contains('━'), "{strip}");
        assert!(lines.iter().any(|l| has_viz_glyph(l)));
    }

    #[test]
    fn compact_view_shows_the_visualizer_only_when_tall_enough() {
        let app = playing_app();
        for h in [9u16, 10, 12] {
            let (lines, size) = render_viz(&app, 100, h);
            assert!(!lines.iter().any(|l| has_viz_glyph(l)), "h={h}");
            assert_eq!(size, (0, 0), "h={h}");
        }
        // 13 rows: 11 inside the border, 7 fixed, 4 spare.
        let (lines, size) = render_viz(&app, 100, COMPACT_VIZ_MIN_HEIGHT);
        assert_eq!(size.1, 4, "{size:?}");
        assert!(lines.iter().any(|l| has_viz_glyph(l)));
        let last = lines.len() - 1;
        assert!(
            lines[last - 1].contains("n next"),
            "help stays at the bottom"
        );
        assert!(lines[last - 2].starts_with('├'), "separator above the help");
        let mut pinned = playing_app();
        pinned.layout_pref = crate::tui::app::LayoutPref::Compact;
        let (_, size) = render_viz(&pinned, 100, 30);
        assert_eq!(size.1, 30 - 2 - 7);
        // Off: rows stay blank.
        let mut off = pinned.clone();
        off.viz_enabled = false;
        let (lines, size) = render_viz(&off, 100, 30);
        assert_eq!(size, (0, 0));
        assert!(!lines.iter().any(|l| has_viz_glyph(l)));
    }

    #[test]
    fn compact_view_below_the_height_rule_matches_the_old_sketch() {
        let mut on = playing_app();
        on.viz_data = loud();
        let mut off = playing_app();
        off.viz_enabled = false;
        for h in [9u16, 12] {
            assert_eq!(render(&on, 100, h), render(&off, 100, h), "h={h}");
        }
    }

    #[test]
    fn every_style_draws_in_both_layouts_without_panicking() {
        use crate::tui::widgets::visualizer::VisualStyle;
        for style in VisualStyle::ALL {
            for (w, h) in [(110u16, 30u16), (70, 20), (100, 13), (64, 12), (40, 9)] {
                let mut app = full_app();
                app.viz_style = style;
                let (lines, _) = render_viz(&app, w, h);
                assert_eq!(lines.len(), h as usize);
            }
        }
    }

    #[test]
    fn visualizer_does_not_panic_on_degenerate_sizes() {
        for pref in [
            crate::tui::app::LayoutPref::Auto,
            crate::tui::app::LayoutPref::Full,
            crate::tui::app::LayoutPref::Compact,
        ] {
            for w in [0u16, 1, 10, 39, 40, 63, 64, 65, 80, 200, 400] {
                for h in [0u16, 1, 5, 8, 9, 11, 12, 13, 14, 16, 60] {
                    let mut app = full_app();
                    app.layout_pref = pref;
                    app.download = Some("x".into());
                    app.download_bytes = Some((1, 0));
                    let _ = render_viz(&app, w, h);
                }
            }
        }
    }

    fn draw_buffer(app: &App, w: u16, h: u16) -> ratatui::buffer::Buffer {
        let mut app = app.clone();
        let mut terminal = Terminal::new(TestBackend::new(w, h)).expect("terminal");
        terminal
            .draw(|f| draw(f, &mut app, Instant::now()))
            .expect("draw");
        terminal.backend().buffer().clone()
    }

    #[test]
    fn non_reset_background_fills_the_whole_frame() {
        use ratatui::style::Color;
        let mut app = full_app();
        app.theme = Theme::preset("nord").expect("preset");
        let buf = draw_buffer(&app, 110, 30);
        for (x, y) in [(0, 0), (109, 29), (55, 15), (3, 20), (109, 0)] {
            assert_eq!(buf[(x, y)].bg, app.theme.background, "({x},{y})");
        }
        // Blank cells keep the base foreground.
        let base_blanks = (0..30u16)
            .flat_map(|y| (0..110u16).map(move |x| (x, y)))
            .filter(|&(x, y)| buf[(x, y)].symbol() == " " && buf[(x, y)].fg == app.theme.foreground)
            .count();
        assert!(base_blanks > 500, "{base_blanks}");
        // The too-small message is drawn over the fill too.
        let small = draw_buffer(&app, 20, 5);
        assert_eq!(small[(19, 4)].bg, app.theme.background);
        assert_ne!(app.theme.background, Color::Reset);
    }

    #[test]
    fn reset_background_paints_nothing() {
        use ratatui::style::Color;
        let mut app = full_app();
        app.theme = Theme::default();
        let buf = draw_buffer(&app, 110, 30);
        for y in 0..30 {
            for x in 0..110 {
                assert_eq!(buf[(x, y)].bg, Color::Reset, "({x},{y})");
            }
        }
    }

    #[test]
    fn theme_colors_reach_borders_list_and_bars() {
        let mut app = full_app();
        app.theme = Theme::preset("dracula").expect("preset");
        let t = app.theme;
        let buf = draw_buffer(&app, 110, 30);
        assert_eq!(buf[(0, 5)].fg, t.border);
        // Selection uses explicit colors instead of reversed video.
        let sel_row = (0..30u16)
            .find(|&y| buf[(4, y)].bg == t.selection_bg && buf[(4, y)].fg == t.selection_fg)
            .expect("a selected row");
        assert!(!buf[(4, sel_row)].modifier.contains(Modifier::REVERSED));
    }

    #[test]
    fn bar_spans_split_filled_and_empty_parts() {
        let (f, e) = (
            Style::default().add_modifier(Modifier::BOLD),
            Style::default().add_modifier(Modifier::DIM),
        );
        let spans = bar_spans("━━╸───".to_string(), f, e);
        assert_eq!(spans.len(), 2);
        assert_eq!(spans[0].content, "━━╸");
        assert_eq!(spans[0].style, f);
        assert_eq!(spans[1].content, "───");
        assert_eq!(spans[1].style, e);
        assert_eq!(bar_spans("▮▮▯▯".to_string(), f, e).len(), 2);
        assert_eq!(bar_spans("━━━━".to_string(), f, e).len(), 1);
        assert_eq!(bar_spans("▯▯".to_string(), f, e)[0].style, e);
        assert!(bar_spans(String::new(), f, e).is_empty());
    }
}
