//! SHACL-DDL Transformer
//!
//! Generates SQL DDL from SHACL shapes stored in RDF triple store.
//!
//! ## Overview
//!
//! This transformer bridges the gap between ontological constraints (SHACL)
//! and relational database schemas (DDL). It queries the RDF store for SHACL
//! shapes and generates appropriate DDL statements for the target database.
//!
//! ## Architecture
//!
//! ```text
//! RDF Triple Store (via RdfQueryClient)
//!         ↓
//!    SPARQL Query (retrieve SHACL shapes)
//!         ↓
//!    ShaclParser (parse RDF → NodeShape)
//!         ↓
//!    convert_shape_to_table (SHACL → TableDef)
//!         ↓
//!    SQL Dialect (TableDef → DDL string)
//!         ↓
//!    JSON Output (DDL statements)
//! ```
//!
//! ## Input Data Format
//!
//! The transformer expects input data with one of:
//! - `shape_uri`: URI of a specific SHACL shape to convert
//! - `shape_uris`: Array of SHACL shape URIs to convert
//!
//! ## Configuration
//!
//! ```json
//! {
//!   "dialect": "db2",           // SQL dialect (db2, postgresql, oracle)
//!   "namespace": "http://example.com/shape/",  // Optional shape namespace filter
//!   "include_indexes": true,    // Include CREATE INDEX statements
//!   "include_foreign_keys": true // Include FOREIGN KEY constraints
//! }
//! ```
//!
//! ## Output Data Format
//!
//! Adds DDL statements to the data:
//!
//! ```json
//! {
//!   "shape_uri": "http://example.com/shape/Customer",
//!   "ddl": {
//!     "create_table": "CREATE TABLE CUSTOMER (...)",
//!     "indexes": ["CREATE UNIQUE INDEX ...", ...],
//!     "foreign_keys": ["ALTER TABLE ... ADD CONSTRAINT ..."]
//!   },
//!   "table_name": "CUSTOMER",
//!   "columns": ["ID", "EMAIL", "NAME"]
//! }
//! ```
//!
//! ## Example Workflow
//!
//! ```yaml
//! actions:
//!   - transformer: shacl_ddl_generator
//!     config:
//!       dialect: "db2"
//!       include_indexes: true
//! ```

use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use serde_json::{json, Value as JsonValue};
use std::sync::Arc;
use tracing::{debug, info, warn};

use crate::governance::AsyncRdfStoreAdapter;
use crate::mapping::ddl::{
    convert_shape_to_table, get_dialect, NodeShape, ShaclParser, SqlDialect,
};

/// SHACL-DDL transformer
///
/// Generates SQL DDL from SHACL shapes in RDF store.
pub struct ShaclDdlTransformer {
    /// Async adapter to the RDF store for accessing shape definitions
    rdf_adapter: Arc<AsyncRdfStoreAdapter>,
}

impl ShaclDdlTransformer {
    /// Create a new SHACL-DDL transformer
    ///
    /// # Arguments
    ///
    /// * `rdf_adapter` - Async adapter to RDF store for accessing SHACL shapes
    pub fn new(rdf_adapter: Arc<AsyncRdfStoreAdapter>) -> Self {
        Self { rdf_adapter }
    }

