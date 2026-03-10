// RDF-star Support for Graphica
//
// This module provides RDF-star (RDF*) support for annotating statements
// with confidence scores, provenance, and temporal metadata.
//
// RDF-star allows us to make statements about statements directly,
// avoiding the verbosity of reification while maintaining semantic clarity.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Trait for converting domain types to RDF-star annotated triples
pub trait ToRdfStarTriples {
    /// Convert to RDF-star annotated triples
    fn to_rdf_star_triples(&self) -> Result<Vec<AnnotatedTriple>>;
}

/// An RDF triple with annotations
///
/// Represents a statement that can have metadata attached directly to it.
/// In RDF-star syntax: << subject predicate object >> annotation_predicate annotation_object
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnnotatedTriple {
    /// Subject of the triple
    pub subject: String,

    /// Predicate of the triple
    pub predicate: String,

    /// Object of the triple
    pub object: String,

    /// Annotations on the triple itself
    pub annotations: Vec<Annotation>,
}

impl AnnotatedTriple {
    /// Create a new annotated triple
    pub fn new(
        subject: impl Into<String>,
        predicate: impl Into<String>,
        object: impl Into<String>,
    ) -> Self {
        Self {
            subject: subject.into(),
            predicate: predicate.into(),
            object: object.into(),
            annotations: Vec::new(),
        }
    }

    /// Add an annotation to the triple
    pub fn with_annotation(mut self, predicate: impl Into<String>, object: TripleValue) -> Self {
        self.annotations.push(Annotation {
            predicate: predicate.into(),
            object,
        });
        self
    }

    /// Add confidence annotation
    pub fn with_confidence(self, confidence: f64) -> Self {
        self.with_annotation(
            "http://graphica.io/ontology#confidence",
            TripleValue::typed_literal(confidence.to_string(), "xsd:decimal"),
        )
    }

    /// Add provenance annotation
    pub fn with_provenance(self, generated_by: impl Into<String>) -> Self {
        self.with_annotation(
            "http://www.w3.org/ns/prov#wasGeneratedBy",
            TripleValue::Uri(generated_by.into()),
        )
    }

    /// Add temporal annotation
    pub fn with_timestamp(self, timestamp: &chrono::DateTime<chrono::Utc>) -> Self {
        self.with_annotation(
            "http://www.w3.org/ns/prov#generatedAtTime",
            TripleValue::typed_literal(timestamp.to_rfc3339(), "xsd:dateTime"),
        )
    }

    /// Add transaction ID annotation
    pub fn with_transaction(self, tx_id: impl Into<String>) -> Self {
        self.with_annotation(
            "http://graphica.io/ontology#transactionId",
            TripleValue::Literal(tx_id.into()),
        )
    }

    /// Add bitemporal valid time annotations (business time)
    ///
    /// # Arguments
    /// * `valid_from` - When the data became true in the real world
    /// * `valid_to` - When the data ceased to be true (None = still valid)
    pub fn with_valid_time(
        self,
        valid_from: chrono::DateTime<chrono::Utc>,
        valid_to: Option<chrono::DateTime<chrono::Utc>>,
    ) -> Self {
        let mut triple = self.with_annotation(
            "http://graphica.io/ontology#validFrom",
            TripleValue::datetime(&valid_from),
        );

        triple = triple.with_annotation(
            "http://graphica.io/ontology#validTo",
            valid_to
                .map(|t| TripleValue::datetime(&t))
                .unwrap_or_else(|| TripleValue::Literal("MAX".to_string())),
        );

        triple
    }

