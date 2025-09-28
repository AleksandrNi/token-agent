use anyhow::{Result, anyhow};
use chrono::Utc;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Token struct holding JWT and computed expiration
#[derive(Debug, Clone)]
pub struct Token {
    pub value: String,
    pub expires_at: i64, // UNIX timestamp
}

/// Parent-aware token cache: source_name -> (fields, token)
#[derive(Debug, Clone, Default)]
pub struct TokenCache {
    inner: Arc<RwLock<HashMap<String, Token>>>
}

impl TokenCache {
    pub fn new() -> Self {
        Self { inner: Arc::new(RwLock::new(HashMap::new())) }
    }

    /// Insert fields + token into cache
    pub async fn set(&self, source: &str, token: Token) {
        let mut map = self.inner.write().await;
        map.insert(source.to_string(), token);
    }

    /// Get token if it exists and is not expired
    pub async fn get(&self, source: &str) -> Option<Token> {
        let map = self.inner.read().await;
        map.get(source)
            .map(|token| token.clone())
            .filter(|t| Utc::now().timestamp() < t.expires_at)
    }

    /// Resolve a specific parent-aware field
    pub async fn resolve_field(&self, source: &str, parent: &str, field: &str) -> Result<String> {
        self.inner.read().await.get(source)
        .map(|t| t.value.to_owned())
        .ok_or_else(|| anyhow!("Source '{}' not found in cache", source))

        // if Utc::now().timestamp() >= token.expires_at {
        //     return Err(anyhow!("Token for source '{}' expired", source));
        // }

        // let key = format!("{}.{}", parent, field);
        // fields.get(&key)
        //     .cloned()
        //     .ok_or_else(|| anyhow!("Field '{}' not found in source '{}'", key, source))
    }
}

lazy_static::lazy_static! {
    pub static ref TOKEN_CACHE: TokenCache = TokenCache::new();
}
