#[cfg(test)]
mod tests {
    use crate::cache::token_cache::TokenCache;
    use crate::observability::service_resources_metrics::collect_process_metrics;
    use crate::sinks::manager::SinkManager;
    use crate::sources::builder_in_order::SourceDag;
    use crate::utils::config_loader;
    use crate::utils::{channel, logging};
    use crate::{server, ServiceConfig};
    use anyhow::Context;
    use anyhow::{anyhow, Result};
    use chrono::Utc;
    use httpmock::Method::{GET, POST};
    use httpmock::MockServer;
    use reqwest::Client;
    use serde_json::json;
    use serial_test::serial;
    use std::sync::Arc;
    use tokio::sync::oneshot::{self, Receiver, Sender};
    use tokio::task;
    use tokio::time::{sleep, Duration};


    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    #[serial]
    async fn test_google_metadata_to_sts_exchange_flow() -> Result<()> {
        // prepare mocks
        let _ = prepare_mocks().await?;

        // load config
        let service_config = prepare_service_configs("examples/google_sts_token_exchange.yaml").await?;

        

        // Create graceful shutdown signal
        let (shutdown_tx, shutdown_rx): (Sender<()>, Receiver<()>) = oneshot::channel();
        // Run app
        let app_task: task::JoinHandle<Result<()>> = task::spawn({
            let service_config = service_config.clone();
            let client = Client::builder().build()?;
            async move {
                run_app(service_config, client, shutdown_rx).await.unwrap();
                Ok(())
            }
        });

        // Spawn test thread
        let test_task = task::spawn({
            async move {
                // Let app init
                sleep(Duration::from_millis(300)).await;
                let metadata_token_ctx_opt = TokenCache::get("metadata", "metadata_token").await;
                assert_eq!(metadata_token_ctx_opt.is_some(), true);
                // check source metadata 
                let metadata_token_ctx = metadata_token_ctx_opt.clone().unwrap();
                assert_eq!(metadata_token_ctx.token.value, "meta-abc-123");
                assert!(metadata_token_ctx.token.exp_unix_ts > Utc::now().timestamp() as u64);
                assert!(metadata_token_ctx.token.exp_unix_ts <= Utc::now().timestamp() as u64 + 3600);

                // check source sts 
                let sts_token_ctx_opt = TokenCache::get("sts_exchange", "sts_token").await;
                assert_eq!(sts_token_ctx_opt.is_some(), true);
                let sts_token_ctx_opt = sts_token_ctx_opt.clone().unwrap();
                // token expired at Fri Jun 26 2026 11:20:21 GMT+0300                assert_eq!(sts_token_ctx_opt.token.value, "eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJjdXN0b21fY2xhaW0iOiIiLCJpc3MiOiJodHRwOi8vd3d3LmV4YW1wbGUuY29tIiwic3ViIjoibmFtZSBvZiBjbGFpbSIsImF1ZCI6IkpvaG4gU21pdGgiLCJleHAiOjE3ODI0NjIwMjF9.U8VHj6cIqSMB5ws1iGkS0WJzFsPDtYXJuGEp_WJKy2o");
                assert!(sts_token_ctx_opt.token.exp_unix_ts > Utc::now().timestamp() as u64);

                Ok::<_, anyhow::Error>(())
            }
        });

        graceful_shutdown(shutdown_tx, app_task, test_task).await
    }

    async fn prepare_mocks() -> Result<()> {
        // Mock servers setup
        let metadata_server = MockServer::start_async().await;
        let _ = prepare_metadata_mock(&metadata_server).await?;

        let sts_server = MockServer::start_async().await;
        let _ = prepare_sts_mock(&sts_server).await?;
        std::env::set_var("METADATA_URL", format!("{}/computeMetadata/v1/instance/service-accounts/default/token", metadata_server.base_url()));
        std::env::set_var("STS_URL", format!("{}/v1/token", sts_server.base_url()));
        Ok(())
    }

