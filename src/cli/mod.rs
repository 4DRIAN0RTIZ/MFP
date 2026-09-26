//! Command-line interface: clap definitions and command dispatch.
//!
//! `mfp` and `mfp play` share [`PlayArgs`]. The UI (TUI, plain text loop or
//! usage overview) is picked by [`resolve_ui_mode`] from `--plain` and the
//! terminal environment.

mod download;
mod fav;
mod interactive;
mod list;
mod play;

use anyhow::Result;
use std::io::{self, IsTerminal};

use clap::{Args, Parser, Subcommand};

/// Top-level command line. Running `mfp` with no subcommand behaves like
/// `mfp play` (same UI flags) when attached to a terminal.
#[derive(Parser)]
#[command(
    name = "mfp",
    version,
    about = "Music For Programming - lightweight terminal radio player",
    long_about = "Music For Programming - lightweight terminal radio player.\n\n\
Running `mfp` without a subcommand is the same as `mfp play`: it starts the \
full-screen terminal UI (TUI) and accepts the same options. When stdin or \
stdout is not a terminal (or TERM=dumb), `mfp` prints a command overview \
instead of starting playback; `mfp play` falls back to the plain text UI."
)]
struct Cli {
    #[command(flatten)]
    play: PlayArgs,
    #[command(subcommand)]
    command: Option<Commands>,
}

/// Options shared by `mfp play` and bare `mfp`.
#[derive(Args, Debug, Clone, Default, PartialEq, Eq)]
struct PlayArgs {
    /// Start at this episode number (e.g. 75)
    ///
    /// Playback starts at the first episode titled "Episode <N>..." (a plain
    /// substring match, so 1 also matches "Episode 12"). If none matches, or
    /// without this option, it starts at the beginning of the playlist.
    #[arg(short, long)]
    episode: Option<usize>,
    /// Shuffle the playlist
    ///
    /// Toggle it at any time with `s`.
    #[arg(short, long)]
    shuffle: bool,
    /// Play only favorites (see `mfp fav`)
    #[arg(short, long)]
    favorites: bool,
    /// Use the plain text UI instead of the full-screen TUI
    ///
    /// The plain UI prints status lines and reads typed commands followed by
    /// Enter (n, b, p, s, f, m, +, -, i, d, q). It is used automatically when
    /// stdin or stdout is not a terminal, or when TERM=dumb.
    ///
    /// When stdin is not a terminal, commands are read as lines and no live
    /// progress line is drawn. If stdin reaches EOF (e.g. `< /dev/null`),
    /// playback keeps going and MPRIS controls keep working until stopped
    /// with MPRIS Quit or Ctrl+C. If only stdout is redirected, the live
    /// progress line is omitted.
    #[arg(long)]
    plain: bool,
    /// Start the TUI in the compact (player only) layout
    ///
    /// Without it the TUI picks the full layout (episode list + player) when
    /// the terminal is at least 80x16, and the compact one otherwise. Press
    /// `v` in the TUI to switch layouts. Cannot be combined with --plain.
    #[arg(long, conflicts_with = "plain")]
    compact: bool,
}

/// How the process should present itself, decided from the environment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UiMode {
    /// Full-screen terminal UI (default).
    Tui,
    /// Typed-command text loop (`--plain` or automatic fallback).
    Plain,
    /// Print the command overview and exit (bare `mfp` without a terminal).
    Usage,
}

/// Decides the UI from the `--plain` flag and the terminal environment.
///
/// Pure so the whole matrix is unit-testable. Without a real terminal
/// (stdin or stdout redirected, or `TERM=dumb`) `mfp play` degrades to the
/// plain loop, while bare `mfp` only prints usage so audio never starts
/// unattended inside a pipe.
fn resolve_ui_mode(
    plain: bool,
    stdin_is_tty: bool,
    stdout_is_tty: bool,
    term_is_dumb: bool,
    has_subcommand: bool,
) -> UiMode {
    let interactive = stdin_is_tty && stdout_is_tty && !term_is_dumb;
    if !interactive && !has_subcommand {
        return UiMode::Usage;
    }
    if plain || !interactive {
        UiMode::Plain
    } else {
        UiMode::Tui
    }
}

/// Reads the real terminal environment and resolves the UI mode.
fn detect_ui_mode(plain: bool, has_subcommand: bool) -> UiMode {
    resolve_ui_mode(
        plain,
        io::stdin().is_terminal(),
        io::stdout().is_terminal(),
        std::env::var("TERM").is_ok_and(|t| t == "dumb"),
        has_subcommand,
    )
}

