// tests/common/mod.rs
pub use axum::{Router, body::Body};
pub use serde_json::json;
pub use tokio::task::JoinHandle;

use std::net::SocketAddr;
use reqwest::Client;
use regex::Regex;
use std::collections::HashMap;

/// Spawn an Axum router on an ephemeral port and return (JoinHandle, SocketAddr)
pub async fn spawn_axum(router: Router) -> (JoinHandle<()>, SocketAddr) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind failed");
    let addr = listener.local_addr().unwrap();
    let handle = tokio::spawn(async move {
        axum::serve(listener, router).await.expect("server failed");
    });
    (handle, addr)
}

/// Simple, deterministic template renderer (tests only).
/// Accepts placeholders like `{{source.token_id}}` or `{{source.parent.token_id}}`.
/// Returns None if any placeholder has no matching key in `ctx`.
pub fn render_template(template: &str, ctx: &HashMap<String, String>) -> Option<String> {
    let re = Regex::new(r"\{\{\s*([a-zA-Z0-9_\.:-]+)\s*\}\}").unwrap();
    let mut out = template.to_owned();
    for caps in re.captures_iter(template) {
        let key = caps.get(1).unwrap().as_str();
        if let Some(val) = ctx.get(key) {
            out = out.replace(&format!("{{{{{}}}}}", key), val);
        } else {
            return None;
        }
    }
    Some(out)
}

pub fn build_reqwest_client() -> Client {
    Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .expect("reqwest client")
}
