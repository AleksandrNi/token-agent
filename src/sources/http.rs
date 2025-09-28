use crate::cache::token_cache::{Token, TOKEN_CACHE};
use crate::config::types::{SourceConfig, HeaderValue, SettingsConfig};
use crate::parser::parser::parse_response;
use anyhow::{Result, anyhow};
use reqwest::Client;

#[derive(Debug, Clone)]
pub struct HttpSource {
    pub name: String,
    pub cfg: SourceConfig,
    pub client: Client,
}

impl HttpSource {
    pub fn new(name: String, cfg: SourceConfig) -> Self {
        let client = Client::builder()
            .build()
            .expect("Failed to build HTTP client");

        Self { name, cfg, client }
    }

    pub fn safety_margin(&self, global_defaults: Option<&SettingsConfig>) -> u64 {
        self.cfg
            .safety_margin_seconds
            .or(global_defaults.and_then(|g| g.safety_margin_seconds))
            .unwrap_or(10)
    }

    pub async fn fetch_token(&self) -> Result<Token> {
        let req_cfg = self.cfg.request.as_ref()
            .ok_or_else(|| anyhow!("Missing request config for HTTP source"))?;

        let mut request = self.client.request(req_cfg.method.parse()?, &req_cfg.url);

        // Build headers dynamically
        if let Some(headers) = &req_cfg.headers {
            for (k, v) in headers {
                let val = match v {
                    HeaderValue::Literal { value } => value.clone(),
                    HeaderValue::FromEnv { from_env } => std::env::var(from_env)?,
                    HeaderValue::FromFile { from_file } => std::fs::read_to_string(from_file)?.trim().to_string(),
                    HeaderValue::Ref { source, field, prefix } => {
                        let fetched = TOKEN_CACHE.resolve_field(source, "body", field).await?;
                        format!("{}{}", prefix.clone().unwrap_or_default(), fetched)
                    }
                };
                request = request.header(k, val);
            }
        }

        let response = request.send().await?;
        if !response.status().is_success() {
            return Err(anyhow!("HTTP request failed: {}", response.status()));
        }
        let headers = response.headers().clone();
        let body = response.text().await?;
        parse_response(&self.name, self.cfg.parse.as_ref().unwrap(), &body, &headers).await
    }
}
