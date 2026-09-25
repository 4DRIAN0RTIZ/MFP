mod cli;
mod config;
mod models;
mod mpris;
mod operations;
mod player;

fn main() -> anyhow::Result<()> {
    cli::run()
}
