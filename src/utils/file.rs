use serde::Serialize;
use std::fs;
use std::io::Write;
use std::path::Path;

use fs2::FileExt;

pub fn write_json_atomic<T: Serialize>(
    path: &Path,
    value: &T,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let parent = path.parent().ok_or("no parent")?;
    fs::create_dir_all(parent)?;
    let tmp = path.with_extension("tmp");
    let s = serde_json::to_string_pretty(value)?;
    let mut f = fs::File::create(&tmp)?;
    f.write_all(s.as_bytes())?;
    f.sync_all()?;
    fs::rename(&tmp, path)?;
    Ok(())
}

pub fn read_json<T: for<'de> serde::Deserialize<'de>>(
    path: &Path,
) -> Result<T, Box<dyn std::error::Error + Send + Sync>> {
    let s = fs::read_to_string(path)?;
    let v = serde_json::from_str(&s)?;
    Ok(v)
}

/// Open (and create if missing) a lock file alongside the target path and lock it.
fn open_lock_file(
    path: &Path,
    exclusive: bool,
) -> Result<fs::File, Box<dyn std::error::Error + Send + Sync>> {
    let lock_path = path.with_extension("lock");
    if let Some(parent) = lock_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let f = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .open(&lock_path)?;
    if exclusive {
        f.lock_exclusive()?;
    } else {
        f.lock_shared()?;
    }
    Ok(f)
}

pub fn read_json_with_lock<T: for<'de> serde::Deserialize<'de>>(
    path: &Path,
) -> Result<T, Box<dyn std::error::Error + Send + Sync>> {
    // Acquire shared lock on the lock file
    let _lock = open_lock_file(path, false)?;
    let s = fs::read_to_string(path)?;
    let v = serde_json::from_str(&s)?;
    Ok(v)
}

pub fn write_json_with_lock<T: Serialize>(
    path: &Path,
    value: &T,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Acquire exclusive lock on the lock file
    let _lock = open_lock_file(path, true)?;
    write_json_atomic(path, value)
}
