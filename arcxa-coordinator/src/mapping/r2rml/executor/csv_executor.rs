//! R2RML Executor (Refactored for Structured Data Sources)
//!
//! Executes R2RML mappings against ANY structured data source to generate RDF triples.
//!
//! ## Architecture Change (2024)
//!
//! Previously: Accepted file paths directly (`execute_csv(path: &Path)`)
//! Now: Accepts `StructuredDataSource` trait (`execute(source: &dyn StructuredDataSource)`)
//!
//! This enforces the architecture principle:
//! **All data must come from the file library or explicit streaming sources.**

use crate::mapping::data_source::{SourceRecord, StructuredDataSource};
use crate::mapping::r2rml::types::*;
use anyhow::{Context, Result};
use std::collections::HashMap;

/// R2RML Executor
///
/// Executes R2RML mappings against CSV data to generate RDF triples.
///
/// ## Workflow
///
/// 1. Parse CSV file
/// 2. For each row:
///    a. Generate subject URI from SubjectMap
///    b. Generate rdf:type triples from SubjectMap classes
///    c. For each PredicateObjectMap:
///       - Generate predicate URI
///       - Generate object value
///       - Emit triple (subject, predicate, object)
pub struct R2rmlExecutor {
    mapping: R2rmlMapping,
}

impl R2rmlExecutor {
    /// Create a new R2RML executor
    pub fn new(mapping: R2rmlMapping) -> Self {
        Self { mapping }
    }

    /// Execute the mapping against any structured data source
    ///
    /// ## Arguments
    /// - `source`: Any implementation of StructuredDataSource (CSV from file library, DB, Parquet, etc.)
    ///
    /// ## Returns
    /// Vector of RDF triples (subject, predicate, object, datatype)
    ///
    /// ## Architecture Enforcement
    ///
    /// This method now accepts `StructuredDataSource` instead of a path.
    /// This ensures all data comes from the file library or explicit streaming sources.
    pub async fn execute(&self, source: &dyn StructuredDataSource) -> Result<Vec<RdfTriple>> {
        // Validate mapping first
        self.mapping.validate()?;

        let mut triples = Vec::new();

        // Get source schema for field mapping
        let schema = source
            .schema()
            .await
            .context("Failed to get source schema")?;

        tracing::info!(
            "Executing R2RML mapping on source: {} ({} fields)",
            source.description(),
            schema.fields.len()
        );

        // Create record stream
        let mut record_stream = source
            .records()
            .await
            .context("Failed to create record stream")?;

        // Process each record
        let mut record_count = 0;
        while let Some(record) = record_stream.next().await? {
            let row = self.source_record_to_map(&record);

            // Execute all triples maps for this row
            for triples_map in &self.mapping.triples_maps {
                let mut row_triples = self.execute_triples_map(triples_map, &row)?;
                triples.append(&mut row_triples);
            }

            record_count += 1;
            if record_count % 1000 == 0 {
                tracing::debug!(
                    "Processed {} records, generated {} triples",
                    record_count,
                    triples.len()
                );
            }
        }

        tracing::info!(
            "R2RML execution complete: {} records → {} triples",
            record_count,
            triples.len()
        );

        Ok(triples)
    }

