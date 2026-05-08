use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::SystemTime;

use tokio::process::Child;
use tokio::sync::Mutex;

#[derive(Debug, Clone)]
pub struct RunningInfo {
    pub id: String,
    pub pid: u32,
    pub start_time: SystemTime,
}

struct Entry {
    child: Child,
    start_time: SystemTime,
}

#[derive(Clone)]
pub struct ProcessManager {
    inner: Arc<Mutex<HashMap<String, Entry>>>,
}

impl ProcessManager {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Start a process and track it under `id`.
    /// `args` is a slice of strings for command arguments.
    pub async fn start(&self, id: &str, exe: PathBuf, args: &[String]) -> Result<RunningInfo, Box<dyn std::error::Error + Send + Sync>> {
        // Prevent duplicate starts
        let mut map = self.inner.lock().await;
        if map.contains_key(id) {
            return Err(format!("already running: {}", id).into());
        }

        let mut cmd = tokio::process::Command::new(&exe);
        if !args.is_empty() {
            cmd.args(args);
        }
        // On Windows, ensure creation of a child process that we can kill.
        let child = cmd.spawn()?;
        let pid = child.id().unwrap_or(0);
        let start_time = SystemTime::now();

        // Insert into map
        map.insert(
            id.to_string(),
            Entry {
                child,
                start_time,
            },
        );

        Ok(RunningInfo {
            id: id.to_string(),
            pid,
            start_time,
        })
    }

    /// Try to stop the process gracefully, then force-kill if it doesn't exit.
    pub async fn stop(&self, id: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut map = self.inner.lock().await;
        if let Some(mut entry) = map.remove(id) {
            // First try graceful shutdown
            #[cfg(unix)]
            {
                use nix::sys::signal::{kill, Signal};
                use nix::unistd::Pid;
                if let Some(pid) = entry.child.id() {
                    let _ = kill(Pid::from_raw(pid as i32), Signal::TERM);
                }
            }

            #[cfg(windows)]
            {
                // Best-effort: try to kill; no standard graceful signal on windows
            }

            // Wait a short time for exit, otherwise kill
            use tokio::time::{sleep, Duration};
            let wait = sleep(Duration::from_millis(500));
            tokio::pin!(wait);

            // Poll for exit
            match tokio::time::timeout(Duration::from_secs(2), entry.child.wait()).await {
                Ok(Ok(_status)) => {
                    // exited
                    return Ok(());
                }
                _ => {
                    // try kill
                    let _ = entry.child.kill().await;
                    let _ = entry.child.wait().await;
                    return Ok(());
                }
            }
        }
        Err(format!("not running: {}", id).into())
    }

    /// Check if a tracked id is running
    pub async fn is_running(&self, id: &str) -> bool {
        let map = self.inner.lock().await;
        map.get(id).map(|e| e.child.id().is_some()).unwrap_or(false)
    }

    /// Return running info if present
    pub async fn get_info(&self, id: &str) -> Option<RunningInfo> {
        let map = self.inner.lock().await;
        map.get(id).and_then(|e| e.child.id().map(|pid| RunningInfo {
            id: id.to_string(),
            pid,
            start_time: e.start_time,
        }))
    }
}

impl Default for ProcessManager {
    fn default() -> Self {
        Self::new()
    }
}
