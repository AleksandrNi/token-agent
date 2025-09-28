use crate::cache::token_cache::{Token, TOKEN_CACHE};
use crate::config::types::{SourceConfig, BodyValue, SettingsConfig};
use crate::parser::parser::parse_response;
use anyhow::{Result, anyhow};
use reqwest::Client;

#[derive(Debug, Clone)]
pub struct OAuth2Source {
    pub name: String,
    pub cfg: SourceConfig,
    pub client: Client,
}

impl OAuth2Source {
    pub fn new(name: String, cfg: SourceConfig) -> Self {
        let client = Client::builder().build().expect("Failed to build HTTP client");
        Self { name, cfg, client }
    }

    pub fn safety_margin(&self, global_defaults: Option<&SettingsConfig>) -> u64 {
        self.cfg
            .safety_margin_seconds
            .or(global_defaults.and_then(|g| g.safety_margin_seconds))
            .unwrap_or(20)
    }

    pub async fn fetch_token(&self) -> Result<Token> {
        let req_cfg = self.cfg.request.as_ref().ok_or_else(|| anyhow!("Missing request config"))?;
        let mut form = std::collections::HashMap::new();

        if let Some(body) = &req_cfg.body {
            for (k, v) in body {
                let value = match v {
                    BodyValue::Literal { value } => value.clone(),
                    BodyValue::Ref { source, field } => TOKEN_CACHE.resolve_field(source, "body", field).await?,
                };
                form.insert(k.clone(), value);
            }
        }

        let response = self.client.post(&req_cfg.url).form(&form).send().await?;
        if !response.status().is_success() {
            return Err(anyhow!("OAuth2 token request failed: {}", response.status()));
        }

        let headers = response.headers().clone();
        let body = response.text().await?;
        parse_response(&self.name, self.cfg.parse.as_ref().unwrap(), &body, &headers).await
    }
}
