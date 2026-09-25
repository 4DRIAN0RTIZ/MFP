use crate::favorites::Favorites;
use crate::feed::Feed;
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

/// Extracts the episode number from a title such as "Episode 12: Name".
fn extract_episode_number(title: &str) -> Option<usize> {
    title
        .split(':')
        .next()?
        .trim()
        .strip_prefix("Episode ")?
        .parse()
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_episode_number_standard_title() {
        assert_eq!(extract_episode_number("Episode 12: Datassette"), Some(12));
        assert_eq!(extract_episode_number("Episode 1: A"), Some(1));
    }

    #[test]
    fn extract_episode_number_without_colon() {
        assert_eq!(extract_episode_number("Episode 7"), Some(7));
    }

    #[test]
    fn extract_episode_number_trims_whitespace_before_colon() {
        assert_eq!(extract_episode_number("  Episode 5  : X"), Some(5));
        assert_eq!(extract_episode_number("Episode 5 : X"), Some(5));
    }

    #[test]
    fn extract_episode_number_leading_zeros() {
        assert_eq!(extract_episode_number("Episode 007: Bond"), Some(7));
    }

    #[test]
    fn extract_episode_number_only_uses_text_before_first_colon() {
        assert_eq!(extract_episode_number("Episode 3: Episode 4: X"), Some(3));
    }

    #[test]
    fn extract_episode_number_rejects_bad_prefix() {
        assert_eq!(extract_episode_number(""), None);
        assert_eq!(extract_episode_number("episode 3: X"), None);
        assert_eq!(extract_episode_number("Ep 3: X"), None);
        assert_eq!(extract_episode_number("Episode: X"), None);
        assert_eq!(extract_episode_number("Episode3: X"), None);
    }

    #[test]
    fn extract_episode_number_rejects_non_numeric() {
        assert_eq!(extract_episode_number("Episode abc: X"), None);
        assert_eq!(extract_episode_number("Episode -1: X"), None);
        assert_eq!(extract_episode_number("Episode 1.5: X"), None);
    }

    #[test]
    fn extract_episode_number_rejects_overflow() {
        assert_eq!(
            extract_episode_number("Episode 99999999999999999999999: X"),
            None
        );
    }
}
