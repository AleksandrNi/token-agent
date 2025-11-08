use crate::config::sinks::{ResponseField};
use crate::ServiceConfig;

pub fn initiate_default_values(mut config: ServiceConfig) -> ServiceConfig {
    config.sinks = config
        .sinks
        .into_iter()
        .map(|(sink_id, mut sink_config)| {
            // propogate sink id to SingConfig
            sink_config.sink_id = sink_id.to_owned();
            // propogate token id to ResponseField
            let token_id = sink_config.token_id.to_owned();
            if let Some(response_block) = &mut sink_config.response {
                if let Some(headers_map) = &mut response_block.headers {
                    for (_, mut field) in headers_map.iter_mut() {
                        set_response_field_token_id(&mut field, &token_id);
                    }
                }

                if let Some(body_map) = &mut response_block.body {
                    for (_, mut field) in body_map.iter_mut() {
                        set_response_field_token_id(&mut field, &token_id);
                    }
                }
            }

            (sink_id, sink_config)
        })
        .collect();

    config
}


fn set_response_field_token_id(field: &mut ResponseField, token_id: &str) -> () {
    match field {
        ResponseField::Token { id } => {
            *id = token_id.to_owned();
        }
        ResponseField::Expiration { format: _, id } => {
            *id = token_id.to_owned();
        }
        ResponseField::String { value: _ } => {}
    };
}