    /// Add bitemporal transaction time annotations (system time)
    ///
    /// # Arguments
    /// * `tx_id` - Transaction identifier
    /// * `tx_to` - When this version was superseded (None = current version)
    pub fn with_transaction_time(
        self,
        tx_id: &crate::governance::bitemporal::TransactionId,
        tx_to: Option<chrono::DateTime<chrono::Utc>>,
    ) -> Self {
        let mut triple = self
            .with_annotation(
                "http://graphica.io/ontology#txId",
                TripleValue::integer(tx_id.seq as i64),
            )
            .with_annotation(
                "http://graphica.io/ontology#txFrom",
                TripleValue::datetime(&tx_id.timestamp),
            )
            .with_annotation(
                "http://graphica.io/ontology#nodeId",
                TripleValue::integer(tx_id.node_id as i64),
            );

        triple = triple.with_annotation(
            "http://graphica.io/ontology#txTo",
            tx_to
                .map(|t| TripleValue::datetime(&t))
                .unwrap_or_else(|| TripleValue::Literal("MAX".to_string())),
        );

        triple
    }
}

/// An annotation on a triple
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Annotation {
    /// Predicate of the annotation
    pub predicate: String,

    /// Value of the annotation
    pub object: TripleValue,
}

impl Annotation {
    /// Create a new annotation
    pub fn new(predicate: impl Into<String>, object: TripleValue) -> Self {
        Self {
            predicate: predicate.into(),
            object,
        }
    }

    /// Create a confidence annotation
    pub fn confidence(value: f64) -> Self {
        Self::new(
            "http://graphica.io/ontology#confidence",
            TripleValue::typed_literal(value.to_string(), "xsd:decimal"),
        )
    }

    /// Create a model annotation
    pub fn model(model_id: impl Into<String>) -> Self {
        Self::new(
            "http://www.w3.org/ns/prov#wasGeneratedBy",
            TripleValue::Uri(format!("http://graphica.io/model/{}", model_id.into())),
        )
    }

    /// Create a timestamp annotation
    pub fn timestamp(dt: &chrono::DateTime<chrono::Utc>) -> Self {
        Self::new(
            "http://www.w3.org/ns/prov#generatedAtTime",
            TripleValue::typed_literal(dt.to_rfc3339(), "xsd:dateTime"),
        )
    }
}

/// Value types for RDF triples
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TripleValue {
    /// URI/IRI reference
    Uri(String),

    /// Plain literal
    Literal(String),

    /// Typed literal with datatype
    TypedLiteral { value: String, datatype: String },

    /// Language-tagged literal
    LanguageLiteral { value: String, lang: String },
}

impl TripleValue {
    /// Create a URI value
    pub fn uri(uri: impl Into<String>) -> Self {
        Self::Uri(uri.into())
    }

    /// Create a plain literal
    pub fn literal(value: impl Into<String>) -> Self {
        Self::Literal(value.into())
    }

    /// Create a typed literal
    pub fn typed_literal(value: impl Into<String>, datatype: impl Into<String>) -> Self {
        Self::TypedLiteral {
            value: value.into(),
            datatype: datatype.into(),
        }
    }

    /// Create a language-tagged literal
    pub fn lang_literal(value: impl Into<String>, lang: impl Into<String>) -> Self {
        Self::LanguageLiteral {
            value: value.into(),
            lang: lang.into(),
        }
    }

    /// Create a decimal value
    pub fn decimal(value: f64) -> Self {
        Self::typed_literal(value.to_string(), "xsd:decimal")
    }

    /// Create an integer value
    pub fn integer(value: i64) -> Self {
        Self::typed_literal(value.to_string(), "xsd:integer")
    }

    /// Create a boolean value
    pub fn boolean(value: bool) -> Self {
        Self::typed_literal(value.to_string(), "xsd:boolean")
    }

    /// Create a datetime value
    pub fn datetime(dt: &chrono::DateTime<chrono::Utc>) -> Self {
        Self::typed_literal(dt.to_rfc3339(), "xsd:dateTime")
    }
}

impl From<String> for TripleValue {
    fn from(s: String) -> Self {
        // Simple heuristic: URIs start with http:// or https://
        if s.starts_with("http://") || s.starts_with("https://") {
            Self::Uri(s)
        } else {
            Self::Literal(s)
        }
    }
}

impl From<&str> for TripleValue {
    fn from(s: &str) -> Self {
        Self::from(s.to_string())
    }
}

impl From<f64> for TripleValue {
    fn from(value: f64) -> Self {
        Self::decimal(value)
    }
}

