use crate::data::local::InstalledGame;
use crate::data::remote::{ReleaseAsset, ReleaseList};
use futures_util::StreamExt;
use reqwest::Client;
use sha2::{Digest, Sha256};
use std::error::Error;
use std::fs;
use std::io::Read;
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
    mut cancel_rx: Option<watch::Receiver<bool>>,
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
        .get(0)
        .ok_or(InstallError::Other("no assets in release".to_string()))?
        .clone();

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
                Some(p) => extract_dir.join(p.to_owned()),
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
            if let Some(rx) = cancel_rx {
                if *rx.borrow() {
                    return Err(InstallError::Cancelled);
                }
            }
        }
        if final_root.exists() {
            std::fs::remove_dir_all(&final_root).map_err(|e| InstallError::Other(e.to_string()))?;
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

    // Build InstalledGame struct
    let installed = InstalledGame {
        id: repo.to_string(),
        name: asset.name.clone(),
        version: release.latest.version.clone(),
        install_path: final_root.to_string_lossy().to_string(),
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
        .map_err(|e| InstallError::Other(e.to_string()))
        .map_err(|e| e)?;
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
        if let Some(rx) = cancel_rx {
            if *rx.borrow() {
                return Err(InstallError::Cancelled);
            }
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
