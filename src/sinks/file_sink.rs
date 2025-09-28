use crate::cache::token_cache::TOKEN_CACHE;
use crate::config::types::SinkConfig;
use crate::sources::SourceKind;
use anyhow::{Result, anyhow};
use tokio::fs;
use tokio::sync::watch;
use std::path::PathBuf;
use std::time::Duration;
use tokio::time::sleep;

/// File sink for token values
#[derive(Debug, Clone)]
pub struct FileSink {
    pub name: String,
    pub path: PathBuf,
    pub source: String,
    pub watch: bool,
}

impl FileSink {
    pub fn new(name: String, cfg: &SinkConfig, source: String) -> Result<Self> {
        let path = cfg.path.as_ref().ok_or_else(|| anyhow!("File sink missing path"))?;
        Ok(Self {
            name,
            path: PathBuf::from(path),
            source,
            watch: cfg.watch.unwrap_or(false),
        })
    }

    /// Write token to file
    pub async fn write(&self) -> Result<()> {
        let token = TOKEN_CACHE.get(&self.source).await
            .ok_or_else(|| anyhow!("Token for source '{}' not found", self.source))?;

        fs::write(&self.path, token.value).await?;
        Ok(())
    }

    /// Optional watch loop to refresh file when token changes
    pub async fn watch_loop(&self, mut rx: watch::Receiver<()>) -> Result<()> {
        while rx.changed().await.is_ok() {
            self.write().await?;
        }
        Ok(())
    }
}
