use kazane_game_launcher::data::remote::provider::GitHubRawProvider;
use kazane_game_launcher::installer::install::{Progress, install_from_repo};
use std::time::Duration;
use tokio::sync::{mpsc, watch};

#[tokio::test]
async fn test_progress_events_during_install() {
    let provider = GitHubRawProvider::new(None);
    let (tx, mut rx) = mpsc::channel::<Progress>(16);
    let (_cancel_tx, cancel_rx) = watch::channel(false);
    let game_data = std::path::Path::new("local").join("game_data.json");
    let games_dir = std::path::PathBuf::from("games");

    // run install in background
    let handle = tokio::spawn(async move {
        install_from_repo(
            &provider,
            "kazanefu",
            "zip_sample_game",
            &games_dir,
            &game_data,
            Some(tx),
            Some(cancel_rx),
        )
        .await
    });

    // collect some progress events
    let mut got = false;
    let mut events = Vec::new();
    let start = tokio::time::Instant::now();
    while start.elapsed() < Duration::from_secs(10) {
        if let Some(p) = rx.recv().await {
            events.push(p);
            got = true;
            if events.len() >= 2 {
                break;
            }
        } else {
            break;
        }
    }

    // don't cancel here; wait for install to finish
    let res = handle.await.expect("task join");
    assert!(res.is_ok());
    assert!(got, "no progress events received");
}

#[tokio::test]
async fn test_cancel_install() {
    let provider = GitHubRawProvider::new(None);
    let (tx, mut rx) = mpsc::channel::<Progress>(16);
    let (cancel_tx, cancel_rx) = watch::channel(false);
    let game_data = std::path::Path::new("local").join("game_data.json");
    let games_dir = std::path::PathBuf::from("games");

    let handle = tokio::spawn(async move {
        install_from_repo(
            &provider,
            "kazanefu",
            "zip_sample_game",
            &games_dir,
            &game_data,
            Some(tx),
            Some(cancel_rx),
        )
        .await
    });

    // wait for first progress
    if let Some(_p) = rx.recv().await {
        // signal cancel
        let _ = cancel_tx.send(true);
    }

    let res = handle.await.expect("task join");
    assert!(res.is_err(), "install should have been cancelled");
}
