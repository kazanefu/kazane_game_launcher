use crate::data::local::{InstalledGame, LocalGameData, Settings};
use crate::data::remote::provider::GitHubRawProvider;
use crate::installer::api::LauncherApi;
use crate::process::{ProcessManager, RunningInfo};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone)]
pub struct AppState {
    pub settings: Settings,
    pub local: LocalGameData,
    pub process: ProcessManager,
    pub launcher_api: LauncherApi<GitHubRawProvider>,
    // runtime logs kept in memory and persisted to a file
    pub logs: Arc<Mutex<Vec<String>>>,
    pub log_path: PathBuf,
}

impl AppState {
    /// Create AppState with launcher API configured.
    pub fn new(
        settings: Settings,
        local: LocalGameData,
        games_dir: PathBuf,
        game_data_path: PathBuf,
        game_list_path: PathBuf,
        provider_branch: Option<&str>,
    ) -> Self {
        let provider = GitHubRawProvider::new(provider_branch);
        let launcher_api = LauncherApi::new(
            provider,
            games_dir.clone(),
            game_data_path.clone(),
            game_list_path,
        );
        // place runtime log next to game_data.json (local dir)
        let log_path = game_data_path
            .parent()
            .unwrap_or(Path::new(""))
            .join("runtime.log");
        // ensure log file exists
        if let Some(parent) = log_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path);
        let s = Self {
            settings,
            local,
            process: ProcessManager::new(),
            launcher_api,
            logs: Arc::new(Mutex::new(Vec::new())),
            log_path,
        };
        // initial startup log
        s.append_log("INFO", "app started");
        s
    }

    /// Return an Arc-wrapped shared instance for UI/CLI usage
    pub fn shared(self) -> Arc<Self> {
        Arc::new(self)
    }

    fn append_log_to_file(&self, line: &str) {
        if let Some(parent) = self.log_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        match OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.log_path)
        {
            Ok(mut f) => {
                let _ = f.write_all(line.as_bytes());
                let _ = f.write_all(b"\n");
                let _ = f.sync_all();
            }
            Err(e) => {
                eprintln!(
                    "failed to write log file {}: {}",
                    self.log_path.display(),
                    e
                );
            }
        }
    }

    /// Append a runtime log line (thread-safe). Also persist to disk and print to stderr.
    pub fn append_log(&self, level: &str, msg: &str) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let line = format!("[{}] {}: {}", now, level, msg);
        // in-memory
        if let Ok(mut logs) = self.logs.lock() {
            logs.push(line.clone());
            if logs.len() > 1000 {
                // keep bounded
                let excess = logs.len() - 1000;
                logs.drain(0..excess);
            }
        }
        // persist
        self.append_log_to_file(&line);
        // console
        eprintln!("{}", line);
    }

    /// Return a copy of recent logs
    pub fn get_logs(&self) -> Vec<String> {
        if let Ok(logs) = self.logs.lock() {
            logs.clone()
        } else {
            Vec::new()
        }
    }

    /// Start a game process tracked under `id`. `exe` is the executable path and `args` are arguments.
    pub async fn start_game(
        &self,
        id: &str,
        exe: PathBuf,
        args: &[String],
    ) -> Result<RunningInfo, Box<dyn std::error::Error + Send + Sync>> {
        let info = self.process.start(id, exe, args).await?;
        Ok(info)
    }

    /// Stop the tracked game process
    pub async fn stop_game(
        &self,
        id: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.process.stop(id).await
    }

    /// Check if a game is running
    pub async fn is_running(&self, id: &str) -> bool {
        self.process.is_running(id).await
    }

    /// Get running info if available
    pub async fn get_running_info(&self, id: &str) -> Option<RunningInfo> {
        self.process.get_info(id).await
    }

    /// Install by game id, optional progress and cancel channels
    /// Install by game id without progress/cancel
    pub async fn install_game_by_id(
        &self,
        id: &str,
    ) -> Result<InstalledGame, Box<dyn std::error::Error + Send + Sync>> {
        self.launcher_api.install_game_by_id(id).await
    }

    /// Install by game id with optional progress/cancel
    pub async fn install_game_by_id_with(
        &self,
        id: &str,
        progress_tx: Option<crate::installer::ProgressSender>,
        cancel_rx: Option<tokio::sync::watch::Receiver<bool>>,
    ) -> Result<InstalledGame, Box<dyn std::error::Error + Send + Sync>> {
        self.launcher_api
            .install_game_by_id_with(id, progress_tx, cancel_rx)
            .await
    }

    /// Update by game id without progress/cancel
    pub async fn update_game_by_id(
        &self,
        id: &str,
    ) -> Result<Option<InstalledGame>, Box<dyn std::error::Error + Send + Sync>> {
        self.launcher_api.update_game_by_id(id).await
    }

    /// Update by game id with optional progress/cancel
    pub async fn update_game_by_id_with(
        &self,
        id: &str,
        progress_tx: Option<crate::installer::ProgressSender>,
        cancel_rx: Option<tokio::sync::watch::Receiver<bool>>,
    ) -> Result<Option<InstalledGame>, Box<dyn std::error::Error + Send + Sync>> {
        self.launcher_api
            .update_game_by_id_with(id, progress_tx, cancel_rx)
            .await
    }

    /// Uninstall by game id
    pub fn uninstall_game_by_id(
        &self,
        id: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.launcher_api.uninstall_game_by_id(id)
    }

    pub async fn get_readme_by_id(
        &self,
        id: &str,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        self.launcher_api.get_readme_by_id(id).await
    }
    pub async fn get_readme_by_local_id(
        &self,
        id: &str,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        self.launcher_api.get_readme_by_local_id(id).await
    }
}
