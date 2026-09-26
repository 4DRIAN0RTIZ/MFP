//! Glue between the UI state and the playback layer.
//!
//! The `Session` owns the playlist and favorites, applies commands through
//! `operations::playback::apply`, and starts background work (stream start-up,
//! downloads, feed fetch) on std threads that report back over an mpsc channel.
//! The UI thread never waits on any of it. `Player` is not `Send` (it owns the
//! audio output stream), so it stays on the UI thread; only its non-blocking
//! `start` is called here.

use std::sync::mpsc::Sender;
use std::thread;
use std::time::Instant;

use crate::logging::log;
use crate::models::Episode;
use crate::mpris::{MprisCommand, MprisController};
use crate::operations::downloads::{DownloadEvent, Downloader};
use crate::operations::favorites::Favorites;
use crate::operations::feed::Feed;
use crate::operations::playback::{self, Action, Outcome};
use crate::operations::playlist::Playlist;
use crate::player::{self, Player, PlayerStage};

use super::app::{download_text, Activity, App, EpisodeView, LayoutMode};
use super::events::UiCommand;

/// Options selecting what to play, mirroring `mfp play` flags.
#[derive(Debug, Clone, Copy, Default)]
pub struct PlayOptions {
    /// Start at this episode number.
    pub episode: Option<usize>,
    /// Enable shuffle.
    pub shuffle: bool,
    /// Play only favorites.
    pub favorites_only: bool,
    /// Start with the episode list hidden instead of picking a layout from the terminal size.
    pub compact: bool,
}

/// Messages sent from background threads to the UI loop.
#[derive(Debug)]
pub enum AppMsg {
    /// Feed fetch finished.
    Feed(Result<Vec<Episode>, String>),
    /// Stream start-up progress, tagged with the playback generation that
    /// produced it so stale stages from a skipped episode are ignored.
    Stage(u64, PlayerStage),
    /// Download progress.
    Download(DownloadEvent),
    /// Download finished (Ok) or failed (Err with the message).
    DownloadDone(Result<(), String>),
}

/// Whether the UI loop should keep running.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flow {
    /// Keep going.
    Continue,
    /// Leave the UI.
    Quit,
}

/// Runtime state behind the UI.
pub struct Session<'a> {
    player: &'a Player,
    playlist: Option<Playlist>,
    favorites: Favorites,
    mpris: Option<&'a MprisController>,
    mpris_rx: Option<async_channel::Receiver<MprisCommand>>,
    tx: Sender<AppMsg>,
    options: PlayOptions,
    generation: u64,
    downloading: bool,
}

impl<'a> Session<'a> {
    /// Creates a session and starts fetching the feed in the background.
    pub fn new(
        player: &'a Player,
        favorites: Favorites,
        mpris: Option<&'a MprisController>,
        options: PlayOptions,
        tx: Sender<AppMsg>,
    ) -> Self {
        let feed_tx = tx.clone();
        thread::spawn(move || {
            let result = Feed::fetch()
                .map(|feed| feed.episodes().to_vec())
                .map_err(|e| format!("{:#}", e));
            let _ = feed_tx.send(AppMsg::Feed(result));
        });
        Self {
            player,
            playlist: None,
            favorites,
            mpris,
            mpris_rx: mpris.map(|m| m.command_receiver()),
            tx,
            options,
            generation: 0,
            downloading: false,
        }
    }

