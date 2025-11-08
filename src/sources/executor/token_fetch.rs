use std::sync::Arc;
use std::time::Duration;

use crate::cache::token_cache::TokenCache;
use crate::cache::token_context::TokenContext;
use crate::config::settings::RetryConfig;
use crate::config::sinks::SinkMessage;
use crate::config::sources::SourceConfig;
use crate::helpers::time::{get_instant, now_i64};
use crate::observability::metrics::get_metrics;
use crate::resilience::retry::RetrySettings;
use crate::sources::builder_in_order::SourceDag;
use crate::sources::fetch::{FetchTokens, Source};

use anyhow::{Result};
use chrono::{DateTime, Utc};
use reqwest::Client;
use tokio::sync::broadcast::Sender;
use tracing::{debug, info};

static  ERROR_MSG: &'static str =  "error";
static  HTTP_MSG: &'static str =  "http";

impl SourceDag {
    /// Execute all sources in DAG order, respecting dependencies and retry policies.
    pub async fn loop_refrech_tokens(
        &self,
        client: &Client,
        retry: &Option<RetryConfig>,
        safety_margin_seconds_settings: Option<u64>,
        tx: Sender<SinkMessage>,
    ) -> Result<()> {
        // prepare retry policies
        let retry = RetrySettings {
            attempts: retry.as_ref().and_then(|r| r.attempts).unwrap_or(3),
            base_delay_ms: retry.as_ref().and_then(|r| r.base_delay_ms).unwrap_or(200),
            max_delay_ms: retry.as_ref().and_then(|r| r.max_delay_ms).unwrap_or(1000),
        };

        let sources_ordered = self.ordered.clone();
        let client = client.clone();
        let retry = retry.clone();
        let _ = tokio::spawn(async move {
            loop {
                let mut sleep_until = i64::MAX;
                info!("refetch token cycle start");
                for node in sources_ordered.iter() {
                    let source_id = node.id.as_str();

                    info!("fetching source '{}', deps: '{:?}'", source_id, node.deps);

                    // define should fetch
                    let mut should_fetch: bool = false;
                    for source_token in &node.config.parse.tokens {
                        let token_context_opt = 
                        TokenCache::get(source_id, &source_token.id).await;

                        if token_context_opt
                            .map(|token_context|{
                                if (token_context.fetched_at_unix_ts as i64) < sleep_until {
                                    sleep_until = token_context.fetched_at_unix_ts as i64;
                                }
                                token_context
                            })
                            .filter(|token_config| !token_config.should_update())
                            .is_none()
                        {
                            info!("fetching source '{}' now", source_id);
                            sleep_until = now_i64();
                            should_fetch = true;
                            break;
                        }
                    }
                    if !should_fetch {
                        continue;
                    }

                    // fetch tokens for source

                    if let Ok(token_contexts) = SourceDag::fetch_tokens_by_source_id(source_id,node.config.clone(),safety_margin_seconds_settings,&client,&retry).await {
                        info!("fetched total tokens {} for source_id {}",token_contexts.len(),source_id);

                        let stored_tokens = match SourceDag::store_tokens_by_source_id(source_id, token_contexts).await {
                            Ok(v) => v,
                            Err(err) => {
                                info!("storing tokens for source_id {} failed, {}", source_id, err);
                            Vec::with_capacity(0)
                            },
                        };
                        info!("stored total tokens {} for source_id {}",stored_tokens.len(),source_id);
                    };

                    // sink active propogation
                    let _ = tx.send(SinkMessage(source_id.to_owned()))
                    .map_err(|err|{
                        debug!("message for source_id {} was sent {}", source_id, err);
                    });
                }
                debug!("sleep until {}", sleep_until);
                sleep_until_next_token_fetch_check(sleep_until).await;
            }
        });
        Ok(())
    }

    async fn fetch_tokens_by_source_id(
        source_id: &str,
        config: Arc<SourceConfig>,
        safety_margin_seconds_settings: Option<u64>,
        client: &Client,
        retry: &RetrySettings,
    ) -> Result<Vec<TokenContext>> {
        let metrics = get_metrics().await;
        let start = get_instant();
        metrics.source_fetch_requests.with_label_values(&[&source_id, &HTTP_MSG, &&config.request.method.as_str()]).inc();
        retry
            .run_with_retry(|| {
                let source = Source(config.clone());
                async move {
                    source
                        .fetch_tokens(client, safety_margin_seconds_settings)
                        .await
                }
            })
            .await
            .map(|source_token_contexts| {
                metrics.source_fetch_duration.with_label_values(&[source_id]).observe(start.elapsed().as_secs_f64());
                info!(
                    "source '{}' successfully fetched tokens, total tokens received: {:?}",
                    source_id,
                    source_token_contexts.len()
                );
                source_token_contexts
            })
            .map_err(|e| {
                metrics.source_fetch_duration.with_label_values(&[source_id]).observe(start.elapsed().as_secs_f64());
                metrics.source_fetch_failures.with_label_values(&[source_id, ERROR_MSG]).inc();
                e
            })
    }
}

async fn sleep_until_next_token_fetch_check(sleep_until: i64) {
    info!(
        "now: {}",
        DateTime::from_timestamp_secs(Utc::now().timestamp() as i64).unwrap()
    );

    let mut sleep_interval = sleep_until - now_i64();

    if sleep_interval <= 0 {
        sleep_interval = 1
    }

    if sleep_interval > 0 {
        info!("sleep interval {} seconds, next check start at {}", sleep_interval, DateTime::from_timestamp_secs(sleep_until).unwrap());
        tokio::time::sleep(Duration::from_secs(sleep_interval as u64)).await;
    }
}
