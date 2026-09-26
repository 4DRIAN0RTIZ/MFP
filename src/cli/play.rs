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
use std::io::{self, BufRead, IsTerminal, Write};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread;
use std::time::Duration;

/// How the plain loop reads commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InputMode {
    /// stdin is a terminal: raw mode, keys typed until Enter, live progress line.
    Tty,
    /// stdin is a pipe/file/`/dev/null`: no raw mode and no crossterm reads;
    /// commands arrive as lines read by a background thread.
    Lines,
}

/// Picks the input mode from whether stdin is a terminal (raw mode and
/// crossterm key events need one).
fn input_mode(stdin_is_tty: bool) -> InputMode {
    if stdin_is_tty {
        InputMode::Tty
    } else {
        InputMode::Lines
    }
}

/// Maps a typed command (already trimmed) to a playback action. `i`/`d` and
/// unknown input are handled by the callers.
fn parse_action(command: &str) -> Option<Action> {
    match command {
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
    }
}

const UNKNOWN_COMMAND_HELP: &str = "Use: n (next) | b (back) | p (pause) | +/- (vol) | m (mute) | s (shuffle) | f (fav) | i (info) | d (download) | q (quit)";

/// Reads stdin lines on a background thread. The channel disconnects at EOF
/// (or on a read error); the thread ends with the process.
fn spawn_stdin_lines() -> Receiver<String> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        for line in io::stdin().lock().lines() {
            match line {
                Ok(line) => {
                    if tx.send(line).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });
    rx
}

/// Runs the full-screen TUI player (the default UI of `mfp` and `mfp play`).
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

/// Runs the plain text player (`--plain` or automatic fallback): playlist loop
/// with typed commands followed by Enter, plus MPRIS control.
///
/// With a terminal on stdin it uses raw mode and a live progress line (the
/// progress line is skipped when stdout is not a terminal). Otherwise it runs
/// in line mode, see [`run_line_session`].
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

    let line_rx = match input_mode(io::stdin().is_terminal()) {
        InputMode::Tty => None,
        InputMode::Lines => Some(spawn_stdin_lines()),
    };
    let stdout_is_tty = io::stdout().is_terminal();

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

        if let Some(line_rx) = line_rx.as_ref() {
            let mut session = LineSession {
                line_rx,
                mpris_cmd_rx: mpris_cmd_rx.as_ref(),
                player: &player,
                mpris: mpris.as_ref(),
                downloader: &downloader,
                episode_title: &episode_title,
                episode_duration: &episode_duration,
                episode_url: &episode_url,
            };
            if run_line_session(&mut session, &mut playlist, &mut favorites) {
                return Ok(());
            }
            continue;
        }

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

            if stdout_is_tty {
                print!(
                    "\r[{}/{}] {} {}% | -{} > {}",
                    elapsed_str, total_str, bar, percent, remaining_str, command_buffer
                );
                io::stdout().flush()?;
            }

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

                            let action = parse_action(&command);

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
                                        println!("{}", UNKNOWN_COMMAND_HELP);
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

/// Everything the line mode needs about the episode being played.
struct LineSession<'a> {
    line_rx: &'a Receiver<String>,
    mpris_cmd_rx: Option<&'a async_channel::Receiver<crate::mpris::MprisCommand>>,
    player: &'a Player,
    mpris: Option<&'a MprisController>,
    downloader: &'a Downloader,
    episode_title: &'a str,
    episode_duration: &'a str,
    episode_url: &'a str,
}

