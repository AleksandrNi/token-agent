use crate::sources::SourceKind;
use crate::cache::token_cache::TOKEN_CACHE;
use anyhow::{Result, anyhow};
use std::collections::{HashMap, HashSet};
use futures::future::join_all;

/// DAG scheduler for fetching sources respecting dependencies
pub struct DagScheduler {
    /// All sources by name
    pub sources: HashMap<String, SourceKind>,
}

impl DagScheduler {
    pub fn new(sources: HashMap<String, SourceKind>) -> Self {
        Self { sources }
    }

    /// Fetch all sources asynchronously in DAG order
    pub async fn fetch_all(&self) -> Result<()> {
        let order = self.resolve_order()?;

        for batch in order {
            // Fetch batch concurrently
            let futures: Vec<_> = batch
                .into_iter()
                .map(|name| {
                    let src = self.sources.get(&name).unwrap();
                    let name_clone = name.clone();
                    async move {
                        let token = src.fetch_token().await?;
                        TOKEN_CACHE.set(&name_clone, token);
                        Ok::<(), anyhow::Error>(())
                    }
                })
                .collect();

            // Wait for batch
            let results = join_all(futures).await;
            for r in results {
                r?; // propagate error if any
            }
        }

        Ok(())
    }

    /// Resolve sources in topological order
    fn resolve_order(&self) -> Result<Vec<Vec<String>>> {
        let mut visited: HashSet<String> = HashSet::new();
        let mut temp_mark: HashSet<String> = HashSet::new();
        let mut batches: Vec<Vec<String>> = Vec::new();
        let mut current_batch: Vec<String> = Vec::new();

        for name in self.sources.keys() {
            self.visit(name, &mut visited, &mut temp_mark, &mut batches)?;
        }

        Ok(batches)
    }

    fn visit(
        &self,
        name: &str,
        visited: &mut HashSet<String>,
        temp_mark: &mut HashSet<String>,
        batches: &mut Vec<Vec<String>>,
    ) -> Result<()> {
        if visited.contains(name) {
            return Ok(());
        }

        if temp_mark.contains(name) {
            return Err(anyhow!("Cycle detected in source dependencies at '{}'", name));
        }

        temp_mark.insert(name.to_owned());

        let src = self.sources.get(name)
            .ok_or_else(|| anyhow!("Source '{}' not found", name))?;

        // Visit dependencies first
        let inputs = match src {
            SourceKind::Http(s) => s.cfg.inputs.clone(),
            SourceKind::Metadata(s) => s.cfg.inputs.clone(),
            SourceKind::OAuth2(s) => s.cfg.inputs.clone(),
        }.unwrap_or_default();

        for dep in inputs {
            self.visit(&dep, visited, temp_mark, batches)?;
        }

        temp_mark.remove(name);
        visited.insert(name.to_owned());

        // Add to batch (simple: each source is its own batch, can optimize for parallel)
        batches.push(vec![name.to_string()]);

        Ok(())
    }
}
