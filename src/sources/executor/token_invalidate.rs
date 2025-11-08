use std::collections::HashMap;
use std::time::{Duration};

use crate::cache::token_cache::TokenCache;
use crate::config::sinks::SinkMessage;
use crate::config::sources::SourceConfig;
use crate::helpers::time::{get_token_safety_margin_seconds, now_i64};
use crate::sources::builder_in_order::SourceDag;
use anyhow::Result;
use chrono::{DateTime, Utc};
use tokio::sync::broadcast::Sender;
use tracing::{debug, info};

impl SourceDag {
    /// Execute all sources in DAG order, respecting dependencies and retry policies.
    pub async fn loop_check_token_exp(
        &self,
        sources: &HashMap<String, SourceConfig>,
        safety_margin_seconds_settings: &Option<u64>,
        tx: Sender<SinkMessage>,
    ) -> Result<()> {
        let sources_ordered = self.ordered.clone();
        let sources = sources.clone();
        let safety_margin_seconds_settings = safety_margin_seconds_settings.to_owned();
        let _ = tokio::spawn(async move {
            loop {
                let mut sleep_until = i64::MAX;
                info!("remove outdated tokens cycle start");
                for node in sources_ordered.iter() {
                    let source_id = node.id.as_str();

                    info!("check source tokens exp '{}'", source_id);

                    // define should fetch
                    let mut should_remove_token: bool = false;
                    for source_token in &node.config.parse.tokens {
                        let token_context_opt = TokenCache::get(source_id, &source_token.id).await;

                        let should_remove_at_opt =
                            token_context_opt.map(|token_context| token_context.should_remove_at());

                        if let Some(should_remove_at) = should_remove_at_opt {
                            if should_remove_at <= sleep_until {
                                sleep_until = should_remove_at;
                            }
                        } else {
                            let safety_margin_source = &sources
                                .get(source_id)
                                .map(|source_config| source_config.safety_margin_seconds.to_owned())
                                .flatten();
                            sleep_until = now_i64() + get_token_safety_margin_seconds(
                                    safety_margin_seconds_settings.to_owned(),
                                    safety_margin_source.to_owned(),
                                ) as i64;
                                info!("token absent: sleep_until: {}", sleep_until)
                        }
                        
                        if sleep_until <= now_i64() {
                            should_remove_token = true;
                            break;
                        }
                    }

                    if !should_remove_token {
                        continue;
                    }

                    let _ = match SourceDag::invalidate_tokens_by_source_id(source_id).await {
                        Ok(v) => v,
                        Err(err) => {
                            info!("removing tokens by source_id {} failed, {}", source_id, err);
                        }
                    };

                    // sink active propogation
                    let _ = tx.send(SinkMessage(source_id.to_owned())).map_err(|err| {
                        debug!("message for source_id {} was sent {}", source_id, err);
                    });
                }

                sleep_until_next_token_exp_check(sleep_until).await;
                process_metrics().await;
            }
        });
        Ok(())
    }
}

async fn sleep_until_next_token_exp_check(sleep_until: i64) {
    let now = Utc::now().timestamp();
    debug!(
        "now UTC: {}, UNIX: {}", DateTime::from_timestamp_secs(now as i64).unwrap(), now);

    let mut sleep_interval = sleep_until - now_i64();

    if sleep_interval <= 0 {
        sleep_interval = 0
    }

    if sleep_interval > 0 {
        info!(
            "sleep interval {} seconds, next check start at {}",
            sleep_interval,
            DateTime::from_timestamp_secs(sleep_until).unwrap()
        );
        tokio::time::sleep(Duration::from_secs(sleep_interval as u64)).await;
    }
}

async fn process_metrics() -> () {
    TokenCache::process_metrics().await;
}
