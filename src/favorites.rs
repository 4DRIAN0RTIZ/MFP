use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct Favorites {
    episodes: HashSet<String>,
    /// Test seam: when set, `save` writes here instead of the user config dir.
    #[serde(skip)]
    path_override: Option<PathBuf>,
}

impl Favorites {
    fn config_path() -> Result<PathBuf> {
        let config_dir = dirs::config_dir()
            .context("Failed to find config directory")?
            .join("mfp");

        fs::create_dir_all(&config_dir).context("Failed to create config directory")?;

        Ok(config_dir.join("favorites.json"))
    }

    pub fn load() -> Result<Self> {
        let path = Self::config_path()?;

        if !path.exists() {
            return Ok(Self::default());
        }

        let content = fs::read_to_string(&path).context("Failed to read favorites file")?;

        serde_json::from_str(&content).context("Failed to parse favorites file")
    }

    pub fn save(&self) -> Result<()> {
        let path = match &self.path_override {
            Some(path) => path.clone(),
            None => Self::config_path()?,
        };
        let content =
            serde_json::to_string_pretty(self).context("Failed to serialize favorites")?;

        fs::write(&path, content).context("Failed to write favorites file")
    }

    pub fn add(&mut self, title: String) -> bool {
        let added = self.episodes.insert(title);
        if added {
            let _ = self.save();
        }
        added
    }

    pub fn remove(&mut self, title: &str) -> bool {
        let removed = self.episodes.remove(title);
        if removed {
            let _ = self.save();
        }
        removed
    }

    pub fn is_favorite(&self, title: &str) -> bool {
        self.episodes.contains(title)
    }

    pub fn list(&self) -> Vec<&String> {
        let mut list: Vec<_> = self.episodes.iter().collect();
        list.sort();
        list
    }

    pub fn toggle(&mut self, title: String) -> bool {
        if self.is_favorite(&title) {
            self.remove(&title);
            false
        } else {
            self.add(title);
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static COUNTER: AtomicUsize = AtomicUsize::new(0);

    fn temp_file(name: &str) -> PathBuf {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!(
            "mfp-test-favorites-{}-{}-{}",
            std::process::id(),
            n,
            name
        ));
        fs::create_dir_all(&dir).unwrap();
        dir.join("favorites.json")
    }

    fn favs_at(path: &PathBuf) -> Favorites {
        Favorites {
            path_override: Some(path.clone()),
            ..Favorites::default()
        }
    }

    fn cleanup(path: &PathBuf) {
        if let Some(dir) = path.parent() {
            let _ = fs::remove_dir_all(dir);
        }
    }

    #[test]
    fn default_is_empty() {
        let f = Favorites::default();
        assert!(f.list().is_empty());
        assert!(!f.is_favorite("x"));
    }

    #[test]
    fn add_returns_true_then_false_for_duplicate() {
        let path = temp_file("add");
        let mut f = favs_at(&path);
        assert!(f.add("Episode 1: A".to_string()));
        assert!(!f.add("Episode 1: A".to_string()));
        assert!(f.is_favorite("Episode 1: A"));
        cleanup(&path);
    }

    #[test]
    fn remove_returns_true_only_if_present() {
        let path = temp_file("remove");
        let mut f = favs_at(&path);
        f.add("a".to_string());
        assert!(f.remove("a"));
        assert!(!f.remove("a"));
        assert!(!f.is_favorite("a"));
        cleanup(&path);
    }

    #[test]
    fn list_is_sorted_lexicographically() {
        let path = temp_file("list");
        let mut f = favs_at(&path);
        f.add("b".to_string());
        f.add("Episode 10".to_string());
        f.add("Episode 2".to_string());
        f.add("a".to_string());
        let list: Vec<&str> = f.list().iter().map(|s| s.as_str()).collect();
        // Plain string ordering: "Episode 10" < "Episode 2", uppercase before lowercase.
        assert_eq!(list, vec!["Episode 10", "Episode 2", "a", "b"]);
        cleanup(&path);
    }

    #[test]
    fn toggle_flips_and_returns_new_state() {
        let path = temp_file("toggle");
        let mut f = favs_at(&path);
        assert!(f.toggle("a".to_string()));
        assert!(f.is_favorite("a"));
        assert!(!f.toggle("a".to_string()));
        assert!(!f.is_favorite("a"));
        cleanup(&path);
    }

    #[test]
    fn add_persists_json_and_roundtrips() {
        let path = temp_file("persist");
        let mut f = favs_at(&path);
        f.add("a".to_string());
        let content = fs::read_to_string(&path).unwrap();
        let value: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(value["episodes"], serde_json::json!(["a"]));
        assert!(value.get("path_override").is_none());
        let loaded: Favorites = serde_json::from_str(&content).unwrap();
        assert!(loaded.is_favorite("a"));
        cleanup(&path);
    }

    #[test]
    fn failed_add_does_not_write_file() {
        let path = temp_file("nowrite");
        let mut f = favs_at(&path);
        f.add("a".to_string());
        fs::remove_file(&path).unwrap();
        assert!(!f.add("a".to_string()));
        assert!(!path.exists());
        cleanup(&path);
    }

    #[test]
    fn save_error_is_swallowed_by_add() {
        // Parent directory does not exist, so save fails; add still reports success.
        let path = temp_file("swallow")
            .parent()
            .unwrap()
            .join("missing")
            .join("f.json");
        let mut f = favs_at(&path);
        assert!(f.add("a".to_string()));
        assert!(f.is_favorite("a"));
        assert!(f.save().is_err());
        let _ = fs::remove_dir_all(path.parent().unwrap().parent().unwrap());
    }
}
