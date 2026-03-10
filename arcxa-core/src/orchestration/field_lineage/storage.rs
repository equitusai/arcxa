//! Field Lineage RDF Storage
//!
//! Persists field-level provenance to RDF triple store using W3C PROV ontology.

use super::ontology::*;
use super::types::*;
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};

/// Field lineage storage interface
pub struct FieldLineageStore {
    /// Base URI for field lineage graph
    graph_uri: String,
}

impl FieldLineageStore {
    /// Create a new field lineage store
    pub fn new() -> Self {
        Self {
            graph_uri: "http://graphica.io/graph/field-lineage".to_string(),
        }
    }

    /// Create with custom graph URI
    pub fn with_graph_uri(graph_uri: String) -> Self {
        Self { graph_uri }
    }

    /// Convert FieldValue to RDF triples
    pub fn field_value_to_triples(&self, field_value: &FieldValue) -> String {
        let value_uri = field_value_uri(&field_value.entity_id, &field_value.field_name);
        let entity_uri = format!(
            "http://graphica.io/ontology#entity/{}",
            field_value.entity_id
        );
        let resolution_uri = format!(
            "http://graphica.io/field#resolution/{}",
            field_value.resolution_id
        );

        let value_str =
            serde_json::to_string(&field_value.value).unwrap_or_else(|_| "\"\"".to_string());

        let mut triples = vec![
            format!("<{}> a <{}FieldValue> .", value_uri, FIELD_NS),
            format!(
                "<{}> <{}fieldName> \"{}\" .",
                value_uri, FIELD_NS, field_value.field_name
            ),
            format!("<{}> <{}fieldValue> {} .", value_uri, FIELD_NS, value_str),
            format!(
                "<{}> <{}valueType> \"{}\" .",
                value_uri, FIELD_NS, field_value.value_type
            ),
            format!(
                "<{}> <{}confidence> \"{}\"^^<{}double> .",
                value_uri, FIELD_NS, field_value.confidence, XSD_NS
            ),
            format!(
                "<{}> <{}atTime> \"{}\"^^<{}dateTime> .",
                value_uri,
                PROV_NS,
                field_value.resolved_at.to_rfc3339(),
                XSD_NS
            ),
            format!(
                "<{}> <{}validFrom> \"{}\"^^<{}dateTime> .",
                value_uri,
                FIELD_NS,
                field_value.valid_from.to_rfc3339(),
                XSD_NS
            ),
            format!(
                "<{}> <{}wasGeneratedBy> <{}> .",
                value_uri, PROV_NS, resolution_uri
            ),
        ];

        // Link to entity
        triples.push(format!(
            "<{}> <{}hasDerivedAttribute> <{}> .",
            entity_uri, "http://graphica.io/ontology#", value_uri
        ));

        // Valid to (if present)
        if let Some(valid_to) = field_value.valid_to {
            triples.push(format!(
                "<{}> <{}validTo> \"{}\"^^<{}dateTime> .",
                value_uri,
                FIELD_NS,
                valid_to.to_rfc3339(),
                XSD_NS
            ));
        }

        // Supersedes (if present)
        if let Some(ref supersedes) = field_value.supersedes {
            let prev_uri = field_value_uri(&field_value.entity_id, &field_value.field_name);
            triples.push(format!(
                "<{}> <{}supersedes> <{}/{}> .",
                value_uri, FIELD_NS, prev_uri, supersedes
            ));
        }

        // Explanation (if present)
        if let Some(ref explanation) = field_value.explanation {
            triples.push(format!(
                "<{}> <{}explanation> \"{}\" .",
                value_uri,
                FIELD_NS,
                explanation.replace('"', "\\\"")
            ));
        }

        triples.join("\n        ")
    }

