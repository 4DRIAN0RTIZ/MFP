//! TUI state: everything the views need, with no I/O.

use std::collections::HashSet;
use std::time::{Duration, Instant};

use crate::operations::downloads::DownloadEvent;
use crate::player::PlayerStage;

use super::list::{filter_indices, move_selection, scroll_offset, ListMove};
use super::widgets::visualizer::{VisualStyle, VisualizerData};

/// How long a transient status message stays visible.
const STATUS_TTL: Duration = Duration::from_secs(4);

/// What the player is busy with, shown in the status line.
#[derive(Debug, Clone, PartialEq)]
pub enum Activity {
    /// Nothing in progress.
    Idle,
    /// Fetching the episode feed.
    Loading,
    /// Opening the audio stream.
    Connecting,
    /// Filling the initial buffer.
    Buffering,
    /// A fatal problem (e.g. feed unavailable); stays until the user quits.
    Failed(String),
}

/// Kind of a transient status message (drives its color).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusKind {
    /// Regular feedback.
    Info,
    /// Something went wrong.
    Error,
}

/// Smallest terminal (width, height) where `Auto` picks the full view.
pub const FULL_AUTO_MIN: (u16, u16) = (80, 16);
/// Smallest terminal (width, height) where the full view can be forced.
pub const FULL_MIN: (u16, u16) = (64, 12);
/// Rows per page before the first draw reports the real list height.
const DEFAULT_PAGE: usize = 10;

/// User preference for the layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LayoutPref {
    /// Full when the terminal is big enough, compact otherwise.
    #[default]
    Auto,
    /// Always compact (manual override).
    Compact,
    /// Full whenever it fits at all (manual override).
    Full,
}

/// The layout actually drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutMode {
    /// Player only.
    Compact,
    /// Episode list plus player and status panels.
    Full,
}

/// Resolves the layout to draw for a preference and terminal size.
pub fn resolve_layout(pref: LayoutPref, width: u16, height: u16) -> LayoutMode {
    let fits = |min: (u16, u16)| width >= min.0 && height >= min.1;
    match pref {
        LayoutPref::Compact => LayoutMode::Compact,
        LayoutPref::Full if fits(FULL_MIN) => LayoutMode::Full,
        LayoutPref::Auto if fits(FULL_AUTO_MIN) => LayoutMode::Full,
        _ => LayoutMode::Compact,
    }
}

/// Which part of the UI receives typed characters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    /// Letters are playback shortcuts; arrows move the selection.
    List,
    /// Letters are typed into the search query.
    Search,
}

/// The episode being played.
#[derive(Debug, Clone, PartialEq)]
pub struct EpisodeView {
    /// Episode title.
    pub title: String,
    /// Duration text as published in the feed.
    pub duration: String,
    /// Duration in seconds (0 when unknown).
    pub total_seconds: u64,
}

/// Complete UI state.
#[derive(Debug, Clone)]
pub struct App {
    /// Current episode, if a playlist is loaded.
    pub episode: Option<EpisodeView>,
    /// Seconds played.
    pub elapsed: u64,
    /// Volume, 0.0 to 2.0.
    pub volume: f32,
    /// Whether playback is paused.
    pub paused: bool,
    /// Whether shuffle is on.
    pub shuffle: bool,
    /// Whether the current episode is a favorite.
    pub favorite: bool,
    /// Whether an MPRIS controller is running.
    pub mpris_connected: bool,
    /// What the player is doing right now.
    pub activity: Activity,
    /// Download progress text while a download runs.
    pub download: Option<String>,
    /// Download completion in `(downloaded, total)` bytes, when known.
    pub download_bytes: Option<(u64, u64)>,
    /// Titles of the playlist, in feed order (what the list shows).
    pub episodes: Vec<String>,
    /// Titles that are favorites.
    pub favorites: HashSet<String>,
    /// Index into `episodes` of the playing episode.
    pub playing: Option<usize>,
    /// Layout preference (`v` and `--compact` change it).
    pub layout_pref: LayoutPref,
    /// Search query typed so far (applied live as a filter).
    pub query: String,
    /// Where typed characters go.
    pub input_mode: InputMode,
    /// Selected row within the filtered list.
    selected: usize,
    /// First visible row of the filtered list, maintained by the renderer.
    list_offset: usize,
    /// Visible list rows, reported by the renderer; drives paging.
    list_page: usize,
    /// Terminal size seen by the last draw.
    viewport: (u16, u16),
    status: Option<(String, StatusKind, Instant)>,
    /// Whether the audio visualizer is switched on (`W` toggles it).
    pub viz_enabled: bool,
    /// Current visualizer style (`w` rotates it).
    pub viz_style: VisualStyle,
    /// Latest visualizer frame, refreshed by the event loop before each draw.
    pub viz_data: VisualizerData,
    /// Size of the visualizer area drawn last frame; `(0, 0)` when none.
    viz_size: (u16, u16),
}

