use kazane_game_launcher::installer::LauncherApi;
use kazane_game_launcher::data::remote::provider::GitHubRawProvider;
use kazane_game_launcher::data::local::LocalGameData;
use std::path::PathBuf;
use tempfile::tempdir;

#[tokio::test]
async fn test_install_update_uninstall_flow() {
    let provider = GitHubRawProvider::new(None);
    let tmp = tempdir().unwrap();
    let games_dir = tmp.path().join("games");
    let game_data = tmp.path().join("game_data.json");
    // ensure clean
    if games_dir.exists() { std::fs::remove_dir_all(&games_dir).ok(); }
    if game_data.exists() { std::fs::remove_file(&game_data).ok(); }

    let api = LauncherApi::new(provider, games_dir.clone(), game_data.clone(), PathBuf::from("data/game_list.json"));

    // Install zip_sample_game (game_list id is 'zip-sample')
    let installed = api.install_game_by_id("zip-sample").await.expect("install");
    assert!(std::path::Path::new(&installed.install_path).exists());

    // Simulate older local version to force update
    let mut local = LocalGameData::load(&game_data).expect("load");
    for it in local.installed.iter_mut() {
        if it.id == "zip_sample_game" {
            it.version = "0.0.1".to_string();
        }
    }
    local.save_atomic(&game_data).expect("save local");

    // run update (should perform install and return Some)
    let updated = api.update_game_by_id("zip-sample").await.expect("update");
    assert!(updated.is_some());
    let new_local = LocalGameData::load(&game_data).expect("read");
    let found = new_local.installed.iter().find(|g| g.id == "zip_sample_game").expect("found");
    assert!(found.version != "0.0.1");

    // uninstall
    api.uninstall_game_by_id("zip_sample_game").expect("uninstall");
    assert!(!std::path::Path::new(&installed.install_path).exists());
    let after = LocalGameData::load(&game_data).expect("load");
    assert!(after.installed.iter().all(|g| g.id != "zip_sample_game"));
}
