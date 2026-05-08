use std::path::PathBuf;
use tempfile::tempdir;

#[test]
fn test_settings_default_load() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("settings.json");
    let s = kazane_game_launcher::data::local::Settings::load(&path).unwrap();
    assert_eq!(s.install_dir, "games");
    assert_eq!(s.theme, "dark");
}

#[test]
fn test_local_game_data_load_default() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("game_data.json");
    let d = kazane_game_launcher::data::local::LocalGameData::load(&path).unwrap();
    assert!(d.installed.is_empty());
}

#[test]
fn test_write_and_read_json_atomic() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("settings.json");
    let s = kazane_game_launcher::data::local::Settings::default();
    kazane_game_launcher::utils::file::write_json_atomic(&path, &s).unwrap();
    let s2: kazane_game_launcher::data::local::Settings =
        kazane_game_launcher::utils::file::read_json(&path).unwrap();
    assert_eq!(s.install_dir, s2.install_dir);
    assert_eq!(s.theme, s2.theme);
}

#[test]
fn test_game_list_deserialize() {
    let json = r#"{"games":[{"id":"sample-game","name":"Sample Game","repo":"https://github.com/owner/sample-game"}]}"#;
    let gl: kazane_game_launcher::data::remote::GameList = serde_json::from_str(json).unwrap();
    assert_eq!(gl.games.len(), 1);
    assert_eq!(gl.games[0].id, "sample-game");
}
