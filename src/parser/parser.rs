use crate::cache::token::Token;

use crate::config::sources::{ExpirationSource, ExpirationSourceFormat, JwtClaims, ParseConfig, TokenField, TokenType};
use crate::cache::token_context::TokenContext;
use crate::helpers::time::get_token_safety_margin_seconds;
use anyhow::{anyhow, Result};
use base64::Engine;
use chrono::Utc;
use http::HeaderMap;
use serde_json::Value;
use tracing::{debug, error, warn};


static HEADER_FIELD: &str = "header";

/// Parse both header and body tokens according to configuration.
///
/// Returns all tokens (active + inactive stubs).
pub async fn parse_tokens(
    headers: HeaderMap,
    body: String,
    parse_config: ParseConfig,
    safety_margin_settings: Option<u64>,
    safety_margin_source: Option<u64>,
) -> Result<Vec<TokenContext>> {
    let mut token_context_vec = Vec::with_capacity(parse_config.tokens.len());

    let json_body: Option<Value> = match serde_json::from_str(&body) {
        Ok(v) => Some(v),
        Err(e) => {
            warn!("Body is not valid JSON: {}", e);
            None
        }
    };

    // -------------------------------
    // 1. Parse HEADER tokens
    // -------------------------------

    for token_field in parse_config.tokens.iter().filter(|t| t.parent == HEADER_FIELD) {
        let safety_margin = get_token_safety_margin_seconds(safety_margin_settings, safety_margin_source);

        match parse_header_token(token_field, &headers, json_body.as_ref(), safety_margin) {
            Ok(ctx) => { token_context_vec.push(ctx); },
            Err(e) => {
                error!(id = %token_field.id, error = ?e, "header token parse failed");
            }
        };
    }

    // -------------------------------
    // 2. Parse BODY tokens
    // -------------------------------
    
    for token_field in parse_config.tokens.iter().filter(|t| t.parent == "body") {
        let safety_margin = get_token_safety_margin_seconds(safety_margin_settings, safety_margin_source);

        match parse_body_token(token_field, json_body.as_ref(), &headers, safety_margin)
        {
            Ok(ctx) => {
                token_context_vec.push(ctx);
            },
            Err(e) => { error!(id = %token_field.id, error = ?e, "body token parse failed"); }
        };
    }

    Ok(token_context_vec)
}

/// Handle a header-based token
fn parse_header_token(
    token_field: &TokenField,
    headers: &HeaderMap,
    json_body: Option<&Value>,
    safety_margin: u64,
) -> Result<TokenContext> {
    let token_value = get_header_value(headers, &token_field.pointer)?;
    let expiration = match token_field.token_type {
        TokenType::Jwt => get_jwt_token_expiration(&token_value)?,
        TokenType::PlainText => {
            let json = json_body.ok_or_else(|| anyhow!("body required for plain text token"))?;
            get_plain_text_expiration(token_field, json, headers)?
        }
    };

    Ok(TokenContext::new(
        token_field.id.clone(),
        Token::new(token_value, expiration),
        safety_margin,
    ))
}

/// Handle a body-based token
fn parse_body_token(
    token_field: &TokenField,
    json_body: Option<&Value>,
    headers: &HeaderMap,
    safety_margin: u64,
) -> Result<TokenContext> {
    let json = json_body.ok_or_else(|| anyhow!("missing body for body token"))?;
    let token_value = json[&token_field.pointer]
        .as_str()
        .ok_or_else(|| anyhow!("body field '{}' not found or not a string", token_field.pointer))?
        .to_owned();

    let expiration = match token_field.token_type {
        TokenType::Jwt => get_jwt_token_expiration(&token_value)?,
        TokenType::PlainText => get_plain_text_expiration(token_field, json, headers)?,
    };

    Ok(TokenContext::new(
        token_field.id.clone(),
        Token::new(token_value, expiration),
        safety_margin,
    ))
}

fn decode_jwt_from_string(token_string: &str) -> Result<JwtClaims> {
    let parts: Vec<&str> = token_string.split('.').collect();
    if parts.len() != 3 {
        return Err(anyhow!("invalid JWT format"));
    }

    let payload = parts[1];
    let decoded = base64::engine::general_purpose::STANDARD_NO_PAD
        .decode(payload)
        .map_err(|e| anyhow!("base64 decode error: {}", e))?;

    serde_json::from_slice::<JwtClaims>(&decoded)
        .map_err(|e| anyhow!("invalid JWT payload: {}", e))
}