    /// Legacy method for backward compatibility
    ///
    /// **DEPRECATED**: Use `execute()` with `FileLibraryCsvSource` instead.
    ///
    /// This method is kept for backward compatibility but will log a warning.
    /// Direct path access bypasses the file library architecture.
    #[deprecated(
        since = "2024.0.0",
        note = "Use execute() with FileLibraryCsvSource instead to enforce file library architecture"
    )]
    pub async fn execute_csv(&self, csv_path: &std::path::Path) -> Result<Vec<RdfTriple>> {
        tracing::warn!(
            "⚠️  DEPRECATED: execute_csv() bypasses file library architecture. \
             File: {:?}. Please use execute() with FileLibraryCsvSource.",
            csv_path
        );

        // For now, create a minimal StructuredDataSource wrapper
        // This is a temporary bridge - callers should be updated to use file library
        use crate::mapping::data_source::{
            FieldDefinition, SourceSchema, SourceType, UniversalDataType,
        };
        use crate::mapping::loader::orchestration::async_csv_reader::{
            AsyncCsvReader, AsyncCsvReaderConfig,
        };

        let csv_config = AsyncCsvReaderConfig {
            file_path: csv_path.to_path_buf(),
            delimiter: b',',
            has_header: true,
            buffer_size: 8192,
            ..Default::default()
        };

        let mut reader = AsyncCsvReader::new(csv_config)
            .await
            .context("Failed to open CSV file")?;

        let headers = reader.headers().clone();
        let mut triples = Vec::new();

        // Process each row (old way)
        while let Some(record) = reader.next_row().await? {
            let row = self.record_to_map(&headers, &record);

            for triples_map in &self.mapping.triples_maps {
                let mut row_triples = self.execute_triples_map(triples_map, &row)?;
                triples.append(&mut row_triples);
            }
        }

        Ok(triples)
    }

    /// Execute a single triples map for a row
    fn execute_triples_map(
        &self,
        triples_map: &TriplesMap,
        row: &HashMap<String, String>,
    ) -> Result<Vec<RdfTriple>> {
        let mut triples = Vec::new();

        // Generate subject URI
        let subject = triples_map.subject_map.generate_subject(row)?;

        // Generate rdf:type triples from subject map classes
        if let Some(classes) = &triples_map.subject_map.class {
            for class in classes {
                triples.push(RdfTriple {
                    subject: subject.clone(),
                    predicate: "http://www.w3.org/1999/02/22-rdf-syntax-ns#type".to_string(),
                    object: class.clone(),
                    datatype: None,
                    language: None,
                });
            }
        }

        // Generate predicate-object triples
        for pom in &triples_map.predicate_object_maps {
            let (predicate, object, datatype) = pom.generate_predicate_object(row)?;
            triples.push(RdfTriple {
                subject: subject.clone(),
                predicate,
                object,
                datatype,
                language: pom.object_map.get_language().map(|s| s.to_string()),
            });
        }

        Ok(triples)
    }

    /// Convert SourceRecord to HashMap (new abstracted method)
    fn source_record_to_map(&self, record: &SourceRecord) -> HashMap<String, String> {
        record
            .fields
            .iter()
            .map(|(k, v)| (k.clone(), v.to_string()))
            .collect()
    }

    /// Convert CSV record to HashMap (legacy method for backward compat)
    fn record_to_map(
        &self,
        headers: &[String],
        record: &csv_async::StringRecord,
    ) -> HashMap<String, String> {
        headers
            .iter()
            .zip(record.iter())
            .map(|(h, v)| (h.clone(), v.to_string()))
            .collect()
    }
}

/// RDF Triple
///
/// Represents a generated RDF triple with optional datatype and language tag.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RdfTriple {
    pub subject: String,
    pub predicate: String,
    pub object: String,
    pub datatype: Option<String>,
    pub language: Option<String>,
}

impl RdfTriple {
    /// Convert triple to N-Triples format
    pub fn to_ntriples(&self) -> String {
        let object_str = if let Some(datatype) = &self.datatype {
            format!("\"{}\"^^<{}>", self.object, datatype)
        } else if let Some(language) = &self.language {
            format!("\"{}\"@{}", self.object, language)
        } else {
            // Check if object is a URI or literal
            if self.object.starts_with("http://") || self.object.starts_with("https://") {
                format!("<{}>", self.object)
            } else {
                format!("\"{}\"", self.object)
            }
        };

        format!("<{}> <{}> {} .", self.subject, self.predicate, object_str)
    }

    /// Convert triple to Turtle format
    pub fn to_turtle(&self) -> String {
        self.to_ntriples()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rdf_triple_to_ntriples() {
        let triple = RdfTriple {
            subject: "http://example.com/customer/123".to_string(),
            predicate: "http://schema.org/name".to_string(),
            object: "Alice Smith".to_string(),
            datatype: None,
            language: None,
        };

        let ntriples = triple.to_ntriples();
        assert!(ntriples.contains("<http://example.com/customer/123>"));
        assert!(ntriples.contains("<http://schema.org/name>"));
        assert!(ntriples.contains("\"Alice Smith\""));
    }

    #[test]
    fn test_rdf_triple_with_datatype() {
        let triple = RdfTriple {
            subject: "http://example.com/customer/123".to_string(),
            predicate: "http://schema.org/age".to_string(),
            object: "30".to_string(),
            datatype: Some("http://www.w3.org/2001/XMLSchema#integer".to_string()),
            language: None,
        };

        let ntriples = triple.to_ntriples();
        assert!(ntriples.contains("\"30\"^^<http://www.w3.org/2001/XMLSchema#integer>"));
    }

    #[test]
    fn test_rdf_triple_with_language() {
        let triple = RdfTriple {
            subject: "http://example.com/customer/123".to_string(),
            predicate: "http://schema.org/name".to_string(),
            object: "Alice".to_string(),
            datatype: None,
            language: Some("en".to_string()),
        };

        let ntriples = triple.to_ntriples();
        assert!(ntriples.contains("\"Alice\"@en"));
    }
}
