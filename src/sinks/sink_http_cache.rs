// use std::{collections::HashMap, sync::Arc};

// use tokio::sync::{OnceCell, RwLock};

// // Declare the static OnceCell to hold the TokenCache.
// static SINK_HTTP_CACHE_INSTANCE: OnceCell<SinkHttpCache> = OnceCell::const_new();

// /// Asynchronously initializes and gets a reference to the static `TokenCache`.
// async fn get_sink_http_cache() -> &'static SinkHttpCache {
//     SINK_HTTP_CACHE_INSTANCE.get_or_init(|| async {
//         // Here you could perform an async setup, like loading from a HTTP or DB.
//         info!("Initializing static SinkHttpCache...");
//         SinkHttpCache::new()
//     }).await
// }


// #[derive(Clone)]
// pub struct SinkHttpTokenResponseMeta {
    
//     pub response: String,
//     pub content_type: String,
//     pub exp: u64,
// }
// impl SinkHttpTokenResponseMeta {
//     pub fn new (response: String, content_type: String, exp: u64) -> Self {
//         Self {response, content_type, exp }
//     }
// }
// #[derive(Clone)]
// pub struct SinkHttpCache {
//     // path -> {response, exp}
//     inner: Arc<RwLock<HashMap<String, SinkHttpTokenResponseMeta>>>,
// }


// impl SinkHttpCache {
//     pub fn new() -> Self {
//         Self { inner: Arc::new(RwLock::new(HashMap::new()))}
//     }

//     pub async fn get_by_path(path: &str) -> Option<SinkHttpTokenResponseMeta> {
//         let guard = get_sink_http_cache().await.inner.read().await;
//         guard.get(path)
//         .map(|meta| meta.to_owned())
//     }
    

//     pub async fn set(path: &str, meta: SinkHttpTokenResponseMeta) -> bool {
//         let mut guard = get_sink_http_cache().await.inner.write().await;
//         guard.insert(path.to_owned(), meta);
//         true
//     } 

//         pub async fn remove(path: &str) -> () {
//         let mut guard = get_sink_http_cache().await.inner.write().await;
//         guard.remove(path);
//     } 
// }