/// Starts playback with the UI chosen by `mode`.
fn start_play(args: PlayArgs, mode: UiMode) -> Result<()> {
    match mode {
        UiMode::Tui => play::play_tui(args.episode, args.shuffle, args.favorites, args.compact),
        UiMode::Plain => play::play_radio(args.episode, args.shuffle, args.favorites),
        UiMode::Usage => interactive::interactive_mode(),
    }
}

#[derive(Subcommand)]
enum Commands {
    /// List all available episodes
    List,
    /// Play episodes in the terminal UI (default) or the plain text UI
    #[command(long_about = "Play episodes.\n\n\
By default this opens the full-screen terminal UI (TUI): episode list, \
progress, volume and single-key controls. Use --plain for the text loop, \
where commands are typed followed by Enter. If stdin or stdout is not a \
terminal, or TERM=dumb, the plain UI is used automatically (with a piped \
stdin, commands are read line by line; at EOF playback continues).")]
    Play(PlayArgs),
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
        Some(Commands::Play(args)) => {
            let mode = detect_ui_mode(args.plain, true);
            start_play(args, mode)?
        }
        Some(Commands::Fav { add, remove, list }) => fav::manage_favorites(add, remove, list)?,
        Some(Commands::Download {
            episode,
            list,
            delete,
            size,
        }) => download::manage_downloads(episode, list, delete, size)?,
        None => {
            let mode = detect_ui_mode(cli.play.plain, false);
            start_play(cli.play, mode)?
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interactive_terminal_defaults_to_tui() {
        assert_eq!(resolve_ui_mode(false, true, true, false, true), UiMode::Tui);
        assert_eq!(
            resolve_ui_mode(false, true, true, false, false),
            UiMode::Tui
        );
    }

    #[test]
    fn plain_flag_selects_plain_on_a_terminal() {
        assert_eq!(
            resolve_ui_mode(true, true, true, false, true),
            UiMode::Plain
        );
        assert_eq!(
            resolve_ui_mode(true, true, true, false, false),
            UiMode::Plain
        );
    }

    #[test]
    fn play_falls_back_to_plain_without_a_terminal() {
        // stdin redirected, stdout redirected, both, or TERM=dumb.
        for (stdin, stdout, dumb) in [
            (false, true, false),
            (true, false, false),
            (false, false, false),
            (true, true, true),
        ] {
            for plain in [false, true] {
                assert_eq!(
                    resolve_ui_mode(plain, stdin, stdout, dumb, true),
                    UiMode::Plain,
                    "stdin={stdin} stdout={stdout} dumb={dumb} plain={plain}"
                );
            }
        }
    }

    #[test]
    fn bare_mfp_prints_usage_without_a_terminal() {
        for (stdin, stdout, dumb) in [
            (false, true, false),
            (true, false, false),
            (false, false, false),
            (true, true, true),
        ] {
            for plain in [false, true] {
                assert_eq!(
                    resolve_ui_mode(plain, stdin, stdout, dumb, false),
                    UiMode::Usage,
                    "stdin={stdin} stdout={stdout} dumb={dumb} plain={plain}"
                );
            }
        }
    }

    fn parse(args: &[&str]) -> Result<Cli, clap::Error> {
        Cli::try_parse_from(std::iter::once("mfp").chain(args.iter().copied()))
    }

    #[test]
    fn bare_mfp_accepts_play_flags() {
        let cli = parse(&["-e", "75", "-s", "-f", "--compact"]).unwrap();
        assert!(cli.command.is_none());
        assert_eq!(
            cli.play,
            PlayArgs {
                episode: Some(75),
                shuffle: true,
                favorites: true,
                plain: false,
                compact: true,
            }
        );
        assert_eq!(parse(&[]).unwrap().play, PlayArgs::default());
    }

    #[test]
    fn play_subcommand_parses_plain_with_episode() {
        let cli = parse(&["play", "--plain", "-e", "75"]).unwrap();
        match cli.command {
            Some(Commands::Play(args)) => {
                assert_eq!(args.episode, Some(75));
                assert!(args.plain);
                assert!(!args.compact);
            }
            _ => panic!("expected the play subcommand"),
        }
    }

    #[test]
    fn plain_and_compact_conflict() {
        for args in [
            &["play", "--plain", "--compact"][..],
            &["--plain", "--compact"][..],
        ] {
            let err = parse(args).err().expect("should be rejected");
            assert_eq!(err.kind(), clap::error::ErrorKind::ArgumentConflict);
        }
    }

    #[test]
    fn removed_tui_flag_is_rejected() {
        assert!(parse(&["play", "--tui"]).is_err());
        assert!(parse(&["--tui"]).is_err());
    }
}
