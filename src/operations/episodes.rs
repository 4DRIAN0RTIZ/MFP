//! Episode lookup helpers based on the episode number in the title.

use crate::models::Episode;

/// Extracts the episode number from a title such as "Episode 12: Name".
///
/// Case-sensitive; only the text before the first colon is considered.
pub fn extract_episode_number(title: &str) -> Option<usize> {
    title
        .split(':')
        .next()?
        .trim()
        .strip_prefix("Episode ")?
        .parse()
        .ok()
}

/// Returns the index of the first episode whose title contains
/// `"Episode <number>"` (substring match, so `1` also matches "Episode 12").
pub fn position_by_number(episodes: &[Episode], number: usize) -> Option<usize> {
    let target_title = format!("Episode {}", number);
    episodes
        .iter()
        .position(|e| e.title.contains(&target_title))
}

/// Returns the first episode whose title contains `"Episode <number>"`.
pub fn find_by_number(episodes: &[Episode], number: usize) -> Option<&Episode> {
    position_by_number(episodes, number).map(|i| &episodes[i])
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

    fn ep(title: &str) -> Episode {
        Episode {
            title: title.to_string(),
            audio_url: String::new(),
            duration: String::new(),
            pub_date: String::new(),
            description: String::new(),
        }
    }

    #[test]
    fn find_by_number_matches_by_substring_first_hit() {
        let eps = vec![ep("Episode 12: A"), ep("Episode 1: B")];
        // Substring quirk: "Episode 1" matches "Episode 12" first.
        assert_eq!(position_by_number(&eps, 1), Some(0));
        assert_eq!(position_by_number(&eps, 12), Some(0));
        assert_eq!(
            find_by_number(&eps, 1).map(|e| e.title.as_str()),
            Some("Episode 12: A")
        );
    }

    #[test]
    fn find_by_number_none_when_missing() {
        let eps = vec![ep("Episode 2: A")];
        assert!(find_by_number(&eps, 3).is_none());
        assert_eq!(position_by_number(&eps, 2), Some(0));
    }
}