impl App {
    /// Creates the initial state (feed still loading).
    pub fn new(mpris_connected: bool) -> Self {
        Self {
            episode: None,
            elapsed: 0,
            volume: 1.0,
            paused: false,
            shuffle: false,
            favorite: false,
            mpris_connected,
            activity: Activity::Loading,
            download: None,
            download_bytes: None,
            episodes: Vec::new(),
            favorites: HashSet::new(),
            playing: None,
            layout_pref: LayoutPref::Auto,
            query: String::new(),
            input_mode: InputMode::List,
            selected: 0,
            list_offset: 0,
            list_page: DEFAULT_PAGE,
            viewport: (0, 0),
            status: None,
            viz_enabled: true,
            viz_style: VisualStyle::default(),
            viz_data: VisualizerData::default(),
            viz_size: (0, 0),
        }
    }

    /// Records the visualizer area of the frame being drawn (`(0, 0)` when
    /// the layout leaves no room for it).
    pub fn set_viz_size(&mut self, width: u16, height: u16) {
        self.viz_size = (width, height);
    }

    /// Width and height of the visualizer area drawn last frame.
    pub fn viz_size(&self) -> (u16, u16) {
        self.viz_size
    }

    /// Whether the visualizer is on and had room on screen last frame.
    pub fn viz_visible(&self) -> bool {
        self.viz_enabled && self.viz_size.0 > 0 && self.viz_size.1 > 0
    }

    /// Whether audio is actually flowing (an episode is playing, not paused
    /// and no start-up stage pending).
    pub fn is_playing(&self) -> bool {
        self.episode.is_some() && !self.paused && self.activity == Activity::Idle
    }

    /// `w`: switches the visualizer on if it was off, otherwise moves to the
    /// next style. Returns the status message to show.
    pub fn viz_next(&mut self) -> String {
        if self.viz_enabled {
            self.viz_style = self.viz_style.next();
        } else {
            self.viz_enabled = true;
        }
        format!("Visualizer: {}", self.viz_style.name())
    }

    /// `W`: toggles the visualizer on or off. Returns the status message.
    pub fn viz_toggle(&mut self) -> String {
        self.viz_enabled = !self.viz_enabled;
        if self.viz_enabled {
            format!("Visualizer: {}", self.viz_style.name())
        } else {
            "Visualizer: off".to_string()
        }
    }

    /// Layout that the current terminal size resolves to.
    pub fn layout(&self) -> LayoutMode {
        resolve_layout(self.layout_pref, self.viewport.0, self.viewport.1)
    }

    /// Input mode in effect: search only counts while the list is on screen,
    /// so a resize to the compact view never leaves keys swallowed by an
    /// invisible search box.
    pub fn effective_input_mode(&self) -> InputMode {
        if self.layout() == LayoutMode::Full {
            self.input_mode
        } else {
            InputMode::List
        }
    }

    /// Records the terminal size (called by the renderer).
    pub fn set_viewport(&mut self, width: u16, height: u16) {
        self.viewport = (width, height);
    }

    /// Switches between compact and full and pins the choice for the session.
    ///
    /// Returns `false` (leaving the preference alone) when the full view was
    /// requested but the terminal is too small for it.
    pub fn toggle_layout(&mut self) -> bool {
        match self.layout() {
            LayoutMode::Full => {
                self.layout_pref = LayoutPref::Compact;
                true
            }
            LayoutMode::Compact => {
                if resolve_layout(LayoutPref::Full, self.viewport.0, self.viewport.1)
                    == LayoutMode::Full
                {
                    self.layout_pref = LayoutPref::Full;
                    true
                } else {
                    false
                }
            }
        }
    }

