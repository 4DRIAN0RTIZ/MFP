use crate::operations::episodes::extract_episode_number;
use crate::operations::favorites::Favorites;
use crate::operations::feed::Feed;
use anyhow::Result;

/// Prints every episode with its number, title, duration and favorite marker.
pub(super) fn list_episodes() -> Result<()> {
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
