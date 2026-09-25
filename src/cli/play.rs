use crate::downloader::Downloader;
use crate::favorites::Favorites;
use crate::feed::Feed;
use crate::mpris::{MprisCommand, MprisController, PlaybackStatus};
use crate::player::{self, Player};
use crate::playlist::Playlist;
use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent},
    terminal::{disable_raw_mode, enable_raw_mode},
};
use std::io::{self, Write};
use std::time::Duration;

/// Runs the terminal radio player (playlist loop, keyboard and MPRIS control).
pub(super) fn play_radio(episode_num: Option<usize>, shuffle: bool, fav_mode: bool) -> Result<()> {
    println!("Cargando feed...");
    let feed = Feed::fetch()?;
    let mut favorites = Favorites::load()?;

    let mut playlist = if fav_mode {
        let fav_list = favorites.list();
        if fav_list.is_empty() {
            println!("No tienes favoritos guardados. Usa 'mfp fav --add \"Episode XX: Title\"'");
            return Ok(());
        }
        Playlist::from_favorites(feed.episodes(), &fav_list)
    } else {
        Playlist::new(feed.episodes().to_vec())
    };

    if shuffle {
        playlist.enable_shuffle();
    }

    if let Some(num) = episode_num {
        let target_title = format!("Episode {}", num);
        if let Some(pos) = playlist
            .all_episodes()
            .iter()
            .position(|e| e.title.contains(&target_title))
        {
            for _ in 0..pos {
                playlist.next();
            }
        }
    }

    let player = Player::new()?;

    // MPRIS integration
    let mpris = MprisController::new()?;
    let mpris_cmd_rx = mpris.command_receiver();

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
        if let Err(e) = mpris.update_metadata(episode_title.clone(), total_seconds) {
            eprintln!("Failed to update MPRIS metadata: {}", e);
        }
        if let Err(e) = mpris.update_playback_status(PlaybackStatus::Playing) {
            eprintln!("Failed to update MPRIS playback status: {}", e);
        }
        if let Err(e) = mpris.update_shuffle(playlist.is_shuffled()) {
            eprintln!("Failed to update MPRIS shuffle: {}", e);
        }
        if let Err(e) = mpris.update_navigation(true, true) {
            eprintln!("Failed to update MPRIS navigation: {}", e);
        }

        let is_fav = favorites.is_favorite(&episode_title);
        println!("\n{} {}", if is_fav { "*" } else { ">" }, episode_title);
        println!(
            "Duración: {} | Shuffle: {}\n",
            episode_duration,
            if playlist.is_shuffled() { "ON" } else { "OFF" }
        );

        player.play(&episode_url)?;

        println!("Controles:");
        println!("  [n]ext | [b]ack | [p]ausa | [s]huffle | [f]avorite | [q]uit");
        println!("  [+/-] volumen | [m]ute | [i]nfo | [d]ownload");

        let downloader = Downloader::new()?;
        let total_seconds = player::parse_duration(&episode_duration).unwrap_or(0);

        enable_raw_mode()?;

        let mut command_buffer = String::new();

        loop {
            // Process MPRIS commands
            if let Ok(cmd) = mpris_cmd_rx.try_recv() {
                match cmd {
                    MprisCommand::PlayPause => {
                        if player.is_paused() {
                            player.resume();
                            let _ = mpris.update_playback_status(PlaybackStatus::Playing);
                        } else {
                            player.pause();
                            let _ = mpris.update_playback_status(PlaybackStatus::Paused);
                        }
                    }
                    MprisCommand::Next => {
                        player.stop();
                        playlist.next();
                        break; // exit inner loop to play next episode
                    }
                    MprisCommand::Previous => {
                        player.stop();
                        playlist.previous();
                        break;
                    }
                    MprisCommand::SetVolume(vol) => {
                        player.set_volume(vol);
                        let _ = mpris.update_volume(vol);
                    }
                    MprisCommand::Quit => {
                        player.stop();
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
                if let Event::Key(KeyEvent { code, .. }) = event::read()? {
                    match code {
                        KeyCode::Enter => {
                            let command = command_buffer.trim().to_string();
                            command_buffer.clear();

                            disable_raw_mode()?;

                            let should_break = match command.as_str() {
                                "n" | "next" => {
                                    print!("\r{}\r", " ".repeat(120));
                                    player.stop();
                                    playlist.next();
                                    true
                                }
                                "b" | "back" | "prev" | "previous" => {
                                    print!("\r{}\r", " ".repeat(120));
                                    player.stop();
                                    playlist.previous();
                                    true
                                }
                                "p" | "pause" | "play" => {
                                    print!("\r{}\r", " ".repeat(120));
                                    let new_status = if player.is_paused() {
                                        PlaybackStatus::Playing
                                    } else {
                                        PlaybackStatus::Paused
                                    };
                                    if player.is_paused() {
                                        player.resume();
                                        println!("Playing");
                                    } else {
                                        player.pause();
                                        println!("Paused");
                                    }
                                    mpris.update_playback_status(new_status).ok();
                                    false
                                }
                                "+" | "up" => {
                                    print!("\r{}\r", " ".repeat(120));
                                    let current_vol = player.volume();
                                    let new_vol = (current_vol + 0.1).min(2.0);
                                    player.set_volume(new_vol);
                                    mpris.update_volume(new_vol).ok();
                                    println!("Volume: {:.0}%", new_vol * 100.0);
                                    false
                                }
                                "-" | "down" => {
                                    print!("\r{}\r", " ".repeat(120));
                                    let current_vol = player.volume();
                                    let new_vol = (current_vol - 0.1).max(0.0);
                                    player.set_volume(new_vol);
                                    mpris.update_volume(new_vol).ok();
                                    println!("Volume: {:.0}%", new_vol * 100.0);
                                    false
                                }
                                "m" | "mute" => {
                                    print!("\r{}\r", " ".repeat(120));
                                    let current_vol = player.volume();
                                    let new_vol = if current_vol > 0.0 { 0.0 } else { 1.0 };
                                    player.set_volume(new_vol);
                                    mpris.update_volume(new_vol).ok();
                                    if current_vol > 0.0 {
                                        println!("Muted");
                                    } else {
                                        println!("Volume: 100%");
                                    }
                                    false
                                }
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
                                "s" | "shuffle" => {
                                    print!("\r{}\r", " ".repeat(120));
                                    playlist.toggle_shuffle();
                                    mpris.update_shuffle(playlist.is_shuffled()).ok();
                                    println!(
                                        "Shuffle: {}",
                                        if playlist.is_shuffled() { "ON" } else { "OFF" }
                                    );
                                    false
                                }
                                "f" | "fav" | "favorite" => {
                                    print!("\r{}\r", " ".repeat(120));
                                    let is_now_fav = favorites.toggle(episode_title.clone());
                                    println!(
                                        "{}",
                                        if is_now_fav {
                                            "Added to favorites"
                                        } else {
                                            "Removed from favorites"
                                        }
                                    );
                                    false
                                }
                                "d" | "download" => {
                                    print!("\r{}\r", " ".repeat(120));
                                    println!("\nDownloading episode for offline...");
                                    match downloader.download_episode(&episode_title, &episode_url)
                                    {
                                        Ok(_) => println!("Episode downloaded\n"),
                                        Err(e) => println!("Error: {}\n", e),
                                    }
                                    false
                                }
                                "q" | "quit" | "exit" => {
                                    print!("\r{}\r", " ".repeat(120));
                                    player.stop();
                                    disable_raw_mode()?;
                                    return Ok(());
                                }
                                "" => false,
                                _ => {
                                    print!("\r{}\r", " ".repeat(120));
                                    println!("Unknown command");
                                    println!("Use: n (next) | b (back) | p (pause) | +/- (vol) | m (mute) | s (shuffle) | f (fav) | i (info) | d (download) | q (quit)");
                                    false
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
