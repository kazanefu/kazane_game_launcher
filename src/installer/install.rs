use crate::data::local::InstalledGame;
use crate::data::remote::{ReleaseAsset, ReleaseList};
use reqwest::Client;
use sha2::{Digest, Sha256};
use std::error::Error;
use std::fs;
use std::path::Path;
use tempfile::tempdir;
use tokio::sync::{mpsc, watch};
use zip::ZipArchive;

#[derive(Debug, Clone)]
pub enum InstallStage {
    Downloading,
    Verifying,
    Extracting,
    Installing,
    UpdatingLocalData,
    Done,
}

#[derive(Debug, Clone)]
pub struct Progress {
    pub stage: InstallStage,
    pub downloaded: u64,
    pub total: Option<u64>,
}

#[derive(Debug)]
pub enum InstallError {
    Cancelled,
    Other(String),
}

impl From<InstallError> for Box<dyn Error + Send + Sync> {
    fn from(e: InstallError) -> Box<dyn Error + Send + Sync> {
        match e {
            InstallError::Cancelled => "install cancelled".into(),
            InstallError::Other(s) => s.into(),
        }
    }
}

pub type ProgressSender = mpsc::Sender<Progress>;

pub async fn install_from_repo<
    P: super::super::data::remote::provider::RemoteProvider + Send + Sync,
>(
    provider: &P,
    owner: &str,
    repo: &str,
    dest_dir: &Path,
    game_data_path: &Path,
    mut progress_tx: Option<ProgressSender>,
    cancel_rx: Option<watch::Receiver<bool>>,
) -> Result<InstalledGame, Box<dyn Error + Send + Sync>> {
    let release = provider.fetch_release(owner, repo).await?;
    let installed = install_from_release_info(
        &release,
        owner,
        repo,
        dest_dir,
        progress_tx.as_ref(),
        cancel_rx.as_ref(),
    )
    .await
    .map_err(|e| match e {
        InstallError::Cancelled => Box::<dyn Error + Send + Sync>::from("cancelled"),
        InstallError::Other(s) => Box::<dyn Error + Send + Sync>::from(s),
    })?;

    // update local/game_data.json
    if let Some(parent) = game_data_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut local_data = crate::data::local::LocalGameData::load(game_data_path)?;
    local_data.add_or_update(installed.clone());
    if let Some(tx) = progress_tx.as_mut() {
        let _ = tx
            .send(Progress {
                stage: InstallStage::UpdatingLocalData,
                downloaded: 0,
                total: None,
            })
            .await;
    }
    local_data.save_atomic(game_data_path)?;
    if let Some(tx) = progress_tx.as_mut() {
        let _ = tx
            .send(Progress {
                stage: InstallStage::Done,
                downloaded: 0,
                total: None,
            })
            .await;
    }

    Ok(installed)
}

