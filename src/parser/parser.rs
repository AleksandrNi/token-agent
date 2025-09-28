use std::collections::HashMap;

use crate::cache::token_cache::{Token, TOKEN_CACHE};
use crate::config::types::{FieldKind, ParseConfig, ParseField};
use anyhow::{anyhow, Error, Result};
use http::HeaderMap;
use jsonwebtoken::{ decode, DecodingKey, Validation};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Parse source response and store in cache
pub async fn parse_response(
    source_name: &str,
    cfg: &ParseConfig,
    body_str: &str,
    headers: &HeaderMap,
) -> Result<Token> {
    let mut fields_map: HashMap<String, String> = HashMap::new();
    let json_body: Option<Value> = serde_json::from_str(body_str).ok();

    let mut token_str: Option<String> = None;
    let mut expires_val: Option<i64> = None;

    for field in &cfg.fields {
        let (parent, field_name, value_opt) = match field {
            ParseField::Body {
                name,
                pointer,
                r#type,
                default,
                ..
            } => {
                let val = json_body
                    .as_ref()
                    .and_then(|json| {
                        json.pointer(pointer)
                            .map(|v| apply_type(v.clone(), r#type).ok())
                            .flatten()
                    })
                    .or(default.clone());
                ("body", name, val)
            }
            ParseField::Header {
                name,
                key,
                r#type,
                default,
                ..
            } => {
                let val = headers
                    .get(key)
                    .and_then(|v| v.to_str().ok())
                    .map(|s| apply_type(Value::String(s.to_string()), r#type).ok())
                    .flatten()
                    .or(default.clone());
                ("header", name, val)
            }
            ParseField::Status { name, default, .. } => ("status", name, default.clone()),
        };

        if let Some(val) = value_opt.clone() {
            let cache_key = format!("{}.{}", parent, field_name);
            fields_map.insert(cache_key.clone(), val.clone());

            match field.kind() {
                FieldKind::Token => token_str = Some(val),
                FieldKind::Expiration => expires_val = Some(parse_expiration(&val)?),
                FieldKind::Other => {}
            }
        }
    }

    let token_value = token_str.ok_or_else(|| anyhow!("Token not found in parsed response"))?;

    // Compute expiration from JWT if not explicitly provided
    let expires_at = if let Some(exp) = expires_val {
        exp
    } else {
        extract_jwt_exp(&token_value)?
    };

    let token = Token {
        value: token_value,
        expires_at,
    };

    TOKEN_CACHE.set(source_name, token.clone()).await;

    Ok(token)
}

/// Decode JWT `exp` claim
fn extract_jwt_exp(token: &str) -> Result<i64, Error> {
    // let data = dangerous_insecure_decode::<Value>(token)
    //     .map_err(|e| anyhow!("Failed to decode JWT: {}", e))?;
    // let exp = data.claims.get("exp")
    //     .and_then(|v| v.as_i64())
    //     .ok_or_else(|| anyhow!("JWT missing 'exp' claim"))?;
    // Ok(exp)
    let decoding_key = DecodingKey::from_secret("dummy_secret".as_bytes());

    // Configure Validation to skip signature verification
    let mut validation = Validation::new(jsonwebtoken::Algorithm::HS256); // Use the algorithm specified in the token header
    validation.validate_exp = false; // Disable expiration validation for this example
    validation.validate_nbf = false; // Disable not-before validation if present
    validation.insecure_disable_signature_validation(); // Crucial for parsing without a secret

    match decode::<Claims>(&token, &decoding_key, &validation) {
        Ok(token_data) => {
            println!("Decoded claims: {:?}", token_data.claims);
            println!("Expiration time (exp): {}", token_data.claims.exp);
            Ok(token_data.claims.exp.try_into().unwrap())
        }
        Err(err) => { Err(err.into()) }
    }
}

/// Convert JSON value to string with optional type
fn apply_type(val: Value, r#type: &Option<String>) -> Result<String> {
    match r#type.as_deref() {
        Some("string") | None => Ok(val.to_string().trim_matches('"').to_string()),
        Some("int") => val
            .as_i64()
            .map(|v| v.to_string())
            .ok_or_else(|| anyhow!("Expected int value")),
        Some("datetime") => {
            let s = val
                .as_str()
                .ok_or_else(|| anyhow!("Expected datetime string"))?;
            let ts = chrono::DateTime::parse_from_rfc3339(s)
                .map_err(|_| anyhow!("Invalid datetime format"))?
                .timestamp();
            Ok(ts.to_string())
        }
        Some(t) => Err(anyhow!("Unknown type transform: {}", t)),
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    sub: String,
    exp: usize,
    // Add other claims as needed
}

/// Parse expiration field (integer seconds)
fn parse_expiration(val: &str) -> Result<i64> {
    val.parse::<i64>()
        .map_err(|e| anyhow!("Invalid expiration value: {}", e))
}
