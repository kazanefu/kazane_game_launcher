use kazane_game_launcher::data::local::LocalGameData;
use kazane_game_launcher::data::local::Settings;
use kazane_game_launcher::data::remote::provider::GitHubRawProvider;
use kazane_game_launcher::state::AppState;
use std::path::PathBuf;
use tempfile::tempdir;
use tokio::sync::{mpsc, watch};

#[tokio::test]
async fn test_appstate_install_with_progress() {
    let tmp = tempdir().unwrap();
    let games_dir = tmp.path().join("games");
    let game_data = tmp.path().join("game_data.json");
    let game_list = PathBuf::from("data/game_list.json");
    // create settings and local
    let settings = Settings::default();
    let local = LocalGameData::default();
    let app = AppState::new(
        settings,
        local,
        games_dir.clone(),
        game_data.clone(),
        game_list.clone(),
        None,
    );

    // progress channel
    let (tx, mut rx) = mpsc::channel(32);
    let (cancel_tx, cancel_rx) = watch::channel(false);

    // start install (zip-sample from data/game_list.json)
    let res = app
        .install_game_by_id_with("zip-sample", Some(tx), Some(cancel_rx))
        .await
        .expect("install by id");
    assert!(std::path::Path::new(&res.install_path).exists());

    // ensure we received at least one progress update
    let mut got = false;
    while let Ok(p) = rx.try_recv() {
        got = true;
        break;
    }
    assert!(got);

    // cleanup
    let _ = app.uninstall_game_by_id(&res.id);
}
