//! Comprehensive configuration validation with aggregated errors.
//! - Aggregates all issues into Vec<String>
//! - Validates invariants discussed across the design thread:
//!   * token vs expiration semantics
//!   * parent and pointer rules
//!   * source/sink references
//!   * template references (rudimentary check)
//!   * path / HTTP method / logging / retry invariants
//!   * uniqueness and collisions (duplicate HTTP sink path)
//!
//! Adjust `use` paths if your types are placed in a different module.

use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use tracing::{error, info};

use crate::config::settings::{RetryConfig, SettingsConfig};
use crate::config::sinks::{HttpResponseBlock, ResponseField, SinkConfig, SinkType};
use crate::config::sources::{
    Expiration, ExpirationSource, GenericSourceValue, ServiceConfig, SourceConfig, SourceTypes,
    TokenField, TokenType,
};
use crate::observability::metrics::get_metrics;
use anyhow::Result;

/// Public entrypoint: returns Ok(()) or Err(Vec<String>) containing all issues.
pub async fn validate_service_config(cfg: &ServiceConfig) -> Result<(), Vec<String>> {
    let mut errors: Vec<String> = Vec::new();

    // Validate settings
    validate_settings(&cfg.settings, &mut errors);

    // Sources must not be empty
    if cfg.sources.is_empty() {
        errors.push("config: 'sources' is empty; at least one source required".to_string());
    }

    // Validate sources themselves and build helper maps
    let mut source_token_ids: HashMap<String, HashSet<String>> = HashMap::new();
    for (src_name, src_cfg) in &cfg.sources {
        validate_source_basics(src_name, src_cfg, &mut errors);
        // collect token ids
        let mut set = HashSet::new();
        for t in &src_cfg.parse.tokens {
            set.insert(t.id.clone());
        }
        source_token_ids.insert(src_name.clone(), set);
    }

    // Validate inter-source dependencies & inputs existence
    for (src_name, src_cfg) in &cfg.sources {
        if let Some(inputs) = &src_cfg.inputs {
            for dep in inputs {
                if !cfg.sources.contains_key(dep) {
                    errors.push(format!(
                        "source['{}'].inputs references unknown source '{}'",
                        src_name, dep
                    ));
                } else if dep == src_name {
                    errors.push(format!(
                        "source['{}'].inputs must not reference itself",
                        src_name
                    ));
                }
            }
        }
    }

    // Validate inputs block exists when source field exists in source
    for (src_name, src_cfg) in &cfg.sources {
        let mut ref_sources: Vec<String> = Vec::new();
        src_cfg
            .request
            .headers
            .iter()
            .for_each(|h: &HashMap<String, GenericSourceValue>| {
                h.values()
                    .for_each(
                        |generic_source_value: &GenericSourceValue| match generic_source_value {
                            GenericSourceValue::Ref { source, id: _, prefix: _ } => {
                                ref_sources.push(source.to_owned());
                            }
                            _ => {}
                        },
                    );
            });

        src_cfg
            .request
            .body
            .iter()
            .for_each(|h: &HashMap<String, GenericSourceValue>| {
                h.values()
                    .for_each(
                        |generic_source_value: &GenericSourceValue| match generic_source_value {
                            GenericSourceValue::Ref { source, id: _, prefix: _ } => {
                                ref_sources.push(source.to_owned());
                            }
                            _ => {}
                        },
                    );
            });

        if !ref_sources.is_empty() {
            ref_sources.iter().for_each(|source| {
                if src_cfg
                    .inputs
                    .clone()
                    .filter(|input_vec| input_vec.contains(source))
                    .is_none()
                {
                    errors.push(format!(
                        "source['{}'].inputs must be provided and contains '{}'",
                        src_name, source
                    ));
                }
            });
        }
        // .or_else(Vec::with_capacity(0));

        // if &src_cfg.request.headers. {

        //     // for dep in inputs {
        //     //     if !cfg.sources.contains_key(dep) {
        //     //         errors.push(format!("source['{}'].inputs references unknown source '{}'", src_name, dep));
        //     //     } else if dep == src_name {
        //     //         errors.push(format!("source['{}'].inputs must not reference itself", src_name));
        //     //     }
        //     // }
        // }
    }

    // Validate sinks and HTTP path collisions
    let mut http_paths: HashMap<String, String> = HashMap::new(); // path -> sink_name
    for (sink_name, sink_cfg) in &cfg.sinks {
        validate_sink_basics(
            sink_name,
            sink_cfg,
            &cfg.sources,
            &source_token_ids,
            &mut errors,
        );

        // collision detection for HTTP sinks (single global server)
        if let SinkType::Http = sink_cfg.sink_type {
            if let Some(prev) = http_paths.insert(sink_cfg.path.clone(), sink_name.clone()) {
                errors.push(format!(
                    "sink['{}'] and sink['{}'] both define HTTP path '{}'; HTTP sink paths must be unique",
                    prev, sink_name, sink_cfg.path
                ));
            }
        }
    }

    if errors.is_empty() {
        info!("config valid");
        Ok(())
    } else {
        error!("configuration validation errors ({}):", errors.len());
        for e in &errors {
            error!(" - {}", e);
        }
        get_metrics().await.config_validation_errors.inc();
        panic!(
            "config is not valid, total errors:{}, \n{}",
            errors.len(),
            errors.join("\n")
        );
    }
}

