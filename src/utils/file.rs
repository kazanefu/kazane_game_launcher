use serde::Serialize;
use std::fs;
use std::io::Write;
use std::path::Path;

pub fn write_json_atomic<T: Serialize>(
    path: &Path,
    value: &T,
) -> Result<(), Box<dyn std::error::Error>> {
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
) -> Result<T, Box<dyn std::error::Error>> {
    let s = fs::read_to_string(path)?;
    let v = serde_json::from_str(&s)?;
    Ok(v)
}
