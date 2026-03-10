//! CSV Integration for Ontology-Driven DDL Generation
//!
//! This module provides a bridge between CSV file discovery and the ontology-driven
//! DDL generation pipeline. It discovers schema from CSV files and generates DDL with
//! ontology mappings.

use super::{OntologyDdlConfig, OntologyDdlOrchestrator, OntologyDdlResult};
use crate::mapping::discovery::types::{ColumnStatistics, DiscoveredColumn, DiscoveredTable};
use crate::mapping::ontology_registry::RegistryClient;
use anyhow::{Context, Result};
use std::path::Path;

/// Generate ontology-aware DDL from CSV file
///
/// This function:
/// 1. Discovers schema from CSV file
/// 2. Maps columns to ontology concepts (including custom ontologies if provided)
/// 3. Generates SHACL shapes with semantic constraints
/// 4. Creates DDL statements with consistent constraints
/// 5. Records RDF lineage triples
///
/// # Arguments
/// * `csv_path` - Path to CSV file
/// * `table_name` - Target table name
/// * `target_dialect` - SQL dialect (postgresql, db2, oracle)
/// * `config` - Optional configuration (uses defaults if None)
/// * `registry_client` - Optional custom ontology registry (uses default schema.org if None)
///
/// # Returns
/// * `OntologyDdlResult` - DDL statements, mappings, SHACL shape, and lineage
///
/// # Example
/// ```ignore
/// // With default schema.org ontologies
/// let result = generate_ontology_ddl_from_csv(
///     Path::new("/data/customers.csv"),
///     "customers",
///     "postgresql",
///     None, // Use defaults
///     None, // No custom ontologies
/// )?;
///
/// // With custom ontologies
/// let registry_client = RegistryClient::new(Some(registry));
/// let result = generate_ontology_ddl_from_csv(
///     Path::new("/data/customers.csv"),
///     "customers",
///     "postgresql",
///     None,
///     Some(&registry_client),
/// )?;
///
/// // Use the generated DDL
/// for stmt in result.ddl_statements {
///     println!("{}", stmt);
/// }
/// ```
pub async fn generate_ontology_ddl_from_csv(
    csv_path: &Path,
    table_name: &str,
    target_dialect: &str,
    config: Option<OntologyDdlConfig>,
    registry_client: Option<&RegistryClient>,
) -> Result<OntologyDdlResult> {
    // Step 1: Discover schema from CSV
    let discovered_table = discover_csv_schema(csv_path, table_name)
        .with_context(|| format!("Failed to discover schema from CSV: {:?}", csv_path))?;

    // Step 2: Create orchestrator with config and optional custom ontologies
    let cfg = config.unwrap_or_default();
    let orchestrator = if let Some(client) = registry_client {
        tracing::info!("Using custom ontologies from registry for CSV DDL generation");
        OntologyDdlOrchestrator::with_custom_ontologies(cfg, client)?
    } else {
        tracing::info!("Using default schema.org ontologies for CSV DDL generation");
        OntologyDdlOrchestrator::new(cfg)
    };

    // Step 3: Generate DDL with ontology mappings
    let result = orchestrator
        .generate_ddl(&discovered_table, target_dialect)
        .await
        .with_context(|| format!("Failed to generate ontology DDL for table '{}'", table_name))?;

    tracing::info!(
        "Generated ontology-driven DDL: {} statements, {} ontology mappings",
        result.ddl_statements.len(),
        result.ontology_mappings.len()
    );

    Ok(result)
}

/// Discover schema from CSV file
///
/// Reads CSV headers and samples data to infer column types and statistics.
fn discover_csv_schema(csv_path: &Path, table_name: &str) -> Result<DiscoveredTable> {
    use csv::ReaderBuilder;
    use std::fs::File;

    // Open CSV file
    let file =
        File::open(csv_path).with_context(|| format!("Failed to open CSV file: {:?}", csv_path))?;

    let mut reader = ReaderBuilder::new().has_headers(true).from_reader(file);

    // Get headers
    let headers = reader
        .headers()
        .context("Failed to read CSV headers")?
        .clone();

    tracing::debug!("Discovered {} columns from CSV", headers.len());

    // Sample first N rows to infer types and collect statistics
    const SAMPLE_SIZE: usize = 100;
    let mut sample_data: Vec<Vec<String>> = Vec::new();

    for result in reader.records().take(SAMPLE_SIZE) {
        let record = result.context("Failed to read CSV record")?;
        sample_data.push(record.iter().map(|s| s.to_string()).collect());
    }

    tracing::debug!("Sampled {} rows for type inference", sample_data.len());

    // Create discovered columns
    let columns: Vec<DiscoveredColumn> = headers
        .iter()
        .enumerate()
        .map(|(idx, header)| {
            // Collect sample values for this column
            let samples: Vec<String> = sample_data
                .iter()
                .filter_map(|row| row.get(idx).cloned())
                .filter(|v| !v.is_empty())
                .take(10) // Keep top 10 samples
                .collect();

            // Infer data type from samples
            let data_type = infer_column_type(&samples);

            // Calculate statistics
            let distinct_count = samples
                .iter()
                .collect::<std::collections::HashSet<_>>()
                .len();
            let null_fraction = if sample_data.is_empty() {
                0.0
            } else {
                let nulls = sample_data
                    .iter()
                    .filter(|row| row.get(idx).map(|v| v.is_empty()).unwrap_or(true))
                    .count();
                nulls as f64 / sample_data.len() as f64
            };

            // Calculate average length for strings
            let avg_length = if !samples.is_empty() {
                Some(samples.iter().map(|s| s.len()).sum::<usize>() as f64 / samples.len() as f64)
            } else {
                None
            };

            DiscoveredColumn {
                name: header.to_string(),
                data_type,
                nullable: null_fraction > 0.0,
                primary_key: false,  // Cannot infer from CSV
                semantic_type: None, // Will be inferred by ontology mapper
                confidence: 0.0,
                patterns: vec![],
                statistics: ColumnStatistics {
                    distinct_count: distinct_count as i64,
                    null_fraction,
                    sample_count: samples.len(),
                    most_common_values: None,
                    avg_length,
                    min_value: None, // Could be computed for numeric types
                    max_value: None, // Could be computed for numeric types
                },
                sample_values: samples,
            }
        })
        .collect();

    Ok(DiscoveredTable {
        name: table_name.to_string(),
        columns,
        row_count: Some(sample_data.len() as u64),
    })
}