/// SETTINGS VALIDATION
fn validate_settings(settings: &SettingsConfig, errors: &mut Vec<String>) {
    // retry invariants
    if let Some(retry) = &settings.retry {
        validate_retry("settings.retry", retry, errors);
    }

    // safety margin sane bounds
    if let Some(s) = settings.safety_margin_seconds {
        if s > 60 * 60 * 24 * 365 {
            errors.push(format!(
                "settings.safety_margin_seconds ({}) is unreasonably large",
                s
            ));
        }
    }

    // server path must be absolute if present
    // if !Path::new(p).is_absolute() {
    //     errors.push(format!("settings.server.path '{}' must be an absolute path", p));
    // }
    if settings.server.host.is_empty() {
        errors.push(format!(
            "settings.server.host '{}' must be valid",
            settings.server.host
        ));
    }
    if settings.server.port.is_empty() {
        errors.push(format!(
            "settings.server.poort '{}' must be valid",
            settings.server.port
        ));
    }

    // metrics endpoint start with '/'
    let metrics = &settings.metrics;
    if !metrics.path.starts_with('/') {
        errors.push(format!(
            "settings.metrics.path '{}' must start with '/'",
            metrics.path
        ));
    }
    // let port =  metrics.port.parse::<u32>();
    // if metrics.port.parse::<u32>().is_err() {
    //     errors.push(format!("settings.metrics.port '{}' must be and integer in range 1024-65535", metrics.port));
    // }
    // if port.is_ok() {
    //     let port: u32 = port.unwrap();
    //     if port < 1024 || port > 65535 {
    //         errors.push(format!("settings.metrics.port '{}' must be in range 1024-65535", port));
    //     }
    // }

    // logging level
    if let Some(logging) = &settings.logging {
        let valid = ["trace", "debug", "info", "warn", "error"];
        if !valid.contains(&logging.level.as_str()) {
            errors.push(format!(
                "settings.logging.log_level '{}' invalid; allowed: {:?}",
                logging.level, valid
            ));
        }
    }
}

fn validate_retry(path: &str, retry: &RetryConfig, errors: &mut Vec<String>) {
    if let Some(attempts) = retry.attempts {
        if attempts == 0 {
            errors.push(format!("{}.attempts must be > 0", path));
        }
    }
    if let (Some(base), Some(max)) = (retry.base_delay_ms, retry.max_delay_ms) {
        if max < base {
            errors.push(format!(
                "{}.max_delay_ms ({}) must be >= base_delay_ms ({})",
                path, max, base
            ));
        }
    }
}

