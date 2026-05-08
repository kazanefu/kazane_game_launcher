use std::path::Path;

pub async fn install_from_release(_release_url: &str, _dest: &Path) -> Result<(), Box<dyn std::error::Error>> {
    // TODO: 実際のダウンロード・検証・解凍の実装
    Err(Box::new(std::io::Error::new(std::io::ErrorKind::Other, "not implemented")))
}
