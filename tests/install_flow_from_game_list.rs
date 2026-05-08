use kazane_game_launcher::data::remote::GameList;
use kazane_game_launcher::data::remote::provider::GitHubRawProvider;
use kazane_game_launcher::installer::install::install_from_repo;
use kazane_game_launcher::utils::file;
use std::path::Path;

fn parse_owner_repo(url: &str) -> Option<(String, String)> {
    // expect formats like: https://github.com/owner/repo or https://github.com/owner/repo/
    url.strip_prefix("https://github.com/")
        .or_else(|| url.strip_prefix("http://github.com/"))
        .map(|s| s.trim_end_matches('/'))
        .and_then(|s| s.split_once('/'))
        .map(|(a, b)| (a.to_string(), b.to_string()))
}

#[tokio::test]
async fn test_install_flow_steps_5_to_7_for_all_games() {
    // Load local data/game_list.json
    let gl: GameList = file::read_json(Path::new("data/game_list.json")).expect("load game_list");
    let provider = GitHubRawProvider::new(None);
    let repo_root = Path::new(".");
    let games_dir = repo_root.join("games");
    let game_data = repo_root.join("local").join("game_data.json");

    // Ensure clean state
    if game_data.exists() {
        std::fs::remove_file(&game_data).ok();
    }
    if games_dir.exists() {
        std::fs::remove_dir_all(&games_dir).ok();
    }

    for g in gl.games {
        if let Some((owner, repo)) = parse_owner_repo(&g.repo) {
            let installed =
                install_from_repo(&provider, &owner, &repo, &games_dir, &game_data, None, None)
                    .await
                    .expect("install");
            // verify entry in local/game_data.json
            let local_data: kazane_game_launcher::data::local::LocalGameData =
                file::read_json(&game_data).expect("read game_data");
            let found = local_data.installed.iter().any(|it| {
                it.id == repo
                    || it.name == installed.name
                    || it.install_path == installed.install_path
            });
            assert!(found, "installed game not recorded in game_data.json");
        }
    }
}
