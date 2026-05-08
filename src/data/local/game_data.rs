use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct InstalledGame {
    pub id: String,
    pub name: String,
    pub version: String,
    pub install_path: String,
    pub repo: String,
    pub installed: bool,
    pub last_checked: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct LocalGameData {
    pub installed: Vec<InstalledGame>,
}

impl LocalGameData {
    pub fn load(path: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        if !path.exists() {
            return Ok(LocalGameData::default());
        }
        let s = std::fs::read_to_string(path)?;
        let v = serde_json::from_str(&s)?;
        Ok(v)
    }

    pub fn save_atomic(&self, path: &Path) -> Result<(), Box<dyn std::error::Error>> {
        crate::utils::file::write_json_atomic(path, self)
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