/// Infer column type from sample values
///
/// Uses simple heuristics to determine the most appropriate SQL type.
fn infer_column_type(samples: &[String]) -> String {
    if samples.is_empty() {
        return "VARCHAR(255)".to_string();
    }

    // Check if all samples are integers
    let all_integers = samples.iter().all(|s| s.parse::<i64>().is_ok());
    if all_integers {
        return "INTEGER".to_string();
    }

    // Check if all samples are floats
    let all_floats = samples.iter().all(|s| s.parse::<f64>().is_ok());
    if all_floats {
        return "DOUBLE PRECISION".to_string();
    }

    // Check if all samples are booleans
    let all_bools = samples.iter().all(|s| {
        let lower = s.to_lowercase();
        lower == "true"
            || lower == "false"
            || lower == "t"
            || lower == "f"
            || lower == "yes"
            || lower == "no"
            || lower == "y"
            || lower == "n"
            || lower == "1"
            || lower == "0"
    });
    if all_bools {
        return "BOOLEAN".to_string();
    }

    // Check if all samples are dates (basic ISO 8601 check)
    let all_dates = samples.iter().all(|s| {
        // Very basic date pattern matching
        s.contains('-') && s.split('-').count() == 3
    });
    if all_dates {
        return "TIMESTAMP".to_string();
    }

    // Default to VARCHAR with appropriate length
    let max_length = samples.iter().map(|s| s.len()).max().unwrap_or(255);
    let length = match max_length {
        0..=50 => 100,
        51..=100 => 255,
        101..=500 => 1000,
        _ => 4000,
    };

    format!("VARCHAR({})", length)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_infer_integer_type() {
        let samples = vec!["123".to_string(), "456".to_string(), "789".to_string()];
        assert_eq!(infer_column_type(&samples), "INTEGER");
    }

    #[test]
    fn test_infer_float_type() {
        let samples = vec!["123.45".to_string(), "678.90".to_string()];
        assert_eq!(infer_column_type(&samples), "DOUBLE PRECISION");
    }

    #[test]
    fn test_infer_boolean_type() {
        let samples = vec!["true".to_string(), "false".to_string(), "TRUE".to_string()];
        assert_eq!(infer_column_type(&samples), "BOOLEAN");
    }

    #[test]
    fn test_infer_date_type() {
        let samples = vec!["2024-01-15".to_string(), "2024-02-20".to_string()];
        assert_eq!(infer_column_type(&samples), "TIMESTAMP");
    }

    #[test]
    fn test_infer_varchar_type() {
        let samples = vec!["John Doe".to_string(), "Jane Smith".to_string()];
        assert!(infer_column_type(&samples).starts_with("VARCHAR"));
    }

    #[test]
    fn test_infer_varchar_length_short() {
        let samples = vec!["abc".to_string(), "def".to_string()];
        assert_eq!(infer_column_type(&samples), "VARCHAR(100)");
    }

    #[test]
    fn test_infer_varchar_length_medium() {
        let samples = vec!["a".repeat(150)];
        // 150 chars falls in 101..=500 range → VARCHAR(1000)
        assert_eq!(infer_column_type(&samples), "VARCHAR(1000)");
    }

    #[test]
    fn test_infer_varchar_length_long() {
        let samples = vec!["a".repeat(600)];
        // 600 chars is > 500 → VARCHAR(4000)
        assert_eq!(infer_column_type(&samples), "VARCHAR(4000)");
    }

    #[test]
    fn test_infer_empty_samples() {
        let samples: Vec<String> = vec![];
        assert_eq!(infer_column_type(&samples), "VARCHAR(255)");
    }
}
