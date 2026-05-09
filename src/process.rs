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

        // Resolve executable path if a directory was passed
        let exe_path = if exe.is_dir() {
            resolve_executable_in_dir(&exe).ok_or(format!("no executable found in {}", exe.display()))?
        } else {
            exe
        };

        // debug
        eprintln!("ProcessManager::start - exe_path={:?} args={:?}", exe_path, args);

        // Build command. On Windows, try to spawn with a new console for .exe so console games show a window and keep running.
        #[cfg(windows)]
        let mut maybe_child: Option<tokio::process::Child> = None;

        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            // If an .exe is provided, prefer creating a new console so console games are visible
            if exe_path.extension().and_then(|s| s.to_str()).map(|s| s.eq_ignore_ascii_case("exe")).unwrap_or(false) {
                let mut std_cmd = std::process::Command::new(&exe_path);
                if !args.is_empty() { std_cmd.args(args); }
                if let Some(parent) = exe_path.parent() { std_cmd.current_dir(parent); }
                // CREATE_NEW_CONSOLE = 0x00000010
                std_cmd.creation_flags(0x00000010);
                match tokio::process::Command::from(std_cmd).spawn() {
                    Ok(c) => {
                        maybe_child = Some(c);
                    }
                    Err(e) => {
                        eprintln!("failed to spawn with CREATE_NEW_CONSOLE: {}", e);
                    }
                }
            }
        }

        // If Windows spawn with new console didn't work or not on Windows, fall back to building a normal command
        let child = if cfg!(windows) {
            if let Some(c) = maybe_child {
                c
            } else {
                // fallback: if bare program name, use cmd /C to resolve PATH, else spawn directly
                let mut use_prog_str = false;
                #[cfg(windows)]
                {
                    let s = exe_path.to_string_lossy();
                    if !s.contains('\\') && !s.contains('/') {
                        use_prog_str = true;
                    }
                }
                let mut cmd = if use_prog_str {
                    let prog = exe_path.file_name()
                        .and_then(|s| s.to_str())
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| exe_path.to_string_lossy().into_owned());
                    let mut c = tokio::process::Command::new("cmd");
                    let mut all_args = vec!["/C".to_string(), prog.clone()];
                    for a in args { all_args.push(a.clone()); }
                    c.args(all_args);
                    c
                } else {
                    let mut c = tokio::process::Command::new(&exe_path);
                    if !args.is_empty() { c.args(args); }
                    if let Some(parent) = exe_path.parent() { c.current_dir(parent); }
                    c
                };
                cmd.spawn()?
            }
        } else {
            // non-windows
            let mut cmd = tokio::process::Command::new(&exe_path);
            if !args.is_empty() { cmd.args(args); }
            if let Some(parent) = exe_path.parent() { cmd.current_dir(parent); }
            cmd.spawn()?
        };


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

/// Search for an executable inside a directory (exe on Windows, executable-bit on Unix)
fn resolve_executable_in_dir(dir: &PathBuf) -> Option<PathBuf> {
    let mut stack = vec![dir.clone()];
    while let Some(p) = stack.pop() {
        if let Ok(rd) = std::fs::read_dir(&p) {
            for e in rd.flatten() {
                let pth = e.path();
                if pth.is_dir() {
                    stack.push(pth);
                } else if let Some(ext) = pth.extension().and_then(|s| s.to_str()) {
                    if ext.eq_ignore_ascii_case("exe") {
                        return Some(pth);
                    }
                } else {
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::PermissionsExt;
                        if let Ok(md) = pth.metadata() {
                            if md.permissions().mode() & 0o111 != 0 {
                                return Some(pth);
                            }
                        }
                    }
                }
            }
        }
    }
    None
}

impl Default for ProcessManager {
    fn default() -> Self {
        Self::new()
    }
}