impl From<i64> for TripleValue {
    fn from(value: i64) -> Self {
        Self::integer(value)
    }
}

impl From<bool> for TripleValue {
    fn from(value: bool) -> Self {
        Self::boolean(value)
    }
}

/// Builder for creating annotated triples
pub struct AnnotatedTripleBuilder {
    subject: String,
    predicate: String,
    object: String,
    annotations: Vec<Annotation>,
}

impl AnnotatedTripleBuilder {
    /// Create a new builder
    pub fn new(
        subject: impl Into<String>,
        predicate: impl Into<String>,
        object: impl Into<String>,
    ) -> Self {
        Self {
            subject: subject.into(),
            predicate: predicate.into(),
            object: object.into(),
            annotations: Vec::new(),
        }
    }

    /// Add an annotation
    pub fn annotation(mut self, predicate: impl Into<String>, object: TripleValue) -> Self {
        self.annotations.push(Annotation::new(predicate, object));
        self
    }

    /// Add confidence
    pub fn confidence(self, value: f64) -> Self {
        self.annotation(
            "http://graphica.io/ontology#confidence",
            TripleValue::decimal(value),
        )
    }

    /// Add model reference
    pub fn model(self, model_id: impl Into<String>, version: impl Into<String>) -> Self {
        let model_uri = format!("http://graphica.io/model/{}", model_id.into());
        self.annotation(
            "http://www.w3.org/ns/prov#wasGeneratedBy",
            TripleValue::uri(model_uri),
        )
        .annotation(
            "http://graphica.io/ml#modelVersion",
            TripleValue::literal(version),
        )
    }

    /// Add timestamp
    pub fn timestamp(self, dt: chrono::DateTime<chrono::Utc>) -> Self {
        self.annotation(
            "http://www.w3.org/ns/prov#generatedAtTime",
            TripleValue::datetime(&dt),
        )
    }

    /// Add transaction ID
    pub fn transaction(self, tx_id: impl Into<String>) -> Self {
        self.annotation(
            "http://graphica.io/ontology#transactionId",
            TripleValue::literal(tx_id),
        )
    }

    /// Add fusion metadata
    pub fn fusion(
        self,
        method: impl Into<String>,
        confidence: f64,
        reversal: Option<chrono::DateTime<chrono::Utc>>,
    ) -> Self {
        let mut builder = self
            .annotation(
                "http://graphica.io/ontology#fusionMethod",
                TripleValue::literal(method),
            )
            .annotation(
                "http://graphica.io/ontology#fusionConfidence",
                TripleValue::decimal(confidence),
            );

        if let Some(rev_time) = reversal {
            builder = builder.annotation(
                "http://graphica.io/ontology#reversalTimestamp",
                TripleValue::datetime(&rev_time),
            );
        } else {
            builder = builder.annotation(
                "http://graphica.io/ontology#reversalTimestamp",
                TripleValue::literal("null"),
            );
        }

        builder
    }

    /// Add bitemporal valid time annotations (business time)
    ///
    /// # Arguments
    /// * `valid_from` - When the data became true in the real world
    /// * `valid_to` - When the data ceased to be true (None = still valid)
    pub fn valid_time(
        self,
        valid_from: chrono::DateTime<chrono::Utc>,
        valid_to: Option<chrono::DateTime<chrono::Utc>>,
    ) -> Self {
        let builder = self.annotation(
            "http://graphica.io/ontology#validFrom",
            TripleValue::datetime(&valid_from),
        );

        builder.annotation(
            "http://graphica.io/ontology#validTo",
            valid_to
                .map(|t| TripleValue::datetime(&t))
                .unwrap_or_else(|| TripleValue::literal("MAX")),
        )
    }

