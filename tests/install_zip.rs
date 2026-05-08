use kazane_game_launcher::data::remote::provider::GitHubRawProvider;
use kazane_game_launcher::installer::install::install_from_repo;
use std::path::Path;
use tempfile::tempdir;

#[tokio::test]
async fn test_install_zip_sample_game() {
    let provider = GitHubRawProvider::new(None);
    let repo_root = Path::new(".");
    let games_dir = repo_root.join("games");
    let game_data = repo_root.join("local").join("game_data.json");
    let installed = install_from_repo(
        &provider,
        "kazanefu",
        "zip_sample_game",
        &games_dir,
        &game_data,
    )
    .await
    .expect("install zip");
    let p = std::path::Path::new(&installed.install_path);
    assert!(p.exists());
    // check for an exe or other expected file
    let mut found_any = false;
    for entry in std::fs::read_dir(p).unwrap() {
        let _e = entry.unwrap();
        found_any = true;
        break;
    }
    assert!(found_any, "no files extracted");
}
