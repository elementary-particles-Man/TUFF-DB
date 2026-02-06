use crate::models::{Evidence, RequiredFact, SourceMeta};
use crate::pipeline::traits::FactFetcher;
use async_trait::async_trait;
use chrono::Utc;
use reqwest::Client;
use sha2::{Digest, Sha256};
use std::env;
use url::Url;

pub struct WebFetcher {
    client: Client,
}

impl WebFetcher {
    pub fn new() -> Self {
        Self {
            client: Client::builder()
                .user_agent("TUFF-DB/0.1")
                .build()
                .expect("reqwest client"),
        }
    }

    fn target_url() -> anyhow::Result<Url> {
        let raw = env::var("TARGET_URL")
            .unwrap_or_else(|_| "https://www.kantei.go.jp/jp/rekidai/index.html".to_string());
        Ok(Url::parse(&raw)?)
    }
}

#[async_trait]
impl FactFetcher for WebFetcher {
    async fn fetch(&self, _fragment: &str) -> anyhow::Result<Vec<RequiredFact>> {
        let url = Self::target_url()?;
        let body = self.client.get(url.clone()).send().await?.text().await?;
        let mut hasher = Sha256::new();
        hasher.update(body.as_bytes());
        let sha256_hex = format!("{:x}", hasher.finalize());

        let source = SourceMeta {
            url: url.clone(),
            retrieved_at_rfc3339: Utc::now().to_rfc3339(),
            sha256_hex,
        };

        let evidence = Evidence {
            source,
            snippet: body.chars().take(1200).collect(),
        };

        Ok(vec![RequiredFact {
            key: "target_url".to_string(),
            value: url.to_string(),
            evidence: vec![evidence],
        }])
    }
}
