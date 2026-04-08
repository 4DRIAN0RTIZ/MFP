//! MPRIS D-Bus interface implementation for MFP
//! Runs in a separate async thread to avoid blocking the main sync code

use anyhow::{anyhow, Result};
use async_channel::{Receiver, Sender};
use mpris_server::{Metadata, Player, Time, Volume, PlaybackStatus as MprisPlaybackStatus};
use std::sync::Arc;
use std::thread::JoinHandle;

use crate::player;
use futures::future::{select, Either, FutureExt};

/// Playback status for MFP (simplified version of MPRIS status)
#[derive(Debug, Clone, Copy)]
pub enum PlaybackStatus {
    Playing,
    Paused,
    Stopped,
}

/// Commands sent from MPRIS callbacks to main thread
#[derive(Debug)]
pub enum MprisCommand {
    PlayPause,
    Next,
    Previous,
    SetVolume(f32),
    Quit,
}

/// State updates sent from main thread to MPRIS thread
#[derive(Debug)]
pub enum MprisStateUpdate {
    Metadata {
        title: String,
        duration_secs: u64,
    },
    PlaybackStatus(PlaybackStatus),
    Volume(f32),
    CanGoNext(bool),
    CanGoPrevious(bool),
    Shuffle(bool),
}

/// Controller for MPRIS integration
/// Spawns an async thread that runs the MPRIS server
pub struct MprisController {
    /// Channel to send state updates to MPRIS thread
    state_tx: Option<Sender<MprisStateUpdate>>,
    /// Channel to receive commands from MPRIS callbacks
    callback_rx: Receiver<MprisCommand>,
    /// Channel to send stop signal to MPRIS thread
    stop_tx: Option<Sender<()>>,
    /// Handle to the async thread
    thread_handle: Option<JoinHandle<()>>,
}

impl MprisController {
    /// Create a new MPRIS controller and spawn the async thread
    pub fn new() -> Result<Self> {
        let (state_tx, state_rx) = async_channel::unbounded();
        let (callback_tx, callback_rx) = async_channel::unbounded();
        let (stop_tx, stop_rx) = async_channel::unbounded();

        let thread_handle = std::thread::spawn(move || {
            // Run async runtime in this thread
            if let Err(e) = async_std::task::block_on(Self::run_async_mpris(state_rx, callback_tx, stop_rx)) {
                eprintln!("MPRIS thread error: {}", e);
            }
        });

        Ok(Self {
            state_tx: Some(state_tx),
            callback_rx,
            stop_tx: Some(stop_tx),
            thread_handle: Some(thread_handle),
        })
    }

    /// Get receiver for commands from MPRIS callbacks
    pub fn command_receiver(&self) -> Receiver<MprisCommand> {
        self.callback_rx.clone()
    }

    /// Update metadata (title and duration)
    pub fn update_metadata(&self, title: String, duration_secs: u64) -> Result<()> {
        self.state_tx
            .as_ref()
            .unwrap()
            .send_blocking(MprisStateUpdate::Metadata {
                title,
                duration_secs,
            })
            .map_err(|e| anyhow!("Failed to send metadata update: {}", e))
    }

    /// Update playback status
    pub fn update_playback_status(&self, status: PlaybackStatus) -> Result<()> {
        self.state_tx
            .as_ref()
            .unwrap()
            .send_blocking(MprisStateUpdate::PlaybackStatus(status))
            .map_err(|e| anyhow!("Failed to send playback status: {}", e))
    }

    /// Update volume (0.0 to 1.0)
    pub fn update_volume(&self, volume: f32) -> Result<()> {
        self.state_tx
            .as_ref()
            .unwrap()
            .send_blocking(MprisStateUpdate::Volume(volume))
            .map_err(|e| anyhow!("Failed to send volume update: {}", e))
    }

    /// Update shuffle status
    pub fn update_shuffle(&self, shuffle: bool) -> Result<()> {
        self.state_tx
            .as_ref()
            .unwrap()
            .send_blocking(MprisStateUpdate::Shuffle(shuffle))
            .map_err(|e| anyhow!("Failed to send shuffle update: {}", e))
    }

    /// Update navigation capabilities
    pub fn update_navigation(&self, can_go_next: bool, can_go_previous: bool) -> Result<()> {
        self.state_tx
            .as_ref()
            .unwrap()
            .send_blocking(MprisStateUpdate::CanGoNext(can_go_next))?;
        self.state_tx
            .as_ref()
            .unwrap()
            .send_blocking(MprisStateUpdate::CanGoPrevious(can_go_previous))
            .map_err(|e| anyhow!("Failed to send navigation update: {}", e))
    }

