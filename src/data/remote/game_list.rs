use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GameListEntry {
    pub id: String,
    pub name: String,
    pub repo: String,
    pub description: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct GameList {
    pub games: Vec<GameListEntry>,
}

impl GameList {
    /// Search games by query (partial match against id or name, case-insensitive)
    /// and optional tags (all provided tags must be present on an entry).
    pub fn search(&self, query: &str, tags: Option<&[&str]>) -> Vec<GameListEntry> {
        let q = query.trim().to_lowercase();
        self.games
            .iter()
            .filter(|g| {
                let mut matched = true;
                if !q.is_empty() {
                    let idm = g.id.to_lowercase();
                    let namem = g.name.to_lowercase();
                    matched = idm.contains(&q) || namem.contains(&q);
                }
                if matched && let Some(ts) = tags {
                    // require all tags to exist (case-insensitive)
                    let entry_tags: Vec<String> = g.tags.iter().map(|t| t.to_lowercase()).collect();
                    for &t in ts.iter() {
                        if !entry_tags.contains(&t.to_lowercase()) {
                            return false;
                        }
                    }
                }
                matched
            })
            .cloned()
            .collect()
    }
    pub fn save_atomic(&self, path: &Path) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Use locked write to ensure exclusive access across processes
        crate::utils::file::write_json_with_lock(path, self)
    }
}
