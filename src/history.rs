use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;

use anyhow::Result;
use chrono::{DateTime, Local, Utc};
use serde::{Deserialize, Serialize};

/// Entry for a single server's connection history
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub last_connected: DateTime<Utc>,
    pub connect_count: u32,
}

/// Connection history storage
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct History {
    /// Map from server host to history entry
    pub entries: HashMap<String, HistoryEntry>,
    /// Set of favorite server hosts
    #[serde(default)]
    pub favorites: HashSet<String>,
    /// Last used sort order
    #[serde(default)]
    pub sort_order: String,
}

impl History {
    /// Get the history file path
    fn history_path() -> Result<PathBuf> {
        let config_dir = dirs::config_dir()
            .ok_or_else(|| anyhow::anyhow!("Could not find config directory"))?;
        let ggoto_dir = config_dir.join("ggoto");
        fs::create_dir_all(&ggoto_dir)?;
        Ok(ggoto_dir.join("history.json"))
    }

    /// Load history from disk
    pub fn load() -> Result<Self> {
        let path = Self::history_path()?;
        if path.exists() {
            let content = fs::read_to_string(&path)?;
            let history: History = serde_json::from_str(&content)?;
            Ok(history)
        } else {
            Ok(History::default())
        }
    }

    /// Save history to disk
    pub fn save(&self) -> Result<()> {
        let path = Self::history_path()?;
        let content = serde_json::to_string_pretty(&self)?;
        fs::write(path, content)?;
        Ok(())
    }

    /// Record a connection to a server
    pub fn record_connection(&mut self, host: &str) {
        let entry = self.entries.entry(host.to_string()).or_insert(HistoryEntry {
            last_connected: Utc::now(),
            connect_count: 0,
        });
        entry.last_connected = Utc::now();
        entry.connect_count += 1;
    }

    /// Get last connection time for a server
    pub fn last_connected(&self, host: &str) -> Option<DateTime<Utc>> {
        self.entries.get(host).map(|e| e.last_connected)
    }

    /// Get connection count for a server
    #[allow(dead_code)]
    pub fn connect_count(&self, host: &str) -> u32 {
        self.entries.get(host).map(|e| e.connect_count).unwrap_or(0)
    }

    /// Check if a server is a favorite
    pub fn is_favorite(&self, host: &str) -> bool {
        self.favorites.contains(host)
    }

    /// Toggle favorite status for a server
    pub fn toggle_favorite(&mut self, host: &str) {
        if self.favorites.contains(host) {
            self.favorites.remove(host);
        } else {
            self.favorites.insert(host.to_string());
        }
    }

    /// Set sort order
    pub fn set_sort_order(&mut self, order: &str) {
        self.sort_order = order.to_string();
    }

    /// Get sort order
    pub fn get_sort_order(&self) -> &str {
        &self.sort_order
    }

    /// Format last connected time as relative string
    pub fn format_last_connected(&self, host: &str) -> String {
        match self.last_connected(host) {
            Some(dt) => {
                let local: DateTime<Local> = dt.into();
                let now = Local::now();
                let duration = now.signed_duration_since(local);

                if duration.num_minutes() < 1 {
                    "just now".to_string()
                } else if duration.num_hours() < 1 {
                    format!("{}m ago", duration.num_minutes())
                } else if duration.num_days() < 1 {
                    format!("{}h ago", duration.num_hours())
                } else if duration.num_days() < 7 {
                    format!("{}d ago", duration.num_days())
                } else if duration.num_weeks() < 4 {
                    format!("{}w ago", duration.num_weeks())
                } else {
                    local.format("%m/%d").to_string()
                }
            }
            None => "-".to_string(),
        }
    }
}
