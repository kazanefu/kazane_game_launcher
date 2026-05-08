mod data;
mod installer;
mod state;
mod utils;

use data::local::{LocalGameData, Settings};
use state::AppState;
use std::path::PathBuf;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let exe_dir = std::env::current_exe()?
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));
    let launcher_dir = exe_dir.join("launcher");
    let local_dir = exe_dir.join("local");
    let settings_path = launcher_dir.join("settings.json");
    let game_data_path = local_dir.join("game_data.json");

    std::fs::create_dir_all(&launcher_dir)?;
    std::fs::create_dir_all(&local_dir)?;

    let settings = Settings::load(&settings_path)?;
    let local = LocalGameData::load(&game_data_path)?;

    let _app_state = AppState::new(settings.clone(), local.clone());

    println!("Settings: {:?}", settings);
    println!("Installed games: {}", _app_state.local.installed.len());

    Ok(())
}
