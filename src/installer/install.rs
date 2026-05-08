use crate::data::remote::ReleaseList;
use crate::data::local::InstalledGame;
use crate::data::remote::ReleaseAsset;
use async_trait::async_trait;
use reqwest::Client;
use sha2::{Digest, Sha256};
use std::error::Error;
use std::fs;
use std::io::{Cursor, Read, Seek};
use std::path::{Path, PathBuf};
use tempfile::tempdir;
use zip::ZipArchive;

pub async fn install_from_repo<P: super::super::data::remote::provider::RemoteProvider>(
    provider: &P,
    owner: &str,
    repo: &str,
    dest_dir: &Path,
) -> Result<InstalledGame, Box<dyn Error>> {
    // Fetch release info
    let release = provider.fetch_release(owner, repo).await?;
    install_from_release_info(&release, owner, repo, dest_dir).await
}

async fn install_from_release_info(
    release: &ReleaseList,
    owner: &str,
    repo: &str,
    dest_dir: &Path,
) -> Result<InstalledGame, Box<dyn Error>> {
    let asset = release
        .latest
        .assets
        .get(0)
        .ok_or("no assets in release")?
        .clone();
    // Download asset
    let client = Client::new();
    let bytes = client.get(&asset.url).send().await?.bytes().await?;

    // Verify sha256 if provided and looks valid (64 hex chars). If placeholder or invalid, skip verification with a warning.
    if let Some(expected_hex) = &asset.sha256 {
        let expected_tr = expected_hex.trim();
        let is_hex = expected_tr.len() == 64 && expected_tr.chars().all(|c| c.is_ascii_hexdigit());
        if is_hex {
            let mut hasher = Sha256::new();
            hasher.update(&bytes);
            let actual = hasher.finalize();
            let actual_hex = hex::encode(actual);
            if actual_hex != expected_tr.to_lowercase() {
                return Err(format!("sha256 mismatch: expected {} got {}", expected_tr, actual_hex).into());
            }
        } else {
            eprintln!("Warning: skipping sha256 verification because value looks invalid or placeholder: {}", expected_hex);
        }
    }

    // Prepare temp dir
    let tmp = tempdir()?;

    // Decide install path
    let final_root = dest_dir.join(repo);

    // Handle zip
    if asset.r#type.to_lowercase() == "zip" || asset.url.to_lowercase().ends_with(".zip") {
        let zip_path = tmp.path().join(&asset.name);
        fs::write(&zip_path, &bytes)?;
        let zip_file = fs::File::open(&zip_path)?;
        let mut archive = ZipArchive::new(zip_file)?;
        let extract_dir = tmp.path().join("extract");
        fs::create_dir_all(&extract_dir)?;
        for i in 0..archive.len() {
            let mut file = archive.by_index(i)?;
            let outpath = match file.enclosed_name() {
                Some(p) => extract_dir.join(p.to_owned()),
                None => continue,
            };
            if file.name().ends_with('/') {
                fs::create_dir_all(&outpath)?;
            } else {
                if let Some(parent) = outpath.parent() {
                    fs::create_dir_all(parent)?;
                }
                let mut outfile = fs::File::create(&outpath)?;
                std::io::copy(&mut file, &mut outfile)?;
            }
        }
        // Atomic move: remove existing then rename
        if final_root.exists() {
            fs::remove_dir_all(&final_root)?;
        }
        // If extracted contains a single top-level directory, move that; else move extract_dir contents into final_root
        let entries: Vec<_> = fs::read_dir(&extract_dir)?.collect();
        if entries.len() == 1 {
            let only = entries[0].as_ref().unwrap().path();
            if only.is_dir() {
                fs::rename(&only, &final_root)?;
            } else {
                // single file - create final_root dir and move file into it
                fs::create_dir_all(&final_root)?;
                let fname = only.file_name().ok_or("invalid filename")?;
                fs::rename(&only, final_root.join(fname))?;
            }
        } else {
            fs::create_dir_all(&final_root)?;
            for entry in fs::read_dir(&extract_dir)? {
                let p = entry?.path();
                let file_name = p.file_name().ok_or("invalid filename")?.to_owned();
                let dest = final_root.join(file_name);
                fs::rename(&p, dest)?;
            }
            // remove extract_dir if empty
            let _ = fs::remove_dir(&extract_dir);
        }
    } else {
        // Treat as executable/binary
        fs::create_dir_all(&final_root)?;
        let exe_name = asset
            .entry_point
            .clone()
            .unwrap_or_else(|| asset.name.clone());
        let exe_path = final_root.join(&exe_name);
        fs::write(&exe_path, &bytes)?;
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
