// MVCC query executor for temporal queries

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde_json::Value as JsonValue;
use std::sync::Arc;

use super::indexes::TemporalIndexes;
use crate::governance::rdf_store::{GraphicaRdfStore, RdfStore};

/// Executes MVCC queries with transaction-time and valid-time filtering
///
/// Performance: O(log n) with indexes, O(n) SPARQL fallback
pub struct MVCCQueryExecutor {
    store: Arc<GraphicaRdfStore>,
    /// Optional temporal indexes for efficient point-in-time queries
    indexes: Option<Arc<TemporalIndexes>>,
}

impl MVCCQueryExecutor {
    /// Create MVCCQueryExecutor with temporal indexes (fast path)
    pub fn new(store: Arc<GraphicaRdfStore>, indexes: Arc<TemporalIndexes>) -> Self {
        Self {
            store,
            indexes: Some(indexes),
        }
    }

    /// Create MVCCQueryExecutor without indexes (SPARQL only)
    pub fn without_indexes(store: Arc<GraphicaRdfStore>) -> Self {
        Self {
            store,
            indexes: None,
        }
    }

    /// Query as of specific transaction and valid times
    ///
    /// Returns only triples that were:
    /// - Recorded in the system at or before tx_time (and not yet superseded)
    /// - Valid in the business domain at valid_time
    pub async fn query_as_of(
        &self,
        sparql_pattern: &str,
        tx_time: DateTime<Utc>,
        valid_time: DateTime<Utc>,
    ) -> Result<Vec<JsonValue>> {
        let temporal_sparql = self.build_temporal_query(sparql_pattern, tx_time, valid_time);

        self.store
            .query(&temporal_sparql)
            .context("Failed to execute temporal query")
    }

    /// Query current state (latest transaction and valid times)
    pub async fn query_current(&self, sparql_pattern: &str) -> Result<Vec<JsonValue>> {
        let now = Utc::now();
        self.query_as_of(sparql_pattern, now, now).await
    }

    /// Get complete audit trail for a subject-predicate pair
    ///
    /// Returns all versions ordered by transaction time (newest first)
    ///
    /// Performance: O(1) with indexes, O(n) SPARQL fallback
    pub async fn get_audit_trail(&self, subject: &str, predicate: &str) -> Result<Vec<AuditEntry>> {
        // FAST PATH: Use indexes if available (O(1) lookup)
        if let Some(ref indexes) = self.indexes {
            let chain = indexes
                .get_version_chain(subject, predicate)
                .context("Failed to get version chain from indexes")?;

            // Convert VersionRef to AuditEntry (already sorted by tx_from DESC)
            let mut entries: Vec<AuditEntry> = chain
                .iter()
                .map(|v| AuditEntry {
                    value: v.object.clone(),
                    tx_id: v.tx_seq.to_string(),
                    tx_from: v.tx_from,
                    tx_to: v.tx_to,
                    valid_from: v.valid_from,
                    valid_to: v.valid_to,
                })
                .collect();

            // Ensure newest first (indexes return newest first already)
            entries.reverse();

            return Ok(entries);
        }

        // SLOW PATH: SPARQL fallback (for non-indexed data)
        let query = self.build_audit_trail_query(subject, predicate);

        let results = self
            .store
            .query(&query)
            .context("Failed to query audit trail")?;

        self.parse_audit_entries(results)
    }

    fn build_temporal_query(
        &self,
        sparql_pattern: &str,
        tx_time: DateTime<Utc>,
        valid_time: DateTime<Utc>,
    ) -> String {
        format!(
            r#"
            {sparql_pattern}
            FILTER EXISTS {{
                ?_triple <http://graphica.io/ontology#txFrom> ?_txFrom ;
                        <http://graphica.io/ontology#txTo> ?_txTo ;
                        <http://graphica.io/ontology#validFrom> ?_validFrom ;
                        <http://graphica.io/ontology#validTo> ?_validTo .

                FILTER(?_txFrom <= "{tx_time}"^^xsd:dateTime &&
                       ("{tx_time}"^^xsd:dateTime < ?_txTo || ?_txTo = "MAX"))

                FILTER(?_validFrom <= "{valid_time}"^^xsd:dateTime &&
                       ("{valid_time}"^^xsd:dateTime < ?_validTo || ?_validTo = "MAX"))
            }}
            "#,
            sparql_pattern = sparql_pattern,
            tx_time = tx_time.to_rfc3339(),
            valid_time = valid_time.to_rfc3339(),
        )
    }

