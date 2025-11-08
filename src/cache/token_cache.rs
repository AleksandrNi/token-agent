use anyhow::Result;
use tracing::{debug};
use std::collections::HashMap;
use tokio::sync::{OnceCell, RwLock};

use crate::{cache::token_context::TokenContext, observability::metrics::get_metrics};


// Declare the static OnceCell to hold the TokenCache.
static TOKEN_CACHE_INSTANCE: OnceCell<TokenCache> = OnceCell::const_new();

/// Asynchronously initializes and gets a reference to the static `TokenCache`.
async fn get_token_cache() -> &'static TokenCache {
    TOKEN_CACHE_INSTANCE.get_or_init(|| async {
        debug!("Initializing static TokenCache...");
        TokenCache::new()
    }).await
}



/// Token cache: source_name -> token_id -> TokenContext
#[derive(Debug, Default)]
pub struct TokenCache {
    inner: RwLock<HashMap<String, HashMap<String, TokenContext>>>,
}

impl TokenCache {
    pub fn new() -> Self {
        Self {
            inner: RwLock::new(HashMap::new()),
        }
    }

    /// Insert or update a token
    pub async fn set(source_id: String, source_token_contexts: Vec<TokenContext>) -> Result<Vec<String>> {
        let mut guard = get_token_cache().await.inner.write().await;
        let source_map = guard.entry(source_id.to_owned()).or_default();
        
        let mut updated_tokens: Vec<String> = Vec::new();
        
        source_token_contexts.into_iter()

            .for_each(|token_context| {
                updated_tokens.push( token_context.id.to_owned());

                match source_map.get_mut(&token_context.id){
                    Some(existing_token_context) => {                
                        *existing_token_context = token_context;
                    },
                    None => {
                        debug!("inserted token token_id {} exp {} fetched_at_unix_ts: {}", &token_context.id, &token_context.token.exp_unix_ts, &token_context.fetched_at_unix_ts);
                        source_map.insert(token_context.id.to_owned(), token_context);
                    },
                }
        });
        get_metrics().await.cached_tokens.with_label_values(&[&source_id.as_str()]).set(source_map.values().len() as i64);
        Ok(updated_tokens)
    }

    /// Get token by source_id and token_id
    pub async fn get(source_id: &str, token_id: &str) -> Option<TokenContext> {
        let guard = get_token_cache().await.inner.read().await;
        guard.get(source_id).and_then(|m| m.get(token_id).cloned())
    }

    /// Check if source_id exists
    pub async fn contains_source_id(source_id: &str) -> bool {
        let guard = get_token_cache().await.inner.read().await;
        guard.contains_key(source_id)
    }

    /// Invalidate token by source_id
    pub async fn invalidate_expired_tokens_by_source_id(source_id: &str) -> bool {
        let mut guard = get_token_cache().await.inner.write().await;
        if !guard.contains_key(source_id) {
            return false;
        }
        let source_map = guard.get_mut(source_id).unwrap();
        source_map.retain(|_, token_context| !token_context.should_remove());
        true
    }
    
    pub async fn process_metrics() -> () {
        let metrics = get_metrics().await;
        let guard = get_token_cache().await.inner.read().await;
        guard.iter().for_each(|(source_id, source_map)| {
            metrics.cached_tokens.with_label_values(&[&source_id.as_str()]).set(source_map.values().len() as i64);
            source_map.iter().for_each(|(token_id, token_context)|{
                metrics.token_expiry_unix.with_label_values(&[&source_id.as_str(), &token_id.as_str()])
                .set(token_context.token.exp_unix_ts as i64);
            });
        });        
    }

    pub async fn cleanup() -> () {
        let mut guard = get_token_cache().await.inner.write().await;
        guard.clear();
    }

    pub async fn println() -> () {
        let guard = get_token_cache().await.inner.read().await;
        debug!("token cache: {:?}", guard);
    }

}
