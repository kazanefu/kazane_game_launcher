use crate::data::local::{LocalGameData, InstalledGame, Settings};
use crate::process::{ProcessManager, RunningInfo};
use crate::data::remote::provider::GitHubRawProvider;
use crate::installer::api::LauncherApi;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub settings: Settings,
    pub local: LocalGameData,
    pub process: ProcessManager,
    pub launcher_api: LauncherApi<GitHubRawProvider>,
}

impl AppState {
    /// Create AppState with launcher API configured.
    pub fn new(settings: Settings, local: LocalGameData, games_dir: PathBuf, game_data_path: PathBuf, game_list_path: PathBuf, provider_branch: Option<&str>) -> Self {
        let provider = GitHubRawProvider::new(provider_branch);
        let launcher_api = LauncherApi::new(provider, games_dir.clone(), game_data_path.clone(), game_list_path);
        Self { settings, local, process: ProcessManager::new(), launcher_api }
    }

    /// Return an Arc-wrapped shared instance for UI/CLI usage
    pub fn shared(self) -> Arc<Self> {
        Arc::new(self)
    }

    /// Start a game process tracked under `id`. `exe` is the executable path and `args` are arguments.
    pub async fn start_game(&self, id: &str, exe: PathBuf, args: &[String]) -> Result<RunningInfo, Box<dyn std::error::Error + Send + Sync>> {
        let info = self.process.start(id, exe, args).await?;
        Ok(info)
    }

    /// Stop the tracked game process
    pub async fn stop_game(&self, id: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
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
    pub async fn install_game_by_id(&self, id: &str) -> Result<InstalledGame, Box<dyn std::error::Error + Send + Sync>> {
        self.launcher_api.install_game_by_id(id).await
    }

    /// Install by game id with optional progress/cancel
    pub async fn install_game_by_id_with(&self, id: &str, progress_tx: Option<crate::installer::ProgressSender>, cancel_rx: Option<tokio::sync::watch::Receiver<bool>>) -> Result<InstalledGame, Box<dyn std::error::Error + Send + Sync>> {
        self.launcher_api.install_game_by_id_with(id, progress_tx, cancel_rx).await
    }

    /// Update by game id without progress/cancel
    pub async fn update_game_by_id(&self, id: &str) -> Result<Option<InstalledGame>, Box<dyn std::error::Error + Send + Sync>> {
        self.launcher_api.update_game_by_id(id).await
    }

    /// Update by game id with optional progress/cancel
    pub async fn update_game_by_id_with(&self, id: &str, progress_tx: Option<crate::installer::ProgressSender>, cancel_rx: Option<tokio::sync::watch::Receiver<bool>>) -> Result<Option<InstalledGame>, Box<dyn std::error::Error + Send + Sync>> {
        self.launcher_api.update_game_by_id_with(id, progress_tx, cancel_rx).await
    }

    /// Uninstall by game id
    pub fn uninstall_game_by_id(&self, id: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.launcher_api.uninstall_game_by_id(id)
    }
}
