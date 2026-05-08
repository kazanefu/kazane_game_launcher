use async_trait::async_trait;
use crate::data::remote::{GameList, ReleaseList};
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

    async fn get_json<T: for<'de> serde::Deserialize<'de>>(&self, owner: &str, repo: &str, path: &str) -> Result<T, Box<dyn Error>> {
        let url = format!("https://raw.githubusercontent.com/{}/{}/{}/{}", owner, repo, self.branch, path.trim_start_matches('/'));
        let mut attempt = 0u32;
        loop {
            let res = self.client.get(&url).send().await;
            match res {
                Ok(resp) => {
                    if resp.status().is_success() {
                        let txt = resp.text().await?;
                        let v = serde_json::from_str(&txt)?;
                        return Ok(v);
                    } else {
                        return Err(format!("HTTP error {} fetching {}", resp.status(), url).into());
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
