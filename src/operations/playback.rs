//! Playback control shared by every front end (CLI keyboard, MPRIS, future TUI).
//!
//! A front end translates its input into an [`Action`] and calls [`apply`],
//! which performs the state change on the player/playlist/favorites and pushes
//! the matching MPRIS updates in one place. MPRIS is best-effort: it is passed
//! as an `Option` and update errors are ignored.

use crate::mpris::{MprisCommand, MprisController, PlaybackStatus};
use crate::operations::favorites::Favorites;
use crate::operations::playlist::Playlist;
use crate::player::Player;

/// Maximum player volume (200%).
const MAX_VOLUME: f32 = 2.0;
/// Volume change per step.
const VOLUME_STEP: f32 = 0.1;
/// Volume restored when unmuting.
const UNMUTED_VOLUME: f32 = 1.0;

/// A playback action requested by any front end.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Action {
    /// Toggle between paused and playing.
    PlayPause,
    /// Stop and move to the next episode.
    Next,
    /// Stop and move to the previous episode.
    Previous,
    /// Increase volume by one step (clamped to 200%).
    VolumeUp,
    /// Decrease volume by one step (clamped to 0%).
    VolumeDown,
    /// Set an absolute volume, as requested by MPRIS (not clamped).
    SetVolume(f32),
    /// Mute, or restore 100% when already at 0.
    ToggleMute,
    /// Toggle playlist shuffle.
    ToggleShuffle,
    /// Add or remove the current episode from favorites.
    ToggleFavorite,
    /// Stop playback and leave the player.
    Quit,
}

impl From<MprisCommand> for Action {
    fn from(cmd: MprisCommand) -> Self {
        match cmd {
            MprisCommand::PlayPause => Action::PlayPause,
            MprisCommand::Next => Action::Next,
            MprisCommand::Previous => Action::Previous,
            MprisCommand::SetVolume(v) => Action::SetVolume(v),
            MprisCommand::Quit => Action::Quit,
        }
    }
}

/// What the caller must do after an action was applied.
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    /// Keep playing the current episode; optional text to show the user.
    Continue(Option<String>),
    /// The playlist advanced; play the (new) current episode.
    NextEpisode,
    /// The playlist went back; play the (new) current episode.
    PreviousEpisode,
    /// Leave the player.
    Quit,
}

/// Volume after one step up, clamped to the maximum.
pub fn volume_up(current: f32) -> f32 {
    (current + VOLUME_STEP).min(MAX_VOLUME)
}

/// Volume after one step down, clamped to zero.
pub fn volume_down(current: f32) -> f32 {
    (current - VOLUME_STEP).max(0.0)
}

/// Volume after a mute toggle: any audible volume mutes, silence restores 100%.
pub fn mute_target(current: f32) -> f32 {
    if current > 0.0 {
        0.0
    } else {
        UNMUTED_VOLUME
    }
}

/// Message for a volume change, e.g. `Volume: 110%`.
pub fn volume_message(volume: f32) -> String {
    format!("Volume: {:.0}%", volume * 100.0)
}

/// Message for a mute toggle given the volume before the toggle.
pub fn mute_message(previous_volume: f32) -> String {
    if previous_volume > 0.0 {
        "Muted".to_string()
    } else {
        "Volume: 100%".to_string()
    }
}

/// Message for a shuffle state.
pub fn shuffle_message(on: bool) -> String {
    format!("Shuffle: {}", if on { "ON" } else { "OFF" })
}

/// Message for a favorite toggle given the resulting state.
pub fn favorite_message(is_now_favorite: bool) -> &'static str {
    if is_now_favorite {
        "Added to favorites"
    } else {
        "Removed from favorites"
    }
}

/// Status pushed to MPRIS after a pause toggle, given the state before it.
fn status_after_toggle(was_paused: bool) -> PlaybackStatus {
    if was_paused {
        PlaybackStatus::Playing
    } else {
        PlaybackStatus::Paused
    }
}

