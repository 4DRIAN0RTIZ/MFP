//! Plain data types shared across layers.

use serde::{Deserialize, Serialize};

/// A single podcast episode as parsed from the RSS feed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Episode {
    pub title: String,
    pub audio_url: String,
    pub duration: String,
    pub pub_date: String,
    pub description: String,
}

impl Episode {
    /// Human-readable name shown in listings.
    pub fn display_name(&self) -> &str {
        &self.title
    }
}
