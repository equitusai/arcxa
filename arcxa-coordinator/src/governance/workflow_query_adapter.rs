//! Workflow Query Adapter - Bridges Workflow Engine to Graph Query System
//!
//! Provides a concrete implementation of the workflow `QueryExecutor` trait
//! that wraps the coordinator's distributed query system.

use anyhow::{Context, Result};
use graphica_core::orchestration::workflow::QueryExecutor;
use serde_json::Value as JsonValue;
use std::sync::Arc;

use super::shard_coordinator::query::QueryExecutor as CoordinatorQueryExecutor;

/// Adapter that implements workflow QueryExecutor using coordinator's query system
pub struct WorkflowQueryAdapter {
    /// Coordinator's query executor
    query_executor: Arc<CoordinatorQueryExecutor>,
}

impl WorkflowQueryAdapter {
    /// Create new workflow query adapter
    pub fn new(query_executor: Arc<CoordinatorQueryExecutor>) -> Self {
        Self { query_executor }
    }
}

#[async_trait::async_trait]
impl QueryExecutor for WorkflowQueryAdapter {
    async fn execute_query(&self, query: &str, graph: Option<&str>) -> Result<Vec<JsonValue>> {
        // Build SPARQL query with optional graph clause
        let full_query = if let Some(graph_uri) = graph {
            // Wrap query in GRAPH clause
            let trimmed = query.trim();
            if trimmed.to_uppercase().starts_with("SELECT") {
                // Extract SELECT clause and WHERE clause
                if let Some(where_pos) = trimmed.to_uppercase().find("WHERE") {
                    let select_part = &trimmed[..where_pos];
                    let where_part = &trimmed[where_pos + 5..]; // Skip "WHERE"

                    format!(
                        "{} WHERE {{ GRAPH <{}> {} }}",
                        select_part.trim(),
                        graph_uri,
                        where_part.trim()
                    )
                } else {
                    // No WHERE clause, add one
                    format!(
                        "{} WHERE {{ GRAPH <{}> {{ ?s ?p ?o }} }}",
                        trimmed, graph_uri
                    )
                }
            } else {
                query.to_string()
            }
        } else {
            query.to_string()
        };

        // Execute via coordinator's query executor
        self.query_executor
            .execute_query(&full_query)
            .await
            .context("Failed to execute workflow query")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_graph_wrapping() {
        // Test SELECT query wrapping
        let query = "SELECT ?s ?p ?o WHERE { ?s ?p ?o }";
        let expected = "SELECT ?s ?p ?o WHERE { GRAPH <http://example.com/g> { ?s ?p ?o } }";

        // Would need mock to test actual execution
        // This just demonstrates the structure
    }
}