/// Applies `action`, updating MPRIS (if present) and returning what to do next.
///
/// `episode_title` is the current episode, used for the favorite toggle.
pub fn apply(
    action: Action,
    player: &Player,
    playlist: &mut Playlist,
    favorites: &mut Favorites,
    mpris: Option<&MprisController>,
    episode_title: &str,
) -> Outcome {
    match action {
        Action::PlayPause => {
            let was_paused = player.is_paused();
            if was_paused {
                player.resume();
            } else {
                player.pause();
            }
            if let Some(m) = mpris {
                let _ = m.update_playback_status(status_after_toggle(was_paused));
            }
            Outcome::Continue(Some(
                if was_paused { "Playing" } else { "Paused" }.to_string(),
            ))
        }
        Action::Next => {
            player.stop();
            playlist.next();
            Outcome::NextEpisode
        }
        Action::Previous => {
            player.stop();
            playlist.previous();
            Outcome::PreviousEpisode
        }
        Action::VolumeUp => {
            let new_vol = volume_up(player.volume());
            set_volume(player, mpris, new_vol);
            Outcome::Continue(Some(volume_message(new_vol)))
        }
        Action::VolumeDown => {
            let new_vol = volume_down(player.volume());
            set_volume(player, mpris, new_vol);
            Outcome::Continue(Some(volume_message(new_vol)))
        }
        Action::SetVolume(vol) => {
            set_volume(player, mpris, vol);
            Outcome::Continue(None)
        }
        Action::ToggleMute => {
            let current = player.volume();
            set_volume(player, mpris, mute_target(current));
            Outcome::Continue(Some(mute_message(current)))
        }
        Action::ToggleShuffle => {
            playlist.toggle_shuffle();
            let on = playlist.is_shuffled();
            if let Some(m) = mpris {
                let _ = m.update_shuffle(on);
            }
            Outcome::Continue(Some(shuffle_message(on)))
        }
        Action::ToggleFavorite => {
            let now_fav = favorites.toggle(episode_title.to_string());
            Outcome::Continue(Some(favorite_message(now_fav).to_string()))
        }
        Action::Quit => {
            player.stop();
            Outcome::Quit
        }
    }
}

/// Pushes the state of a newly started episode to MPRIS (metadata, status,
/// shuffle, navigation).
///
/// Every step is attempted; the returned messages describe the steps that
/// failed (empty on success) so each front end decides how to report them.
pub fn announce_episode(
    mpris: &MprisController,
    title: &str,
    total_seconds: u64,
    shuffled: bool,
) -> Vec<String> {
    let mut errors = Vec::new();
    if let Err(e) = mpris.update_metadata(title.to_string(), total_seconds) {
        errors.push(format!("Failed to update MPRIS metadata: {}", e));
    }
    if let Err(e) = mpris.update_playback_status(PlaybackStatus::Playing) {
        errors.push(format!("Failed to update MPRIS playback status: {}", e));
    }
    if let Err(e) = mpris.update_shuffle(shuffled) {
        errors.push(format!("Failed to update MPRIS shuffle: {}", e));
    }
    if let Err(e) = mpris.update_navigation(true, true) {
        errors.push(format!("Failed to update MPRIS navigation: {}", e));
    }
    errors
}

fn set_volume(player: &Player, mpris: Option<&MprisController>, volume: f32) {
    player.set_volume(volume);
    if let Some(m) = mpris {
        let _ = m.update_volume(volume);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn volume_up_clamps_at_max() {
        assert!((volume_up(1.0) - 1.1).abs() < 1e-6);
        assert_eq!(volume_up(1.95), 2.0);
        assert_eq!(volume_up(2.0), 2.0);
    }

    #[test]
    fn volume_down_clamps_at_zero() {
        assert!((volume_down(1.0) - 0.9).abs() < 1e-6);
        assert_eq!(volume_down(0.05), 0.0);
        assert_eq!(volume_down(0.0), 0.0);
    }

    #[test]
    fn mute_toggles_between_zero_and_full() {
        assert_eq!(mute_target(0.5), 0.0);
        assert_eq!(mute_target(2.0), 0.0);
        // Quirk: unmuting always restores 100%, not the previous volume.
        assert_eq!(mute_target(0.0), 1.0);
    }

    #[test]
    fn messages_match_legacy_text() {
        assert_eq!(volume_message(1.1), "Volume: 110%");
        assert_eq!(volume_message(0.0), "Volume: 0%");
        assert_eq!(mute_message(0.3), "Muted");
        assert_eq!(mute_message(0.0), "Volume: 100%");
        assert_eq!(shuffle_message(true), "Shuffle: ON");
        assert_eq!(shuffle_message(false), "Shuffle: OFF");
        assert_eq!(favorite_message(true), "Added to favorites");
        assert_eq!(favorite_message(false), "Removed from favorites");
    }

    #[test]
    fn pause_toggle_status() {
        assert!(matches!(status_after_toggle(true), PlaybackStatus::Playing));
        assert!(matches!(status_after_toggle(false), PlaybackStatus::Paused));
    }

    #[test]
    fn mpris_commands_map_to_actions() {
        assert_eq!(Action::from(MprisCommand::PlayPause), Action::PlayPause);
        assert_eq!(Action::from(MprisCommand::Next), Action::Next);
        assert_eq!(Action::from(MprisCommand::Previous), Action::Previous);
        assert_eq!(
            Action::from(MprisCommand::SetVolume(1.5)),
            Action::SetVolume(1.5)
        );
        assert_eq!(Action::from(MprisCommand::Quit), Action::Quit);
    }
}
