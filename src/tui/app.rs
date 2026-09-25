//! TUI state: everything the views need, with no I/O.

use std::time::{Duration, Instant};

use crate::operations::downloads::DownloadEvent;
use crate::player::PlayerStage;

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
    status: Option<(String, StatusKind, Instant)>,
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
            status: None,
        }
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
        if let Some((text, kind)) = self.status_at(now) {
            return Some((text.to_string(), kind));
        }
        match &self.activity {
            Activity::Loading => return Some(("Cargando feed...".into(), StatusKind::Info)),
            Activity::Connecting => return Some(("Connecting...".into(), StatusKind::Info)),
            Activity::Buffering => {
                return Some(("Connecting... buffering...".into(), StatusKind::Info))
            }
            Activity::Failed(e) => return Some((e.clone(), StatusKind::Error)),
            Activity::Idle => {}
        }
        self.download.clone().map(|d| (d, StatusKind::Info))
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
}
