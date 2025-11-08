use std::{collections::HashMap, sync::Arc};

use tokio::sync::{OnceCell, RwLock};
use tracing::info;

// Declare the static OnceCell to hold the TokenCache.
static SINK_UDS_CACHE_INSTANCE: OnceCell<SinkUdsCache> = OnceCell::const_new();

/// Asynchronously initializes and gets a reference to the static `TokenCache`.
async fn get_sink_uds_cache() -> &'static SinkUdsCache {
    SINK_UDS_CACHE_INSTANCE.get_or_init(|| async {
        info!("Initializing static uds TokenCache...");
        SinkUdsCache::new()
    }).await
}


#[derive(Clone)]
pub struct SinkUdsTokenMeta {
    pub exp: u64,
    pub path: String
}
impl SinkUdsTokenMeta {
    pub fn new (exp: u64, path: String) -> Self {
        Self { exp, path }
    }
}
#[derive(Clone)]
pub struct SinkUdsCache {
    inner: Arc<RwLock<HashMap<String, HashMap<String, SinkUdsTokenMeta>>>>,
}


impl SinkUdsCache {
    pub fn new() -> Self {
        Self { inner: Arc::new(RwLock::new(HashMap::new()))}
    }

    pub async fn get_by_source_id_and_token_id(source_id: &str, token_id: &str) -> Option<SinkUdsTokenMeta> {
        let guard = get_sink_uds_cache().await.inner.read().await;
        guard.get(source_id)
        .and_then(|map| map.get(token_id).cloned())
    }
    

    pub async fn set(source_id: &str, token_id: String, meta: SinkUdsTokenMeta) -> bool {
        let mut guard = get_sink_uds_cache().await.inner.write().await;
        guard.get_mut(source_id)
            .map(|m| {
                m.insert(token_id, meta);
        })
        .unwrap_or({guard.insert(source_id.to_owned(), HashMap::new());});
        true
    } 

        pub async fn remove(source_id: &str, token_id: &str) -> () {
        let mut guard = get_sink_uds_cache().await.inner.write().await;
        guard.get_mut(source_id)
            .map(|m| {
                m.remove(token_id);
        });
        
    } 
}