    async fn prepare_metadata_mock(metadata_server: &MockServer) -> Result<()> {
        let metadata_path = "/computeMetadata/v1/instance/service-accounts/default/token";
        metadata_server.mock(|when, then| {
            when.method(GET)
                .path(metadata_path)
                .header("Metadata-Flavor", "Google");
            then.status(200)
                .header("Content-Type", "application/json")
                .json_body(json!({
                    "access_token": "meta-abc-123",
                    "expires_in": 3600,
                    "token_type": "Bearer"
                }));
        });
        Ok(())
    }

    async fn prepare_sts_mock(sts_server: &MockServer) -> Result<()> {
        let sts_path = "/v1/token";
        sts_server.mock(|when, then| {
            when.method(POST).path(sts_path);
            then.status(200)
                .header("Content-Type", "application/json")
                .json_body(json!({
                    "access_token": "eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJjdXN0b21fY2xhaW0iOiIiLCJpc3MiOiJodHRwOi8vd3d3LmV4YW1wbGUuY29tIiwic3ViIjoibmFtZSBvZiBjbGFpbSIsImF1ZCI6IkpvaG4gU21pdGgiLCJleHAiOjE3ODI0NjIwMjF9.U8VHj6cIqSMB5ws1iGkS0WJzFsPDtYXJuGEp_WJKy2o",
                    "issued_token_type": "urn:ietf:params:oauth:token-type:access_token",
                    "scope": "https://www.googleapis.com/auth/cloud-platform",
                    "token_type": "Bearer",
                }));
        });
        Ok(())
    }

    async fn prepare_service_configs(path: &str) -> Result<Arc<ServiceConfig>> {
        let service_config = config_loader::run(path)
            .await
            .context("failed to load test config")?;
        logging::run(&service_config, None).await?;
        let service_config = Arc::new(service_config);
        Ok(service_config)
    }

    pub async fn run_app(
        service_config: Arc<ServiceConfig>,
        client: Client,
        mut shutdown: oneshot::Receiver<()>,
    ) -> Result<()> {
        let dag = SourceDag::build(&service_config.sources)?;
        let sink_sender = channel::run();
        let safety_margin_seconds = service_config.settings.safety_margin_seconds;
        let retry = &service_config.settings.retry;

        let receiver =
            dag.loop_refrech_tokens(&client, retry, safety_margin_seconds, sink_sender.clone());
        let cleaner = dag.loop_check_token_exp(
            &service_config.sources,
            &safety_margin_seconds,
            sink_sender.clone(),
        );
        let sink_manager = SinkManager::new(service_config.sinks.clone());
        let http_server = server::server::start(&service_config.settings, &service_config.sinks);
        let service_metrics = collect_process_metrics(service_config.settings.metrics.is_enabled);
        let active_sinks = sink_manager.start_active_sinks(sink_sender.clone());

        tokio::select! {
            res = async {
                tokio::try_join!(
                    receiver,
                    cleaner,
                    active_sinks,
                    http_server,
                    service_metrics
                )
            } => {
                res?;
            },
            _ = &mut shutdown => {
                tracing::info!("🧹 shutting down app gracefully");
            }
        }
        Ok(())
    }

    async fn graceful_shutdown(shutdown_tx: Sender<()>, app_task: task::JoinHandle<Result<()>>, test_task: task::JoinHandle<Result<()>>) -> Result<()> {
        let test_result = tokio::time::timeout(Duration::from_secs(15), test_task).await;
        assert!(test_result.is_ok(), "Test timed out!");

        // Signal graceful shutdown
        let _ = shutdown_tx.send(());

        // Wait for the app task to finish
        let _ = app_task.await?;

        match test_result.map_err(|_| anyhow!("test timed out"))? {
            Ok(_) => Ok(()),
            Err(e) => Err(anyhow!(e)),
        }

    }
}
