use std::collections::HashMap;
use std::io::ErrorKind;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use crate::cache::token::TOKEN_VALUE_STUB;
use crate::cache::token_cache::TokenCache;
use crate::cache::token_context::TokenContext;
use crate::config::sinks::{SinkConfig, SinkMessage, SinkType};
use crate::observability::metrics::get_metrics;
use crate::sinks::manager::{SinkManager, SyncType};
use crate::sinks::sink_file_cache::{SinkFileTokenMeta, SinkFileCache};
use anyhow::Result;
use tokio::signal::unix::{signal, SignalKind};
use tokio::{fs, join, select};
use tokio::sync::broadcast::Receiver;
use tracing::{error, info};

static FILE_MSG: &'static str = "file";
static  ERROR_MSG: &'static str =  "error";

impl SinkManager {
    // Token cache: source_id -> token_id -> expiration_at
    pub async fn start_file_sinks(self, rx: Receiver<SinkMessage>) -> Result<()> {
        info!("start sink 'type: file'");
        let cleanup = cleanup_resourses(self.sinks.clone());
        let worker = sink_http_worker(self.sinks.clone(), rx);
        
        let _ = join!(cleanup, worker);
        Ok(())
    }
}

async fn sink_http_worker(sinks: Arc<HashMap<String, SinkConfig>>, mut rx: Receiver<SinkMessage>) {
    let metrics = get_metrics().await;
    loop {
        if let Ok(message) = rx.recv().await {
            let start = Instant::now();
            let source_id = message.0;
                
            for (_, cfg) in sinks.iter() {
                if cfg.sink_type != SinkType::File || cfg.source_id != source_id {
                    continue;
                }
                info!("sink file:: sink file for source_id: {}", source_id);
                let token_context_opt = TokenCache::get(&cfg.source_id, &cfg.token_id).await;

                let token_opt = if let Some(token_context)= token_context_opt {
                    // skip storing if token iwth the same exp already exists in cache
                    if check_if_token_should_be_skipped(&source_id, &token_context).await {
                        continue;
                    }
                    sync_token_with_local_cache(&source_id, &cfg.path, &token_context.id, token_context
                        .token.exp_unix_ts, SyncType::ADD).await;
                    Some(token_context.token)

                // removed tokes
                } else {
                    info!("sink file: token  writes to {}", &cfg.path);
                    // remove from local cache
                    sync_token_with_local_cache(&source_id, &cfg.path, &&cfg.token_id, 0, SyncType::REMOVE).await;
                    // cleanup token
                    None
                };

                match token_opt {
                    Some(token) => {
                        // store new token
                        info!("token id '{}' writes, path '{}'", &cfg.token_id, &cfg.path);
                        let _ = tokio::fs::write(&cfg.path, token.value.as_bytes()).await
                        .inspect(|_| {
                                metrics
                                    .sink_propagations
                                    .with_label_values(&[
                                        &cfg.sink_id.as_str(),
                                        &FILE_MSG,
                                        &source_id.as_str(),
                                        &cfg.token_id.as_str(),
                                    ])
                                    .inc();
                                metrics
                                    .sink_duration
                                    .with_label_values(&[&cfg.sink_id.as_str()])
                                    .observe(start.elapsed().as_secs_f64());
                        })
                            .inspect_err(|err| {
                                error!("{}", err);
                                metrics.sink_failures.with_label_values(&[&cfg.sink_id.as_str(), &ERROR_MSG]).inc();
                            });    
                    },
                    None => {
                        // cleanup content
                        info!("token id '{}' cleanup, path '{}'", &cfg.token_id, &cfg.path);
                        let _ = tokio::fs::write(&cfg.path, TOKEN_VALUE_STUB.as_bytes()).await
                            .inspect_err(|err| {
                                error!("{}", err);
                                metrics.sink_failures.with_label_values(&[&cfg.sink_id.as_str(), &ERROR_MSG]).inc();
                            });   
                    },
                }
            }
        }
    }
}

async fn check_if_token_should_be_skipped(source_id: &str, token_context: &TokenContext) -> bool {
    // store token in local cache
    let token_already_exists: bool = SinkFileCache::get_by_source_id_and_token_id(&source_id, token_context.id.as_str()).await
    .filter(|sink_file_token_meta| sink_file_token_meta.exp == token_context.token.exp_unix_ts)
    .is_some();

    info!("sink file: does token exists in cache: {}", token_already_exists);
    token_already_exists
}

async fn sync_token_with_local_cache(source_id: &str, path: &str, token_id: &str, exp: u64, sync_type: SyncType) -> () {
    // store token in local cache 
    let token_meta = SinkFileTokenMeta::new(exp, path.to_owned());
    match sync_type {
        SyncType::ADD => {
            SinkFileCache::set(source_id, token_id.to_owned(),  token_meta).await;
        },
        SyncType::REMOVE => {
            SinkFileCache::remove(source_id, token_id).await;
        },
    }
    
}

async fn cleanup_resourses(sinks: Arc<HashMap<String, SinkConfig>>) -> Result<()>{
        let mut sigint = signal(SignalKind::interrupt()).unwrap();
        let mut sigterm = signal(SignalKind::terminate()).unwrap();
        select! {
            _ = sigint.recv() => {
                info!("\nReceived SIGINT (Ctrl+C). Initiating graceful shutdown...");
                let _ = cleanup_stored_tokens_after_cancelling(sinks.clone()).await;
            }
            _ = sigterm.recv() => {
                info!("Received SIGTERM. Initiating graceful shutdown...");
                let _ = cleanup_stored_tokens_after_cancelling(sinks.clone()).await;
            }
            // Add other async tasks or application logic here
            // _ = my_application_task() => { /* handle task completion */ }
        }
        Ok(())
}

async fn  cleanup_stored_tokens_after_cancelling(sinks: Arc<HashMap<String, SinkConfig>>) -> Result<()> {
    for (_, cfg) in sinks.iter() {
        let path = cfg.path.as_str();
        if cfg.sink_type == SinkType::File {
            info!("remove token file at path path : {}", path);
            if Path::new(path).exists() {
                match fs::remove_file(path).await {
                    Ok(_) => info!("Deleted file: {}", path),
                    Err(e) if e.kind() == ErrorKind::NotFound => {
                        info!("File not found, nothing to delete: {}", path);
                    }
                    Err(e) => {
                        info!("Failed to delete {}: {}", path, e);
                    }
                }
            }
        }
    }

    println!("Exiting application.");
    std::process::exit(0);
}

