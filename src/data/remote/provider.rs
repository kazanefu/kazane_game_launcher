use crate::data::remote::{GameList, ReleaseList};
use async_trait::async_trait;
use base64::{Engine as _, engine::general_purpose};
use reqwest::Client;
use std::error::Error;
use std::time::Duration;
use tokio::time::sleep;

#[derive(Clone)]
pub struct GitHubRawProvider {
    client: Client,
    branch: String,
    retries: usize,
}

impl GitHubRawProvider {
    pub fn new(branch: Option<&str>) -> Self {
        Self {
            client: Client::new(),
            branch: branch.unwrap_or("main").to_string(),
            retries: 3,
        }
    }

    async fn get_json<T: for<'de> serde::Deserialize<'de>>(
        &self,
        owner: &str,
        repo: &str,
        path: &str,
    ) -> Result<T, Box<dyn Error>> {
        let raw_url = format!(
            "https://raw.githubusercontent.com/{}/{}/{}/{}",
            owner,
            repo,
            self.branch,
            path.trim_start_matches('/')
        );
        let api_url = format!(
            "https://api.github.com/repos/{}/{}/contents/{}?ref={}",
            owner,
            repo,
            path.trim_start_matches('/'),
            self.branch
        );
        let mut attempt = 0u32;
        loop {
            let res = self.client.get(&raw_url).send().await;
            match res {
                Ok(resp) => {
                    if resp.status().is_success() {
                        let txt = resp.text().await?;
                        if txt.trim().is_empty() {
                            // Fallback to GitHub API contents endpoint (handles private/encoded or empty-raw cases)
                            let api_resp = self
                                .client
                                .get(&api_url)
                                .header("User-Agent", "kazane-game-launcher")
                                .send()
                                .await?;
                            if api_resp.status().is_success() {
                                let api_txt = api_resp.text().await?;
                                let api_json: serde_json::Value = serde_json::from_str(&api_txt)?;
                                if let Some(encoded) =
                                    api_json.get("content").and_then(|v| v.as_str())
                                {
                                    let decoded = general_purpose::STANDARD
                                        .decode(encoded.replace('\n', ""))?;
                                    let s = String::from_utf8(decoded)?;
                                    let v = serde_json::from_str(&s)?;
                                    return Ok(v);
                                } else {
                                    return Err(
                                        format!("API returned no content for {}", api_url).into()
                                    );
                                }
                            } else {
                                return Err(format!(
                                    "API HTTP error {} fetching {}",
                                    api_resp.status(),
                                    api_url
                                )
                                .into());
                            }
                        }

                        let v = serde_json::from_str(&txt)?;
                        return Ok(v);
                    } else {
                        return Err(
                            format!("HTTP error {} fetching {}", resp.status(), raw_url).into()
                        );
                    }
                }
                Err(e) => {
                    attempt += 1;
                    if (attempt as usize) >= self.retries {
                        return Err(Box::new(e));
                    }
                    let backoff = Duration::from_millis(200 * 2u64.pow(attempt));
                    sleep(backoff).await;
                }
            }
        }
    }
}

#[async_trait]
pub trait RemoteProvider: Send + Sync + 'static {
    async fn fetch_game_list(&self, owner: &str, repo: &str) -> Result<GameList, Box<dyn Error>>;
    async fn fetch_release(&self, owner: &str, repo: &str) -> Result<ReleaseList, Box<dyn Error>>;
}

#[async_trait]
impl RemoteProvider for GitHubRawProvider {
    async fn fetch_game_list(&self, owner: &str, repo: &str) -> Result<GameList, Box<dyn Error>> {
        self.get_json(owner, repo, "data/game_list.json").await
    }

    async fn fetch_release(&self, owner: &str, repo: &str) -> Result<ReleaseList, Box<dyn Error>> {
        self.get_json(owner, repo, "launcher/release.json").await
    }
}