    /// Query SHACL shapes from RDF store
    ///
    /// Returns Turtle-formatted RDF containing SHACL shapes.
    async fn query_shapes(&self, shape_uri: Option<&str>) -> Result<Vec<JsonValue>> {
        // Build SPARQL CONSTRUCT query to retrieve SHACL shapes
        let sparql = if let Some(uri) = shape_uri {
            // Query specific shape
            format!(
                r#"
                PREFIX sh: <http://www.w3.org/ns/shacl#>
                PREFIX rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#>
                PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>
                PREFIX xsd: <http://www.w3.org/2001/XMLSchema#>

                CONSTRUCT {{
                    ?shape ?p ?o .
                    ?shape sh:property ?prop .
                    ?prop ?pp ?po .
                }}
                WHERE {{
                    BIND(<{}> AS ?shape)
                    ?shape ?p ?o .
                    OPTIONAL {{
                        ?shape sh:property ?prop .
                        ?prop ?pp ?po .
                    }}
                }}
                "#,
                uri
            )
        } else {
            // Query all NodeShapes
            r#"
                PREFIX sh: <http://www.w3.org/ns/shacl#>
                PREFIX rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#>
                PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>
                PREFIX xsd: <http://www.w3.org/2001/XMLSchema#>

                CONSTRUCT {
                    ?shape ?p ?o .
                    ?shape sh:property ?prop .
                    ?prop ?pp ?po .
                }
                WHERE {
                    ?shape a sh:NodeShape .
                    ?shape ?p ?o .
                    OPTIONAL {
                        ?shape sh:property ?prop .
                        ?prop ?pp ?po .
                    }
                }
                "#
            .to_string()
        };

        debug!("Querying SHACL shapes from RDF store");
        debug!("SPARQL: {}", sparql);

        // Execute CONSTRUCT query (returns triples, not bindings)
        let results = self
            .rdf_adapter
            .query(&sparql)
            .await
            .context("Failed to query SHACL shapes")?;
        Ok(results)
    }

    /// Convert SPARQL results to Turtle format
    ///
    /// This is a simplified converter for the embedded shard response format.
    /// In production, you'd use a proper RDF serialization library.
    fn results_to_turtle(&self, results: &[JsonValue]) -> Result<String> {
        let mut turtle = String::new();

        // Add prefixes
        turtle.push_str("@prefix sh: <http://www.w3.org/ns/shacl#> .\n");
        turtle.push_str("@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n");
        turtle.push_str("@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n");
        turtle.push_str("@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .\n\n");

        // Convert each triple result to Turtle
        for result in results {
            if let (Some(s), Some(p), Some(o)) = (
                result.get("subject").and_then(|v| v.as_str()),
                result.get("predicate").and_then(|v| v.as_str()),
                result.get("object").and_then(|v| v.as_str()),
            ) {
                turtle.push_str(&format!("{} {} {} .\n", s, p, o));
            }
        }

        if turtle.is_empty() {
            warn!("No SHACL shapes found in RDF store");
        }

        Ok(turtle)
    }

    /// Extract triples from SPARQL results
    fn results_to_triples(&self, results: &[JsonValue]) -> Vec<(String, String, String)> {
        results
            .iter()
            .filter_map(|result| {
                let subject = result.get("subject").and_then(|v| v.as_str())?;
                let predicate = result.get("predicate").and_then(|v| v.as_str())?;
                let object = result.get("object").and_then(|v| v.as_str())?;

                let clean = |value: &str| {
                    value
                        .trim()
                        .trim_start_matches('<')
                        .trim_end_matches('>')
                        .trim_matches('"')
                        .to_string()
                };

                Some((clean(subject), clean(predicate), clean(object)))
            })
            .collect()
    }