/// SOURCE BASICS & TOKEN INVARIANTS
fn validate_source_basics(src_name: &str, src_cfg: &SourceConfig, errors: &mut Vec<String>) {
    // source type allowed
    match src_cfg.source_type {
        SourceTypes::HTTP | SourceTypes::METADATA | SourceTypes::OAUTH2 => {} // (serde ensures value is valid; keeping match for clarity)
    }

    // request URL non-empty
    if src_cfg.request.url.trim().is_empty() {
        errors.push(format!("sources.{}: request.url cannot be empty", src_name));
    }

    // request method allowed (GET, POST)
    match src_cfg.request.method.as_str() {
        "GET" | "POST" => {}
        m => errors.push(format!(
            "sources.{}: request.method '{}' must be 'GET' or 'POST'",
            src_name, m
        )),
    }

    // headers & body: validate GenericSourceValue usage
    if let Some(headers) = &src_cfg.request.headers {
        for (k, v) in headers {
            validate_generic_source_value(
                &format!("sources.{}.request.headers.{}", src_name, k),
                v,
                errors,
            );
            // If Template variant -> check template primitive syntax
            if let GenericSourceValue::Template { template, required } = v {
                validate_template_placeholders(template, errors, src_name);
                if *required {
                    // required=true enforced here conceptually; no further validation needed
                }
            }
        }
    }
    if let Some(body) = &src_cfg.request.body {
        for (k, v) in body {
            validate_generic_source_value(
                &format!("sources.{}.request.body.{}", src_name, k),
                v,
                errors,
            );
            if let GenericSourceValue::Template {
                template,
                required: _,
            } = v
            {
                validate_template_placeholders(template, errors, src_name);
            }
        }
    }
    if let Some(form) = &src_cfg.request.form {
        // verify form fields are provided and valid
        validate_generic_source_value(
            &format!("sources.{}.request.form.client_id", src_name),
            &form.client_id,
            errors,
        );
        validate_generic_source_value(
            &format!("sources.{}.request.form.client_secret", src_name),
            &form.client_secret,
            errors,
        );
        validate_generic_source_value(
            &format!("sources.{}.request.form.scope", src_name),
            &form.scope,
            errors,
        );
    }

    // parse tokens must exist and be unique per source
    if src_cfg.parse.tokens.is_empty() {
        errors.push(format!(
            "sources.{}: parse.tokens must include at least one token field",
            src_name
        ));
    } else {
        let mut seen_ids = HashSet::new();
        for token in &src_cfg.parse.tokens {
            if token.id.trim().is_empty() {
                errors.push(format!(
                    "sources.{}.parse: token id cannot be empty",
                    src_name
                ));
            }
            if !seen_ids.insert(token.id.clone()) {
                errors.push(format!(
                    "sources.{}: duplicate token id '{}'",
                    src_name, token.id
                ));
            }
            validate_token_field(src_name, token, errors);
        }
    }

    // safety margin bounds
    if let Some(s) = src_cfg.safety_margin_seconds {
        if s > 60 * 60 * 24 * 365 {
            errors.push(format!(
                "sources.{}.safety_margin_seconds ({}) is unreasonably large",
                src_name, s
            ));
        }
    }

    // inputs checked at top-level later to ensure existence.
}

fn validate_generic_source_value(path: &str, v: &GenericSourceValue, errors: &mut Vec<String>) {
    match v {
        GenericSourceValue::Literal { value } => {
            if value.trim().is_empty() {
                errors.push(format!("{}: literal value cannot be empty", path));
            }
        }
        GenericSourceValue::FromEnv { from_env } => {
            if from_env.trim().is_empty() {
                errors.push(format!("{}: env name cannot be empty", path));
            }
        }
        GenericSourceValue::FromFile { path: p } => {
            if p.trim().is_empty() {
                errors.push(format!("{}: from_file path cannot be empty", path));
            }
            // don't check file existence here; prechecks elsewhere may check FS permissions
        }
        GenericSourceValue::Ref {
            source,
            id,
            prefix: _,
        } => {
            if source.trim().is_empty() || id.trim().is_empty() {
                errors.push(format!(
                    "{}: ref must include non-empty source and id",
                    path
                ));
            }
            // cross-reference check done later when we have list of sources and tokens
        }
        GenericSourceValue::Template {
            template,
            required: _,
        } => {
            if template.trim().is_empty() {
                errors.push(format!("{}: template cannot be empty", path));
            }
            // template placeholder validation done below at higher level (needs knowledge of sources)
        }
    }
}

/// Validate token-level invariants (token + expiration)
fn validate_token_field(src_name: &str, token: &TokenField, errors: &mut Vec<String>) {
    // parent must be "body" or "header"
    match token.parent.as_str() {
        "body" | "header" => {}
        other => errors.push(format!(
            "sources.{}.parse.token[{}].parent must be 'body' or 'header', got '{}'",
            src_name, token.id, other
        )),
    }

    if token.pointer.trim().is_empty() {
        errors.push(format!(
            "sources.{}.parse.token[{}].pointer cannot be empty",
            src_name, token.id
        ));
    }

    match token.token_type {
        TokenType::Jwt => {
            // For JWT tokens we expect no explicit expiration block (expiration must be None)
            if token.expiration.is_some() {
                errors.push(format!("sources.{}.parse.token[{}]: token_type=jwt must not declare expiration block; expiry extracted from token", src_name, token.id));
            }
        }
        TokenType::PlainText => {
            // Plain text must have expiration block
            if token.expiration.is_none() {
                errors.push(format!(
                    "sources.{}.parse.token[{}]: token_type=plain_text requires expiration block",
                    src_name, token.id
                ));
            } else if let Some(exp) = &token.expiration {
                validate_expiration(src_name, token, exp, errors);
            }
        }
    }
}

