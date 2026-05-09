use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct InstalledGame {
    pub id: String,
    pub name: String,
    pub version: String,
    /// Directory where the game was installed
    pub install_path: String,
    /// Optional explicit executable path for launching the game
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exe_path: Option<String>,
    pub repo: String,
    pub installed: bool,
    pub last_checked: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct LocalGameData {
    pub installed: Vec<InstalledGame>,
}

impl LocalGameData {
    pub fn load(path: &Path) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        if !path.exists() {
            return Ok(LocalGameData::default());
        }
        // Use locked read to avoid concurrent writers
        let v: LocalGameData = crate::utils::file::read_json_with_lock(path)?;
        Ok(v)
    }

    pub fn save_atomic(&self, path: &Path) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Use locked write to ensure exclusive access across processes
        crate::utils::file::write_json_with_lock(path, self)
    }

    pub fn find(&self, id: &str) -> Option<&InstalledGame> {
        self.installed.iter().find(|g| g.id == id)
    }

    pub fn add_or_update(&mut self, game: InstalledGame) {
        if let Some(existing) = self.installed.iter_mut().find(|g| g.id == game.id) {
            *existing = game;
        } else {
            self.installed.push(game);
        }
    }
}
