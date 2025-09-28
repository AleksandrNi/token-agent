// use crate::cache::token_cache::TOKEN_CACHE;
// use crate::config::types::{ParseConfig, ParseField};
// use anyhow::{Result, anyhow};
// use http::HeaderMap;
// use serde_json::Value;
// use chrono::Utc;

// /// Parse HTTP/metadata response and store parent-aware fields in TOKEN_CACHE
// pub async fn parse_response(
//     source_name: &str,
//     cfg: &ParseConfig,
//     body_str: &str,
//     headers: &HeaderMap,
// ) -> Result<()> {
//     let mut fields_map = std::collections::HashMap::new();
//     let json_body: Option<Value> = serde_json::from_str(body_str).ok();

//     let mut expires_at: i64 = Utc::now().timestamp() + 3600; // fallback default

//     for field in &cfg.fields {
//         let (parent, field_name) = match field {
//             ParseField::Body { name, .. } => ("body", name),
//             ParseField::Header { name, .. } => ("header", name),
//             ParseField::Status { name, .. } => ("status", name),
//         };

//         let value_opt = match field {
//             ParseField::Body { pointer, r#type, default, .. } => {
//                 if let Some(json) = &json_body {
//                     json.pointer(pointer)
//                         .map(|v| apply_type(v.clone(), r#type))
//                         .transpose()?
//                         .or(default.clone())
//                 } else {
//                     default.clone()
//                 }
//             }
//             ParseField::Header { key, r#type, default, .. } => {
//                 headers.get(key)
//                     .and_then(|v| v.to_str().ok())
//                     .map(|s| apply_type(Value::String(s.to_string()), r#type).ok())
//                     .flatten()
//                     .or(default.clone())
//             }
//             ParseField::Status { default, .. } => default.clone(),
//         };

//         if let Some(val) = value_opt {
//             let cache_key = format!("{}.{}", parent, field_name);
//             fields_map.insert(cache_key.clone(), val.clone());

//             // Update expiration if field represents expiry
//             if field_name == "expires_at" || field_name == "expires_in" {
//                 expires_at = val.parse::<i64>().unwrap_or(expires_at);
//             }
//         }
//     }

//     TOKEN_CACHE.set(source_name, fields_map, expires_at).await;

//     Ok(())
// }

// /// Convert JSON Value to string according to optional type
// fn apply_type(val: Value, r#type: &Option<String>) -> Result<String> {
//     match r#type.as_deref() {
//         Some("string") | None => Ok(val.to_string().trim_matches('"').to_string()),
//         Some("int") => val.as_i64()
//             .map(|v| v.to_string())
//             .ok_or_else(|| anyhow!("Expected int value")),
//         Some("datetime") => {
//             let s = val.as_str().ok_or_else(|| anyhow!("Expected datetime string"))?;
//             let ts = chrono::DateTime::parse_from_rfc3339(s)
//                 .map_err(|_| anyhow!("Invalid datetime format"))?
//                 .timestamp();
//             Ok(ts.to_string())
//         }
//         Some(t) => Err(anyhow!("Unknown type transform: {}", t)),
//     }
// }
