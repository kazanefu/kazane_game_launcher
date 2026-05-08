use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ReleaseAsset {
    pub name: String,
    pub url: String,
    pub r#type: String,
    pub sha256: Option<String>,
    pub entry_point: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ReleaseInfo {
    pub version: String,
    pub assets: Vec<ReleaseAsset>,
    pub channel: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ReleaseList {
    pub latest: ReleaseInfo,
}
