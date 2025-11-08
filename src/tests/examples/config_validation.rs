// #[tokio::test]
// async fn validate_examples_aws_assume_role_with_webidentity_is_valid() {
//     let path = Path::new("examples/aws_assume_role_with_webidentity.yaml");
//     let service_config: ServiceConfig = file_to_config(path).await.expect("/examples/aws_assume_role_with_webidentity.yaml must exist in repo root for tests");
//     validate_service_config(&service_config).await.unwrap();
// }

// #[tokio::test]
// async fn validate_examples_aws_imds_and_role_creds_is_valid() {
//     let path = Path::new("examples/aws_imds_and_role_creds.yaml");
//     let service_config: ServiceConfig = file_to_config(path).await.expect("/examples/aws_imds_and_role_creds.yaml must exist in repo root for tests");
//     validate_service_config(&service_config).await.unwrap();
// }
// #[tokio::test]
// async fn validate_examples_azure_managed_identity_is_valid() {
//     let path = Path::new("examples/azure_managed_identity.yaml");
//     let service_config: ServiceConfig = file_to_config(path).await.expect("/examples/azure_managed_identity.yaml must exist in repo root for tests");
//     validate_service_config(&service_config).await.unwrap();
// }

// #[tokio::test]
// async fn validate_examples_azure_oauth2_token_exchange_is_valid() {

//     let path = Path::new("examples/azure_oauth2_token_exchange.yaml");
//     let service_config: ServiceConfig = file_to_config(path).await.expect("/examples/azure_oauth2_token_exchange.yaml must exist in repo root for tests");
//     validate_service_config(&service_config).await.unwrap();
// }

// #[tokio::test]
// async fn validate_examples_basic_metadata_token_is_valid() {

//     let path = Path::new("examples/basic_metadata_token.yaml");
//     let service_config: ServiceConfig = file_to_config(path).await.expect("/examples/basic_metadata_token.yaml must exist in repo root for tests");
//     validate_service_config(&service_config).await.unwrap();
// }

#[cfg(test)]
mod tests {

    use std::{path::Path};

    use anyhow::Error;
    use tracing::{info};

    use crate::config::proc_loader::file_to_config;
    use crate::config::proc_loader::parse_config;
    use crate::config::proc_validator::validate_service_config;
    use crate::ServiceConfig;

    #[tokio::test]
    async fn validate_examples_google_metadata_token_is_valid() {
        let path = Path::new("examples/google_metadata_token.yaml");
        let service_config: ServiceConfig = file_to_config(path)
            .await
            .expect("/examples/google_metadata_token.yaml must exist in repo root for tests");
        validate_service_config(&service_config).await.unwrap();
    }

    #[tokio::test]
    #[should_panic(expected = "config is not valid")]
    async fn invalid_config_reports_all_errors() {
        // Intentionally invalid config (duplicate token id, missing expiry pointer, relative path)
        let invalid_yaml = r#"
settings:
  safety_margin_seconds: 10
  server:
    host: 127.0.0.1
    port: 8080
  metrics:
    path: "/metrics"
    port: 8081
  logging:
    level: info
    format: compact
sources:
  s1:
    type: http
    request:
      url: "http://localhost/ok"
      method: GET
    parse:
      tokens:
        - id: t
          parent: body
          pointer: "/a"
          token_type: plain_text
        - id: t
          parent: body
          pointer: "/b"
          token_type: plain_text
sinks:
  bad_file:
    type: file
    source_id: s1
    path: "relative/path"
    token_id: missing_token
"#;
        info!("{}", invalid_yaml);
        let cfg: Result<ServiceConfig, Error> = parse_config(invalid_yaml.to_string()).await;
        info!("{:?}", cfg);
        match validate_service_config(&cfg.unwrap()).await {
            Ok(()) => panic!("invalid config unexpectedly validated"),
            Err(errs) => {
                // Expect multiple aggregated errors
                assert!(
                    errs.iter().any(|e| e.contains("duplicate")),
                    "expected duplicate token id error"
                );
                assert!(
                    errs.iter().any(|e| e.contains("relative")),
                    "expected relative path error"
                );
                assert!(
                    errs.iter()
                        .any(|e| e.contains("missing_token") || e.contains("not found")),
                    "expected missing token_id reference error"
                );
                assert!(
                    errs.iter().any(|e| e.contains("requires expiration")),
                    "expected missing expiration error for plain_text"
                );
            }
        }
    }
}
