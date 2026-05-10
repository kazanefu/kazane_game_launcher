use kazane_game_launcher::process::ProcessManager;
use std::path::PathBuf;

#[tokio::test]
async fn test_start_and_stop_process() {
    let mgr = ProcessManager::new();
    // choose platform-appropriate command
    #[cfg(windows)]
    let (exe, args): (PathBuf, Vec<String>) = {
        (
            PathBuf::from("ping"),
            vec!["-n".to_string(), "4".to_string(), "127.0.0.1".to_string()],
        )
    };
    #[cfg(not(windows))]
    let (exe, args): (PathBuf, Vec<String>) = { (PathBuf::from("sleep"), vec!["2".to_string()]) };

    let id = "test-process";
    let info = mgr
        .start(
            id,
            ".".into(),
            exe.clone().into(),
            &args.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
        )
        .await
        .expect("start");
    assert!(info.pid != 0);
    assert!(mgr.is_running(id).await);
    // stop
    mgr.stop(id).await.expect("stop");
    assert!(!mgr.is_running(id).await);
}
