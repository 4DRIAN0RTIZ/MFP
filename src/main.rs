mod cli;
mod config;
mod logging;
mod models;
mod mpris;
mod operations;
mod player;
mod tui;

fn main() -> anyhow::Result<()> {
    cli::run()
}
