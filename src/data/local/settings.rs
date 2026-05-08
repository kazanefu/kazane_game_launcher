use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Settings {
    pub install_dir: String,
    pub theme: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            install_dir: "games".to_string(),
            theme: "dark".to_string(),
        }
    }
}

impl Settings {
    pub fn load(path: &Path) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        if !path.exists() {
            return Ok(Settings::default());
        }
        let s = std::fs::read_to_string(path)?;
        let v = serde_json::from_str(&s)?;
        Ok(v)
    }

    pub fn save_atomic(&self, path: &Path) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        crate::utils::file::write_json_atomic(path, self)
    }
}