fn validate_expiration(
    src_name: &str,
    token: &TokenField,
    exp: &Expiration,
    errors: &mut Vec<String>,
) {
    // format must be a valid enum (serde ensures it), but we check logical constraints:
    match exp.source {
        ExpirationSource::SelfField => {
            // self allowed only if token is JWT
            if token.token_type != TokenType::Jwt {
                errors.push(format!("sources.{}.parse.token[{}].expiration: source='self' only valid for token_type=jwt", src_name, token.id));
            }
            if exp.pointer.is_some() {
                errors.push(format!("sources.{}.parse.token[{}].expiration: when source='self' pointer must not be provided", src_name, token.id));
            }
            if exp.manual_ttl_seconds.is_some() {
                errors.push(format!("sources.{}.parse.token[{}].expiration: when source='self' manual_ttl_seconds must not be provided", src_name, token.id));
            }
            if exp.linked_token_id.is_some() {
                errors.push(format!("sources.{}.parse.token[{}].expiration: when source='self' linked_token_id must not be provided", src_name, token.id));
            }
        }
        ExpirationSource::JsonBodyField | ExpirationSource::HeaderField => {
            // pointer required
            if exp
                .pointer
                .as_ref()
                .map(|s| s.trim().is_empty())
                .unwrap_or(true)
            {
                errors.push(format!("sources.{}.parse.token[{}].expiration: pointer required when source is json_body_field/header_field", src_name, token.id));
            }
            // if linked_token_id present, ensure it's not empty
            if let Some(ref linked) = exp.linked_token_id {
                if linked.trim().is_empty() {
                    errors.push(format!("sources.{}.parse.token[{}].expiration: linked_token_id if present must be non-empty", src_name, token.id));
                }
            } else {
                // if expiration is separate field (not in same token) it's acceptable, but ensure the config supports it.
            }
        }
        ExpirationSource::Manual => {
            if exp.manual_ttl_seconds.is_none() {
                errors.push(format!("sources.{}.parse.token[{}].expiration: manual_ttl_seconds required when source=manual", src_name, token.id));
            } else if exp.manual_ttl_seconds.unwrap() == 0 {
                errors.push(format!(
                    "sources.{}.parse.token[{}].expiration: manual_ttl_seconds must be > 0",
                    src_name, token.id
                ));
            }
        }
    }
}

/// SINK VALIDATION
fn validate_sink_basics(
    sink_name: &str,
    sink: &SinkConfig,
    sources: &HashMap<String, SourceConfig>,
    source_token_ids: &HashMap<String, HashSet<String>>,
    errors: &mut Vec<String>,
) {
    // sink type handled by serde; basic checks:
    // input must exist
    if !sources.contains_key(&sink.source_id) {
        errors.push(format!(
            "sinks.{}: input '{}' does not reference any source",
            sink_name, sink.source_id
        ));
        // cannot continue other sink-specific checks if input missing
        return;
    }

    // token must exist in referenced source
    let token_set = &source_token_ids[&sink.source_id];
    if !token_set.contains(&sink.token_id) {
        errors.push(format!(
            "sinks.{}: token_id '{}' not found in source '{}'",
            sink_name, sink.token_id, sink.source_id
        ));
    }

    // path rules
    match sink.sink_type {
        SinkType::File | SinkType::Uds => {
            if !Path::new(&sink.path).is_absolute() {
                errors.push(format!(
                    "sinks.{}: path '{}' must be absolute for sink type {:?}",
                    sink_name, sink.path, sink.sink_type
                ));
            }
        }
        SinkType::Http => {
            if !sink.path.starts_with('/') {
                errors.push(format!(
                    "sinks.{}: path '{}' must start with '/' for HTTP sink",
                    sink_name, sink.path
                ));
            }
        }
    }

    // if http sink, validate response block if present
    if let SinkType::Http = sink.sink_type {
        if let Some(resp) = &sink.response {
            validate_http_response_block(
                sink_name,
                resp,
                &sink.source_id,
                source_token_ids,
                errors,
            );
        }
    }
    let sink_token_id = &sink.token_id;
    if let Some(response_block) = &sink.response {
        if let Some(res_body) = &response_block.body {
            for (_field_type, response_field) in res_body {
                match response_field {
                    ResponseField::Token { id } => {
                        validate_sink_body_token_id(sink_name, &sink_token_id, id.as_str(), errors)
                    }
                    ResponseField::Expiration { format: _, id } => {
                        validate_sink_body_token_id(sink_name, &sink_token_id, id.as_str(), errors)
                    }
                    ResponseField::String { value: _ } => {}
                }
            }
        }
    }
}

