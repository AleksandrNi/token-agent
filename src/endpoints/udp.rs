use crate::cache::token_cache::TOKEN_CACHE;
use tokio::net::{UnixListener, UnixStream};
use tokio::io::{AsyncWriteExt};
use anyhow::{Result, anyhow};

/// UDS endpoint for serving a single source token
#[derive(Debug, Clone)]
pub struct UdsEndpoint {
    pub path: String,
    pub source: String,
}

impl UdsEndpoint {
    pub fn new(path: &str, source: &str) -> Self {
        Self {
            path: path.to_string(),
            source: source.to_string(),
        }
    }

    /// Start listening on the UDS path
    pub async fn run(&self) -> Result<()> {
        // Remove existing socket if present
        let _ = std::fs::remove_file(&self.path);

        let listener = UnixListener::bind(&self.path)?;
        loop {
            let (mut stream, _) = listener.accept().await?;
            let source = self.source.clone();
            tokio::spawn(async move {
                if let Err(e) = handle_connection(&mut stream, &source).await {
                    eprintln!("UDS connection error: {}", e);
                }
            });
        }
    }
}

/// Handle single connection: write token
async fn handle_connection(stream: &mut UnixStream, source: &str) -> Result<()> {
    if let Some(token) = TOKEN_CACHE.get(source).await {
        stream.write_all(token.value.as_bytes()).await?;
        stream.shutdown().await?;
    } else {
        return Err(anyhow!("Token for source '{}' not found", source));
    }
    Ok(())
}
