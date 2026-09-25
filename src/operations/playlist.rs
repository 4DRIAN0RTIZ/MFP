use crate::models::Episode;
use crate::operations::episodes::position_by_number;
use rand::seq::SliceRandom;
use rand::thread_rng;

pub struct Playlist {
    episodes: Vec<Episode>,
    current_index: usize,
    shuffle: bool,
    shuffled_indices: Vec<usize>,
}

impl Playlist {
    pub fn new(episodes: Vec<Episode>) -> Self {
        let indices: Vec<usize> = (0..episodes.len()).collect();
        Self {
            episodes,
            current_index: 0,
            shuffle: false,
            shuffled_indices: indices,
        }
    }

    pub fn from_favorites(all_episodes: &[Episode], favorite_titles: &[&String]) -> Self {
        let episodes: Vec<Episode> = all_episodes
            .iter()
            .filter(|e| favorite_titles.contains(&&e.title))
            .cloned()
            .collect();

        Self::new(episodes)
    }

    /// Builds the playlist for a play session.
    ///
    /// `favorite_titles` restricts the playlist to favorites when `Some`;
    /// `shuffle` enables shuffle; `start_episode` skips ahead to the first
    /// episode whose title matches that number (ignored when not found).
    pub fn for_session(
        episodes: &[Episode],
        favorite_titles: Option<&[&String]>,
        shuffle: bool,
        start_episode: Option<usize>,
    ) -> Self {
        let mut playlist = match favorite_titles {
            Some(favs) => Self::from_favorites(episodes, favs),
            None => Self::new(episodes.to_vec()),
        };
        if shuffle {
            playlist.enable_shuffle();
        }
        if let Some(num) = start_episode {
            if let Some(pos) = position_by_number(playlist.all_episodes(), num) {
                for _ in 0..pos {
                    playlist.next();
                }
            }
        }
        playlist
    }

    pub fn enable_shuffle(&mut self) {
        self.shuffle = true;
        self.reshuffle();
    }

    pub fn disable_shuffle(&mut self) {
        self.shuffle = false;
        self.shuffled_indices = (0..self.episodes.len()).collect();
    }

    pub fn toggle_shuffle(&mut self) {
        if self.shuffle {
            self.disable_shuffle();
        } else {
            self.enable_shuffle();
        }
    }

    fn reshuffle(&mut self) {
        let mut rng = thread_rng();
        self.shuffled_indices = (0..self.episodes.len()).collect();
        self.shuffled_indices.shuffle(&mut rng);
    }

    pub fn current(&self) -> Option<&Episode> {
        if self.episodes.is_empty() {
            return None;
        }

        let index = if self.shuffle {
            self.shuffled_indices.get(self.current_index).copied()?
        } else {
            self.current_index
        };

        self.episodes.get(index)
    }

    pub fn next(&mut self) -> Option<&Episode> {
        if self.episodes.is_empty() {
            return None;
        }

        self.current_index = (self.current_index + 1) % self.episodes.len();
        self.current()
    }

    pub fn previous(&mut self) -> Option<&Episode> {
        if self.episodes.is_empty() {
            return None;
        }

        if self.current_index == 0 {
            self.current_index = self.episodes.len() - 1;
        } else {
            self.current_index -= 1;
        }
        self.current()
    }

    pub fn len(&self) -> usize {
        self.episodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.episodes.is_empty()
    }

    pub fn is_shuffled(&self) -> bool {
        self.shuffle
    }