async fn install_from_release_info(
    release: &ReleaseList,
    owner: &str,
    repo: &str,
    dest_dir: &Path,
    progress_tx: Option<&ProgressSender>,
    cancel_rx: Option<&watch::Receiver<bool>>,
) -> Result<InstalledGame, InstallError> {
    let asset = release
        .latest
        .assets
        .first()
        .cloned()
        .ok_or(InstallError::Other("no assets in release".to_string()))?;

    // Download asset with progress & cancellation
    if let Some(tx) = progress_tx {
        let _ = tx.clone().try_send(Progress {
            stage: InstallStage::Downloading,
            downloaded: 0,
            total: None,
        });
    }
    let bytes = download_asset_with_progress(&asset, progress_tx, cancel_rx).await?;

    // Verify sha
    if let Some(expected_hex) = &asset.sha256 {
        if let Some(tx) = progress_tx {
            let _ = tx.clone().try_send(Progress {
                stage: InstallStage::Verifying,
                downloaded: 0,
                total: None,
            });
        }
        let expected_tr = expected_hex.trim();
        let is_hex = expected_tr.len() == 64 && expected_tr.chars().all(|c| c.is_ascii_hexdigit());
        if is_hex {
            let mut hasher = Sha256::new();
            hasher.update(&bytes);
            let actual = hasher.finalize();
            let actual_hex = hex::encode(actual);
            if actual_hex != expected_tr.to_lowercase() {
                return Err(InstallError::Other(format!(
                    "sha256 mismatch: expected {} got {}",
                    expected_tr, actual_hex
                )));
            }
        } else {
            eprintln!(
                "Warning: skipping sha256 verification because value looks invalid or placeholder: {}",
                expected_hex
            );
        }
    }

    // Prepare temp dir
    let tmp = tempdir().map_err(|e| InstallError::Other(e.to_string()))?;

    // Decide install path
    let final_root = dest_dir.join(repo);

    // Handle zip
    if asset.r#type.to_lowercase() == "zip" || asset.url.to_lowercase().ends_with(".zip") {
        if let Some(tx) = progress_tx {
            let _ = tx.clone().try_send(Progress {
                stage: InstallStage::Extracting,
                downloaded: 0,
                total: None,
            });
        }
        let zip_path = tmp.path().join(&asset.name);
        std::fs::write(&zip_path, &bytes).map_err(|e| InstallError::Other(e.to_string()))?;
        let zip_file =
            std::fs::File::open(&zip_path).map_err(|e| InstallError::Other(e.to_string()))?;
        let mut archive =
            ZipArchive::new(zip_file).map_err(|e| InstallError::Other(e.to_string()))?;
        let extract_dir = tmp.path().join("extract");
        std::fs::create_dir_all(&extract_dir).map_err(|e| InstallError::Other(e.to_string()))?;
        for i in 0..archive.len() {
            let mut file = archive
                .by_index(i)
                .map_err(|e| InstallError::Other(e.to_string()))?;
            let outpath = match file.enclosed_name() {
                Some(p) => extract_dir.join(p),
                None => continue,
            };
            if file.name().ends_with('/') {
                std::fs::create_dir_all(&outpath)
                    .map_err(|e| InstallError::Other(e.to_string()))?;
            } else {
                if let Some(parent) = outpath.parent() {
                    std::fs::create_dir_all(parent)
                        .map_err(|e| InstallError::Other(e.to_string()))?;
                }
                let mut outfile = std::fs::File::create(&outpath)
                    .map_err(|e| InstallError::Other(e.to_string()))?;
                std::io::copy(&mut file, &mut outfile)
                    .map_err(|e| InstallError::Other(e.to_string()))?;
            }
            // check cancellation
            if let Some(rx) = cancel_rx
                && *rx.borrow()
            {
                return Err(InstallError::Cancelled);
            }
        }
        if final_root.exists() {
            // attempt to remove readonly flags then remove; if that fails, try to rename old dir out of the way
            if let Err(e) = crate::utils::file::clear_readonly_recursive(&final_root) {
                eprintln!(
                    "warning: failed to clear readonly on {}: {}",
                    final_root.display(),
                    e
                );
            }
            if let Err(e) = std::fs::remove_dir_all(&final_root) {
                eprintln!(
                    "warning: failed to remove existing dir {}: {}",
                    final_root.display(),
                    e
                );
                // try to rename old dir as fallback
                let ts = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                let backup = final_root.with_extension(format!("old-{}", ts));
                if let Err(_e2) = std::fs::rename(&final_root, &backup) {
                    return Err(InstallError::Other(format!(
                        "failed to remove or rename existing install dir {}: {}",
                        final_root.display(),
                        e
                    )));
                } else {
                    eprintln!(
                        "renamed existing {} -> {}",
                        final_root.display(),
                        backup.display()
                    );
                }
            }
        }
        let entries: Vec<_> = std::fs::read_dir(&extract_dir)
            .map_err(|e| InstallError::Other(e.to_string()))?
            .collect();
        if entries.len() == 1 {
            let only = entries[0].as_ref().unwrap().path();
            if only.is_dir() {
                std::fs::rename(&only, &final_root)
                    .map_err(|e| InstallError::Other(e.to_string()))?;
            } else {
                std::fs::create_dir_all(&final_root)
                    .map_err(|e| InstallError::Other(e.to_string()))?;
                let fname = only
                    .file_name()
                    .ok_or(InstallError::Other("invalid filename".to_string()))?;
                std::fs::rename(&only, final_root.join(fname))
                    .map_err(|e| InstallError::Other(e.to_string()))?;
            }
        } else {
            std::fs::create_dir_all(&final_root).map_err(|e| InstallError::Other(e.to_string()))?;
            for entry in
                std::fs::read_dir(&extract_dir).map_err(|e| InstallError::Other(e.to_string()))?
            {
                let p = entry
                    .map_err(|e| InstallError::Other(e.to_string()))?
                    .path();
                let file_name = p
                    .file_name()
                    .ok_or(InstallError::Other("invalid filename".to_string()))?
                    .to_owned();
                let dest = final_root.join(file_name);
                std::fs::rename(&p, dest).map_err(|e| InstallError::Other(e.to_string()))?;
            }
            let _ = std::fs::remove_dir(&extract_dir);
        }
    } else {
        if let Some(tx) = progress_tx {
            let _ = tx.clone().try_send(Progress {
                stage: InstallStage::Installing,
                downloaded: 0,
                total: None,
            });
        }
        std::fs::create_dir_all(&final_root).map_err(|e| InstallError::Other(e.to_string()))?;
        let exe_name = asset
            .entry_point
            .clone()
            .unwrap_or_else(|| asset.name.clone());
        let exe_path = final_root.join(&exe_name);
        std::fs::write(&exe_path, &bytes).map_err(|e| InstallError::Other(e.to_string()))?;
    }

    // Try to find an executable inside final_root (or use final_root directly)
    fn find_executable_in(path: &std::path::Path) -> Option<std::path::PathBuf> {
        if path.is_file() {
            return Some(path.to_path_buf());
        }
        // search recursively for obvious executable files
        let mut stack = vec![path.to_path_buf()];
        while let Some(p) = stack.pop() {
            if let Ok(rd) = std::fs::read_dir(&p) {
                for e in rd.flatten() {
                    let pth = e.path();
                    if pth.is_dir() {
                        stack.push(pth);
                    } else if let Some(ext) = pth.extension().and_then(|s| s.to_str()) {
                        // Windows common
                        if ext.eq_ignore_ascii_case("exe") {
                            return Some(pth);
                        }
                    } else {
                        // on unix, check executable bit
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

    let exe_path_from_asset = asset.entry_point.as_ref().map(|ep| final_root.join(ep));
    let exe_candidate = if let Some(p) = exe_path_from_asset.as_ref().filter(|p| p.exists()) {
        Some(p.clone())
    } else {
        #[cfg(debug_assertions)]
        if let Some(p) = &exe_path_from_asset {
            println!("Debug: Specified entry_point {:?} not found, searching...", p);
        }
        find_executable_in(&final_root)
    };

    let (install_path_str, exe_path_opt) = if let Some(p) = exe_candidate {
        // install_path is the directory, exe_path is the executable
        let exe_path = p.to_string_lossy().to_string();
        let dir = final_root.to_string_lossy().to_string();
        (dir, Some(exe_path))
    } else {
        (final_root.to_string_lossy().to_string(), None)
    };

    // Build InstalledGame struct
    let installed = InstalledGame {
        id: repo.to_string(),
        name: asset.name.clone(),
        version: release.latest.version.clone(),
        install_path: install_path_str,
        exe_path: exe_path_opt,
        repo: format!("https://github.com/{}/{}", owner, repo),
        installed: true,
        last_checked: None,
    };

    Ok(installed)
}

async fn download_asset_with_progress(
    asset: &ReleaseAsset,
    progress_tx: Option<&ProgressSender>,
    cancel_rx: Option<&watch::Receiver<bool>>,
) -> Result<Vec<u8>, InstallError> {
    let client = Client::new();
    let resp = client
        .get(&asset.url)
        .send()
        .await
        .map_err(|e| InstallError::Other(e.to_string()))?;
    if !resp.status().is_success() {
        return Err(InstallError::Other(format!(
            "HTTP error {} fetching {}",
            resp.status(),
            asset.url
        )));
    }
    let total = resp.content_length();
    let mut stream = resp.bytes_stream();
    use futures_util::StreamExt;
    let mut downloaded: u64 = 0;
    let mut buf: Vec<u8> = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| InstallError::Other(e.to_string()))?;
        // cancellation check
        if let Some(rx) = cancel_rx
            && *rx.borrow()
        {
            return Err(InstallError::Cancelled);
        }
        downloaded += chunk.len() as u64;
        buf.extend_from_slice(&chunk);
        if let Some(tx) = progress_tx {
            let _ = tx.clone().try_send(Progress {
                stage: InstallStage::Downloading,
                downloaded,
                total,
            });
        }
    }
    Ok(buf)
}
