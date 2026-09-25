mod cli;
mod config;
mod logging;
mod models;
mod mpris;
mod operations;
mod player;

fn main() -> anyhow::Result<()> {
    cli::run()
}
