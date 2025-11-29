mod downloader;
mod favorites;
mod feed;
mod player;
mod playlist;

use anyhow::Result;
use clap::{Parser, Subcommand};
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent},
    terminal::{disable_raw_mode, enable_raw_mode},
};
use downloader::Downloader;
use favorites::Favorites;
use feed::Feed;
use player::Player;
use playlist::Playlist;
use std::io::{self, Write};
use std::time::Duration;

#[derive(Parser)]
#[command(name = "mfp")]
#[command(about = "Music For Programming - Radio player ligero", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// List all available episodes
    List,
    /// Play a specific episode
    Play {
        /// Episode number (e.g. 75)
        #[arg(short, long)]
        episode: Option<usize>,
        /// Enable shuffle mode
        #[arg(short, long)]
        shuffle: bool,
        /// Play only favorites
        #[arg(short, long)]
        favorites: bool,
    },
    /// Manage favorites
    Fav {
        /// Add episode to favorites
        #[arg(short, long)]
        add: Option<String>,
        /// Remove episode from favorites
        #[arg(short, long)]
        remove: Option<String>,
        /// List favorites
        #[arg(short, long)]
        list: bool,
    },
    /// Manage offline downloads
    Download {
        /// Download episode by number
        #[arg(short, long)]
        episode: Option<usize>,
        /// List downloaded episodes
        #[arg(short, long)]
        list: bool,
        /// Delete downloaded episode
        #[arg(short = 'd', long)]
        delete: Option<String>,
        /// Show disk usage
        #[arg(short = 's', long)]
        size: bool,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::List) => list_episodes()?,
        Some(Commands::Play { episode, shuffle, favorites: fav_mode }) => {
            play_radio(episode, shuffle, fav_mode)?
        }
        Some(Commands::Fav { add, remove, list }) => manage_favorites(add, remove, list)?,
        Some(Commands::Download { episode, list, delete, size }) => {
            manage_downloads(episode, list, delete, size)?
        }
        None => interactive_mode()?,
    }

    Ok(())
}

fn list_episodes() -> Result<()> {
    println!("Obteniendo episodios...");
    let feed = Feed::fetch()?;
    let favorites = Favorites::load()?;

    for (i, episode) in feed.episodes().iter().enumerate() {
        let fav_marker = if favorites.is_favorite(&episode.title) {
            "*"
        } else {
            " "
        };
        println!(
            "{} {:3}. {} [{}]",
            fav_marker,
            extract_episode_number(&episode.title).unwrap_or(i + 1),
            episode.title,
            episode.duration
        );
    }

    Ok(())
}

fn extract_episode_number(title: &str) -> Option<usize> {
    title
        .split(':')
        .next()?
        .trim()
        .strip_prefix("Episode ")?
        .parse()
        .ok()
}

