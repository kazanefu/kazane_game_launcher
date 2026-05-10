use kazane_game_launcher::data::RemoteProvider;
use kazane_game_launcher::data::remote::provider::GitHubRawProvider;

#[tokio::test]
async fn test_fetch_readme() {
    let provider = GitHubRawProvider::new(None);
    let readme = provider
        .fetch_readme("kazanefu", "exe_sample_game")
        .await
        .expect("fetch readme");
    println!("README content:\n{}", readme);
    assert!(readme.contains("**太文字**"));
}
