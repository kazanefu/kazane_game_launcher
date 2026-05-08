use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GameListEntry {
    pub id: String,
    pub name: String,
    pub repo: String,
    pub description: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct GameList {
    pub games: Vec<GameListEntry>,
}