fn get_jwt_token_expiration(token_value: &str) -> Result<u64> {
    let claims = decode_jwt_from_string(token_value)?;
    let exp = claims.exp;
    let now = Utc::now().timestamp() as u64;

    if exp <= now {
        Err(anyhow!("JWT expired at {}", exp))
    } else {
        debug!(expires_at = exp, "jwt parsed successfully");
        Ok(exp)
    }
}

fn get_plain_text_expiration(
    token_field: &TokenField,
    json_body: &Value,
    headers: &HeaderMap,
) -> Result<u64> {
    let exp_cfg = token_field
        .expiration
        .clone()
        .ok_or_else(|| anyhow!("expiration required for plain_text"))?;

    let exp_row_value_res = match exp_cfg.source {
        ExpirationSource::SelfField => Err(anyhow!(
            "expiration.source=self not valid for plain_text"
        )),
        ExpirationSource::JsonBodyField => {
            let pointer = exp_cfg
                .pointer
                .ok_or_else(|| anyhow!("expiration.pointer required"))?;
            json_body[&pointer]
                .as_u64()
                .ok_or_else(|| anyhow!("body {} not found or not u64, ", &pointer))
        }
        ExpirationSource::HeaderField => {
            let key = exp_cfg
                .pointer
                .ok_or_else(|| anyhow!("expiration.pointer required"))?;
            let val = get_header_value(headers, &key)?;
            val.parse::<u64>()
                .map_err(|e| anyhow!("invalid header value '{}': {}", key, e))
        }
        ExpirationSource::Manual => {
             exp_cfg.manual_ttl_seconds.ok_or_else(|| {
                anyhow!("manual_ttl_seconds must be provided for manual expiration")
            })
        }
    };
    
    // caclulate token expiration according to token expiration format form config 
    exp_row_value_res
    .map(|exp_row_value| {
        match exp_cfg.format {
            ExpirationSourceFormat::Seconds => Utc::now().timestamp() as u64 + exp_row_value,
            ExpirationSourceFormat::Unix => exp_row_value,
        }
    })

}

fn get_header_value(headers: &HeaderMap, key: &str) -> Result<String> {
    headers
        .get(key)
        .ok_or_else(|| anyhow!("header '{}' not found", key))?
        .to_str()
        .map(|s| s.to_owned())
        .map_err(|e| anyhow!("invalid header '{}': {}", key, e))
}



#[cfg(test)]
mod tests {
    use std::str::FromStr;
    use base64::{engine::general_purpose::STANDARD_NO_PAD, Engine};
    use chrono::{Utc};
    use http::{HeaderMap, HeaderName, HeaderValue};
    use serde_json::json;
    use crate::parser::parser::{ParseConfig, parse_tokens};

