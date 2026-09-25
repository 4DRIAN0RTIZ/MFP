//! Terminal UI (ratatui). Currently the compact player view, opt-in via
//! `mfp play --tui`.
//!
//! The UI loop is synchronous. Anything slow (feed fetch, stream start-up,
//! downloads) runs on std threads and reports back over an mpsc channel; MPRIS
//! is polled with `try_recv` each tick. Nothing here prints to the terminal
//! apart from the ratatui backend; diagnostics go to `logging::log`.

mod app;
mod events;
mod session;
mod theme;
mod ui;
mod widgets;

use std::io::{self, Stdout};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use crossterm::{
    cursor::Show,
    event::{self, Event},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};

use crate::mpris::MprisController;
use crate::operations::favorites::Favorites;
use crate::player::Player;

use app::App;
use session::{Flow, Session};

pub use session::PlayOptions;

/// Event poll interval; also the redraw tick.
const TICK: Duration = Duration::from_millis(100);

/// Puts the terminal in raw mode on the alternate screen and restores it on
/// drop, so every exit path (return, `?`, panic unwinding) leaves the shell usable.
struct TerminalGuard;

impl TerminalGuard {
    fn enter() -> Result<Self> {
        install_panic_hook();
        enable_raw_mode().context("Failed to enable raw mode")?;
        if let Err(e) = execute!(io::stdout(), EnterAlternateScreen) {
            let _ = disable_raw_mode();
            return Err(e).context("Failed to enter the alternate screen");
        }
        Ok(Self)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        restore_terminal();
    }
}

/// Leaves raw mode and the alternate screen. Safe to call more than once.
fn restore_terminal() {
    let _ = disable_raw_mode();
    let _ = execute!(io::stdout(), LeaveAlternateScreen, Show);
}

/// Restores the terminal before the previous panic hook prints its message,
/// so the panic output is readable instead of lost on the alternate screen.
fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        restore_terminal();
        previous(info);
    }));
}

/// Runs the TUI until the user (or MPRIS) quits.
///
/// The terminal is restored before the MPRIS controller is dropped (which
/// joins its thread), so a slow MPRIS shutdown never leaves raw mode active.
pub fn run(options: PlayOptions, favorites: Favorites) -> Result<()> {
    let player = Player::new()?;
    // MPRIS is best-effort: a missing bus must never stop playback.
    let mpris = match MprisController::new() {
        Ok(m) => Some(m),
        Err(e) => {
            crate::logging::log(&format!("MPRIS unavailable, continuing without it: {}", e));
            None
        }
    };

    let result = run_terminal(&player, favorites, mpris.as_ref(), options);

    player.stop();
    drop(mpris);
    result
}

/// Owns the terminal for the duration of the UI loop.
fn run_terminal(
    player: &Player,
    favorites: Favorites,
    mpris: Option<&MprisController>,
    options: PlayOptions,
) -> Result<()> {
    let _guard = TerminalGuard::enter()?;
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))
        .context("Failed to initialize the terminal")?;
    event_loop(&mut terminal, player, favorites, mpris, options)
}

fn event_loop(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    player: &Player,
    favorites: Favorites,
    mpris: Option<&MprisController>,
    options: PlayOptions,
) -> Result<()> {
    let (tx, rx) = mpsc::channel();
    let mut app = App::new(mpris.is_some());
    let mut session = Session::new(player, favorites, mpris, options, tx);

    loop {
        let now = Instant::now();
        session.sync(&mut app);
        app.expire_status(now);
        terminal.draw(|frame| ui::draw(frame, &app, now))?;

        if event::poll(TICK)? {
            if let Event::Key(key) = event::read()? {
                if let Some(command) = events::map_key(key) {
                    if session.handle_command(command, &mut app, Instant::now()) == Flow::Quit {
                        return Ok(());
                    }
                }
            }
        }

        let now = Instant::now();
        for msg in rx.try_iter() {
            session.handle_msg(msg, &mut app, now);
        }
        if session.poll_mpris(&mut app, now) == Flow::Quit {
            return Ok(());
        }
    }
}