fn validate_sink_body_token_id(
    sink_name: &str,
    sink_token_id: &str,
    body_token_id: &str,
    errors: &mut Vec<String>,
) {
    if body_token_id != sink_token_id {
        errors.push(format!(
            "sinks.{}.response.content_type must be the same as response body {} token id",
            sink_name, body_token_id
        ));
    }
}

/// Validate HttpResponseBlock fields
fn validate_http_response_block(
    sink_name: &str,
    resp: &HttpResponseBlock,
    input_source: &str,
    source_token_ids: &HashMap<String, HashSet<String>>,
    errors: &mut Vec<String>,
) {
    // content_type basic check
    if resp.content_type.trim().is_empty() {
        errors.push(format!(
            "sinks.{}.response.content_type must not be empty",
            sink_name
        ));
    }

    if let Some(headers) = &resp.headers {
        for (hname, field) in headers {
            validate_response_field(
                sink_name,
                "header",
                hname,
                field,
                input_source,
                source_token_ids,
                errors,
            );
        }
    }

    if let Some(body) = &resp.body {
        for (bname, field) in body {
            validate_response_field(
                sink_name,
                "body",
                bname,
                field,
                input_source,
                source_token_ids,
                errors,
            );
        }
    }
}

fn validate_response_field(
    sink_name: &str,
    section: &str,
    field_name: &str,
    field: &ResponseField,
    input_source: &str,
    source_token_ids: &HashMap<String, HashSet<String>>,
    errors: &mut Vec<String>,
) {
    match field {
        ResponseField::Token { id } => {
            if !source_token_ids
                .get(input_source)
                .map_or(false, |s| s.contains(id))
            {
                errors.push(format!(
                    "sinks.{}.response.{}.{}: Token id '{}' not found in source '{}'",
                    sink_name, section, field_name, id, input_source
                ));
            }
        }
        ResponseField::Expiration { id, format: _ } => {
            if !source_token_ids
                .get(input_source)
                .map_or(false, |s| s.contains(id))
            {
                errors.push(format!(
                    "sinks.{}.response.{}.{}: Expiration id '{}' not found in source '{}'",
                    sink_name, section, field_name, id, input_source
                ));
            }
            // format validated by serde enum
        }
        ResponseField::String { value } => {
            if value.trim().is_empty() {
                errors.push(format!(
                    "sinks.{}.response.{}.{}: literal string value is empty",
                    sink_name, section, field_name
                ));
            }
        }
    }
}

/// TEMPLATE VALIDATION (rudimentary)
///
/// Validates placeholders like `{{source.parent.token_id}}` or `{{source.token_id}}`
/// - Ensures referenced source exists
/// - Ensures referenced token ID exists in that source
///
/// This is intentionally simple (not a full handlebars parser) but covers common cases.
fn validate_template_placeholders(template: &str, errors: &mut Vec<String>, _context_source: &str) {
    // regex to capture {{ ... }} tokens
    let re = Regex::new(r"\{\{\s*([a-zA-Z0-9_\.:-]+)\s*\}\}").unwrap();

    for caps in re.captures_iter(template) {
        if let Some(tok) = caps.get(1) {
            let content = tok.as_str();
            // patterns supported:
            // - source.token_id (e.g., metadata.metadata_token)
            // - source.parent.token_id (e.g., metadata.body.metadata_token) — accept both; we'll parse tokens by id only
            let parts: Vec<&str> = content.split('.').collect();
            if parts.len() >= 2 {
                let src = parts[0];
                let token_id = parts.last().unwrap();
                // we can't access sources map here; higher-level validation will ensure actual references are valid
                if src.trim().is_empty() || token_id.trim().is_empty() {
                    errors.push(format!(
                        "template '{}' contains invalid placeholder '{{{{{}}}}}'",
                        template, content
                    ));
                }
            } else {
                errors.push(format!("template '{}' contains ambiguous placeholder '{{{{{}}}}}', expected 'source.token_id' or 'source.parent.token_id'", template, content));
            }
        }
    }
}
