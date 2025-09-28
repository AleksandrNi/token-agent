use crate::cache::token_cache::{Token, TOKEN_CACHE};
use crate::config::types::SourceConfig;
use crate::parser::parser::parse_response;
use anyhow::{Result, anyhow};
use reqwest::Client;

#[derive(Debug, Clone)]
pub struct MetadataSource {
    pub name: String,
    pub cfg: SourceConfig,
    pub client: Client,
}

impl MetadataSource {
    pub fn new(name: String, cfg: SourceConfig) -> Self {
        let client = Client::builder().build().expect("Failed to build HTTP client");
        Self { name, cfg, client }
    }

    pub fn safety_margin(&self, _global_defaults: Option<&crate::config::types::SettingsConfig>) -> u64 {
        self.cfg.safety_margin_seconds.unwrap_or(5)
    }

    pub async fn fetch_token(&self) -> Result<Token> {
        let req_cfg = self.cfg.request.as_ref().ok_or_else(|| anyhow!("Missing request config"))?;
        let mut request = self.client.get(&req_cfg.url);

        if let Some(headers) = &req_cfg.headers {
            for (k, v) in headers {
                let val = match v {
                    crate::config::types::HeaderValue::Literal { value } => value.clone(),
                    crate::config::types::HeaderValue::FromEnv { from_env } => std::env::var(from_env)?,
                    crate::config::types::HeaderValue::FromFile { from_file } => std::fs::read_to_string(from_file)?.trim().to_string(),
                    crate::config::types::HeaderValue::Ref { source, field, prefix } => {
                        let fetched = TOKEN_CACHE.resolve_field(source, "body", field).await?;
                        format!("{}{}", prefix.clone().unwrap_or_default(), fetched)
                    }
                };
                request = request.header(k, val);
            }
        }

        let response = request.send().await?;
        if !response.status().is_success() {
            return Err(anyhow!("Metadata request failed: {}", response.status()));
        }

        let headers = response.headers().clone();
        let body = response.text().await?;
        parse_response(&self.name, self.cfg.parse.as_ref().unwrap(), &body, &headers).await
    }
}
