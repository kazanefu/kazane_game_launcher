use tempfile::tempdir;
use kazane_game_launcher::data::remote::provider::GitHubRawProvider;
use kazane_game_launcher::installer::install::install_from_repo;
use std::path::Path;

#[tokio::test]
async fn test_install_exe_sample_game() {
    let provider = GitHubRawProvider::new(None);
    // install into repository's games/ directory
    let repo_root = Path::new(".");
    let games_dir = repo_root.join("games");
    let game_data = repo_root.join("local").join("game_data.json");
    let installed = install_from_repo(&provider, "kazanefu", "exe_sample_game", &games_dir, &game_data).await.expect("install exe");
    // check install path exists and contains exe
    let p = std::path::Path::new(&installed.install_path);
    assert!(p.exists());
    // find an .exe file inside
    let mut found = false;
    for entry in std::fs::read_dir(p).unwrap() {
        let e = entry.unwrap();
        if let Some(ext) = e.path().extension() {
            if ext == "exe" {
                found = true;
                break;
            }
        }
    }
    assert!(found, "exe not found in installed dir");
}