    /// Replaces the list contents (titles in feed order).
    pub fn set_episodes(&mut self, titles: Vec<String>) {
        self.episodes = titles;
        self.selected = 0;
        self.list_offset = 0;
    }

    /// Indices into `episodes` that pass the current filter.
    pub fn visible(&self) -> Vec<usize> {
        filter_indices(&self.episodes, &self.query)
    }

    /// Selected row within [`App::visible`], clamped to the list.
    pub fn selected(&self) -> usize {
        self.selected.min(self.visible().len().saturating_sub(1))
    }

    /// Index into `episodes` of the selected row, if any.
    pub fn selected_episode(&self) -> Option<usize> {
        self.visible().get(self.selected()).copied()
    }

    /// Moves the selection.
    pub fn move_selection(&mut self, movement: ListMove) {
        let len = self.visible().len();
        self.selected = move_selection(self.selected(), len, movement, self.list_page);
    }

    /// Puts the selection on the playing episode when it passes the filter.
    pub fn select_playing(&mut self) {
        if let Some(playing) = self.playing {
            if let Some(pos) = self.visible().iter().position(|&i| i == playing) {
                self.selected = pos;
            }
        }
    }

    /// Appends a character to the search query.
    pub fn push_query(&mut self, c: char) {
        let keep = self.selected_episode();
        self.query.push(c);
        self.reselect(keep);
    }

    /// Removes the last character of the search query.
    pub fn pop_query(&mut self) {
        let keep = self.selected_episode();
        self.query.pop();
        self.reselect(keep);
    }

    /// Enters search mode, keeping any existing query for editing.
    pub fn start_search(&mut self) {
        self.input_mode = InputMode::Search;
    }

    /// Leaves search mode keeping the filter.
    pub fn accept_search(&mut self) {
        self.input_mode = InputMode::List;
    }

    /// Clears the filter and leaves search mode; the selection stays on the
    /// same episode.
    pub fn cancel_search(&mut self) {
        let keep = self.selected_episode();
        self.input_mode = InputMode::List;
        self.query.clear();
        self.reselect(keep);
    }

    /// After the filter changed: keep `episode` selected when it still
    /// matches, otherwise start from the top.
    fn reselect(&mut self, episode: Option<usize>) {
        let visible = self.visible();
        self.selected = episode
            .and_then(|ep| visible.iter().position(|&i| i == ep))
            .unwrap_or(0);
        self.list_offset = 0;
    }

    /// The visible window for a list `height` rows tall: `(offset, rows)` where
    /// rows are indices into `episodes`. Updates the stored offset and page size.
    pub fn list_window(&mut self, height: usize) -> (usize, Vec<usize>) {
        let visible = self.visible();
        self.list_page = height.saturating_sub(1).max(1);
        let selected = self.selected.min(visible.len().saturating_sub(1));
        self.list_offset = scroll_offset(self.list_offset, selected, height, visible.len());
        let rows = visible
            .into_iter()
            .skip(self.list_offset)
            .take(height)
            .collect();
        (self.list_offset, rows)
    }

    /// Shows `message` for a few seconds.
    pub fn set_status(&mut self, message: impl Into<String>, now: Instant) {
        self.status = Some((message.into(), StatusKind::Info, now));
    }

    /// Shows an error `message` for a few seconds.
    pub fn set_error(&mut self, message: impl Into<String>, now: Instant) {
        self.status = Some((message.into(), StatusKind::Error, now));
    }

    /// The transient message still valid at `now`.
    pub fn status_at(&self, now: Instant) -> Option<(&str, StatusKind)> {
        let (text, kind, since) = self.status.as_ref()?;
        (now.saturating_duration_since(*since) < STATUS_TTL).then_some((text.as_str(), *kind))
    }

    /// Drops the transient message once it has expired.
    pub fn expire_status(&mut self, now: Instant) {
        if self.status_at(now).is_none() {
            self.status = None;
        }
    }

    /// The single line to show in the status area at `now`, with its kind.
    ///
    /// Priority: transient message, then player activity, then download.
    pub fn status_line(&self, now: Instant) -> Option<(String, StatusKind)> {
        self.message_line(now)
            .or_else(|| self.download.clone().map(|d| (d, StatusKind::Info)))
    }

