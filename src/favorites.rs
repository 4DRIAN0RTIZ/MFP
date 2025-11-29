use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct Favorites {
    episodes: HashSet<String>,
}

impl Favorites {
    fn config_path() -> Result<PathBuf> {
        let config_dir = dirs::config_dir()
            .context("Failed to find config directory")?
            .join("mfp");

        fs::create_dir_all(&config_dir)
            .context("Failed to create config directory")?;

        Ok(config_dir.join("favorites.json"))
    }

    pub fn load() -> Result<Self> {
        let path = Self::config_path()?;

        if !path.exists() {
            return Ok(Self::default());
        }

        let content = fs::read_to_string(&path)
            .context("Failed to read favorites file")?;

        serde_json::from_str(&content)
            .context("Failed to parse favorites file")
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::config_path()?;
        let content = serde_json::to_string_pretty(self)
            .context("Failed to serialize favorites")?;

        fs::write(&path, content)
            .context("Failed to write favorites file")
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
