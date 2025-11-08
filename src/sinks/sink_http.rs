use anyhow::{anyhow, Result};
use axum::{
    extract::State,
    http::{HeaderMap, HeaderName, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};

use chrono::{TimeZone, Utc};
use serde_json::Value;
use std::{collections::HashMap, sync::Arc};
use tokio::time::Instant;
use tracing::info;

use crate::config::sinks::{ExpirationSinkFormat, ResponseField, SinkConfig, SinkType};
use crate::server::server::AppState;
use crate::{cache::token_cache::TokenCache, observability::metrics::get_metrics};

static ERROR_MSG: &'static str = "error";
static HTTP_MSG: &'static str = "http";

#[derive(Clone)]
pub struct SinkHttpState {
    sink_routes: Arc<HashMap<String, SinkConfig>>,
}

impl SinkHttpState {
    pub fn new(all_sinks: &HashMap<String, SinkConfig>) -> Result<Self> {
        let mut routes = HashMap::new();

        for (sink_name, cfg) in all_sinks {
            if let SinkType::Http = cfg.sink_type {
                let path = if cfg.path.starts_with('/') {
                    cfg.path.clone()
                } else {
                    format!("/{}", cfg.path)
                };

                if routes.contains_key(&path) {
                    return Err(anyhow!(
                        "duplicate HTTP sink path '{}' (sink '{}')",
                        path,
                        sink_name
                    ));
                }
                routes.insert(path, cfg.clone());
            }
        }

        Ok(Self {
            sink_routes: Arc::new(routes),
        })
    }
}

impl SinkHttpState {
    pub async fn router(&self) -> Router<AppState> {
        let mut router = Router::new();

        for (path, _) in self.sink_routes.iter() {
            info!("served path: {}", &path);
            router = router.route(path, get(handle_request_axum));
        }
        router
    }
}

/// Unified request handler — dynamic dispatch by path.
async fn handle_request_axum(
    State(state): State<AppState>,
    req: axum::http::Request<axum::body::Body>,
) -> Response {
    let metrics = get_metrics().await;
    let start = Instant::now();

    let sink_routes = &state.sink_http_state.sink_routes;
    let path = req.uri().path().to_string();
    info!("path: {}", path);
    let sink = match sink_routes.get(&path) {
        Some(s) => s,
        None => return (StatusCode::NOT_FOUND, "not found").into_response(),
    };
    info!("{}.{:?}", path, sink.response);

    match render_http_response_axum(&sink).await {
        Ok((headers, body, content_type)) => {
            let mut header_map = HeaderMap::new();
            header_map.insert(
                axum::http::header::CONTENT_TYPE,
                HeaderValue::from_str(&content_type).unwrap(),
            );

            for (k, v) in headers {
                if let (Ok(name), Ok(val)) = (
                    HeaderName::from_bytes(k.as_bytes()),
                    HeaderValue::from_str(&v),
                ) {
                    header_map.insert(name, val);
                }
            }
            metrics
                .sink_propagations
                .with_label_values(&[
                    &sink.sink_id.as_str(),
                    &HTTP_MSG,
                    &sink.source_id.as_str(),
                    &sink.token_id.as_str(),
                ])
                .inc();
            metrics
                .sink_duration
                .with_label_values(&[&sink.sink_id.as_str()])
                .observe(start.elapsed().as_secs_f64());

            (header_map, Json(body)).into_response()
        }
        Err(e) => {
            metrics
                .sink_failures
                .with_label_values(&[&sink.sink_id.as_str(), &ERROR_MSG])
                .inc();
            metrics
                .sink_duration
                .with_label_values(&[&sink.sink_id.as_str()])
                .observe(start.elapsed().as_secs_f64());
            (StatusCode::NOT_FOUND, format!("Error: {}", e)).into_response()
        }
    }
}

/// returns header, body, content-type
async fn render_http_response_axum(
    sink: &SinkConfig,
) -> Result<(HashMap<String, String>, Value, String)> {
    let response_block = sink.response.to_owned().unwrap();

    if !TokenCache::contains_source_id(&sink.source_id).await {
        return Err(anyhow!("source input id {} not found", sink.source_id));
    }

    let mut headers = HashMap::new();
    if let Some(hmap) = &response_block.headers {
        for (k, field) in hmap {
            let val = render_field_to_string_axum(&sink.source_id, field).await?;
            headers.insert(k.clone(), val);
        }
    }

    let mut body_obj = serde_json::Map::new();
    if let Some(body_map) = &response_block.body {
        for (k, field) in body_map {
            let v = render_field_to_json_axum(&sink.source_id, field).await?;
            body_obj.insert(k.clone(), v);
        }
    }

    Ok((
        headers,
        Value::Object(body_obj),
        response_block.content_type.clone(),
    ))
}

async fn render_field_to_string_axum(input: &str, field: &ResponseField) -> Result<String> {
    match field {
        ResponseField::Token { id } => TokenCache::get(&input, &id)
            .await
            .ok_or_else(|| anyhow!("type: string token id {}.{} doesnt exists", input, id))
            .map(|token_context| token_context.token.value),
        ResponseField::String { value } => Ok(value.clone()),
        ResponseField::Expiration { .. } => {
            let v = render_field_to_json_axum(input, field).await?;
            Ok(v.to_string())
        }
    }
}

async fn render_field_to_json_axum(input: &str, field: &ResponseField) -> Result<Value> {
    match field {
        ResponseField::Token { id } => TokenCache::get(&input, &id)
            .await
            .map(|token_context| Value::String(token_context.token.value))
            .ok_or_else(|| anyhow!("type:jwt token id {}.{} doesnt exists", input, id)),
        ResponseField::String { value } => Ok(Value::String(value.clone())),
        ResponseField::Expiration { format, id } => {
            let now = Utc::now().timestamp();
            TokenCache::get(&input, &id)
                .await
                .ok_or_else(|| anyhow!("type: jwt expiration id {}.{} doesnt exists", input, id))
                .map(|token_context| {
                    let mut remaining: i64 = token_context.token.exp_unix_ts as i64 - now;
                    if remaining < 0 {
                        remaining = 0;
                    }
                    match format {
                        ExpirationSinkFormat::Seconds => Value::Number((remaining).into()),
                        ExpirationSinkFormat::Rfc3339 => {
                            Utc.timestamp_opt(token_context.token.exp_unix_ts as i64, 0)
                                .single()
                                .map(|date_time| date_time.to_rfc3339()) // default RFC3339
                                .map(|v| Value::String(v))
                                .ok_or_else(|| {
                                    anyhow!("invalid timestamp for {}.{}", input, token_context.id)
                                })
                                .unwrap()
                        }
                        ExpirationSinkFormat::Unix => {
                            Value::Number(token_context.token.exp_unix_ts.into())
                        }
                    }
                })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::StatusCode;
    use serial_test::serial;
    use std::collections::HashMap;

    use crate::cache::token_context::TokenContext;
    use crate::config::sinks::{HttpResponseBlock, ResponseField, SinkConfig, SinkType};
    use crate::server::server::AppState;
    use crate::{
        cache::{token::Token, token_cache::TokenCache},
        tests::common::{build_reqwest_client, spawn_axum},
    };

    #[tokio::test]
    #[serial]
    async fn test_http_sink_serves_token_from_cache() -> anyhow::Result<()> {
        TokenCache::cleanup().await;
        // -------------------------------
        // 1. Setup TokenCache
        // -------------------------------
        let source_id = "source-123".to_string();
        let token_id = "token-abc".to_string();
        let token_value = "super_secret_token";
        let exp_unix_ts = 5_000_000_000;
        let safety_margin_seconds = 10;

        let token = Token {
            value: token_value.to_string(),
            exp_unix_ts,
        };
        let token_ctx = TokenContext::new(token_id.clone(), token, safety_margin_seconds);
        TokenCache::set(source_id.clone(), vec![token_ctx]).await?;

        // -------------------------------
        // 2. Create SinkConfig with HTTP response
        // -------------------------------
        let mut body_map = HashMap::new();
        body_map.insert(
            "access_token".to_string(),
            ResponseField::Token {
                id: token_id.clone(),
            },
        );
        body_map.insert(
            "exp_unix".to_string(),
            ResponseField::Expiration {
                format: ExpirationSinkFormat::Unix,
                id: token_id.clone(),
            },
        );

        let response_block = HttpResponseBlock {
            content_type: "application/json".to_string(),
            headers: None,
            body: Some(body_map),
        };

        let sink_id = "sink-http-1";
        let sink_config = SinkConfig {
            sink_id: sink_id.to_string(),
            sink_type: SinkType::Http,
            source_id: source_id.clone(),
            path: "/tokens/test".to_string(),
            token_id: token_id.clone(),
            response: Some(response_block),
        };

        // -------------------------------
        // 3. Setup SinkHttpState and router
        // -------------------------------
        let mut sinks = HashMap::new();
        sinks.insert(sink_id.to_string(), sink_config.clone());

        let sink_http_state = SinkHttpState::new(&sinks)?;
        let router = sink_http_state.router().await;

        let metrics = &get_metrics().await;
        let app_state = AppState::new(metrics, &sinks);

        let app: Router = router.with_state(app_state);

        // Spawn ephemeral Axum test server
        let (handle, addr) = spawn_axum(app).await;
        let client = build_reqwest_client();

        // -------------------------------
        // 4. Send HTTP request
        // -------------------------------
        let url = format!("http://{}{}", addr, "/tokens/test");
        let response = client.get(&url).send().await.unwrap();

        // -------------------------------
        // 5. Assert status and body
        // -------------------------------
        assert_eq!(response.status(), StatusCode::OK);

        let json: Value = response.json().await.expect("invalid JSON");
        assert_eq!(json["access_token"], token_value);
        assert_eq!(json["exp_unix"], exp_unix_ts);

        handle.abort();
        Ok(())
    }

    
    #[tokio::test]
    #[serial]
    async fn test_http_sink_returns_404_if_token_absent() -> anyhow::Result<()> {
        TokenCache::cleanup().await;
        // -------------------------------
        // 1. Setup TokenCache
        // -------------------------------
        let source_id = "source-123".to_string();
        let token_id = "token-abc".to_string();
        // let token_value = "super_secret_token";
        // let exp_unix_ts = 5_000_000_000;
        // let safety_margin_seconds = 10;

        // let token = Token {
        //     value: token_value.to_string(),
        //     exp_unix_ts,
        // };
        // let token_ctx = TokenContext::new(token_id.clone(), token, safety_margin_seconds);
        TokenCache::set(source_id.clone(), vec![]).await?;

        // -------------------------------
        // 2. Create SinkConfig with HTTP response
        // -------------------------------
        let mut body_map = HashMap::new();
        body_map.insert(
            "access_token".to_string(),
            ResponseField::Token {
                id: token_id.clone(),
            },
        );
        body_map.insert(
            "exp_unix".to_string(),
            ResponseField::Expiration {
                format: ExpirationSinkFormat::Unix,
                id: token_id.clone(),
            },
        );

        let response_block = HttpResponseBlock {
            content_type: "application/json".to_string(),
            headers: None,
            body: Some(body_map),
        };

        let sink_id = "sink-http-1";
        let sink_config = SinkConfig {
            sink_id: sink_id.to_string(),
            sink_type: SinkType::Http,
            source_id: source_id.clone(),
            path: "/tokens/test".to_string(),
            token_id: token_id.clone(),
            response: Some(response_block),
        };

        // -------------------------------
        // 3. Setup SinkHttpState and router
        // -------------------------------
        let mut sinks = HashMap::new();
        sinks.insert(sink_id.to_string(), sink_config.clone());

        let sink_http_state = SinkHttpState::new(&sinks)?;
        let router = sink_http_state.router().await;

        let metrics = &get_metrics().await;
        let app_state = AppState::new(metrics, &sinks);

        let app: Router = router.with_state(app_state);

        // Spawn ephemeral Axum test server
        let (handle, addr) = spawn_axum(app).await;
        let client = build_reqwest_client();

        // -------------------------------
        // 4. Send HTTP request
        // -------------------------------
        let url = format!("http://{}{}", addr, "/tokens/test");
        let response = client.get(&url).send().await.unwrap();

        // -------------------------------
        // 5. Assert status and body
        // -------------------------------
        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        handle.abort();
        Ok(())
    }

}