fn play_radio(episode_num: Option<usize>, shuffle: bool, fav_mode: bool) -> Result<()> {
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

    loop {
        let (episode_title, episode_duration, episode_url) = match playlist.current() {
            Some(ep) => (ep.title.clone(), ep.duration.clone(), ep.audio_url.clone()),
            None => {
                println!("No hay episodios disponibles");
                break;
            }
        };

        let is_fav = favorites.is_favorite(&episode_title);
        println!("\n{} {}", if is_fav { "*" } else { ">" }, episode_title);
        println!("Duración: {} | Shuffle: {}\n", episode_duration, if playlist.is_shuffled() { "ON" } else { "OFF" });

        player.play(&episode_url)?;

        println!("Controles:");
        println!("  [n]ext | [b]ack | [p]ausa | [s]huffle | [f]avorite | [q]uit");
        println!("  [+/-] volumen | [m]ute | [i]nfo | [d]ownload");

        let downloader = Downloader::new()?;
        let total_seconds = player::parse_duration(&episode_duration).unwrap_or(0);

        enable_raw_mode()?;

        let mut command_buffer = String::new();

        loop {
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

            print!("\r[{}/{}] {} {}% | -{} > {}",
                elapsed_str, total_str, bar, percent, remaining_str, command_buffer);
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
                                    if player.is_paused() {
                                        player.resume();
                                        println!("Playing");
                                    } else {
                                        player.pause();
                                        println!("Paused");
                                    }
                                    false
                                }
                                "+" | "up" => {
                                    print!("\r{}\r", " ".repeat(120));
                                    let current_vol = player.volume();
                                    let new_vol = (current_vol + 0.1).min(2.0);
                                    player.set_volume(new_vol);
                                    println!("Volume: {:.0}%", new_vol * 100.0);
                                    false
                                }
                                "-" | "down" => {
                                    print!("\r{}\r", " ".repeat(120));
                                    let current_vol = player.volume();
                                    let new_vol = (current_vol - 0.1).max(0.0);
                                    player.set_volume(new_vol);
                                    println!("Volume: {:.0}%", new_vol * 100.0);
                                    false
                                }
                                "m" | "mute" => {
                                    print!("\r{}\r", " ".repeat(120));
                                    let current_vol = player.volume();
                                    if current_vol > 0.0 {
                                        player.set_volume(0.0);
                                        println!("Muted");
                                    } else {
                                        player.set_volume(1.0);
                                        println!("Volume: 100%");
                                    }
                                    false
                                }
                                "i" | "info" => {
                                    print!("\r{}\r", " ".repeat(120));
                                    println!("\nEpisode: {}", episode_title);
                                    println!("Duration: {}", episode_duration);
                                    println!("Volume: {:.0}%", player.volume() * 100.0);
                                    println!("Status: {}", if player.is_paused() { "Paused" } else { "Playing" });
                                    println!("Shuffle: {}", if playlist.is_shuffled() { "ON" } else { "OFF" });
                                    println!("Favorite: {}\n", if favorites.is_favorite(&episode_title) { "Yes" } else { "No" });
                                    false
                                }
                                "s" | "shuffle" => {
                                    print!("\r{}\r", " ".repeat(120));
                                    playlist.toggle_shuffle();
                                    println!("Shuffle: {}", if playlist.is_shuffled() { "ON" } else { "OFF" });
                                    false
                                }
                                "f" | "fav" | "favorite" => {
                                    print!("\r{}\r", " ".repeat(120));
                                    let is_now_fav = favorites.toggle(episode_title.clone());
                                    println!("{}", if is_now_fav { "Added to favorites" } else { "Removed from favorites" });
                                    false
                                }
                                "d" | "download" => {
                                    print!("\r{}\r", " ".repeat(120));
                                    println!("\nDownloading episode for offline...");
                                    match downloader.download_episode(&episode_title, &episode_url) {
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

fn manage_favorites(add: Option<String>, remove: Option<String>, list: bool) -> Result<()> {
    let mut favorites = Favorites::load()?;

    if let Some(title) = add {
        if favorites.add(title.clone()) {
            println!("* Added: {}", title);
        } else {
            println!("Already in favorites: {}", title);
        }
    }

    if let Some(title) = remove {
        if favorites.remove(&title) {
            println!("Removed: {}", title);
        } else {
            println!("Not in favorites: {}", title);
        }
    }

    if list {
        let fav_list = favorites.list();
        if fav_list.is_empty() {
            println!("No favorites saved");
        } else {
            println!("Favorites:");
            for title in fav_list {
                println!("  * {}", title);
            }
        }
    }

    Ok(())
}

fn manage_downloads(episode: Option<usize>, list: bool, delete: Option<String>, size: bool) -> Result<()> {
    let downloader = Downloader::new()?;

    if size {
        let total_bytes = downloader.get_total_size()?;
        let total_mb = total_bytes as f64 / 1_048_576.0;
        println!("Disk usage: {:.2} MB", total_mb);
        println!("Location: {}", downloader.download_dir().display());
        return Ok(());
    }

    if list {
        let downloaded = downloader.list_downloaded()?;
        if downloaded.is_empty() {
            println!("No downloaded episodes");
        } else {
            println!("Downloaded episodes ({}):", downloaded.len());
            for path in downloaded {
                if let Some(filename) = path.file_name() {
                    println!("  - {}", filename.to_string_lossy());
                }
            }
        }
        return Ok(());
    }

    if let Some(title) = delete {
        downloader.delete_episode(&title)?;
        return Ok(());
    }

    if let Some(ep_num) = episode {
        println!("Obteniendo episodio...");
        let feed = Feed::fetch()?;

        let target_title = format!("Episode {}", ep_num);
        if let Some(ep) = feed.episodes().iter().find(|e| e.title.contains(&target_title)) {
            downloader.download_episode(&ep.title, &ep.audio_url)?;
        } else {
            println!("Episode {} not found", ep_num);
        }
        return Ok(());
    }

    println!("Gestión de descargas offline");
    println!("\nUso:");
    println!("  mfp download -e 75        Descargar episodio 75");
    println!("  mfp download --list       Listar descargados");
    println!("  mfp download --size       Mostrar espacio usado");
    println!("  mfp download --delete \"Episode 75\"  Eliminar episodio");

    Ok(())
}

fn interactive_mode() -> Result<()> {
    println!("Music For Programming - Radio Player");
    println!("\nComandos disponibles:");
    println!("  mfp list                    - Lista todos los episodios");
    println!("  mfp play                    - Reproduce desde el inicio");
    println!("  mfp play -e 75              - Reproduce episodio específico");
    println!("  mfp play -s                 - Reproduce en modo shuffle");
    println!("  mfp play -f                 - Reproduce solo favoritos");
    println!("  mfp fav -l                  - Lista favoritos");
    println!("  mfp fav -a \"Episode XX\"     - Agrega a favoritos");
    println!("  mfp fav -r \"Episode XX\"     - Remueve de favoritos");
    println!("  mfp download -e 75          - Descarga episodio para offline");
    println!("  mfp download --list         - Lista episodios descargados");
    println!("\nUsa 'mfp play' para comenzar a escuchar");

    Ok(())
}