    /// Add bitemporal transaction time annotations (system time)
    ///
    /// # Arguments
    /// * `tx_id` - Transaction identifier
    /// * `tx_to` - When this version was superseded (None = current version)
    pub fn transaction_time(
        self,
        tx_id: &crate::governance::bitemporal::TransactionId,
        tx_to: Option<chrono::DateTime<chrono::Utc>>,
    ) -> Self {
        let builder = self
            .annotation(
                "http://graphica.io/ontology#txId",
                TripleValue::integer(tx_id.seq as i64),
            )
            .annotation(
                "http://graphica.io/ontology#txFrom",
                TripleValue::datetime(&tx_id.timestamp),
            )
            .annotation(
                "http://graphica.io/ontology#nodeId",
                TripleValue::integer(tx_id.node_id as i64),
            );

        builder.annotation(
            "http://graphica.io/ontology#txTo",
            tx_to
                .map(|t| TripleValue::datetime(&t))
                .unwrap_or_else(|| TripleValue::literal("MAX")),
        )
    }

    /// Build the annotated triple
    pub fn build(self) -> AnnotatedTriple {
        AnnotatedTriple {
            subject: self.subject,
            predicate: self.predicate,
            object: self.object,
            annotations: self.annotations,
        }
    }
}

/// Collection of annotation predicates
pub mod predicates {
    /// Graphica ontology namespace
    pub const GRAPHICA_NS: &str = "http://graphica.io/ontology#";

    /// W3C PROV namespace
    pub const PROV_NS: &str = "http://www.w3.org/ns/prov#";

    /// Machine Learning namespace
    pub const ML_NS: &str = "http://graphica.io/ml#";

    // Confidence & Quality
    pub const CONFIDENCE: &str = "http://graphica.io/ontology#confidence";
    pub const ACCURACY: &str = "http://graphica.io/ontology#accuracy";
    pub const QUALITY_SCORE: &str = "http://graphica.io/ontology#qualityScore";

    // Provenance
    pub const GENERATED_BY: &str = "http://www.w3.org/ns/prov#wasGeneratedBy";
    pub const ASSOCIATED_WITH: &str = "http://www.w3.org/ns/prov#wasAssociatedWith";
    pub const GENERATED_AT: &str = "http://www.w3.org/ns/prov#generatedAtTime";
    pub const INVALIDATED_AT: &str = "http://www.w3.org/ns/prov#invalidatedAtTime";

    // Temporal (Valid Time - Business Time)
    pub const VALID_FROM: &str = "http://graphica.io/ontology#validFrom";
    pub const VALID_TO: &str = "http://graphica.io/ontology#validTo";
    pub const OBSERVED_AT: &str = "http://graphica.io/ontology#observedAt";

    // Transaction Time (System Time - MVCC)
    pub const TX_ID: &str = "http://graphica.io/ontology#txId";
    pub const TX_FROM: &str = "http://graphica.io/ontology#txFrom";
    pub const TX_TO: &str = "http://graphica.io/ontology#txTo";
    pub const NODE_ID: &str = "http://graphica.io/ontology#nodeId";

    // Transaction & Audit
    pub const TRANSACTION_ID: &str = "http://graphica.io/ontology#transactionId";
    pub const CORRELATION_ID: &str = "http://graphica.io/ontology#correlationId";
    pub const AUDIT_USER: &str = "http://graphica.io/ontology#auditUser";

    // Fusion & Deduplication
    pub const FUSION_METHOD: &str = "http://graphica.io/ontology#fusionMethod";
    pub const FUSION_CONFIDENCE: &str = "http://graphica.io/ontology#fusionConfidence";
    pub const REVERSAL_TIMESTAMP: &str = "http://graphica.io/ontology#reversalTimestamp";
    pub const SURVIVOR_RULE: &str = "http://graphica.io/ontology#survivorRule";

    // Model Metadata
    pub const MODEL_ID: &str = "http://graphica.io/ml#modelId";
    pub const MODEL_VERSION: &str = "http://graphica.io/ml#modelVersion";
    pub const MODEL_TYPE: &str = "http://graphica.io/ml#modelType";
    pub const TRAINING_DATA_HASH: &str = "http://graphica.io/ml#trainingDataHash";
}

/// Helper functions for common RDF-star patterns
pub mod patterns {
    use super::*;

