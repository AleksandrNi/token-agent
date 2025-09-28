use crate::{cache::token_cache::Token, config::types::{SettingsConfig, SourceConfig}};
use anyhow::Result;

pub mod http;
pub mod metadata;
pub mod oauth2;

use http::HttpSource;
use metadata::MetadataSource;
use oauth2::OAuth2Source;

#[derive(Debug, Clone)]
pub enum SourceKind {
    Http(HttpSource),
    Metadata(MetadataSource),
    OAuth2(OAuth2Source),
}

impl SourceKind {
    pub fn name(&self) -> &str {
        match self {
            SourceKind::Http(s) => &s.name,
            SourceKind::Metadata(s) => &s.name,
            SourceKind::OAuth2(s) => &s.name,
        }
    }

    pub fn safety_margin(&self, global_defaults: Option<&SettingsConfig>) -> u64 {
        match self {
            SourceKind::Http(s) => s.safety_margin(global_defaults),
            SourceKind::Metadata(s) => s.safety_margin(global_defaults),
            SourceKind::OAuth2(s) => s.safety_margin(global_defaults),
        }
    }

    pub async fn fetch_token(&self) -> Result<Token> {
        match self {
            SourceKind::Http(s) => s.fetch_token().await,
            SourceKind::Metadata(s) => s.fetch_token().await,
            SourceKind::OAuth2(s) => s.fetch_token().await,
        }
    }
}

pub fn build_source(name: &str, cfg: &SourceConfig) -> SourceKind {
    use crate::utils::constants::*;
    match cfg.source_type.as_str() {
        SOURCE_HTTP => SourceKind::Http(HttpSource::new(name.to_owned(), cfg.clone())),
        SOURCE_METADATA => SourceKind::Metadata(MetadataSource::new(name.to_owned(), cfg.clone())),
        SOURCE_OAUTH2 => SourceKind::OAuth2(OAuth2Source::new(name.to_owned(), cfg.clone())),
        t => panic!("Unsupported source type '{}' for source '{}'", t, name),
    }
}
