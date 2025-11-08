

#[cfg(test)]
mod tests {
    use serde::Deserialize;
    use crate::cache::token_cache::TokenCache;
    use crate::observability::service_resources_metrics::collect_process_metrics;
    use crate::sinks::manager::SinkManager;
    use crate::sources::builder_in_order::SourceDag;
    use crate::tests::common::{build_reqwest_client};
    use crate::utils::config_loader;
    use crate::utils::{channel, logging};
    use crate::{server, ServiceConfig};
    use anyhow::Context;
    use anyhow::{anyhow, Result};
    use chrono::{DateTime, Utc};
    use httpmock::Method::GET;
    use httpmock::MockServer;
    use reqwest::Client;
    use serde_json::json;
    use serial_test::serial;
    use std::sync::Arc;

    use tokio::sync::oneshot::{self, Receiver, Sender};
    use tokio::task;
    use tokio::time::{sleep, Duration};


    #[derive(Debug, Deserialize, Clone)]
    struct MetadataSecondsResponse{
        pub token: String,
        pub expires_in: u64

    }

    #[derive(Debug, Deserialize, Clone)]
    struct MetadataUnixResponse{
        pub token: String,
        pub expired_at: u64

    }

    #[derive(Debug, Deserialize, Clone)]
    struct MetadataRFC3339Response{
        pub token: String,
        pub expired_at: String

    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    #[serial]
    async fn test_google_metadata_flow() -> Result<()> {
        // prepare mocks
        let _ = prepare_mocks().await?;

        // load config
        let service_config = prepare_service_configs("examples/google_metadata_token.yaml").await?;

        

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
                
                // test source 
                // check token cache 
                let metadata_token_ctx = metadata_token_ctx_opt.clone().unwrap();
                assert_eq!(metadata_token_ctx.token.value, "meta-abc-123");
                assert!(metadata_token_ctx.token.exp_unix_ts > Utc::now().timestamp() as u64);
                assert!(metadata_token_ctx.token.exp_unix_ts <= Utc::now().timestamp() as u64 + 3600);

                
                let client = build_reqwest_client();
                let ttl: u64 = 3;
                
                // -------------------------------
                // test sink response seconds
                // -------------------------------

                let url = format!("http://127.0.0.1:{}/tokens/metadata_seconds", 8080);
                let res = client
                    .get(&url)
                    .send()
                    .await
                    .expect("failed to send request to token-agent");

                // assert HTTP success
                assert!(res.status().is_success(), "unexpected status: {}", res.status());
                let body: MetadataSecondsResponse = res.json().await.expect("failed to parse JSON");

                assert_eq!(body.token, "meta-abc-123");
                assert!(body.expires_in > 0, "missing expires_in field");
                assert!(body.expires_in < Utc::now().timestamp() as u64 + ttl, "missing expires_in field");

                // -------------------------------
                // test sink response unix
                // -------------------------------
                
                let url = format!("http://127.0.0.1:{}/tokens/metadata_unix", 8080);
                let res = client
                    .get(&url)
                    .send()
                    .await
                    .expect("failed to send request to token-agent");

                assert!(res.status().is_success(), "unexpected status: {}", res.status());
                let body: MetadataUnixResponse = res.json().await.expect("failed to parse JSON");
                assert_eq!(body.token, "meta-abc-123");
                assert!(body.expired_at > Utc::now().timestamp() as u64, "invalid low limit expired_at field");
                assert!(body.expired_at <= Utc::now().timestamp() as u64 + ttl, "invalid top limit expired_at field");

                // -------------------------------
                // test sink response rfc3339
                // -------------------------------

                let url = format!("http://127.0.0.1:{}/tokens/metadata_rfc3339", 8080);
                let res = client
                    .get(&url)
                    .send()
                    .await
                    .expect("failed to send request to token-agent");

                assert!(res.status().is_success(), "unexpected status: {}", res.status());
                let body: MetadataRFC3339Response = res.json().await.expect("failed to parse JSON");
                assert_eq!(body.token, "meta-abc-123");
                assert!(DateTime::parse_from_rfc3339(body.expired_at.as_str()).unwrap() > DateTime::from_timestamp_secs(Utc::now().timestamp() as i64).unwrap(), "invalid low limit expired_at field");
                assert!(DateTime::parse_from_rfc3339(body.expired_at.as_str()).unwrap() <= DateTime::from_timestamp_secs(Utc::now().timestamp() as i64 + ttl as i64).unwrap(), "invalid top limit expired_at field");

                Ok::<_, anyhow::Error>(())
            }
        });

        graceful_shutdown(shutdown_tx, app_task, test_task).await
    }


    async fn prepare_mocks() -> Result<()> {
        // Mock servers setup
        let metadata_server = MockServer::start_async().await;
        let _ = prepare_metadata_mock(&metadata_server).await?;
        std::env::set_var("METADATA_URL", format!("{}/computeMetadata/v1/instance/service-accounts/default/token", metadata_server.base_url()));
        
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
                    "expires_in": 3,
                    "token_type": "Bearer"
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