    pub fn all_episodes(&self) -> &[Episode] {
        &self.episodes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ep(title: &str) -> Episode {
        Episode {
            title: title.to_string(),
            audio_url: format!("http://x/{}.mp3", title),
            duration: "01:00".to_string(),
            pub_date: String::new(),
            description: String::new(),
        }
    }

    fn eps(n: usize) -> Vec<Episode> {
        (0..n).map(|i| ep(&format!("e{}", i))).collect()
    }

    fn title(p: &Playlist) -> Option<String> {
        p.current().map(|e| e.title.clone())
    }

    #[test]
    fn for_session_selects_favorites_and_start_episode() {
        let all = vec![ep("Episode 1: a"), ep("Episode 2: b"), ep("Episode 3: c")];
        let fav_b = "Episode 2: b".to_string();
        let fav_c = "Episode 3: c".to_string();
        let favs = [&fav_b, &fav_c];

        let p = Playlist::for_session(&all, Some(&favs), false, Some(3));
        assert_eq!(p.len(), 2);
        assert_eq!(title(&p).as_deref(), Some("Episode 3: c"));

        let p = Playlist::for_session(&all, None, false, Some(99));
        assert_eq!(p.len(), 3);
        assert_eq!(title(&p).as_deref(), Some("Episode 1: a"));
        assert!(!p.is_shuffled());

        assert!(Playlist::for_session(&all, None, true, None).is_shuffled());
    }

    #[test]
    fn empty_playlist_returns_none_everywhere() {
        let mut p = Playlist::new(vec![]);
        assert!(p.is_empty());
        assert_eq!(p.len(), 0);
        assert!(p.current().is_none());
        assert!(p.next().is_none());
        assert!(p.previous().is_none());
    }

    #[test]
    fn empty_playlist_shuffle_does_not_panic() {
        let mut p = Playlist::new(vec![]);
        p.enable_shuffle();
        assert!(p.is_shuffled());
        assert!(p.current().is_none());
    }

    #[test]
    fn starts_at_first_episode_unshuffled() {
        let p = Playlist::new(eps(3));
        assert_eq!(title(&p).as_deref(), Some("e0"));
        assert!(!p.is_shuffled());
        assert_eq!(p.len(), 3);
    }

    #[test]
    fn next_wraps_to_start() {
        let mut p = Playlist::new(eps(3));
        assert_eq!(p.next().map(|e| e.title.clone()).as_deref(), Some("e1"));
        assert_eq!(p.next().map(|e| e.title.clone()).as_deref(), Some("e2"));
        assert_eq!(p.next().map(|e| e.title.clone()).as_deref(), Some("e0"));
    }

    #[test]
    fn previous_wraps_to_end() {
        let mut p = Playlist::new(eps(3));
        assert_eq!(p.previous().map(|e| e.title.clone()).as_deref(), Some("e2"));
        assert_eq!(p.previous().map(|e| e.title.clone()).as_deref(), Some("e1"));
        assert_eq!(p.previous().map(|e| e.title.clone()).as_deref(), Some("e0"));
        assert_eq!(p.previous().map(|e| e.title.clone()).as_deref(), Some("e2"));
    }

    #[test]
    fn single_episode_next_and_previous_stay_put() {
        let mut p = Playlist::new(eps(1));
        assert_eq!(p.next().map(|e| e.title.clone()).as_deref(), Some("e0"));
        assert_eq!(p.previous().map(|e| e.title.clone()).as_deref(), Some("e0"));
    }

    #[test]
    fn shuffle_toggle_flips_flag() {
        let mut p = Playlist::new(eps(5));
        p.toggle_shuffle();
        assert!(p.is_shuffled());
        p.toggle_shuffle();
        assert!(!p.is_shuffled());
    }

    #[test]
    fn shuffled_traversal_visits_every_episode_once() {
        let mut p = Playlist::new(eps(6));
        p.enable_shuffle();
        let mut seen = vec![title(&p).unwrap()];
        for _ in 0..5 {
            seen.push(p.next().unwrap().title.clone());
        }
        seen.sort();
        assert_eq!(seen, vec!["e0", "e1", "e2", "e3", "e4", "e5"]);
        // After a full cycle it wraps to the first shuffled position.
        let first = seen_first(&p);
        assert_eq!(p.next().unwrap().title, first);
    }

    fn seen_first(p: &Playlist) -> String {
        p.episodes[p.shuffled_indices[0]].title.clone()
    }

    #[test]
    fn disable_shuffle_restores_order_but_keeps_position_index() {
        let mut p = Playlist::new(eps(4));
        p.next(); // index 1
        p.enable_shuffle();
        p.disable_shuffle();
        assert!(!p.is_shuffled());
        // current_index is not reset by shuffle changes.
        assert_eq!(title(&p).as_deref(), Some("e1"));
    }

    #[test]
    fn enable_shuffle_keeps_current_index_not_current_episode() {
        // Quirk: the index is preserved, so the current episode may change.
        let mut p = Playlist::new(eps(4));
        p.next();
        p.enable_shuffle();
        let expected = p.episodes[p.shuffled_indices[1]].title.clone();
        assert_eq!(title(&p), Some(expected));
    }

    #[test]
    fn from_favorites_filters_and_keeps_feed_order() {
        let all = eps(4);
        let t3 = "e3".to_string();
        let t1 = "e1".to_string();
        let p = Playlist::from_favorites(&all, &[&t3, &t1]);
        let titles: Vec<&str> = p.all_episodes().iter().map(|e| e.title.as_str()).collect();
        assert_eq!(titles, vec!["e1", "e3"]);
    }

    #[test]
    fn from_favorites_ignores_unknown_titles() {
        let all = eps(2);
        let unknown = "nope".to_string();
        let p = Playlist::from_favorites(&all, &[&unknown]);
        assert!(p.is_empty());
        assert!(p.current().is_none());
    }

    #[test]
    fn all_episodes_returns_original_order_even_when_shuffled() {
        let mut p = Playlist::new(eps(3));
        p.enable_shuffle();
        let titles: Vec<&str> = p.all_episodes().iter().map(|e| e.title.as_str()).collect();
        assert_eq!(titles, vec!["e0", "e1", "e2"]);
    }
}