    fn sample_jwt(exp: u64) -> String {
        // minimal unsigned JWT for tests: {"exp": exp}
        let header = STANDARD_NO_PAD.encode(r#"{"alg":"none"}"#);
        let payload = STANDARD_NO_PAD
            .encode(format!(r#"{{"exp":{}}}"#, exp));
        format!("{}.{}.", header, payload)
    }

    fn make_headers(map: &[(&str, &str)]) -> HeaderMap {
        let mut headers = HeaderMap::new();
        for (k, v) in map.iter() {
          let key = HeaderName::from_str(k).unwrap();
          let value = HeaderValue::from_str(v).unwrap();
            headers.insert(key, value);
        }
        headers
    }

    fn make_parse_config() -> ParseConfig {
        use crate::config::sources::*;
        ParseConfig {
            tokens: vec![
                // JWT from body
                TokenField {
                    id: "jwt_body".into(),
                    parent: "body".into(),
                    pointer: "jwt_token".into(),
                    token_type: TokenType::Jwt,
                    expiration: None,
                },
                // JWT from header
                TokenField {
                    id: "jwt_header".into(),
                    parent: "header".into(),
                    pointer: "x-jwt".into(),
                    token_type: TokenType::Jwt,
                    expiration: None,
                },
                // Plain text with manual TTL
                TokenField {
                    id: "plain_manual".into(),
                    parent: "body".into(),
                    pointer: "plain_token".into(),
                    token_type: TokenType::PlainText,
                    expiration: Some(Expiration {
                        source: ExpirationSource::Manual,
                        format: ExpirationSourceFormat::Seconds,
                        manual_ttl_seconds: Some(60),
                        pointer: None,
                        linked_token_id: None
                    }),
                },
                // Plain text expiration from JSON field
                TokenField {
                    id: "plain_json_exp".into(),
                    parent: "body".into(),
                    pointer: "plain_json_token".into(),
                    token_type: TokenType::PlainText,
                    expiration: Some(Expiration {
                        source: ExpirationSource::JsonBodyField,
                        format: ExpirationSourceFormat::Unix,
                        manual_ttl_seconds: None,
                        pointer: Some("plain_exp".into()),
                        linked_token_id: None
                    }),
                },
                // Plain text expiration from header
                TokenField {
                    id: "plain_header_exp".into(),
                    parent: "header".into(),
                    pointer: "x-plain".into(),
                    token_type: TokenType::PlainText,
                    expiration: Some(Expiration {
                        source: ExpirationSource::HeaderField,
                        format: ExpirationSourceFormat::Unix,
                        manual_ttl_seconds: None,
                        pointer: Some("x-exp".into()),
                        linked_token_id: None
                    }),
                },
            ],
        }
    }

    #[tokio::test]
    async fn test_jwt_valid_and_expired() {
        let now = Utc::now().timestamp() as u64;
        let valid_jwt = sample_jwt(now + 60);
        let expired_jwt = sample_jwt(now - 30);

        let headers = make_headers(&[("x-jwt", valid_jwt.as_str())]);
        let config = make_parse_config();

        let body = json!({ "jwt_token": expired_jwt }).to_string();

        let tokens = parse_tokens(headers, body, config, None, None).await.unwrap();

        // jwt_header → active
        let header_token = tokens.iter().find(|t| t.id == "jwt_header").unwrap();
        assert_eq!(header_token.should_remove(), false);

        // jwt_body → expired, so inactive stub
        let body_token_opt = tokens.iter().find(|t| t.id == "jwt_body");
        assert_eq!(body_token_opt.is_none(), true);
    }

    #[tokio::test]
    async fn test_plain_text_manual_expiration() {
        let headers = HeaderMap::new();
        let config = make_parse_config();
        let body = json!({
            "plain_token": "abc123"
        })
        .to_string();

        let tokens = parse_tokens(headers, body, config, None, None).await.unwrap();
        let t = tokens.iter().find(|t| t.id == "plain_manual").unwrap();

        assert_eq!(t.should_remove(), false);
        let exp = t.token.exp_unix_ts;
        assert!(exp > Utc::now().timestamp() as u64);
    }

    #[tokio::test]
    async fn test_plain_text_json_expiration() {
        let now = Utc::now().timestamp() as u64;
        let headers = HeaderMap::new();
        let config = make_parse_config();

        let body = json!({
            "plain_json_token": "xyz",
            "plain_exp": now + 100
        })
        .to_string();

        let tokens = parse_tokens(headers, body, config, None, None).await.unwrap();
        let t = tokens.iter().find(|t| t.id == "plain_json_exp").unwrap();
        assert_eq!(t.should_remove(), false);
    }

    #[tokio::test]
    async fn test_plain_text_header_expiration() {
        let now = Utc::now().timestamp() as u64;
        let headers = make_headers(&[
            ("x-plain", "pln"),
            ("x-exp", &(now + 30).to_string()),
        ]);
        let config = make_parse_config();

        let body = "{}".to_string();
        let tokens = parse_tokens(headers, body, config, None, None).await.unwrap();

        let t = tokens.iter().find(|t| t.id == "plain_header_exp").unwrap();
        assert_eq!(t.should_remove(), false);
    }

    #[tokio::test]
    async fn test_missing_header_field() {
        let headers = HeaderMap::new();
        let config = make_parse_config();
        let body = json!({ "jwt_token": sample_jwt(Utc::now().timestamp() as u64 + 60) }).to_string();

        let tokens = parse_tokens(headers, body, config, None, None).await.unwrap();
        let header_token_opt = tokens.iter().find(|t| t.id == "jwt_header");        
        assert_eq!(header_token_opt.is_none(), true);
    }

    #[tokio::test]
    async fn test_invalid_json_body() {
        let headers = make_headers(&[("x-jwt", sample_jwt(Utc::now().timestamp() as u64 + 60).as_str())]);
        let config = make_parse_config();
        let body = "{invalid_json".to_string();

        let tokens = parse_tokens(headers, body, config, None, None).await.unwrap();
        let jwt_header = tokens.iter().find(|t| t.id == "jwt_header").unwrap();
        assert_eq!(jwt_header.should_remove(), false);
    }
}

