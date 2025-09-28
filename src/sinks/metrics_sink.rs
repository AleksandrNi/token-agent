use crate::cache::token_cache::TOKEN_CACHE;
use prometheus::{IntGauge, register_int_gauge};
use anyhow::Result;

/// Prometheus metrics sink
#[derive(Debug)]
pub struct MetricsSink {
    pub token_expiry: IntGauge,
    pub source: String,
}

impl MetricsSink {
    pub fn new(source: &str) -> Result<Self> {
        let token_expiry = register_int_gauge!(
            format!("token_expiry_{}", source),
            format!("Expiration timestamp for token {}", source)
        )?;

        Ok(Self {
            token_expiry,
            source: source.to_string(),
        })
    }

    /// Update metric from token cache
    pub async fn update(&self) -> Result<()> {
        if let Some(token) = TOKEN_CACHE.get(&self.source).await {
            self.token_expiry.set(token.expires_at);
        }
        Ok(())
    }
}