/// Line mode: commands are lines read from a non-terminal stdin (same words as
/// the typed commands), with no raw mode and no live progress line, so output
/// stays readable in pipes and logs; each command prints its response line(s).
///
/// EOF rule: a closed stdin is not a quit request. Playback continues and
/// MPRIS commands keep being serviced until MPRIS Quit, `q` (if it arrived
/// before EOF) or a signal such as SIGINT stops the process. The loop sleeps
/// ~100ms per tick, so it never spins.
///
/// Returns `true` when the player should quit, `false` to move to the
/// episode now selected by the playlist.
fn run_line_session(
    session: &mut LineSession<'_>,
    playlist: &mut Playlist,
    favorites: &mut Favorites,
) -> bool {
    let mut stdin_open = true;
    loop {
        if let Some(cmd) = session.mpris_cmd_rx.and_then(|rx| rx.try_recv().ok()) {
            match playback::apply(
                Action::from(cmd),
                session.player,
                playlist,
                favorites,
                session.mpris,
                session.episode_title,
            ) {
                Outcome::Continue(_) => {}
                Outcome::NextEpisode | Outcome::PreviousEpisode => return false,
                Outcome::Quit => return true,
            }
        }

        if stdin_open {
            match session.line_rx.try_recv() {
                Ok(line) => {
                    if let Some(quit) = handle_line(&line, session, playlist, favorites) {
                        return quit;
                    }
                }
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => stdin_open = false,
            }
        }

        thread::sleep(Duration::from_millis(100));
    }
}

/// Handles one command line. `Some(true)` = quit, `Some(false)` = change
/// episode, `None` = keep playing.
fn handle_line(
    line: &str,
    session: &LineSession<'_>,
    playlist: &mut Playlist,
    favorites: &mut Favorites,
) -> Option<bool> {
    let command = line.trim();
    if let Some(action) = parse_action(command) {
        return match playback::apply(
            action,
            session.player,
            playlist,
            favorites,
            session.mpris,
            session.episode_title,
        ) {
            Outcome::Continue(msg) => {
                if let Some(msg) = msg {
                    println!("{}", msg);
                }
                None
            }
            Outcome::NextEpisode | Outcome::PreviousEpisode => Some(false),
            Outcome::Quit => Some(true),
        };
    }
    match command {
        "i" | "info" => {
            println!("\nEpisode: {}", session.episode_title);
            println!("Duration: {}", session.episode_duration);
            println!("Volume: {:.0}%", session.player.volume() * 100.0);
            println!(
                "Status: {}",
                if session.player.is_paused() {
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
                if favorites.is_favorite(session.episode_title) {
                    "Yes"
                } else {
                    "No"
                }
            );
        }
        "d" | "download" => {
            println!("\nDownloading episode for offline...");
            match session.downloader.download_episode(
                session.episode_title,
                session.episode_url,
                super::download::render_download_event,
            ) {
                Ok(_) => println!("Episode downloaded\n"),
                Err(e) => println!("Error: {}\n", e),
            }
        }
        "" => {}
        _ => {
            println!("Unknown command");
            println!("{}", UNKNOWN_COMMAND_HELP);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_mode_follows_stdin_tty() {
        assert_eq!(input_mode(true), InputMode::Tty);
        assert_eq!(input_mode(false), InputMode::Lines);
    }

    #[test]
    fn parse_action_maps_every_alias() {
        let cases = [
            ("n", Action::Next),
            ("next", Action::Next),
            ("b", Action::Previous),
            ("back", Action::Previous),
            ("prev", Action::Previous),
            ("previous", Action::Previous),
            ("p", Action::PlayPause),
            ("pause", Action::PlayPause),
            ("play", Action::PlayPause),
            ("+", Action::VolumeUp),
            ("up", Action::VolumeUp),
            ("-", Action::VolumeDown),
            ("down", Action::VolumeDown),
            ("m", Action::ToggleMute),
            ("mute", Action::ToggleMute),
            ("s", Action::ToggleShuffle),
            ("shuffle", Action::ToggleShuffle),
            ("f", Action::ToggleFavorite),
            ("fav", Action::ToggleFavorite),
            ("favorite", Action::ToggleFavorite),
            ("q", Action::Quit),
            ("quit", Action::Quit),
            ("exit", Action::Quit),
        ];
        for (input, expected) in cases {
            assert!(
                parse_action(input) == Some(expected),
                "input {input:?} mapped wrongly"
            );
        }
    }

    #[test]
    fn parse_action_leaves_info_download_and_unknown_to_callers() {
        for input in ["i", "info", "d", "download", "", "zzz", "N", "n "] {
            assert!(parse_action(input).is_none(), "input {input:?}");
        }
    }
}
