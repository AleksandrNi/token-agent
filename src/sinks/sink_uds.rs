use std::time::Instant;

use tokio::net::UnixStream;
use tokio::io::AsyncWriteExt;
use anyhow::Result;
use tracing::{error, info};

use crate::cache::token::TOKEN_VALUE_STUB;
use crate::cache::token_cache::TokenCache;
use crate::cache::token_context::TokenContext;
use crate::config::sinks::{SinkMessage, SinkType};
use crate::observability::metrics::get_metrics;
use crate::sinks::manager::{SinkManager, SyncType};
use crate::sinks::sink_uds_cache::{SinkUdsCache, SinkUdsTokenMeta};
use tokio::sync::broadcast::Receiver;

static UDS_MSG: &'static str = "uds";
static  ERROR_MSG: &'static str =  "error";

impl SinkManager {
    pub async fn start_uds_sinks(self, mut rx: Receiver<SinkMessage>) -> Result<()> {
        let metrics = get_metrics().await;
        loop {
            if let Ok(message) = rx.recv().await {
                let start = Instant::now();
                let source_id = message.0;
                info!("start sink 'type: uds'");
                for (name, cfg) in self.sinks.iter() {
                    if cfg.sink_type != SinkType::Uds {
                        continue;
                    }

                    info!("sink file:: sink file for source_id: {}", source_id);
                    let token_context_opt = TokenCache::get(&cfg.source_id, &cfg.token_id).await;

                    let token_opt = if let Some(token_context)= token_context_opt {
                        // skip storing if token iwth the same exp already exists in cache
                        if check_if_token_should_be_skipped(&source_id, &token_context).await {
                            continue;
                        }
                        sync_token_with_local_cache(&source_id, &cfg.path, &token_context.id, token_context.token.exp_unix_ts, SyncType::ADD).await;
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
                            if let Err(err) = async {
                                let mut stream = UnixStream::connect(&cfg.path).await?;
                                stream.write_all(token.value.as_bytes()).await?;
                                stream.shutdown().await?;

                                metrics
                                    .sink_propagations
                                    .with_label_values(&[
                                        &cfg.sink_id.as_str(),
                                        &UDS_MSG,
                                        &source_id.as_str(),
                                        &cfg.token_id.as_str(),
                                    ])
                                    .inc();
                                metrics
                                    .sink_duration
                                    .with_label_values(&[&cfg.sink_id.as_str()])
                                    .observe(start.elapsed().as_secs_f64());
                                Ok::<(), std::io::Error>(())
                            }.await {
                                error!("{}", err);
                                metrics
                                    .sink_failures
                                    .with_label_values(&[cfg.sink_id.as_str(), &ERROR_MSG])
                                    .inc();
                            }
                    
                    info!("UDS sink '{}' sent token to '{}'", name, cfg.path);
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
}

async fn check_if_token_should_be_skipped(source_id: &str, token_context: &TokenContext) -> bool {
    // store token in local cache
    let token_already_exists: bool = SinkUdsCache::get_by_source_id_and_token_id(&source_id, token_context.id.as_str()).await
    .filter(|sink_uds_token_meta| sink_uds_token_meta.exp == token_context.token.exp_unix_ts)
    .is_some();

    info!("sink uds: does token exists in cache: {}", token_already_exists);
    token_already_exists
}

async fn sync_token_with_local_cache(source_id: &str, path: &str, token_id: &str, exp: u64, sync_type: SyncType) -> () {
    // store token in local cache 
    let token_meta = SinkUdsTokenMeta::new(exp, path.to_owned());
    match sync_type {
        SyncType::ADD => {
            SinkUdsCache::set(source_id, token_id.to_owned(),  token_meta).await;
        },
        SyncType::REMOVE => {
            SinkUdsCache::remove(source_id, token_id).await;
        },
    }
    
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::{
        io::{AsyncReadExt},
        net::{UnixListener},
        time::{timeout, Duration},
    };
    use tempfile::tempdir;
    use std::collections::HashMap;

    use crate::{cache::{token::Token, token_cache::TokenCache}, config::sinks::SinkConfig, utils::channel};
    use crate::cache::token_context::TokenContext;

    #[tokio::test]
    async fn test_start_uds_sinks_sends_token() -> anyhow::Result<()> {
        // -------------------------------
        // 1. Setup temporary UDS path
        // -------------------------------
        // Setup temporary UDS path
        let dir = tempdir()?;
        let socket_path = dir.path().join("test.sock");
        let socket_path_str = socket_path.to_str().unwrap().to_string();

        // Prepare a Unix listener to simulate the consumer
        let listener = UnixListener::bind(&socket_path)?;

        // -------------------------------
        // 2. Prepare TokenCache
        // -------------------------------

        let token_value = "my-secret-token";
        let source_id = "src-1".to_string();
        let token_id = "tkn-1".to_string();
        let token = Token {
                value: token_value.to_string(),
                exp_unix_ts: 999999,
            };
        let safety_margin_seconds = 10;
        let token_ctx = TokenContext::new(token_id.to_owned(), token, safety_margin_seconds);
        TokenCache::set(source_id.clone(), vec![token_ctx.clone()]).await?;
        
        // -------------------------------
        // 2. Prepare SinkManager config
        // -------------------------------
        
        let sink_config = SinkConfig {
            sink_id: "sink-1".to_string(),
            sink_type: SinkType::Uds,
            source_id: source_id.clone(),
            token_id: token_id.clone(),
            path: socket_path_str.clone(),
            response: None,
        };

        let mut sinks = HashMap::new();
        sinks.insert("uds_sink".to_string(), sink_config);
        let sink_manager = SinkManager::new(sinks);

        // -------------------------------
        // 3. Channel setup to trigger UDS sink event
        // -------------------------------
        
        let sink_sender = channel::run();
        let rx = sink_sender.clone().subscribe();
        // Spawn the sink manager task
        let manager_task = tokio::spawn(async move {
            let _ = sink_manager.start_uds_sinks(rx).await;
        });

        // -------------------------------
        // 4. Send a message to trigger sink
        // -------------------------------

        sink_sender.send(SinkMessage(source_id.clone()))?;

        // -------------------------------
        // 5. Accept the incoming Unix connection and read token
        // -------------------------------
        
        let (mut stream, _) = timeout(Duration::from_secs(5), listener.accept()).await??;
        let mut buf = vec![0u8; 1024];
        let n = timeout(Duration::from_secs(2), stream.read(&mut buf)).await??;
        let received = String::from_utf8_lossy(&buf[..n]);

        println!(">> received: {}", received);


        // -------------------------------
        // 6. Assert that the token was sent correctly
        // -------------------------------
        
        assert_eq!(received, token_value);

        // Cleanup
        drop(listener);
        manager_task.abort();

        Ok(())
    }
}