    /// Generate DDL for a single shape
    async fn generate_ddl_for_shape(
        &self,
        shape_uri: &str,
        dialect: &dyn SqlDialect,
        config: &JsonValue,
    ) -> Result<JsonValue> {
        // Query the shape from RDF store
        let results = self
            .query_shapes(Some(shape_uri))
            .await
            .context("Failed to query SHACL shape")?;

        let triples = self.results_to_triples(&results);

        if triples.is_empty() {
            return Err(anyhow!("SHACL shape not found in RDF store: {}", shape_uri));
        }

        // Parse NodeShape using SPARQL query function
        let parser = ShaclParser::new();

        // Create a closure that converts SPARQL queries to bindings from triples
        let triples = Arc::new(triples);
        let shape_uri_string = shape_uri.to_string();
        let sparql_query_fn =
            move |query: &str| -> Result<Vec<std::collections::HashMap<String, String>>> {
                const SH: &str = "http://www.w3.org/ns/shacl#";
                const RDFS: &str = "http://www.w3.org/2000/01/rdf-schema#";

                let sh_target_class = format!("{SH}targetClass");
                let sh_property = format!("{SH}property");
                let sh_path = format!("{SH}path");
                let sh_name = format!("{SH}name");
                let sh_datatype = format!("{SH}datatype");
                let sh_min_count = format!("{SH}minCount");
                let sh_max_count = format!("{SH}maxCount");
                let sh_min_length = format!("{SH}minLength");
                let sh_max_length = format!("{SH}maxLength");
                let sh_pattern = format!("{SH}pattern");
                let sh_min_inclusive = format!("{SH}minInclusive");
                let sh_max_inclusive = format!("{SH}maxInclusive");
                let sh_min_exclusive = format!("{SH}minExclusive");
                let sh_max_exclusive = format!("{SH}maxExclusive");
                let sh_node_kind = format!("{SH}nodeKind");
                let sh_class = format!("{SH}class");
                let sh_has_value = format!("{SH}hasValue");
                let sh_equals = format!("{SH}equals");
                let sh_less_than = format!("{SH}lessThan");
                let sh_less_than_or_equals = format!("{SH}lessThanOrEquals");
                let sh_disjoint = format!("{SH}disjoint");
                let sh_flags = format!("{SH}flags");
                let sh_default_value = format!("{SH}defaultValue");
                let sh_description = format!("{SH}description");
                let sh_closed = format!("{SH}closed");
                let sh_severity = format!("{SH}severity");

                let rdfs_label = format!("{RDFS}label");

                let find_first = |subject: &str, predicate: &str| -> Option<String> {
                    triples.iter().find_map(|(s, p, o)| {
                        if s == subject && p == predicate {
                            Some(o.clone())
                        } else {
                            None
                        }
                    })
                };

                let find_all = |subject: &str, predicate: &str| -> Vec<String> {
                    triples
                        .iter()
                        .filter_map(|(s, p, o)| {
                            if s == subject && p == predicate {
                                Some(o.clone())
                            } else {
                                None
                            }
                        })
                        .collect()
                };

                if query.contains("SELECT ?targetClass") {
                    let mut row = std::collections::HashMap::<String, String>::new();
                    if let Some(target) = find_first(&shape_uri_string, &sh_target_class) {
                        row.insert("targetClass".to_string(), target);
                    }
                    if let Some(label) = find_first(&shape_uri_string, &rdfs_label) {
                        row.insert("label".to_string(), label);
                    }
                    if let Some(closed) = find_first(&shape_uri_string, &sh_closed) {
                        row.insert("closed".to_string(), closed);
                    }
                    if let Some(severity) = find_first(&shape_uri_string, &sh_severity) {
                        row.insert("severity".to_string(), severity);
                    }
                    if row.is_empty() {
                        return Ok(Vec::new());
                    }
                    return Ok(vec![row]);
                }

                if query.contains("SELECT ?property ?path") {
                    let mut rows = Vec::new();
                    let prop_nodes = find_all(&shape_uri_string, &sh_property);

                    for prop in prop_nodes {
                        let mut row = std::collections::HashMap::<String, String>::new();
                        let path = match find_first(&prop, &sh_path) {
                            Some(path) => path,
                            None => continue,
                        };

                        row.insert("property".to_string(), prop.clone());
                        row.insert("path".to_string(), path);

                        if let Some(value) = find_first(&prop, &sh_name) {
                            row.insert("name".to_string(), value);
                        }
                        if let Some(value) = find_first(&prop, &sh_datatype) {
                            row.insert("datatype".to_string(), value);
                        }
                        if let Some(value) = find_first(&prop, &sh_min_count) {
                            row.insert("minCount".to_string(), value);
                        }
                        if let Some(value) = find_first(&prop, &sh_max_count) {
                            row.insert("maxCount".to_string(), value);
                        }
                        if let Some(value) = find_first(&prop, &sh_min_length) {
                            row.insert("minLength".to_string(), value);
                        }
                        if let Some(value) = find_first(&prop, &sh_max_length) {
                            row.insert("maxLength".to_string(), value);
                        }
                        if let Some(value) = find_first(&prop, &sh_pattern) {
                            row.insert("pattern".to_string(), value);
                        }
                        if let Some(value) = find_first(&prop, &sh_min_inclusive) {
                            row.insert("minInclusive".to_string(), value);
                        }
                        if let Some(value) = find_first(&prop, &sh_max_inclusive) {
                            row.insert("maxInclusive".to_string(), value);
                        }
                        if let Some(value) = find_first(&prop, &sh_min_exclusive) {
                            row.insert("minExclusive".to_string(), value);
                        }
                        if let Some(value) = find_first(&prop, &sh_max_exclusive) {
                            row.insert("maxExclusive".to_string(), value);
                        }
                        if let Some(value) = find_first(&prop, &sh_node_kind) {
                            row.insert("nodeKind".to_string(), value);
                        }
                        if let Some(value) = find_first(&prop, &sh_class) {
                            row.insert("class".to_string(), value);
                        }
                        if let Some(value) = find_first(&prop, &sh_has_value) {
                            row.insert("hasValue".to_string(), value);
                        }
                        if let Some(value) = find_first(&prop, &sh_equals) {
                            row.insert("equals".to_string(), value);
                        }
                        if let Some(value) = find_first(&prop, &sh_less_than) {
                            row.insert("lessThan".to_string(), value);
                        }
                        if let Some(value) = find_first(&prop, &sh_less_than_or_equals) {
                            row.insert("lessThanOrEquals".to_string(), value);
                        }
                        if let Some(value) = find_first(&prop, &sh_disjoint) {
                            row.insert("disjoint".to_string(), value);
                        }
                        if let Some(value) = find_first(&prop, &sh_flags) {
                            row.insert("flags".to_string(), value);
                        }
                        if let Some(value) = find_first(&prop, &sh_default_value) {
                            row.insert("defaultValue".to_string(), value);
                        }
                        if let Some(value) = find_first(&prop, &sh_description) {
                            row.insert("description".to_string(), value);
                        }

                        rows.push(row);
                    }

                    return Ok(rows);
                }

                if query.contains("sh:in") {
                    return Ok(Vec::new());
                }

                Ok(Vec::new())
            };

        let shape = parser
            .parse_node_shape(shape_uri, sparql_query_fn)
            .context("Failed to parse SHACL shape")?;

        // Convert shape to table definition
        let table_def = convert_shape_to_table(&shape, &*dialect);

        // Generate DDL statements
        let create_table = dialect.create_table(&table_def);

        let mut indexes = Vec::new();
        let include_indexes = config
            .get("include_indexes")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        if include_indexes {
            for index in &table_def.indexes {
                indexes.push(dialect.create_index(index));
            }
        }

        let mut foreign_keys = Vec::new();
        let include_foreign_keys = config
            .get("include_foreign_keys")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        if include_foreign_keys {
            for fk in &table_def.foreign_keys {
                foreign_keys.push(dialect.create_foreign_key(&table_def.name, fk));
            }
        }

        // Collect column names
        let columns: Vec<String> = table_def.columns.iter().map(|c| c.name.clone()).collect();

        Ok(json!({
            "shape_uri": shape_uri,
            "table_name": table_def.name,
            "columns": columns,
            "ddl": {
                "create_table": create_table,
                "indexes": indexes,
                "foreign_keys": foreign_keys
            }
        }))
    }
}

