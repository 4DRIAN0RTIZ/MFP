use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

const RSS_URL: &str = "https://musicforprogramming.net/rss.xml";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Episode {
    pub title: String,
    pub audio_url: String,
    pub duration: String,
    pub pub_date: String,
    pub description: String,
}

impl Episode {
    pub fn display_name(&self) -> &str {
        &self.title
    }
}

pub struct Feed {
    episodes: Vec<Episode>,
}

impl Feed {
    pub fn fetch() -> Result<Self> {
        let content = reqwest::blocking::get(RSS_URL)
            .context("Failed to fetch RSS feed")?
            .bytes()
            .context("Failed to read RSS content")?;

        let channel = rss::Channel::read_from(&content[..])
            .context("Failed to parse RSS feed")?;

        let episodes = channel
            .items()
            .iter()
            .filter_map(|item| {
                Some(Episode {
                    title: item.title()?.to_string(),
                    audio_url: item.enclosure()?.url().to_string(),
                    duration: item.itunes_ext()?.duration().unwrap_or("Unknown").to_string(),
                    pub_date: item.pub_date().unwrap_or("Unknown").to_string(),
                    description: item.description().unwrap_or("").to_string(),
                })
            })
            .collect();

        Ok(Feed { episodes })
    }

    pub fn episodes(&self) -> &[Episode] {
        &self.episodes
    }

    pub fn find_by_title(&self, title: &str) -> Option<&Episode> {
        self.episodes.iter().find(|e| e.title == title)
    }
}
