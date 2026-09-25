mod cli;
mod downloader;
mod favorites;
mod feed;
mod mpris;
mod player;
mod playlist;

fn main() -> anyhow::Result<()> {
    cli::run()
}