    /// Convert SourceValue to RDF triples
    pub fn source_value_to_triples(&self, source_value: &SourceValue, field_name: &str) -> String {
        let source_uri = format!("{}source/{}", FIELD_NS, source_value.id);

        let value_str =
            serde_json::to_string(&source_value.value).unwrap_or_else(|_| "\"\"".to_string());

        let mut triples = vec![
            format!("<{}> a <{}SourceValue> .", source_uri, FIELD_NS),
            format!(
                "<{}> <{}fieldName> \"{}\" .",
                source_uri, FIELD_NS, field_name
            ),
            format!("<{}> <{}fieldValue> {} .", source_uri, FIELD_NS, value_str),
            format!(
                "<{}> <{}sourceSystem> \"{}\" .",
                source_uri, FIELD_NS, source_value.source_system
            ),
            format!(
                "<{}> <{}sourceTimestamp> \"{}\"^^<{}dateTime> .",
                source_uri,
                FIELD_NS,
                source_value.source_timestamp.to_rfc3339(),
                XSD_NS
            ),
            format!(
                "<{}> <{}sourceAuthority> \"{}\"^^<{}double> .",
                source_uri, FIELD_NS, source_value.source_authority, XSD_NS
            ),
            format!(
                "<{}> <{}voteCount> \"{}\"^^<{}integer> .",
                source_uri, FIELD_NS, source_value.vote_count, XSD_NS
            ),
            format!(
                "<{}> <{}voteWeight> \"{}\"^^<{}double> .",
                source_uri, FIELD_NS, source_value.vote_weight, XSD_NS
            ),
        ];

        // Confidence (if present)
        if let Some(confidence) = source_value.confidence {
            triples.push(format!(
                "<{}> <{}confidence> \"{}\"^^<{}double> .",
                source_uri, FIELD_NS, confidence, XSD_NS
            ));
        }

        // Metadata
        for (key, value) in &source_value.metadata {
            triples.push(format!(
                "<{}> <{}metadata/{}> \"{}\" .",
                source_uri,
                FIELD_NS,
                key,
                value.replace('"', "\\\"")
            ));
        }

        triples.join("\n        ")
    }

    /// Convert FieldResolution to RDF triples
    pub fn field_resolution_to_triples(&self, resolution: &FieldResolution) -> String {
        let resolution_uri = field_resolution_uri(
            &resolution.entity_id,
            &resolution.field_name,
            resolution.resolved_at.timestamp_millis(),
        );
        let strategy_uri =
            voting_strategy_uri(&format!("{:?}", resolution.strategy.strategy_type).to_lowercase());

        let mut triples = vec![
            format!("<{}> a <{}FieldResolution> .", resolution_uri, FIELD_NS),
            format!(
                "<{}> <{}fieldName> \"{}\" .",
                resolution_uri, FIELD_NS, resolution.field_name
            ),
            format!(
                "<{}> <{}atTime> \"{}\"^^<{}dateTime> .",
                resolution_uri,
                PROV_NS,
                resolution.resolved_at.to_rfc3339(),
                XSD_NS
            ),
            format!(
                "<{}> <{}usedStrategy> <{}> .",
                resolution_uri, FIELD_NS, strategy_uri
            ),
            format!(
                "<{}> <{}wasAttributedTo> \"{}\" .",
                resolution_uri, PROV_NS, resolution.resolved_by
            ),
            format!(
                "<{}> <{}explanation> \"{}\" .",
                resolution_uri,
                FIELD_NS,
                resolution.explanation.replace('"', "\\\"")
            ),
        ];

        // Link to all source values
        for source in &resolution.source_values {
            let source_uri = format!("{}source/{}", FIELD_NS, source.id);
            triples.push(format!(
                "<{}> <{}hadSourceValue> <{}> .",
                resolution_uri, FIELD_NS, source_uri
            ));
        }

        // Link to selected value
        let selected_uri = format!("{}source/{}", FIELD_NS, resolution.selected_value.id);
        triples.push(format!(
            "<{}> <{}selectedValue> <{}> .",
            resolution_uri, FIELD_NS, selected_uri
        ));

        // Link to rejected values
        for rejected in &resolution.rejected_values {
            let rejected_uri = format!("{}source/{}", FIELD_NS, rejected.id);
            triples.push(format!(
                "<{}> <{}rejectedValue> <{}> .",
                resolution_uri, FIELD_NS, rejected_uri
            ));
        }

        // Conflict (if present)
        if let Some(ref conflict) = resolution.conflict {
            let conflict_uri = field_conflict_uri(
                &resolution.entity_id,
                &resolution.field_name,
                resolution.resolved_at.timestamp_millis(),
            );
            triples.push(format!(
                "<{}> <{}hasConflict> <{}> .",
                resolution_uri, FIELD_NS, conflict_uri
            ));

            triples.extend(self.field_conflict_to_triples(conflict, &conflict_uri));
        }

        // Review (if present)
        if let Some(ref review) = resolution.review {
            triples.push(format!(
                "<{}> <{}reviewedBy> <http://graphica.io/ontology#user/{}> .",
                resolution_uri, FIELD_NS, review.reviewed_by
            ));
            triples.push(format!(
                "<{}> <{}reviewedAt> \"{}\"^^<{}dateTime> .",
                resolution_uri,
                FIELD_NS,
                review.reviewed_at.to_rfc3339(),
                XSD_NS
            ));

            if let Some(ref notes) = review.notes {
                triples.push(format!(
                    "<{}> <{}reviewNotes> \"{}\" .",
                    resolution_uri,
                    FIELD_NS,
                    notes.replace('"', "\\\"")
                ));
            }
        }

        triples.join("\n        ")
    }

