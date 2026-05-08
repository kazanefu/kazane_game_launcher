pub mod data;
pub mod installer;
pub mod state;
pub mod utils;
pub mod process;
pub mod api;

use data::local::{LocalGameData, Settings};
use data::remote::provider::{GitHubRawProvider, RemoteProvider};
use std::path::PathBuf;

pub async fn run_from_args(
    args: Vec<String>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
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

    println!("Settings: {:?}", settings);
    println!("Installed games: {}", local.installed.len());

    // CLI hook: fetch-games owner/repo
    if args.len() >= 3 && args[1] == "fetch-games" {
        if let Some(repo_spec) = args.get(2) {
            if let Some((owner, repo)) = repo_spec.split_once('/') {
                let provider = GitHubRawProvider::new(None);
                let list = provider.fetch_game_list(owner, repo).await?;
                println!("Fetched {} games:", list.games.len());
                for g in list.games {
                    println!("- {} ({})", g.name, g.id);
                }
            } else {
                eprintln!("repo must be owner/repo");
            }
        }
    }

    Ok(())
}
