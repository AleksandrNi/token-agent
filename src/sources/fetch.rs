/// Sources module
///
/// Defines all supported token sources and provides a factory to build them from config.

use anyhow::{anyhow, Error, Result};
use http::HeaderMap;
use reqwest::Client;
use std::collections::HashMap;
use std::sync::Arc;
use std::{env, fs};

use crate::cache::token_cache::TokenCache;
use crate::cache::token_context::TokenContext;
use crate::config::sources::{GenericSourceValue, SourceConfig};
use crate::parser::parser;

pub trait FetchTokens {
    fn fetch_tokens(
        &self,
        client: &Client,
        safety_margin_seconds_settings: Option<u64>    
    ) -> impl std::future::Future<Output = Result<Vec<TokenContext>, Error>> + Send;
}

#[derive(Debug, Clone)]
pub struct Source(pub Arc<SourceConfig>);


impl FetchTokens for Source {
    async fn fetch_tokens(&self, client: &Client, safety_margin_seconds_settings: Option<u64>) -> Result<Vec<TokenContext>, Error> {
        let source_config = &self.0;
        let req_cfg = &source_config.request.clone();

        let mut request = client.request(req_cfg.method.clone(), &req_cfg.url);

        // Build headers dynamically
        if let Some(headers) = &req_cfg.headers {
            for (key, v) in headers {
                let value = prepare_generic_source_value(v).await?;
                request = request.header(key, value)
            }
        }
        // Build body dynamically
        if let Some(source_body) = &req_cfg.body {
            let mut body = HashMap::new();
            for (k, v) in source_body {
                let value = prepare_generic_source_value(v).await?;
                body.insert(k.to_owned(), value);
            }
            request = request.json(&body);
        }

        let response = request.send().await?;
        if !response.status().is_success() {
            return Err(anyhow!("HTTP request failed: {}", response.status()));
        }
        let headers: HeaderMap = response.headers().clone();
        let body = response.text().await?;
        parser::parse_tokens(headers, body, source_config.parse.to_owned(), safety_margin_seconds_settings, source_config.safety_margin_seconds).await
    }
}


async fn prepare_generic_source_value(value: &GenericSourceValue) -> Result<String, anyhow::Error> {
    match value {
    GenericSourceValue::Literal { value } => Ok(value.to_owned()),
    GenericSourceValue::FromEnv { from_env } => env::var(from_env).map_err(|err| anyhow!(err)),
    GenericSourceValue::FromFile { path } => {
        fs::read_to_string(path)
        .map_err(|err| anyhow!(err))
        .map(|res| res.trim().to_string())
        
    }
    GenericSourceValue::Ref {
        source,
        id,
        prefix,
    } => TokenCache::get(&source, id.as_str())
        .await
        .map(|token_context| {
            prefix
                .as_ref()
                .map(|prefix| format!("{}{}", prefix, token_context.id))
                .unwrap_or(token_context.token.value)
        }).ok_or(anyhow!("token {}.{} is absent", source, id)),
    GenericSourceValue::Template { template, required } => {
        render_template(template.as_str(), required.to_owned()).await
    }
}
}


async fn render_template(template: &str, _: bool) -> Result<String, Error> {
    let mut result = template.to_string();
    let regex = regex::Regex::new(r"\{\{([a-zA-Z0-9_]+\.[a-zA-Z0-9_]+)\}\}")?;

    for caps in regex.captures_iter(template) {
        // e.g. metadata.metadata_token
        let key = caps.get(1).unwrap().as_str();
        let parts: Vec<&str> = key.split('.').collect();

        if parts.len() != 2 {
            continue;
        }
        let source = parts[0];
        let id = parts[1];
        let token_context = 
        TokenCache::get(source, id).await
        .ok_or_else(|| anyhow!("token for {}.{} is absent", source, id))?;
        result = result.replace(&format!("{{{{{}}}}}", key), &token_context.token.value);
    }

    Ok(result)
}