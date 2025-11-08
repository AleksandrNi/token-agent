use crate::{
    cache::{token_cache::TokenCache, token_context::TokenContext},
    config::sources::SourceConfig
};
use anyhow::{anyhow, Result};
use std::{collections::{HashMap, HashSet}, sync::Arc};
use tracing::info;

/// Represents a single node in the DAG — one `SourceConfig` and its dependencies
#[derive(Debug, Clone)]
pub struct DagNode {
    pub id: String,           // source id
    pub config: Arc<SourceConfig>, // source config
    pub deps: Vec<String>,    // dependencies
}

/// A fully built, ordered DAG of all sources
#[derive(Debug)]
pub struct SourceDag {
    pub ordered: Arc<Vec<DagNode>>,
}

impl SourceDag {
    /// Build a DAG from the source map and return topologically ordered nodes
    pub fn build(sources: &HashMap<String, SourceConfig>) -> Result<Self> {
        let mut visited = HashSet::new();
        let mut temp = HashSet::new();
        let mut order = Vec::new();

        fn visit(
            key: &str,
            sources: &HashMap<String, SourceConfig>,
            visited: &mut HashSet<String>,
            temp: &mut HashSet<String>,
            order: &mut Vec<String>,
        ) -> Result<()> {
            if visited.contains(key) {
                return Ok(());
            }
            if !temp.insert(key.to_string()) {
                return Err(anyhow!("Cycle detected in source dependencies: {}", key));
            }

            if let Some(src) = sources.get(key) {
                if let Some(inputs) = &src.inputs {
                    for dep in inputs {
                        if !sources.contains_key(dep) {
                            return Err(anyhow!("Unknown dependency '{}' for '{}'", dep, key));
                        }
                        visit(dep, sources, visited, temp, order)?;
                    }
                }
            }

            temp.remove(key);
            visited.insert(key.to_string());
            order.push(key.to_string());
            Ok(())
        }

        for k in sources.keys() {
            visit(k, sources, &mut visited, &mut temp, &mut order)?;
        }

        let ordered: Vec<DagNode> = order
            .into_iter()
            .filter_map(|id| {
                sources.get(&id)
                .map(|cfg| DagNode {
                    id: id.clone(),
                    config: Arc::new(cfg.clone()),
                    deps: cfg.inputs.clone().unwrap_or_default(),
                })
            })
            .collect();
        
        info!("Execution order: {:?}", &ordered.iter().map(|node| &node.id).collect::<Vec<&String>>());

        Ok(SourceDag { ordered: Arc::new(ordered) })
    }

    pub async fn store_tokens_by_source_id(
        source_id: &str,
        source_token_contexts: Vec<TokenContext>,
    ) -> Result<Vec<String>> {
        let source_token_contexts_len = source_token_contexts.len();
        TokenCache::set(source_id.to_owned(), source_token_contexts).await
        .map(|updated_token_contexts| {
            match updated_token_contexts.len() == source_token_contexts_len {
                true => info!("all the tokens fetched and updated successfully"),
                false => info!("not all the tokens fetched and updated successfully total tokens {}, updated tokens {}", source_token_contexts_len, updated_token_contexts.len()),
            };
            updated_token_contexts
        })
    }

    pub async fn invalidate_tokens_by_source_id(source_id: &str) -> Result<()> {
        let _ = TokenCache::invalidate_expired_tokens_by_source_id(source_id).await;
        info!("tokens invalidated for source_id: {}", source_id);
        Ok(())
    }
}
