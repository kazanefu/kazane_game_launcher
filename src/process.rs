use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use tokio::process::Child;
use tokio::sync::Mutex;

#[derive(Debug, Clone)]
pub struct RunningInfo {
    pub id: String,
    pub pid: u32,
    pub start_time: SystemTime,
}

#[derive(Clone)]
struct Entry {
    child: Arc<Mutex<Child>>,
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
    pub async fn start(
        &self,
        id: &str,
        install_path: PathBuf,
        exe_path: Option<PathBuf>,
        args: &[String],
    ) -> Result<RunningInfo, Box<dyn std::error::Error + Send + Sync>> {
        // Prevent duplicate starts
        let mut map = loop {
            let map = self.inner.lock().await;
            if let Some(entry) = map.get(id) {
                let child_arc = entry.child.clone();
                drop(map); // drop map lock before locking child to avoid deadlock with monitor
                let mut ch = child_arc.lock().await;
                if let Ok(None) = ch.try_wait() {
                    return Err(format!("already running: {}", id).into());
                }
                // Exited, remove it and continue checking (in case another thread started one in the gap)
                let mut map = self.inner.lock().await;
                map.remove(id);
                continue;
            }
            break map;
        };

        // Resolve executable path: use exe_path if it exists, otherwise search in install_path
        let resolved_exe_path = if let Some(ref p) = exe_path {
            if p.is_file() && p.exists() {
                p.clone()
            } else {
                #[cfg(debug_assertions)]
                println!("Debug: exe_path {:?} not found, searching in {:?}", p, install_path);
                resolve_executable_in_dir(install_path.clone())
                    .ok_or(format!("no executable found in {}", install_path.display()))?
            }
        } else {
            resolve_executable_in_dir(install_path.clone())
                .ok_or(format!("no executable found in {}", install_path.display()))?
        };

        // debug
        eprintln!(
            "ProcessManager::start - exe_path={:?} args={:?}",
            resolved_exe_path, args
        );

        // Build command. On Windows, try to spawn with a new console for .exe so console games show a window and keep running.
        #[cfg(windows)]
        let mut maybe_child: Option<tokio::process::Child> = None;

        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            // If an .exe is provided, prefer creating a new console so console games are visible
            if resolved_exe_path
                .extension()
                .and_then(|s| s.to_str())
                .map(|s| s.eq_ignore_ascii_case("exe"))
                .unwrap_or(false)
            {
                let mut std_cmd = std::process::Command::new(&resolved_exe_path);
                if !args.is_empty() {
                    std_cmd.args(args);
                }
                if let Some(parent) = resolved_exe_path.parent() {
                    std_cmd.current_dir(parent);
                }
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
                    let s = resolved_exe_path.to_string_lossy();
                    if !s.contains('\\') && !s.contains('/') {
                        use_prog_str = true;
                    }
                }
                let mut cmd = if use_prog_str {
                    let prog = resolved_exe_path
                        .file_name()
                        .and_then(|s| s.to_str())
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| resolved_exe_path.to_string_lossy().into_owned());
                    let mut c = tokio::process::Command::new("cmd");
                    let mut all_args = vec!["/C".to_string(), prog.clone()];
                    for a in args {
                        all_args.push(a.clone());
                    }
                    c.args(all_args);
                    c
                } else {
                    let mut c = tokio::process::Command::new(&resolved_exe_path);
                    if !args.is_empty() {
                        c.args(args);
                    }
                    if let Some(parent) = resolved_exe_path.parent() {
                        c.current_dir(parent);
                    }
                    c
                };
                cmd.spawn()?
            }
        } else {
            // non-windows
            let mut cmd = tokio::process::Command::new(&resolved_exe_path);
            if !args.is_empty() {
                cmd.args(args);
            }
            if let Some(parent) = resolved_exe_path.parent() {
                cmd.current_dir(parent);
            }
            cmd.spawn()?
        };

        let pid = child.id().unwrap_or(0);
        let start_time = SystemTime::now();

        // wrap child in shared Arc<Mutex<Child>> so monitor and stop can coordinate
        let child_arc = Arc::new(Mutex::new(child));

        // Insert into map
        map.insert(
            id.to_string(),
            Entry {
                child: child_arc.clone(),
                start_time,
            },
        );

        // spawn a monitor that periodically checks for exit and removes the entry when done
        let inner_clone = self.inner.clone();
        let id_string = id.to_string();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_millis(500)).await;
                // try_wait without holding the map lock so stop() can also acquire child
                let mut ch = child_arc.lock().await;
                match ch.try_wait() {
                    Ok(Some(_status)) => {
                        // exited - remove from map
                        let mut m = inner_clone.lock().await;
                        m.remove(&id_string);
                        break;
                    }
                    Ok(None) => {
                        // still running
                        drop(ch);
                        continue;
                    }
                    Err(e) => {
                        eprintln!("monitor try_wait error for {}: {}", id_string, e);
                        // on error, attempt to remove entry and exit
                        let mut m = inner_clone.lock().await;
                        m.remove(&id_string);
                        break;
                    }
                }
            }
        });

        Ok(RunningInfo {
            id: id.to_string(),
            pid,
            start_time,
        })
    }

    /// Try to stop the process gracefully, then force-kill if it doesn't exit.
    pub async fn stop(&self, id: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut map = self.inner.lock().await;
        if let Some(entry) = map.remove(id) {
            drop(map); // drop map lock before locking child to avoid deadlock with monitor
            let child_arc = entry.child;
            // First try graceful shutdown
            #[cfg(unix)]
            {
                use nix::sys::signal::{Signal, kill};
                use nix::unistd::Pid;
                let ch = child_arc.lock().await;
                if let Some(pid) = ch.id() {
                    let _ = kill(Pid::from_raw(pid as i32), Signal::TERM);
                }
                drop(ch);
            }

            #[cfg(windows)]
            {
                // no-op: we'll attempt a kill if graceful wait fails
            }

            // Wait a short time for exit, otherwise kill
            let mut ch = child_arc.lock().await;
            match tokio::time::timeout(Duration::from_secs(2), ch.wait()).await {
                Ok(Ok(_status)) => {
                    // exited
                    return Ok(());
                }
                _ => {
                    // try kill
                    let _ = ch.kill().await;
                    let _ = ch.wait().await;
                    return Ok(());
                }
            }
        }
        Err(format!("not running: {}", id).into())
    }

    /// Check if a tracked id is running
    pub async fn is_running(&self, id: &str) -> bool {
        // clone child arc to check without holding map lock while awaiting
        let child_arc_opt = {
            let map = self.inner.lock().await;
            map.get(id).map(|e| e.child.clone())
        };
        if let Some(child_arc) = child_arc_opt {
            let mut ch = child_arc.lock().await;
            match ch.try_wait() {
                Ok(None) => true,
                _ => {
                    // exited or error - remove from map
                    let mut map = self.inner.lock().await;
                    map.remove(id);
                    false
                }
            }
        } else {
            false
        }
    }

    /// Return running info if present
    pub async fn get_info(&self, id: &str) -> Option<RunningInfo> {
        let (child_arc_opt, start_time_opt) = {
            let map = self.inner.lock().await;
            map.get(id).map(|e| (e.child.clone(), e.start_time)).unzip()
        };
        if let Some(child_arc) = child_arc_opt {
            let ch = child_arc.lock().await;
            ch.id().map(|pid| RunningInfo {
                id: id.to_string(),
                pid,
                start_time: start_time_opt.unwrap_or(SystemTime::now()),
            })
        } else {
            None
        }
    }
}

/// Search for an executable inside a directory (exe on Windows, executable-bit on Unix)
fn resolve_executable_in_dir(dir: PathBuf) -> Option<PathBuf> {
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
