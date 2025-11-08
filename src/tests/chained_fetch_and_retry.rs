
// This test simulates:
//  - metadata endpoint (GET) -> immediate success
//  - exchange endpoint (POST) -> fails first N times, succeeds later
// Then it runs a simple production-like fetch+retry flow and asserts final success.

#[cfg(test)]
mod test {
    
use std::{sync::{atomic::{AtomicUsize, Ordering}, Arc}, time::Duration};

use axum::{routing::{get, post}, Json, Router};
use http::StatusCode;
use serde_json::json;
use tokio::time::sleep;

use crate::tests::common::{build_reqwest_client, render_template, spawn_axum};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn metadata_to_exchange_with_retries() {
    // metadata server
    let meta_router = Router::new().route("/computeMetadata/v1/instance/service-accounts/default/token", get(|| async {
        Json(json!({"access_token":"meta-abc","expires_in":60}))
    }));
    let (meta_h, meta_addr) = spawn_axum(meta_router).await;

    // exchange server fails first 2 attempts then succeeds
    let counter = Arc::new(AtomicUsize::new(0));
    let counter_clone = counter.clone();
    let exch_router = Router::new().route("/issueToken", post(move |Json(_): Json<serde_json::Value>| {
        let c = counter_clone.clone();
        async move {
            let n = c.fetch_add(1, Ordering::SeqCst);
            if n < 2 {
                (StatusCode::INTERNAL_SERVER_ERROR, "transient".to_owned())
            } else {
                let body = json!({"access_token":"exchange-xyz","expires_in":3600}).to_string();
                (StatusCode::OK, body)
            }
        }
    }));
    let (exch_h, exch_addr) = spawn_axum(exch_router).await;

    // Build reqwest client (simulate token-agent fetcher behavior)
    let client = build_reqwest_client();

    // 1. fetch metadata
    let meta_url = format!("http://{}{}", meta_addr, "/computeMetadata/v1/instance/service-accounts/default/token");
    let meta_resp = client.get(&meta_url).send().await.expect("meta request");
    assert!(meta_resp.status().is_success());
    let meta_json: serde_json::Value = meta_resp.json().await.unwrap();
    let meta_token = meta_json["access_token"].as_str().unwrap().to_string();

    // 2. render header (simple template)
    let mut ctx = std::collections::HashMap::new();
    ctx.insert("metadata.metadata_token".to_string(), meta_token.clone());
    let header_template = "Bearer {{metadata.metadata_token}}";
    let rendered_header = render_template(header_template, &ctx).expect("render template");

    // 3. perform exchange with retries/backoff (production logic should do same)
    let exch_url = format!("http://{}{}", exch_addr, "/issueToken");
    let mut attempts = 0usize;
    let mut final_body: Option<serde_json::Value> = None;
    let max_attempts = 5;
    let base_delay_ms: u64 = 50;
    while attempts < max_attempts {
        attempts += 1;
        let resp = client.post(&exch_url)
            .header("Authorization", rendered_header.clone())
            .json(&json!({"subject_token": meta_token}))
            .send().await;

        match resp {
            Ok(r) if r.status().is_success() => {
                final_body = Some(r.json::<serde_json::Value>().await.unwrap());
                break;
            }
            Ok(_) => {
                // non-2xx -> treat as retryable transient
                let backoff = std::cmp::min(2000, base_delay_ms * (1 << (attempts.saturating_sub(1))));
                sleep(Duration::from_millis(backoff)).await;
            }
            Err(_) => {
                let backoff = std::cmp::min(2000, base_delay_ms * (1 << (attempts.saturating_sub(1))));
                sleep(Duration::from_millis(backoff)).await;
            }
        }
    }

    assert!(final_body.is_some(), "exchange should succeed after retries");
    assert_eq!(counter.load(Ordering::SeqCst), 3, "server should have seen exactly 3 attempts");

    // clean up
    meta_h.abort();
    exch_h.abort();
}

}