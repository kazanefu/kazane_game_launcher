use crate::data::local::LocalGameData;
use crate::data::local::Settings;
use crate::process::{ProcessManager, RunningInfo};
use std::path::PathBuf;

#[derive(Clone)]
pub struct AppState {
    pub settings: Settings,
    pub local: LocalGameData,
    pub process: ProcessManager,
}

impl AppState {
    pub fn new(settings: Settings, local: LocalGameData) -> Self {
        Self {
            settings,
            local,
            process: ProcessManager::new(),
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
}
