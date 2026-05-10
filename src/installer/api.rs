use crate::data::local::{InstalledGame, LocalGameData};
use crate::data::remote::provider::RemoteProvider;
use crate::data::remote::{GameList, GameListEntry};
use crate::installer::ProgressSender;
use crate::utils::file;
use std::path::{Path, PathBuf};
use tokio::sync::watch;

#[derive(Clone)]
pub struct LauncherApi<P> {
    provider: P,
    pub games_dir: PathBuf,
    pub game_data_path: PathBuf,
    pub game_list_path: PathBuf,
}

impl<P> LauncherApi<P>
where
    P: RemoteProvider + Send + Sync,
{
    pub fn new(
        provider: P,
        games_dir: PathBuf,
        game_data_path: PathBuf,
        game_list_path: PathBuf,
    ) -> Self {
        Self {
            provider,
            games_dir,
            game_data_path,
            game_list_path,
        }
    }

    fn find_game_entry(
        &self,
        id: &str,
    ) -> Result<GameListEntry, Box<dyn std::error::Error + Send + Sync>> {
        // Use locked read to avoid races with writers
        let gl: GameList = file::read_json_with_lock(&self.game_list_path)?;
        gl.games
            .into_iter()
            .find(|g| g.id == id)
            .ok_or_else(|| format!("game id not found: {}", id).into())
    }

    fn find_repo_by_local_id(
        &self,
        id: &str,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let gd: LocalGameData = file::read_json_with_lock(&self.game_data_path)?;
        gd.installed
            .into_iter()
            .find(|g| g.id == id)
            .map_or(Err(format!("game id not found: {}", id).into()), |g| {
                Ok(g.repo)
            })
    }

    fn parse_owner_repo(url: &str) -> Option<(String, String)> {
        url.strip_prefix("https://github.com/")
            .or_else(|| url.strip_prefix("http://github.com/"))
            .map(|s| s.trim_end_matches('/'))
            .and_then(|s| s.split_once('/'))
            .map(|(a, b)| (a.to_string(), b.to_string()))
    }

    /// Search games by query and optional tags. Returns matching GameListEntry items.
    pub fn search_games(
        &self,
        query: &str,
        tags: Option<&[&str]>,
    ) -> Result<Vec<GameListEntry>, Box<dyn std::error::Error + Send + Sync>> {
        let gl: GameList = file::read_json_with_lock(&self.game_list_path)?;
        Ok(gl.search(query, tags))
    }

    /// Install by game id (lookup repo from game_list.json). Progress/cancel may be passed from UI.
    pub async fn install_game_by_id_with(
        &self,
        id: &str,
        progress_tx: Option<ProgressSender>,
        cancel_rx: Option<watch::Receiver<bool>>,
    ) -> Result<InstalledGame, Box<dyn std::error::Error + Send + Sync>> {
        let entry = self.find_game_entry(id)?;
        if let Some((owner, repo)) = Self::parse_owner_repo(&entry.repo) {
            let installed = crate::installer::install::install_from_repo(
                &self.provider,
                &owner,
                &repo,
                &self.games_dir,
                &self.game_data_path,
                progress_tx,
                cancel_rx,
            )
            .await?;
            Ok(installed)
        } else {
            Err(format!("invalid repo url for {}", id).into())
        }
    }

    /// Simple install by game id without progress/cancel
    pub async fn install_game_by_id(
        &self,
        id: &str,
    ) -> Result<InstalledGame, Box<dyn std::error::Error + Send + Sync>> {
        self.install_game_by_id_with(id, None, None).await
    }

    /// Uninstall by id (repo name). Removes install dir and removes from local game_data.json
    fn try_remove_path(path: &Path) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if !path.exists() {
            return Ok(());
        }
        // clear readonly flags recursively then attempt removal
        if let Err(e) = file::clear_readonly_recursive(path) {
            // non-fatal: log via stderr and continue
            eprintln!("warning: failed clearing readonly flags: {}", e);
        }
        if path.is_file() {
            match std::fs::remove_file(path) {
                Ok(()) => Ok(()),
                Err(e) => Err(Box::new(e)),
            }
        } else if path.is_dir() {
            match std::fs::remove_dir_all(path) {
                Ok(()) => Ok(()),
                Err(e) => Err(Box::new(e)),
            }
        } else {
            Ok(())
        }
    }

    pub fn uninstall_game_by_id(
        &self,
        id: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // load local data
        let mut local = LocalGameData::load(&self.game_data_path)?;
        if let Some(pos) = local.installed.iter().position(|g| g.id == id) {
            let entry = local.installed.remove(pos);
            // Try to remove explicit exe_path if present
            if let Some(exe) = entry.exe_path.as_ref() {
                let pexe = Path::new(exe);
                if pexe.exists()
                    && let Err(e) = Self::try_remove_path(pexe)
                {
                    eprintln!("failed to remove exe_path {}: {}", pexe.display(), e);
                }
            }
            // remove install_path (may be dir or file)
            let p = Path::new(&entry.install_path);
            if p.exists()
                && let Err(e) = Self::try_remove_path(p)
            {
                eprintln!("failed to remove install_path {}: {}", p.display(), e);
            }
            // as a fallback, if install_path was a file inside a dir, try removing parent if empty
            if let Some(parent) = p.parent()
                && parent.exists()
            {
                // clear readonly on parent then try remove if empty
                let _ = file::clear_readonly_recursive(parent);
                let _ = std::fs::remove_dir(parent);
            }
            local.save_atomic(&self.game_data_path)?;
            Ok(())
        } else {
            Err(format!("not installed: {}", id).into())
        }
    }

    /// Update a game by id. If remote version differs from local, perform install.
    pub async fn update_game_by_id_with(
        &self,
        id: &str,
        progress_tx: Option<ProgressSender>,
        cancel_rx: Option<watch::Receiver<bool>>,
    ) -> Result<Option<InstalledGame>, Box<dyn std::error::Error + Send + Sync>> {
        let entry = self.find_game_entry(id)?;
        if let Some((owner, repo)) = Self::parse_owner_repo(&entry.repo) {
            // fetch remote release
            let release = self.provider.fetch_release(&owner, &repo).await?;
            // load local
            let local = LocalGameData::load(&self.game_data_path)?;
            // Find by game id (same id used by install/uninstall APIs)
            let local_entry = local.installed.iter().find(|g| g.id == id).cloned();
            let remote_version = release.latest.version.clone();
            let do_install = match local_entry {
                Some(ref le) => le.version != remote_version,
                None => true,
            };
            if do_install {
                let installed = crate::installer::install::install_from_repo(
                    &self.provider,
                    &owner,
                    &repo,
                    &self.games_dir,
                    &self.game_data_path,
                    progress_tx,
                    cancel_rx,
                )
                .await?;
                Ok(Some(installed))
            } else {
                Ok(None)
            }
        } else {
            Err(format!("invalid repo url for {}", id).into())
        }
    }

    /// Simple update by game id without progress/cancel
    pub async fn update_game_by_id(
        &self,
        id: &str,
    ) -> Result<Option<InstalledGame>, Box<dyn std::error::Error + Send + Sync>> {
        self.update_game_by_id_with(id, None, None).await
    }

    pub async fn get_readme_by_id(
        &self,
        id: &str,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let entry = self.find_game_entry(id)?;
        if let Some((owner, repo)) = Self::parse_owner_repo(&entry.repo) {
            self.provider.fetch_readme(&owner, &repo).await
        } else {
            Err(format!("invalid repo url for {}", id).into())
        }
    }

    pub async fn get_readme_by_local_id(
        &self,
        id: &str,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let url = self.find_repo_by_local_id(id)?;
        if let Some((owner, repo)) = Self::parse_owner_repo(&url) {
            self.provider.fetch_readme(&owner, &repo).await
        } else {
            Err(format!("invalid repo url for {}", id).into())
        }
    }
}
