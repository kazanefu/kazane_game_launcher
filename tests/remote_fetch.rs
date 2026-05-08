use kazane_game_launcher::data::remote::provider::GitHubRawProvider;
use kazane_game_launcher::data::RemoteProvider;

#[tokio::test]
async fn test_fetch_game_list_from_github_workspace() {
    let provider = GitHubRawProvider::new(Some("workspace"));
    let list = provider.fetch_game_list("kazanefu", "kazane_game_launcher").await.expect("fetch");
    assert!(!list.games.is_empty());
    assert_eq!(list.games[0].id, "sample-game");
}