    /// Create a confidence-annotated derived attribute
    pub fn derived_attribute(
        entity_id: &str,
        attribute_name: &str,
        value: &str,
        confidence: f64,
        model_id: &str,
    ) -> AnnotatedTriple {
        AnnotatedTripleBuilder::new(
            format!("http://graphica.io/entity/{}", entity_id),
            "http://graphica.io/ontology#hasDerivedAttribute",
            format!("http://graphica.io/attribute/{}", attribute_name),
        )
        .confidence(confidence)
        .model(model_id, "latest")
        .timestamp(chrono::Utc::now())
        .build()
    }

    /// Create a fusion relationship
    pub fn fusion_relationship(
        merged_entity: &str,
        source_entity: &str,
        method: &str,
        confidence: f64,
    ) -> AnnotatedTriple {
        AnnotatedTripleBuilder::new(
            format!("http://graphica.io/entity/{}", merged_entity),
            "http://www.w3.org/2002/07/owl#sameAs",
            format!("http://graphica.io/entity/{}", source_entity),
        )
        .fusion(method, confidence, None)
        .timestamp(chrono::Utc::now())
        .build()
    }

    /// Create a lineage relationship
    pub fn lineage_relationship(
        lineage_id: &str,
        source_ref: &str,
        confidence: f64,
        transaction_id: &str,
    ) -> AnnotatedTriple {
        AnnotatedTripleBuilder::new(
            format!("http://graphica.io/lineage/{}", lineage_id),
            "http://www.w3.org/ns/prov#used",
            format!("http://graphica.io/source/{}", source_ref),
        )
        .confidence(confidence)
        .transaction(transaction_id)
        .timestamp(chrono::Utc::now())
        .build()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_annotated_triple_builder() {
        let triple = AnnotatedTripleBuilder::new(
            "http://graphica.io/entity/123",
            "http://graphica.io/ontology#hasDerivedAttribute",
            "http://graphica.io/attribute/gender",
        )
        .confidence(0.85)
        .model("gender_classifier", "v1.2.0")
        .timestamp(chrono::Utc::now())
        .build();

        assert_eq!(triple.subject, "http://graphica.io/entity/123");
        // model() adds 2 annotations (wasGeneratedBy + modelVersion), plus confidence + timestamp = 4
        assert_eq!(triple.annotations.len(), 4);
    }

    #[test]
    fn test_triple_value_conversions() {
        let uri_val = TripleValue::from("http://example.org");
        assert!(matches!(uri_val, TripleValue::Uri(_)));

        let lit_val = TripleValue::from("plain text");
        assert!(matches!(lit_val, TripleValue::Literal(_)));

        let decimal_val = TripleValue::from(0.95);
        assert!(matches!(decimal_val, TripleValue::TypedLiteral { .. }));
    }

    #[test]
    fn test_patterns() {
        let derived =
            patterns::derived_attribute("cust_123", "gender", "female", 0.92, "gender_model_v2");

        assert!(derived.subject.contains("cust_123"));
        assert!(derived
            .annotations
            .iter()
            .any(|a| { a.predicate.contains("confidence") }));

        let fusion =
            patterns::fusion_relationship("entity_final", "entity_source", "exact_match", 0.98);

        assert!(fusion.predicate.contains("sameAs"));
        // fusion() with None reversal adds only 2 annotations: fusionMethod, fusionConfidence
        // (reversalTimestamp is only added when reversal is Some)
        assert_eq!(
            fusion
                .annotations
                .iter()
                .filter(|a| { a.predicate.contains("fusion") })
                .count(),
            2
        );
    }

    #[test]
    fn test_annotation_helpers() {
        let conf_ann = Annotation::confidence(0.85);
        assert!(conf_ann.predicate.contains("confidence"));

        let model_ann = Annotation::model("churn_model_v3");
        assert!(conf_ann.predicate.contains("confidence"));

        let time_ann = Annotation::timestamp(&chrono::Utc::now());
        assert!(time_ann.predicate.contains("generatedAtTime"));
    }
}
