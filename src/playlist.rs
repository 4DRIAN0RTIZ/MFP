use crate::feed::Episode;
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
