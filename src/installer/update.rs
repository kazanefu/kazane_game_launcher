use std::path::Path;

pub async fn update_from_release(
    _release_url: &str,
    _dest: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    // TODO: アップデートの差分/フル更新ロジックを実装
    Err(Box::new(std::io::Error::other("not implemented")))
}