    /// Handles a background message.
    pub fn handle_msg(&mut self, msg: AppMsg, app: &mut App, now: Instant) {
        match msg {
            AppMsg::Feed(Ok(episodes)) => {
                let fav_titles = self.favorites.list();
                let playlist = Playlist::for_session(
                    &episodes,
                    self.options.favorites_only.then_some(fav_titles.as_slice()),
                    self.options.shuffle,
                    self.options.episode,
                );
                app.set_episodes(
                    playlist
                        .all_episodes()
                        .iter()
                        .map(|e| e.title.clone())
                        .collect(),
                );
                self.playlist = Some(playlist);
                self.start_current(app, now);
            }
            AppMsg::Feed(Err(e)) => {
                log(&format!("feed fetch failed: {}", e));
                app.activity = Activity::Failed(format!("Error: {}", e));
            }
            AppMsg::Stage(generation, stage) => {
                if generation == self.generation {
                    app.apply_stage(stage);
                }
            }
            AppMsg::Download(event) => app.apply_download_event(event),
            AppMsg::DownloadDone(result) => {
                self.downloading = false;
                let last = app.download.take();
                app.download_bytes = None;
                match result {
                    Ok(()) => match last {
                        Some(text) if text.starts_with("Episode already") => {
                            app.set_status(text, now)
                        }
                        _ => app.set_status("Episode downloaded", now),
                    },
                    Err(e) => app.set_error(format!("Error: {}", e), now),
                }
            }
        }
    }

    /// Handles a key command.
    pub fn handle_command(&mut self, command: UiCommand, app: &mut App, now: Instant) -> Flow {
        match command {
            UiCommand::Playback(action) => self.apply(action, app, now),
            UiCommand::Info => {
                self.show_info(app, now);
                Flow::Continue
            }
            UiCommand::Download => {
                self.start_download(app, now);
                Flow::Continue
            }
            UiCommand::ToggleList => {
                match app.toggle_list() {
                    Some(message) => app.set_status(message, now),
                    None => app.set_status("Terminal too small for the list", now),
                }
                Flow::Continue
            }
            UiCommand::VizNext => {
                let message = app.viz_next();
                app.set_status(message, now);
                Flow::Continue
            }
            UiCommand::VizToggle => {
                let message = app.viz_toggle();
                app.set_status(message, now);
                Flow::Continue
            }
            // The list is only on screen in the full layout; ignore its keys
            // otherwise so nothing invisible gets played or edited.
            _ if app.layout() != LayoutMode::Full => Flow::Continue,
            UiCommand::Move(movement) => {
                app.move_selection(movement);
                Flow::Continue
            }
            UiCommand::PlaySelected => {
                self.play_selected(app, now);
                Flow::Continue
            }
            UiCommand::StartSearch => {
                app.start_search();
                Flow::Continue
            }
            UiCommand::SearchInput(c) => {
                app.push_query(c);
                Flow::Continue
            }
            UiCommand::SearchBackspace => {
                app.pop_query();
                Flow::Continue
            }
            UiCommand::SearchAccept => {
                app.accept_search();
                Flow::Continue
            }
            UiCommand::SearchCancel => {
                app.cancel_search();
                Flow::Continue
            }
        }
    }

    /// Applies every pending MPRIS command.
    pub fn poll_mpris(&mut self, app: &mut App, now: Instant) -> Flow {
        loop {
            let cmd = match self.mpris_rx.as_ref().map(|rx| rx.try_recv()) {
                Some(Ok(cmd)) => cmd,
                _ => return Flow::Continue,
            };
            if self.apply(Action::from(cmd), app, now) == Flow::Quit {
                return Flow::Quit;
            }
        }
    }

    /// Refreshes the app state from the player and playlist.
    pub fn sync(&self, app: &mut App) {
        app.elapsed = self.player.elapsed_seconds();
        app.volume = self.player.volume();
        app.paused = self.player.is_paused();
        app.mpris_connected = self.mpris.is_some_and(|m| m.is_running());
        app.favorites = self.favorites.list().into_iter().cloned().collect();
        if let Some(playlist) = &self.playlist {
            app.shuffle = playlist.is_shuffled();
            if let Some(ep) = &app.episode {
                app.favorite = self.favorites.is_favorite(&ep.title);
            }
        }
    }