    /// Async function that runs the MPRIS server
    async fn run_async_mpris(
        state_rx: Receiver<MprisStateUpdate>,
        callback_tx: Sender<MprisCommand>,
        stop_rx: Receiver<()>,
    ) -> Result<()> {
        // Create MPRIS player
        let player = Arc::new(
            Player::builder("org.mpris.MediaPlayer2.mfp")
                .identity("MFP")
                .can_play(true)
                .can_pause(true)
                .can_go_next(true)
                .can_go_previous(true)
                .can_seek(false)
                .can_control(true)
                .build()
                .await
                .map_err(|e| anyhow!("Failed to create MPRIS player: {}", e))?
        );

        // Connect callbacks
        let callback_tx_clone = callback_tx.clone();
        player.connect_play_pause(move |_player| {
            let _ = callback_tx_clone.send_blocking(MprisCommand::PlayPause);
        });

        let callback_tx_clone = callback_tx.clone();
        player.connect_next(move |_player| {
            let _ = callback_tx_clone.send_blocking(MprisCommand::Next);
        });

        let callback_tx_clone = callback_tx.clone();
        player.connect_previous(move |_player| {
            let _ = callback_tx_clone.send_blocking(MprisCommand::Previous);
        });

        let callback_tx_clone = callback_tx.clone();
        player.connect_set_volume(move |_player, volume| {
            let _ = callback_tx_clone.send_blocking(MprisCommand::SetVolume(volume as f32));
        });

        // Combine state updates and stop signal into a single event loop
        let player_clone = Arc::clone(&player);
        let event_loop = async move {
            loop {
                let state_fut = state_rx.recv().boxed_local();
                let stop_fut = stop_rx.recv().boxed_local();
                match select(state_fut, stop_fut).await {
                    futures::future::Either::Left((Ok(update), _)) => {
                        if let Err(e) = handle_state_update(&player_clone, update).await {
                            eprintln!("Error handling state update: {}", e);
                        }
                        continue;
                    }
                    futures::future::Either::Left((Err(_), _)) => {
                        // state channel closed
                        break;
                    }
                    futures::future::Either::Right((Ok(_), _)) | futures::future::Either::Right((Err(_), _)) => {
                        // stop signal received or stop channel closed
                        break;
                    }
                }
            }
        };

        // Box the futures to make them Unpin (use boxed_local since we stay on same thread)
        let player_run = player.run().boxed_local();
        let event_loop = event_loop.boxed_local();

        // Run player event loop with combined event handler
        match select(player_run, event_loop).await {
            futures::future::Either::Left((run_result, _)) => {
                // player.run() completed (maybe error)
                run_result;
            }
            futures::future::Either::Right((_, _)) => {
                // event loop terminated (stop signal or state channel closed)
                // Exit cleanly
            }
        }

        Ok(())
    }
}

/// Handle state updates from main thread
async fn handle_state_update(player: &Player, update: MprisStateUpdate) -> Result<()> {
    match update {
        MprisStateUpdate::Metadata { title, duration_secs } => {
            let metadata = Metadata::builder()
                .title(title)
                .artist(["Music For Programming"])
                .album("")
                .length(Time::from_micros(seconds_to_micros(duration_secs)))
                .build();
            player.set_metadata(metadata).await?;
        }
        MprisStateUpdate::PlaybackStatus(status) => {
            let mpris_status = match status {
                PlaybackStatus::Playing => MprisPlaybackStatus::Playing,
                PlaybackStatus::Paused => MprisPlaybackStatus::Paused,
                PlaybackStatus::Stopped => MprisPlaybackStatus::Stopped,
            };
            player.set_playback_status(mpris_status).await?;
        }
        MprisStateUpdate::Volume(volume) => {
            player.set_volume(Volume::from(f64::from(volume))).await?;
        }
        MprisStateUpdate::Shuffle(_shuffle) => {
            // MPRIS doesn't have direct shuffle property, we can set as metadata or ignore
            // For now, we'll store it as a custom property or just note it
            // Could also use xesam:autoRating or something similar
            // We'll skip for now
        }
        MprisStateUpdate::CanGoNext(can) => {
            player.set_can_go_next(can).await?;
        }
        MprisStateUpdate::CanGoPrevious(can) => {
            player.set_can_go_previous(can).await?;
        }
    }
    Ok(())
}

impl Drop for MprisController {
    fn drop(&mut self) {
        // Send stop signal to async thread
        if let Some(stop_tx) = self.stop_tx.take() {
            let _ = stop_tx.send_blocking(());
        }
        // Close the state channel by dropping the sender
        drop(self.state_tx.take());
        
        // Wait for thread to finish
        if let Some(handle) = self.thread_handle.take() {
            let _ = handle.join();
        }
    }
}

/// Convert duration in seconds to microseconds (MPRIS uses µs)
pub fn seconds_to_micros(seconds: u64) -> i64 {
    (seconds as i64) * 1_000_000
}

/// Parse duration string (MM:SS or HH:MM:SS) to microseconds
pub fn parse_duration_to_micros(duration_str: &str) -> Option<i64> {
    player::parse_duration(duration_str).map(seconds_to_micros)
}