    /// Like [`App::status_line`] without the download text: the transient
    /// message or the player activity.
    pub fn message_line(&self, now: Instant) -> Option<(String, StatusKind)> {
        if let Some((text, kind)) = self.status_at(now) {
            return Some((text.to_string(), kind));
        }
        match &self.activity {
            Activity::Loading => Some(("Cargando feed...".into(), StatusKind::Info)),
            Activity::Connecting => Some(("Connecting...".into(), StatusKind::Info)),
            Activity::Buffering => Some(("Connecting... buffering...".into(), StatusKind::Info)),
            Activity::Failed(e) => Some((e.clone(), StatusKind::Error)),
            Activity::Idle => None,
        }
    }

    /// Resets per-episode state when a new episode starts.
    pub fn begin_episode(&mut self, episode: EpisodeView, favorite: bool, shuffle: bool) {
        self.episode = Some(episode);
        self.favorite = favorite;
        self.shuffle = shuffle;
        self.elapsed = 0;
        self.paused = false;
        self.activity = Activity::Connecting;
    }

    /// Applies a stream start-up stage.
    pub fn apply_stage(&mut self, stage: PlayerStage) {
        self.activity = match stage {
            PlayerStage::Connecting => Activity::Connecting,
            PlayerStage::Buffering => Activity::Buffering,
            PlayerStage::Ready => Activity::Idle,
        };
    }

    /// Applies a download event to the progress text.
    pub fn apply_download_event(&mut self, event: DownloadEvent) {
        self.download_bytes = match &event {
            DownloadEvent::Progress { downloaded, total } => Some((*downloaded, *total)),
            DownloadEvent::Finished { downloaded } => Some((*downloaded, *downloaded)),
            _ => self.download_bytes,
        };
        self.download = Some(download_text(&event));
    }
}