    fn apply(&mut self, action: Action, app: &mut App, now: Instant) -> Flow {
        let Some(playlist) = self.playlist.as_mut() else {
            // Nothing is playing yet (feed loading or failed): only quitting works.
            return if action == Action::Quit {
                Flow::Quit
            } else {
                Flow::Continue
            };
        };
        let Some(title) = app.episode.as_ref().map(|e| e.title.clone()) else {
            return if action == Action::Quit {
                Flow::Quit
            } else {
                Flow::Continue
            };
        };
        match playback::apply(
            action,
            self.player,
            playlist,
            &mut self.favorites,
            self.mpris,
            &title,
        ) {
            Outcome::Continue(message) => {
                if let Some(message) = message {
                    app.set_status(message, now);
                }
                self.sync(app);
                Flow::Continue
            }
            Outcome::NextEpisode | Outcome::PreviousEpisode => {
                self.start_current(app, now);
                Flow::Continue
            }
            Outcome::Quit => Flow::Quit,
        }
    }

    /// Plays the episode selected in the list.
    ///
    /// The list shows the playlist's own episodes (all feed episodes, or only
    /// favorites in favorites mode), so the selection maps 1:1 onto
    /// `Playlist::jump_to`, which also handles shuffle. It then takes the same
    /// path as next/back: stop, start, announce to MPRIS, bump the generation.
    fn play_selected(&mut self, app: &mut App, now: Instant) {
        let Some(position) = app.selected_episode() else {
            return;
        };
        let Some(playlist) = self.playlist.as_mut() else {
            return;
        };
        if playlist.jump_to(position).is_none() {
            return;
        }
        self.player.stop();
        self.start_current(app, now);
    }

    /// Starts the playlist's current episode without blocking.
    fn start_current(&mut self, app: &mut App, now: Instant) {
        let Some(playlist) = self.playlist.as_ref() else {
            return;
        };
        let Some(ep) = playlist.current() else {
            app.activity = Activity::Idle;
            app.set_error("No hay episodios disponibles", now);
            return;
        };
        let (title, duration, url) = (ep.title.clone(), ep.duration.clone(), ep.audio_url.clone());
        let total_seconds = player::parse_duration(&duration).unwrap_or(0);

        if let Some(m) = self.mpris {
            for error in
                playback::announce_episode(m, &title, total_seconds, playlist.is_shuffled())
            {
                log(&error);
            }
        }

        let favorite = self.favorites.is_favorite(&title);
        app.playing = playlist.current_position();
        app.select_playing();
        app.begin_episode(
            EpisodeView {
                title,
                duration,
                total_seconds,
            },
            favorite,
            playlist.is_shuffled(),
        );

        self.generation += 1;
        let generation = self.generation;
        let tx = self.tx.clone();
        let started = self.player.start(&url, move |stage| {
            let _ = tx.send(AppMsg::Stage(generation, stage));
        });
        if let Err(e) = started {
            log(&format!("player start failed: {:#}", e));
            app.activity = Activity::Idle;
            app.set_error(format!("Error: {:#}", e), now);
        }
        self.sync(app);
    }

    fn show_info(&self, app: &mut App, now: Instant) {
        let text = format!(
            "Status: {} | Volume: {} | Shuffle: {} | Favorite: {}",
            if self.player.is_paused() {
                "Paused"
            } else {
                "Playing"
            },
            super::widgets::volume_percent(self.player.volume()),
            if self.playlist.as_ref().is_some_and(|p| p.is_shuffled()) {
                "ON"
            } else {
                "OFF"
            },
            if app.favorite { "Yes" } else { "No" },
        );
        app.set_status(text, now);
    }

    fn start_download(&mut self, app: &mut App, now: Instant) {
        if self.downloading {
            app.set_status("Download already in progress", now);
            return;
        }
        let Some(ep) = self.playlist.as_ref().and_then(|p| p.current()) else {
            return;
        };
        let (title, url) = (ep.title.clone(), ep.audio_url.clone());
        self.downloading = true;
        app.download_bytes = None;
        app.download = Some(download_text(&DownloadEvent::Started {
            title: title.clone(),
        }));
        let tx = self.tx.clone();
        thread::spawn(move || {
            let result = Downloader::new().and_then(|d| {
                d.download_episode(&title, &url, |event| {
                    let _ = tx.send(AppMsg::Download(event));
                })
            });
            let result = result.map(|_| ()).map_err(|e| format!("{:#}", e));
            let _ = tx.send(AppMsg::DownloadDone(result));
        });
    }
}
