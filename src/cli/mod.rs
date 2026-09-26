//! Command-line interface: clap definitions and command dispatch.

mod download;
mod fav;
mod interactive;
mod list;
mod play;

use anyhow::Result;
use clap::{Parser, Subcommand};

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
        /// Use the full-screen terminal UI (experimental)
        #[arg(long)]
        tui: bool,
        /// Start the TUI in the compact layout (only with --tui)
        #[arg(long, requires = "tui")]
        compact: bool,
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

/// Parses CLI arguments and dispatches to the matching command handler.
pub fn run() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::List) => list::list_episodes()?,
        Some(Commands::Play {
            episode,
            shuffle,
            favorites: fav_mode,
            tui,
            compact,
        }) => {
            if tui {
                play::play_tui(episode, shuffle, fav_mode, compact)?
            } else {
                play::play_radio(episode, shuffle, fav_mode)?
            }
        }
        Some(Commands::Fav { add, remove, list }) => fav::manage_favorites(add, remove, list)?,
        Some(Commands::Download {
            episode,
            list,
            delete,
            size,
        }) => download::manage_downloads(episode, list, delete, size)?,
        None => interactive::interactive_mode()?,
    }

    Ok(())
}