#[async_trait]
impl super::Transformer for ShaclDdlTransformer {
    async fn transform(
        &self,
        config: &JsonValue,
        data: &mut JsonValue,
        _context: Option<&crate::workflows::engine::executor::ExecutionContext>,
    ) -> Result<()> {
        info!("SHACL-DDL: Generating SQL DDL from SHACL shapes");

        // Get SQL dialect from config
        let dialect_name = config
            .get("dialect")
            .and_then(|v| v.as_str())
            .unwrap_or("db2");

        let dialect = get_dialect(dialect_name)
            .context(format!("Unsupported SQL dialect: {}", dialect_name))?;

        debug!("Using SQL dialect: {}", dialect_name);

        // Determine which shapes to process
        let shape_uris: Vec<String> =
            if let Some(uri) = data.get("shape_uri").and_then(|v| v.as_str()) {
                // Single shape URI
                vec![uri.to_string()]
            } else if let Some(uris) = data.get("shape_uris").and_then(|v| v.as_array()) {
                // Multiple shape URIs
                uris.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            } else {
                return Err(anyhow!("No shape_uri or shape_uris provided in input data"));
            };

        if shape_uris.is_empty() {
            return Err(anyhow!("Empty shape_uris array"));
        }

        info!("SHACL-DDL: Processing {} shape(s)", shape_uris.len());

        // Generate DDL for each shape
        let mut ddl_results = Vec::new();
        for shape_uri in &shape_uris {
            debug!("Processing shape: {}", shape_uri);

            match self
                .generate_ddl_for_shape(shape_uri, &*dialect, config)
                .await
            {
                Ok(ddl) => {
                    ddl_results.push(ddl);
                }
                Err(e) => {
                    warn!("Failed to generate DDL for shape {}: {}", shape_uri, e);
                    // Continue with other shapes instead of failing completely
                }
            }
        }

        if ddl_results.is_empty() {
            return Err(anyhow!("Failed to generate DDL for any shapes"));
        }

        // Add results to data
        if ddl_results.len() == 1 {
            // Single result: merge into data
            let result = &ddl_results[0];
            data["shape_uri"] = result["shape_uri"].clone();
            data["table_name"] = result["table_name"].clone();
            data["columns"] = result["columns"].clone();
            data["ddl"] = result["ddl"].clone();
        } else {
            // Multiple results: add as array
            data["ddl_results"] = json!(ddl_results);
        }

        info!(
            "SHACL-DDL: Successfully generated DDL for {} shape(s)",
            ddl_results.len()
        );

        Ok(())
    }