/// Text for a download event, mirroring the plain CLI wording.
pub fn download_text(event: &DownloadEvent) -> String {
    const MB: f64 = 1_048_576.0;
    match event {
        DownloadEvent::AlreadyDownloaded { filename } => {
            format!("Episode already downloaded: {}", filename)
        }
        DownloadEvent::Started { .. } => "Downloading episode for offline...".to_string(),
        DownloadEvent::Progress { downloaded, total } => format!(
            "Progress: {:.1}% ({:.1}/{:.1} MB)",
            *downloaded as f64 / *total as f64 * 100.0,
            *downloaded as f64 / MB,
            *total as f64 / MB
        ),
        DownloadEvent::Finished { downloaded } => {
            format!("Download complete: {:.2} MB", *downloaded as f64 / MB)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn episode() -> EpisodeView {
        EpisodeView {
            title: "Episode 75: Datassette".into(),
            duration: "01:32:53".into(),
            total_seconds: 5573,
        }
    }

    #[test]
    fn status_expires_after_ttl() {
        let t0 = Instant::now();
        let mut app = App::new(false);
        app.activity = Activity::Idle;
        app.set_status("Paused", t0);
        assert_eq!(app.status_at(t0).map(|s| s.0), Some("Paused"));
        assert!(app.status_at(t0 + Duration::from_secs(3)).is_some());
        assert!(app.status_at(t0 + Duration::from_secs(5)).is_none());
        app.expire_status(t0 + Duration::from_secs(5));
        assert!(app.status_line(t0 + Duration::from_secs(5)).is_none());
    }

    #[test]
    fn status_line_priority_message_then_activity_then_download() {
        let t0 = Instant::now();
        let mut app = App::new(true);
        app.begin_episode(episode(), false, false);
        app.download = Some("Progress: 10.0%".into());
        assert_eq!(
            app.status_line(t0).map(|s| s.0).as_deref(),
            Some("Connecting...")
        );
        app.set_status("Paused", t0);
        assert_eq!(app.status_line(t0).map(|s| s.0).as_deref(), Some("Paused"));
        let later = t0 + Duration::from_secs(10);
        assert_eq!(
            app.status_line(later).map(|s| s.0).as_deref(),
            Some("Connecting...")
        );
        app.apply_stage(PlayerStage::Ready);
        assert_eq!(
            app.status_line(later).map(|s| s.0).as_deref(),
            Some("Progress: 10.0%")
        );
    }

    #[test]
    fn viz_defaults_and_style_rotation() {
        let mut app = App::new(false);
        assert!(app.viz_enabled);
        assert_eq!(app.viz_style, VisualStyle::Bars);
        let names: Vec<String> = (0..6).map(|_| app.viz_next()).collect();
        assert_eq!(
            names,
            [
                "Visualizer: mirror",
                "Visualizer: wave",
                "Visualizer: dots",
                "Visualizer: area",
                "Visualizer: vu",
                "Visualizer: bars"
            ]
        );
    }

    #[test]
    fn viz_toggle_and_next_when_off() {
        let mut app = App::new(false);
        assert_eq!(app.viz_toggle(), "Visualizer: off");
        assert!(!app.viz_enabled);
        // `w` while off switches it back on keeping the style.
        assert_eq!(app.viz_next(), "Visualizer: bars");
        assert!(app.viz_enabled);
        assert_eq!(app.viz_toggle(), "Visualizer: off");
        assert_eq!(app.viz_toggle(), "Visualizer: bars");
    }

    #[test]
    fn viz_visible_needs_enabled_and_room() {
        let mut app = App::new(false);
        assert!(!app.viz_visible());
        app.set_viz_size(40, 5);
        assert!(app.viz_visible());
        app.viz_enabled = false;
        assert!(!app.viz_visible());
        app.viz_enabled = true;
        app.set_viz_size(40, 0);
        assert!(!app.viz_visible());
    }

    #[test]
    fn is_playing_needs_episode_idle_and_not_paused() {
        let mut app = App::new(false);
        assert!(!app.is_playing());
        app.begin_episode(episode(), false, false);
        assert!(!app.is_playing(), "still connecting");
        app.apply_stage(PlayerStage::Ready);
        assert!(app.is_playing());
        app.paused = true;
        assert!(!app.is_playing());
    }

    #[test]
    fn stages_map_to_activity() {
        let mut app = App::new(false);
        app.apply_stage(PlayerStage::Buffering);
        assert_eq!(app.activity, Activity::Buffering);
        app.apply_stage(PlayerStage::Ready);
        assert_eq!(app.activity, Activity::Idle);
    }

    #[test]
    fn begin_episode_resets_playback_state() {
        let mut app = App::new(false);
        app.elapsed = 99;
        app.paused = true;
        app.begin_episode(episode(), true, true);
        assert_eq!(app.elapsed, 0);
        assert!(!app.paused && app.favorite && app.shuffle);
        assert_eq!(app.activity, Activity::Connecting);
    }

    #[test]
    fn download_text_mirrors_cli_wording() {
        assert_eq!(
            download_text(&DownloadEvent::Progress {
                downloaded: 1_048_576,
                total: 4_194_304
            }),
            "Progress: 25.0% (1.0/4.0 MB)"
        );
        assert_eq!(
            download_text(&DownloadEvent::Finished {
                downloaded: 2_097_152
            }),
            "Download complete: 2.00 MB"
        );
        assert_eq!(
            download_text(&DownloadEvent::AlreadyDownloaded {
                filename: "a.mp3".into()
            }),
            "Episode already downloaded: a.mp3"
        );
    }

    fn list_app(n: usize) -> App {
        let mut app = App::new(false);
        app.set_episodes(
            (0..n)
                .map(|i| format!("Episode {}: title {}", 100 - i, i))
                .collect(),
        );
        app
    }

    #[test]
    fn auto_layout_depends_on_terminal_size() {
        use LayoutMode::*;
        assert_eq!(resolve_layout(LayoutPref::Auto, 80, 16), Full);
        assert_eq!(resolve_layout(LayoutPref::Auto, 200, 50), Full);
        assert_eq!(resolve_layout(LayoutPref::Auto, 79, 30), Compact);
        assert_eq!(resolve_layout(LayoutPref::Auto, 120, 15), Compact);
        assert_eq!(resolve_layout(LayoutPref::Compact, 200, 50), Compact);
        // Forced full works below the auto threshold but not below its own minimum.
        assert_eq!(resolve_layout(LayoutPref::Full, 70, 13), Full);
        assert_eq!(resolve_layout(LayoutPref::Full, 63, 30), Compact);
        assert_eq!(resolve_layout(LayoutPref::Full, 100, 11), Compact);
    }

    #[test]
    fn toggle_layout_overrides_auto_and_sticks() {
        let mut app = App::new(false);
        app.set_viewport(110, 30);
        assert_eq!(app.layout(), LayoutMode::Full);
        assert!(app.toggle_layout());
        assert_eq!(app.layout_pref, LayoutPref::Compact);
        // Resizing does not undo the manual choice.
        app.set_viewport(200, 60);
        assert_eq!(app.layout(), LayoutMode::Compact);
        assert!(app.toggle_layout());
        assert_eq!(app.layout_pref, LayoutPref::Full);
        // A forced full view then survives a shrink to a still-fitting size.
        app.set_viewport(70, 20);
        assert_eq!(app.layout(), LayoutMode::Full);
    }

    #[test]
    fn toggle_to_full_is_refused_when_it_does_not_fit() {
        let mut app = App::new(false);
        app.set_viewport(50, 20);
        assert_eq!(app.layout(), LayoutMode::Compact);
        assert!(!app.toggle_layout());
        assert_eq!(app.layout_pref, LayoutPref::Auto);
    }

    #[test]
    fn search_mode_only_counts_while_the_list_is_visible() {
        let mut app = list_app(3);
        app.set_viewport(110, 30);
        app.start_search();
        assert_eq!(app.effective_input_mode(), InputMode::Search);
        app.set_viewport(60, 20);
        assert_eq!(app.effective_input_mode(), InputMode::List);
    }

    #[test]
    fn selection_moves_and_clamps() {
        let mut app = list_app(5);
        app.move_selection(ListMove::Up);
        assert_eq!(app.selected(), 0);
        app.move_selection(ListMove::End);
        assert_eq!(app.selected(), 4);
        app.move_selection(ListMove::Down);
        assert_eq!(app.selected(), 4);
        app.move_selection(ListMove::Home);
        assert_eq!(app.selected_episode(), Some(0));
        app.move_selection(ListMove::PageDown);
        assert_eq!(app.selected(), 4);
    }

    #[test]
    fn selection_starts_on_the_playing_episode() {
        let mut app = list_app(10);
        app.playing = Some(6);
        app.select_playing();
        assert_eq!(app.selected(), 6);
        assert_eq!(app.selected_episode(), Some(6));
    }

    #[test]
    fn typing_filters_live_and_keeps_the_selected_episode_when_it_matches() {
        let mut app = list_app(10);
        app.start_search();
        app.move_selection(ListMove::End);
        assert_eq!(app.selected_episode(), Some(9));
        for c in "title 9".chars() {
            app.push_query(c);
        }
        assert_eq!(app.visible(), vec![9]);
        assert_eq!(app.selected_episode(), Some(9));
        // Backspacing widens the filter; the episode stays selected.
        app.pop_query();
        assert_eq!(app.selected_episode(), Some(9));
        app.cancel_search();
        assert!(app.query.is_empty());
        assert_eq!(app.input_mode, InputMode::List);
        assert_eq!(app.visible().len(), 10);
        assert_eq!(app.selected_episode(), Some(9));
    }

    #[test]
    fn filter_without_the_old_selection_starts_from_the_top() {
        let mut app = list_app(10);
        app.move_selection(ListMove::End);
        app.start_search();
        for c in "title 2".chars() {
            app.push_query(c);
        }
        assert_eq!(app.visible(), vec![2]);
        assert_eq!(app.selected_episode(), Some(2));
        for c in "zz".chars() {
            app.push_query(c);
        }
        assert!(app.visible().is_empty());
        assert_eq!(app.selected_episode(), None);
    }

    #[test]
    fn accepting_search_keeps_the_filter() {
        let mut app = list_app(10);
        app.start_search();
        for c in "title 3".chars() {
            app.push_query(c);
        }
        app.accept_search();
        assert_eq!(app.input_mode, InputMode::List);
        assert_eq!(app.query, "title 3");
        assert_eq!(app.visible(), vec![3]);
    }

    #[test]
    fn list_window_scrolls_with_the_selection() {
        let mut app = list_app(30);
        let (offset, rows) = app.list_window(5);
        assert_eq!((offset, rows.len()), (0, 5));
        for _ in 0..7 {
            app.move_selection(ListMove::Down);
        }
        let (offset, rows) = app.list_window(5);
        assert_eq!(offset, 3);
        assert_eq!(rows, vec![3, 4, 5, 6, 7]);
        // Page size follows the reported height.
        app.move_selection(ListMove::PageDown);
        assert_eq!(app.selected(), 11);
        app.move_selection(ListMove::End);
        let (offset, _) = app.list_window(5);
        assert_eq!(offset, 25);
    }
}