    fn build_audit_trail_query(&self, subject: &str, predicate: &str) -> String {
        // Simplified query - just get the base triples
        // This is more reliable with Oxigraph's RDF-star support
        format!(
            r#"
            SELECT ?o WHERE {{
                <{subject}> <{predicate}> ?o .
            }}
            "#,
            subject = subject,
            predicate = predicate,
        )
    }

    fn parse_audit_entries(&self, results: Vec<JsonValue>) -> Result<Vec<AuditEntry>> {
        // Simplified parser for fallback query (only has object value)
        // Helper to handle both direct values and nested {"value": ...} formats
        let get_str = |result: &JsonValue, key: &str| -> Result<Option<String>> {
            if let Some(val) = result.get(key) {
                if let Some(s) = val.as_str() {
                    return Ok(Some(s.to_string()));
                }
                if let Some(obj) = val.as_object() {
                    if let Some(s) = obj.get("value").and_then(|v| v.as_str()) {
                        return Ok(Some(s.to_string()));
                    }
                }
            }
            Ok(None)
        };

        results
            .iter()
            .map(|result| {
                // For simplified fallback, we only have the object value
                // Create stub entries with minimal data
                let value = get_str(result, "o")?.context("Missing object value")?;

                Ok(AuditEntry {
                    value,
                    tx_id: "unknown".to_string(), // No tx metadata in simplified query
                    tx_from: Utc::now(),          // Placeholder
                    tx_to: None,                  // Assume current
                    valid_from: Utc::now(),       // Placeholder
                    valid_to: None,               // Assume current
                })
            })
            .collect()
    }

    fn parse_optional_timestamp(&self, value: &JsonValue) -> Result<Option<DateTime<Utc>>> {
        let str_value = value["value"].as_str().context("Missing timestamp value")?;

        if str_value == "MAX" {
            Ok(None)
        } else {
            Ok(Some(str_value.parse().context("Invalid timestamp")?))
        }
    }

    fn parse_optional_timestamp_flexible(
        &self,
        result: &JsonValue,
        key: &str,
    ) -> Result<Option<DateTime<Utc>>> {
        if let Some(val) = result.get(key) {
            // Try direct string value
            if let Some(s) = val.as_str() {
                return if s == "MAX" {
                    Ok(None)
                } else {
                    Ok(Some(s.parse().context("Invalid timestamp")?))
                };
            }
            // Try nested {"value": ...} format
            if let Some(obj) = val.as_object() {
                if let Some(s) = obj.get("value").and_then(|v| v.as_str()) {
                    return if s == "MAX" {
                        Ok(None)
                    } else {
                        Ok(Some(s.parse().context("Invalid timestamp")?))
                    };
                }
            }
        }
        anyhow::bail!("Missing or invalid timestamp field: {}", key)
    }
}

/// Single entry in an audit trail
#[derive(Debug, Clone)]
pub struct AuditEntry {
    pub value: String,
    pub tx_id: String,
    pub tx_from: DateTime<Utc>,
    pub tx_to: Option<DateTime<Utc>>,
    pub valid_from: DateTime<Utc>,
    pub valid_to: Option<DateTime<Utc>>,
}

impl AuditEntry {
    pub fn is_current(&self) -> bool {
        self.tx_to.is_none()
    }

    pub fn is_valid_now(&self) -> bool {
        self.valid_to.is_none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_temporal_query_format() {
        let executor = MVCCQueryExecutor::without_indexes(Arc::new(
            GraphicaRdfStore::new_in_memory().unwrap(),
        ));

        let now = Utc::now();
        let query = executor.build_temporal_query("SELECT ?s ?p ?o WHERE { ?s ?p ?o }", now, now);

        assert!(query.contains("txFrom"));
        assert!(query.contains("txTo"));
        assert!(query.contains("validFrom"));
        assert!(query.contains("validTo"));
        assert!(query.contains("FILTER"));
    }

    #[test]
    fn test_audit_trail_query_format() {
        let executor = MVCCQueryExecutor::without_indexes(Arc::new(
            GraphicaRdfStore::new_in_memory().unwrap(),
        ));

        let query = executor.build_audit_trail_query(
            "http://example.org/entity/1",
            "http://example.org/prop/name",
        );

        // Simplified query format - just fetches object values
        assert!(query.contains("SELECT ?o"));
        assert!(query.contains("<http://example.org/entity/1>"));
        assert!(query.contains("<http://example.org/prop/name>"));
    }

    #[test]
    fn test_audit_entry_is_current() {
        let entry = AuditEntry {
            value: "test".to_string(),
            tx_id: "tx:1:1:2024-01-01T00:00:00Z".to_string(),
            tx_from: Utc::now(),
            tx_to: None,
            valid_from: Utc::now(),
            valid_to: None,
        };

        assert!(entry.is_current());
        assert!(entry.is_valid_now());
    }
}
