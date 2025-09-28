// use crate::cache::token_cache::Token;
// use crate::config::types::{ParseConfig, ParseField};
// use anyhow::{anyhow, Result};
// use http::HeaderMap;
// use jsonptr::get as json_ptr;
// use serde_json::Value;
// use std::collections::HashMap;
// use tracing::warn;

// /// Extracts token and expiry safely, tracking (name, source) separately
// pub fn parse_response(cfg: &ParseConfig, body: &str, headers: &HeaderMap) -> Result<Token> {
//     let mut extracted: HashMap<(String, String), String> = HashMap::new();

//     let json_body: Option<Value> = serde_json::from_str(body).ok();

//     for field in &cfg.fields {
//         let source_type = match field {
//             ParseField::Body { .. } => "body",
//             ParseField::Header { .. } => "header",
//         }
//         .to_string();

//         let val = extract_field(field, &json_body, headers)?;

//         if let Some(v) = val {
//             extracted.insert((field_name(field)?, source_type), v);
//         }
//     }

//     // Determine token and expiry with priority: body first, then header
//     let token_val = extracted
//         .get(&("token".to_string(), "body".to_string()))
//         .or_else(|| extracted.get(&("token".to_string(), "header".to_string())))
//         .cloned()
//         .ok_or_else(|| anyhow!("Missing token"))?;

//     let expires_val = extracted
//         .get(&("expires_in".to_string(), "body".to_string()))
//         .or_else(|| extracted.get(&("expires_at".to_string(), "body".to_string())))
//         .or_else(|| extracted.get(&("expires_in".to_string(), "header".to_string())))
//         .or_else(|| extracted.get(&("expires_at".to_string(), "header".to_string())))
//         .and_then(|s| s.parse::<u64>().ok())
//         .unwrap_or(3600);

//     Ok(Token::new(token_val, expires_val))
// }

// /// Extract single field value with fallback and transform
// fn extract_field(field: &ParseField, json_body: &Option<Value>, headers: &HeaderMap) -> Result<Option<String>> {
//     let mut val: Option<String> = None;

//     match field {
//         ParseField::Body { pointer, r#type, fallback, transform, .. } => {
//             val = json_body
//                 .as_ref()
//                 .and_then(|b| json_ptr(b, pointer).ok())
//                 .map(|v| apply_type(v, r#type).ok())
//                 .flatten()
//                 .or_else(|| fallback.clone());

//             if let Some(transforms) = transform {
//                 for t in transforms {
//                     if let Some(ref s) = val {
//                         val = Some(apply_transform(s, t)?);
//                     }
//                 }
//             }
//         }
//         ParseField::Header { key, r#type, fallback, transform, .. } => {
//             val = headers.get(key)
//                 .and_then(|h| h.to_str().ok())
//                 .map(|s| apply_type(Value::String(s.to_string()), r#type).ok())
//                 .flatten()
//                 .or_else(|| fallback.clone());

//             if let Some(transforms) = transform {
//                 for t in transforms {
//                     if let Some(ref s) = val {
//                         val = Some(apply_transform(s, t)?);
//                     }
//                 }
//             }
//         }
//     }

//     Ok(val)
// }

// /// Extract the logical name from ParseField
// fn field_name(field: &ParseField) -> Result<String> {
//     match field {
//         ParseField::Body { name, .. } => Ok(name.clone()),
//         ParseField::Header { name, .. } => Ok(name.clone()),
//     }
// }

// /// Type conversion
// fn apply_type(val: Value, r#type: &Option<String>) -> Result<String> {
//     match r#type.as_deref() {
//         Some("int") => Ok(val.as_i64().ok_or_else(|| anyhow!("Not int"))?.to_string()),
//         Some("datetime") => {
//             let s = val.as_str().ok_or_else(|| anyhow!("Not datetime string"))?;
//             Ok(chrono::DateTime::parse_from_rfc3339(s)?.timestamp().to_string())
//         }
//         _ => Ok(val.to_string().trim_matches('"').to_string()),
//     }
// }

// /// Simple transforms (extend as needed)
// fn apply_transform(val: &str, t: &str) -> Result<String> {
//     match t {
//         "trim" => Ok(val.trim().to_string()),
//         _ => Ok(val.to_string()),
//     }
// }
