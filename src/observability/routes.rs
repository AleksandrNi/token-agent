use std::sync::Arc;

use crate::config::settings::MetricsConfig;
use crate::server::server::AppState;
use axum::routing::get;
use axum::{extract::State, response::IntoResponse, Router};
use http::{header::CONTENT_TYPE, StatusCode};
use prometheus::{Encoder, Registry, TextEncoder};

#[derive(Clone)]
pub struct MetricsState {
    pub registry: Arc<Registry>,
}

impl MetricsState {
    pub fn new (registry: Registry) -> Self {
        Self {
            registry: Arc::new(registry)
        }
    }
}

impl MetricsState {
    pub async fn router(&self, metrics_config: &MetricsConfig) -> Router<AppState> {
    
    // create router
    let mut router = Router::new();
    if metrics_config.is_enabled {
        router = router.route(metrics_config.path.as_str(), get(get_metrics));
    }
    // router.with_state(state)
    router
}
}

async fn get_metrics(State(state): State<AppState>) -> impl IntoResponse {

    let encoder = TextEncoder::new();
    let metric_families = state.metrics_state.registry.gather();
    let mut buffer = Vec::new();

    encoder
        .encode(&metric_families, &mut buffer)
        .expect("Failed to encode metrics");

    let response = String::from_utf8(buffer.clone()).expect("Failed to convert bytes to string");
    (
        StatusCode::OK,
        [(CONTENT_TYPE, "text/plain; version=0.0.4")],
        response,
    )
}
