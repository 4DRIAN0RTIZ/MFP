use crate::mpris::MprisController;
use crate::operations::downloads::Downloader;
use crate::operations::favorites::Favorites;
use crate::operations::feed::Feed;
use crate::operations::playback::{self, Action, Outcome};
use crate::operations::playlist::Playlist;
use crate::player::{self, Player, PlayerStage};
use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind},
    terminal::{disable_raw_mode, enable_raw_mode},
};
use std::io::{self, Write};
use std::time::Duration;

/// Runs the full-screen TUI player (opt-in via `mfp play --tui`).
pub(super) fn play_tui(
    episode_num: Option<usize>,
    shuffle: bool,
    fav_mode: bool,
    compact: bool,
) -> Result<()> {
    let favorites = Favorites::load()?;
    if fav_mode && favorites.list().is_empty() {
        println!("No tienes favoritos guardados. Usa 'mfp fav --add \"Episode XX: Title\"'");
        return Ok(());
    }
    crate::tui::run(
        crate::tui::PlayOptions {
            episode: episode_num,
            shuffle,
            favorites_only: fav_mode,
            compact,
        },
        favorites,
    )
}

/// Runs the terminal radio player (playlist loop, keyboard and MPRIS control).
pub(super) fn play_radio(episode_num: Option<usize>, shuffle: bool, fav_mode: bool) -> Result<()> {
    println!("Cargando feed...");
    let feed = Feed::fetch()?;
    let mut favorites = Favorites::load()?;

    if fav_mode && favorites.list().is_empty() {
        println!("No tienes favoritos guardados. Usa 'mfp fav --add \"Episode XX: Title\"'");
        return Ok(());
    }
    let fav_list = favorites.list();
    let mut playlist = Playlist::for_session(
        feed.episodes(),
        fav_mode.then_some(fav_list.as_slice()),
        shuffle,
        episode_num,
    );

    let player = Player::new()?;

    // MPRIS integration
    // MPRIS is best-effort: a missing bus must never stop playback.
    let mpris = match MprisController::new() {
        Ok(m) => Some(m),
        Err(e) => {
            crate::logging::log(&format!("MPRIS unavailable, continuing without it: {}", e));
            None
        }
    };
    let mpris_cmd_rx = mpris.as_ref().map(|m| m.command_receiver());

    loop {
        let (episode_title, episode_duration, episode_url) = match playlist.current() {
            Some(ep) => (ep.title.clone(), ep.duration.clone(), ep.audio_url.clone()),
            None => {
                println!("No hay episodios disponibles");
                break;
            }
        };

        // Update MPRIS metadata for new episode
        let total_seconds = player::parse_duration(&episode_duration).unwrap_or(0);
        if let Some(m) = mpris.as_ref() {
            for error in
                playback::announce_episode(m, &episode_title, total_seconds, playlist.is_shuffled())
            {
                eprintln!("{}", error);
            }
        }

        let is_fav = favorites.is_favorite(&episode_title);
        println!("\n{} {}", if is_fav { "*" } else { ">" }, episode_title);
        println!(
            "Duración: {} | Shuffle: {}\n",
            episode_duration,
            if playlist.is_shuffled() { "ON" } else { "OFF" }
        );

        player.play(&episode_url, |stage| match stage {
            PlayerStage::Connecting => {
                print!("Connecting...");
                std::io::stdout().flush().ok();
            }
            PlayerStage::Buffering => {
                print!(" buffering...");
                std::io::stdout().flush().ok();
            }
            PlayerStage::Ready => println!(" OK\n"),
        })?;

        println!("Controles:");
        println!("  [n]ext | [b]ack | [p]ausa | [s]huffle | [f]avorite | [q]uit");
        println!("  [+/-] volumen | [m]ute | [i]nfo | [d]ownload");

        let downloader = Downloader::new()?;
        let total_seconds = player::parse_duration(&episode_duration).unwrap_or(0);

        enable_raw_mode()?;

        let mut command_buffer = String::new();

        loop {
            // Process MPRIS commands
            if let Some(cmd) = mpris_cmd_rx.as_ref().and_then(|rx| rx.try_recv().ok()) {
                match playback::apply(
                    Action::from(cmd),
                    &player,
                    &mut playlist,
                    &mut favorites,
                    mpris.as_ref(),
                    &episode_title,
                ) {
                    Outcome::Continue(_) => {}
                    // exit inner loop to play the new episode
                    Outcome::NextEpisode | Outcome::PreviousEpisode => break,
                    Outcome::Quit => {
                        disable_raw_mode()?;
                        return Ok(());
                    }
                }
            }
            let elapsed = player.elapsed_seconds();
            let remaining = total_seconds.saturating_sub(elapsed);

            let elapsed_str = player::format_duration(elapsed);
            let total_str = player::format_duration(total_seconds);
            let remaining_str = player::format_duration(remaining);

            let percent = if total_seconds > 0 {
                (elapsed as f32 / total_seconds as f32 * 100.0) as u8
            } else {
                0
            };

            let bar_length = 40;
            let filled = ((percent as usize * bar_length) / 100).min(bar_length);
            let bar: String = "━".repeat(filled) + &"─".repeat(bar_length - filled);

            print!(
                "\r[{}/{}] {} {}% | -{} > {}",
                elapsed_str, total_str, bar, percent, remaining_str, command_buffer
            );
            io::stdout().flush()?;

            if event::poll(Duration::from_millis(100))? {
                if let Event::Key(KeyEvent {
                    code,
                    kind: KeyEventKind::Press,
                    ..
                }) = event::read()?
                {
                    match code {
                        KeyCode::Enter => {
                            let command = command_buffer.trim().to_string();
                            command_buffer.clear();

                            disable_raw_mode()?;

                            let action = match command.as_str() {
                                "n" | "next" => Some(Action::Next),
                                "b" | "back" | "prev" | "previous" => Some(Action::Previous),
                                "p" | "pause" | "play" => Some(Action::PlayPause),
                                "+" | "up" => Some(Action::VolumeUp),
                                "-" | "down" => Some(Action::VolumeDown),
                                "m" | "mute" => Some(Action::ToggleMute),
                                "s" | "shuffle" => Some(Action::ToggleShuffle),
                                "f" | "fav" | "favorite" => Some(Action::ToggleFavorite),
                                "q" | "quit" | "exit" => Some(Action::Quit),
                                _ => None,
                            };

                            let should_break = if let Some(action) = action {
                                print!("\r{}\r", " ".repeat(120));
                                match playback::apply(
                                    action,
                                    &player,
                                    &mut playlist,
                                    &mut favorites,
                                    mpris.as_ref(),
                                    &episode_title,
                                ) {
                                    Outcome::Continue(msg) => {
                                        if let Some(msg) = msg {
                                            println!("{}", msg);
                                        }
                                        false
                                    }
                                    Outcome::NextEpisode | Outcome::PreviousEpisode => true,
                                    Outcome::Quit => {
                                        disable_raw_mode()?;
                                        return Ok(());
                                    }
                                }
                            } else {
                                match command.as_str() {
                                    "i" | "info" => {
                                        print!("\r{}\r", " ".repeat(120));
                                        println!("\nEpisode: {}", episode_title);
                                        println!("Duration: {}", episode_duration);
                                        println!("Volume: {:.0}%", player.volume() * 100.0);
                                        println!(
                                            "Status: {}",
                                            if player.is_paused() {
                                                "Paused"
                                            } else {
                                                "Playing"
                                            }
                                        );
                                        println!(
                                            "Shuffle: {}",
                                            if playlist.is_shuffled() { "ON" } else { "OFF" }
                                        );
                                        println!(
                                            "Favorite: {}\n",
                                            if favorites.is_favorite(&episode_title) {
                                                "Yes"
                                            } else {
                                                "No"
                                            }
                                        );
                                        false
                                    }
                                    "d" | "download" => {
                                        print!("\r{}\r", " ".repeat(120));
                                        println!("\nDownloading episode for offline...");
                                        match downloader.download_episode(
                                            &episode_title,
                                            &episode_url,
                                            super::download::render_download_event,
                                        ) {
                                            Ok(_) => println!("Episode downloaded\n"),
                                            Err(e) => println!("Error: {}\n", e),
                                        }
                                        false
                                    }
                                    "" => false,
                                    _ => {
                                        print!("\r{}\r", " ".repeat(120));
                                        println!("Unknown command");
                                        println!("Use: n (next) | b (back) | p (pause) | +/- (vol) | m (mute) | s (shuffle) | f (fav) | i (info) | d (download) | q (quit)");
                                        false
                                    }
                                }
                            };

                            enable_raw_mode()?;

                            if should_break {
                                disable_raw_mode()?;
                                break;
                            }
                        }
                        KeyCode::Backspace => {
                            command_buffer.pop();
                        }
                        KeyCode::Char(c) => {
                            command_buffer.push(c);
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    Ok(())
}