    /// Convert FieldConflict to RDF triples
    fn field_conflict_to_triples(
        &self,
        conflict: &FieldConflict,
        conflict_uri: &str,
    ) -> Vec<String> {
        vec![
            format!("<{}> a <{}FieldConflict> .", conflict_uri, FIELD_NS),
            format!(
                "<{}> <{}conflictSeverity> \"{}\" .",
                conflict_uri,
                FIELD_NS,
                format!("{:?}", conflict.severity).to_lowercase()
            ),
            format!(
                "<{}> <{}conflictReason> \"{}\" .",
                conflict_uri,
                FIELD_NS,
                conflict.reason.replace('"', "\\\"")
            ),
            format!(
                "<{}> <{}requiresReview> \"{}\"^^<{}boolean> .",
                conflict_uri, FIELD_NS, conflict.requires_review, XSD_NS
            ),
        ]
    }

    /// Generate SPARQL INSERT query for complete field resolution
    pub fn insert_field_resolution_query(&self, resolution: &FieldResolution) -> String {
        let mut all_triples = Vec::new();

        // Add source value triples
        for source in &resolution.source_values {
            all_triples.push(self.source_value_to_triples(source, &resolution.field_name));
        }

        // Add resolution triples
        all_triples.push(self.field_resolution_to_triples(resolution));

        format!(
            r#"
PREFIX field: <{FIELD_NS}>
PREFIX prov: <{PROV_NS}>
PREFIX xsd: <{XSD_NS}>

INSERT DATA {{
    GRAPH <{}> {{
        {}
    }}
}}
"#,
            self.graph_uri,
            all_triples.join("\n\n        ")
        )
    }

    /// Generate SPARQL query to get field lineage
    pub fn query_field_lineage(&self, entity_id: &str, field_name: &str) -> String {
        format!(
            r#"
PREFIX field: <{FIELD_NS}>
PREFIX prov: <{PROV_NS}>

SELECT ?value ?confidence ?resolvedAt ?strategy ?explanation
       ?sourceValue ?sourceSystem ?sourceAuthority ?voteWeight
WHERE {{
    GRAPH <{}> {{
        # Field value
        ?fieldValue a field:FieldValue ;
            field:fieldName "{}" ;
            field:fieldValue ?value ;
            field:confidence ?confidence ;
            prov:atTime ?resolvedAt ;
            prov:wasGeneratedBy ?resolution .

        # Resolution
        ?resolution field:usedStrategy ?strategy ;
            field:explanation ?explanation .

        # Source values (optional)
        OPTIONAL {{
            ?resolution field:hadSourceValue ?source .
            ?source field:fieldValue ?sourceValue ;
                field:sourceSystem ?sourceSystem ;
                field:sourceAuthority ?sourceAuthority ;
                field:voteWeight ?voteWeight .
        }}

        # Filter by entity
        FILTER(CONTAINS(STR(?fieldValue), "{}"))
    }}
}}
ORDER BY DESC(?resolvedAt)
LIMIT 10
"#,
            self.graph_uri, field_name, entity_id
        )
    }

    /// Generate SPARQL query to get field value history
    pub fn query_field_history(&self, entity_id: &str, field_name: &str) -> String {
        format!(
            r#"
PREFIX field: <{FIELD_NS}>
PREFIX prov: <{PROV_NS}>

SELECT ?value ?confidence ?validFrom ?validTo ?explanation
WHERE {{
    GRAPH <{}> {{
        ?fieldValue a field:FieldValue ;
            field:fieldName "{}" ;
            field:fieldValue ?value ;
            field:confidence ?confidence ;
            field:validFrom ?validFrom ;
            field:explanation ?explanation .

        OPTIONAL {{
            ?fieldValue field:validTo ?validTo .
        }}

        FILTER(CONTAINS(STR(?fieldValue), "{}"))
    }}
}}
ORDER BY DESC(?validFrom)
"#,
            self.graph_uri, field_name, entity_id
        )
    }

    /// Generate SPARQL query to get all conflicts requiring review
    pub fn query_conflicts_requiring_review(&self) -> String {
        format!(
            r#"
PREFIX field: <{FIELD_NS}>
PREFIX prov: <{PROV_NS}>

SELECT ?entityId ?fieldName ?severity ?reason ?resolvedAt
WHERE {{
    GRAPH <{}> {{
        ?resolution a field:FieldResolution ;
            field:fieldName ?fieldName ;
            prov:atTime ?resolvedAt ;
            field:hasConflict ?conflict .

        ?conflict field:conflictSeverity ?severity ;
            field:conflictReason ?reason ;
            field:requiresReview "true"^^<{XSD_NS}boolean> .

        BIND(REPLACE(STR(?resolution), "^.*/([^/]+)/[^/]+/[^/]+$", "$1") AS ?entityId)
    }}
}}
ORDER BY DESC(?resolvedAt)
"#,
            self.graph_uri
        )
    }

