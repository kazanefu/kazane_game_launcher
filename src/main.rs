#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use kazane_game_launcher::data::remote::RemoteProvider;
use std::path::PathBuf;

#[tokio::main]
async fn main() {
    // Minimal binary wrapper — core logic lives in the library to keep main.rs clean for tests.
    let args: Vec<String> = std::env::args().collect();
    if args.len() == 1 || args.iter().any(|a| a == "gui") {
        // create app state and run GUI synchronously
        if let Err(e) = kazane_game_launcher::run_gui({
            let exe_dir = std::env::current_exe()
                .ok()
                .and_then(|p| p.parent().map(|p| p.to_path_buf()))
                .unwrap_or_else(|| PathBuf::from("."));
            let launcher_dir = exe_dir.join("launcher");
            let local_dir = exe_dir.join("local");
            let settings_path = launcher_dir.join("settings.json");
            let game_data_path = local_dir.join("game_data.json");
            let game_list_path = exe_dir.join("data").join("game_list.json");

            // On startup, fetch game list from GitHub and save to disk (overwriting any existing file).
            let provider =
                kazane_game_launcher::data::remote::provider::GitHubRawProvider::new(None);
            if let Ok(game_list) = provider
                .fetch_game_list("kazanefu", "launcher_game_list")
                .await
            {
                if game_list.save_atomic(&game_list_path).is_err() {
                    eprintln!(
                        "failed to save game_list.json to {}",
                        game_list_path.display()
                    );
                }
            } else {
                eprintln!("failed to fetch game list from GitHub");
            }
            std::fs::create_dir_all(&launcher_dir).ok();
            std::fs::create_dir_all(&local_dir).ok();
            let settings = kazane_game_launcher::data::local::Settings::load(&settings_path)
                .unwrap_or_default();
            // If settings file didn't exist, save defaults to create it
            if !settings_path.exists() {
                let _ = settings.save_atomic(&settings_path);
            }
            let local = kazane_game_launcher::data::local::LocalGameData::load(&game_data_path)
                .unwrap_or_default();
            kazane_game_launcher::state::AppState::new(
                settings.clone(),
                local,
                exe_dir.join(&settings.install_dir),
                game_data_path,
                game_list_path,
                None,
            )
            .shared()
        }) {
            eprintln!("GUI error: {}", e);
        }
        return;
    }

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");
    if let Err(e) = rt.block_on(kazane_game_launcher::run_from_args(args)) {
        eprintln!("Error: {}", e);
    }
}
