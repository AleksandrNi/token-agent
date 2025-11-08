use std::collections::HashMap;
use anyhow::Result;
use axum::{Router};
use crate::config::settings::{SettingsConfig};
use crate::config::sinks::SinkConfig;
use crate::observability::metrics::{get_metrics, Metrics};
use crate::observability::routes::{MetricsState};
use crate::sinks::sink_http::{SinkHttpState};

#[derive(Clone)]
pub struct AppState {
    pub metrics_state: MetricsState,
    pub sink_http_state: SinkHttpState
}

impl AppState {
    pub fn new (
        metrics: &Metrics,
        sinks: &HashMap<String, SinkConfig>
    ) -> Self{
        Self { 
            metrics_state: MetricsState::new(metrics.registry.clone()), 
            sink_http_state: SinkHttpState::new(sinks).unwrap() 
        }
    }
}

/// Start one Axum server that dynamically dispatches on the configured sink paths.
pub async fn start(
    settings_config: &SettingsConfig, 
    sinks: &HashMap<String, SinkConfig>
) -> Result<()> {
    let metrics = get_metrics().await;
    let state = AppState::new(metrics, sinks);

    let app = Router::new()
        .merge(state.metrics_state.router(&settings_config.metrics).await)
        .merge(state.sink_http_state.router().await)
        .with_state(state);

    if app.has_routes() {
        let bind_addr  = &settings_config.server.host;
        let port = &settings_config.server.port;
        println!("address: {}, port: {}", bind_addr, port);
        let listener = tokio::net::TcpListener::bind(format!("{}:{}", bind_addr, port))
            .await
            .unwrap();
        metrics.up.set(1);
        axum::serve(listener, app).await.unwrap();    
    }


    Ok(())
}