    /// Generate SPARQL query to retrieve golden record for an entity
    ///
    /// Fetches all current field values (without validTo) for a given entity,
    /// along with their confidence scores, resolutions, and conflicts.
    pub fn query_golden_record(&self, entity_id: &str) -> String {
        format!(
            r#"
PREFIX field: <{FIELD_NS}>
PREFIX prov: <{PROV_NS}>
PREFIX xsd: <{XSD_NS}>

SELECT ?fieldName ?value ?confidence ?resolvedAt ?requiresReview
       ?conflictSeverity ?conflictReason
WHERE {{
    GRAPH <{}> {{
        # Field values for this entity (current values only - no validTo)
        ?fieldValue a field:FieldValue ;
            field:fieldName ?fieldName ;
            field:fieldValue ?value ;
            field:confidence ?confidence ;
            prov:atTime ?resolvedAt ;
            prov:wasGeneratedBy ?resolution .

        # Filter to this entity and current values only
        FILTER(CONTAINS(STR(?fieldValue), "{}"))
        FILTER NOT EXISTS {{ ?fieldValue field:validTo ?anyValidTo }}

        # Optional: Conflict information
        OPTIONAL {{
            ?resolution field:hasConflict ?conflict .
            ?conflict field:conflictSeverity ?conflictSeverity ;
                field:conflictReason ?conflictReason ;
                field:requiresReview ?requiresReview .
        }}
    }}
}}
ORDER BY ?fieldName
"#,
            self.graph_uri, entity_id
        )
    }
}

impl Default for FieldLineageStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn create_test_source_value() -> SourceValue {
        SourceValue {
            id: "src_001".to_string(),
            value: serde_json::json!("test@example.com"),
            source_system: "CRM".to_string(),
            source_timestamp: Utc::now(),
            source_authority: 0.9,
            confidence: Some(0.85),
            vote_count: 3,
            vote_weight: 2.7,
            metadata: HashMap::new(),
        }
    }

    fn create_test_field_value() -> FieldValue {
        FieldValue {
            entity_id: "cust_001".to_string(),
            field_name: "email".to_string(),
            value: serde_json::json!("test@example.com"),
            value_type: "string".to_string(),
            confidence: 0.95,
            resolved_at: Utc::now(),
            valid_from: Utc::now(),
            valid_to: None,
            supersedes: None,
            explanation: Some("Selected by frequency voting".to_string()),
            resolution_id: "res_123".to_string(),
        }
    }

    #[test]
    fn test_field_value_to_triples() {
        let store = FieldLineageStore::new();
        let field_value = create_test_field_value();

        let triples = store.field_value_to_triples(&field_value);

        assert!(triples.contains("FieldValue>"));
        assert!(triples.contains("fieldName>"));
        assert!(triples.contains("fieldValue>"));
        assert!(triples.contains("confidence>"));
        assert!(triples.contains("atTime>"));
        assert!(triples.contains("validFrom>"));
        assert!(triples.contains("Selected by frequency voting"));
    }

    #[test]
    fn test_source_value_to_triples() {
        let store = FieldLineageStore::new();
        let source_value = create_test_source_value();

        let triples = store.source_value_to_triples(&source_value, "email");

        assert!(triples.contains("SourceValue>"));
        assert!(triples.contains("sourceSystem>"));
        assert!(triples.contains("CRM"));
        assert!(triples.contains("sourceAuthority>"));
        assert!(triples.contains("voteCount>"));
        assert!(triples.contains("voteWeight>"));
    }

    #[test]
    fn test_field_lineage_query_generation() {
        let store = FieldLineageStore::new();
        let query = store.query_field_lineage("cust_001", "email");

        assert!(query.contains("SELECT"));
        assert!(query.contains("field:FieldValue"));
        assert!(query.contains("field:fieldName \"email\""));
        assert!(query.contains("cust_001"));
        assert!(query.contains("ORDER BY DESC(?resolvedAt)"));
    }

    #[test]
    fn test_field_history_query_generation() {
        let store = FieldLineageStore::new();
        let query = store.query_field_history("cust_001", "address");

        assert!(query.contains("SELECT"));
        assert!(query.contains("field:validFrom"));
        assert!(query.contains("field:validTo"));
        assert!(query.contains("address"));
        assert!(query.contains("ORDER BY DESC(?validFrom)"));
    }

    #[test]
    fn test_conflicts_query_generation() {
        let store = FieldLineageStore::new();
        let query = store.query_conflicts_requiring_review();

        assert!(query.contains("SELECT"));
        assert!(query.contains("field:hasConflict"));
        assert!(query.contains("field:requiresReview \"true\""));
        assert!(query.contains("ORDER BY DESC(?resolvedAt)"));
    }
}