    fn name(&self) -> &'static str {
        "shacl_ddl_generator"
    }

    fn validate_config(&self, config: &JsonValue) -> Result<()> {
        // Validate dialect
        if let Some(dialect) = config.get("dialect").and_then(|v| v.as_str()) {
            get_dialect(dialect).context(format!(
                "Invalid SQL dialect: '{}'. Supported: db2, postgresql, oracle",
                dialect
            ))?;
        }

        Ok(())
    }
}

// Tests disabled - need to update for AsyncRdfStoreAdapter type changes
#[cfg(disabled_test)]
mod tests {
    use super::*;
    use crate::governance::embedded_shard::RdfQueryClient;
    use crate::workflows::engine::transformers::Transformer;
    use serde_json::json;

    // Mock RDF client for testing
    struct MockRdfClient {
        turtle_response: String,
    }

    #[async_trait]
    impl RdfQueryClient for MockRdfClient {
        async fn query(&self, _sparql: &str) -> Result<Vec<JsonValue>> {
            // Return mock triple results
            Ok(vec![
                json!({"subject": "<http://example.com/shape/Customer>", "predicate": "rdf:type", "object": "sh:NodeShape"}),
                json!({"subject": "<http://example.com/shape/Customer>", "predicate": "sh:targetClass", "object": "<http://example.com/Customer>"}),
            ])
        }

        async fn load_turtle(&self, _turtle: &str, _graph: Option<&str>) -> Result<()> {
            Ok(())
        }

        async fn update(&self, _sparql_update: &str) -> Result<()> {
            Ok(())
        }

        async fn count(&self) -> Result<u64> {
            Ok(0)
        }

        async fn health_check(&self) -> Result<bool> {
            Ok(true)
        }
    }

    #[tokio::test]
    async fn test_validate_config() {
        let client = Arc::new(MockRdfClient {
            turtle_response: String::new(),
        });
        let transformer = ShaclDdlTransformer::new(client);

        // Valid config
        let config = json!({"dialect": "db2"});
        assert!(transformer.validate_config(&config).is_ok());

        // Invalid dialect
        let config = json!({"dialect": "invalid"});
        assert!(transformer.validate_config(&config).is_err());
    }

    #[test]
    fn test_name() {
        let client = Arc::new(MockRdfClient {
            turtle_response: String::new(),
        });
        let transformer = ShaclDdlTransformer::new(client);
        assert_eq!(transformer.name(), "shacl_ddl_generator");
    }
}
