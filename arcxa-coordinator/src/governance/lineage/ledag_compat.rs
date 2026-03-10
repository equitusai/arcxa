//! LE-DAG Compatibility Module
//!
//! Provides forward-compatibility layer for future LE-DAG (Lineage Expression DAG)
//! implementation. This module defines the interfaces and types that will be used
//! when LE-DAG optimization is introduced in Phase 3.0.
//!
//! Current implementation uses simple RDF storage, but the interface is designed
//! to transparently support LE-DAG when it's implemented.

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

use crate::governance::lineage::rdf_lineage::{LineageDescriptor, LineageStore};

/// Future-compatible lineage structure that can be stored as RDF or LE-DAG
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FutureCompatibleLineage {
    /// Unique identifier
    pub id: String,

    /// Storage backend hint
    pub storage_type: StorageType,

    /// For simple lineage: direct references
    pub simple: Option<SimpleLineage>,

    /// For complex lineage: expression reference
    pub expression: Option<ExpressionRef>,

    /// Common metadata
    pub metadata: LineageMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StorageType {
    /// Standard RDF triples (current implementation)
    RdfTriples,

    /// LE-DAG expression (future optimization)
    LedagExpression,

    /// Hybrid (RDF with LE-DAG for large sets)
    Hybrid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimpleLineage {
    pub sources: Vec<String>,
    pub transforms: Vec<String>,
    pub outputs: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExpressionRef {
    /// URI pointing to LE-DAG root node
    pub expression_uri: String,

    /// Estimated cardinality (for query planning)
    pub cardinality_estimate: Option<u64>,

    /// Storage location hint
    pub storage_ref: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LineageMetadata {
    pub created_at: DateTime<Utc>,
    pub tenant_id: String,
    pub correlation_id: Option<String>,
}

/// Operation types that will map to LE-DAG operators
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OperationType {
    /// Set operations
    Union,
    Intersection,
    Difference,

    /// Filters
    Filter { predicate: String },

    /// Transformations
    Map { function: String },
    FlatMap { function: String },

    /// Aggregations
    Aggregate {
        function: String,
        group_by: Vec<String>,
    },

    /// Joins
    Join {
        join_type: String,
        on: Vec<String>,
    },

    /// Custom operations
    Custom {
        operation: String,
        params: serde_json::Value,
    },
}

/// Lineage operation that can be converted to LE-DAG node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LineageOperation {
    pub id: String,
    pub operation: OperationType,
    pub inputs: Vec<String>,
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Storage adapter that will support both RDF and LE-DAG backends
pub struct LineageStorageAdapter {
    /// Current backend (RDF)
    rdf_store: Arc<dyn LineageStore>,

    /// Future backend (LE-DAG) - placeholder
    ledag_store: Option<Arc<dyn LedagStore>>,

    /// Configuration
    config: StorageConfig,
}

#[derive(Debug, Clone)]
pub struct StorageConfig {
    /// Threshold for using LE-DAG (number of items)
    pub ledag_threshold: usize,

    /// Enable hybrid storage
    pub hybrid_enabled: bool,

    /// Cache configuration
    pub cache_size: usize,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            ledag_threshold: 10_000,
            hybrid_enabled: false,
            cache_size: 1000,
        }
    }
}

impl LineageStorageAdapter {
    pub fn new(rdf_store: Arc<dyn LineageStore>) -> Self {
        Self {
            rdf_store,
            ledag_store: None,
            config: StorageConfig::default(),
        }
    }

    pub fn with_config(mut self, config: StorageConfig) -> Self {
        self.config = config;
        self
    }

    /// Store lineage with automatic backend selection
    pub async fn store_lineage(&self, lineage: FutureCompatibleLineage) -> Result<()> {
        match lineage.storage_type {
            StorageType::RdfTriples => {
                // Current implementation: store as RDF
                self.store_as_rdf(lineage).await
            }
            StorageType::LedagExpression => {
                // Future: store as LE-DAG
                if let Some(ledag) = &self.ledag_store {
                    ledag.store_expression(lineage).await
                } else {
                    // Fallback to RDF if LE-DAG not available
                    self.store_as_rdf(lineage).await
                }
            }
            StorageType::Hybrid => {
                // Store metadata in RDF, expression in LE-DAG
                self.store_hybrid(lineage).await
            }
        }
    }

    /// Store as RDF triples (current implementation)
    async fn store_as_rdf(&self, lineage: FutureCompatibleLineage) -> Result<()> {
        // Convert to LineageDescriptor for RDF storage
        let descriptor = self.to_lineage_descriptor(lineage)?;
        self.rdf_store.store_descriptor(descriptor).await
    }

    /// Store hybrid (future implementation)
    async fn store_hybrid(&self, lineage: FutureCompatibleLineage) -> Result<()> {
        // For now, just store as RDF
        // In future: store metadata in RDF, large sets in LE-DAG
        self.store_as_rdf(lineage).await
    }

    /// Query lineage with transparent backend selection
    pub async fn query_lineage(&self, uri: &str) -> Result<FutureCompatibleLineage> {
        // Try LE-DAG store first if available
        if let Some(ledag) = &self.ledag_store {
            if let Ok(result) = ledag.query_expression(uri).await {
                return Ok(result);
            }
        }

        // Fall back to RDF store
        let descriptor = self.rdf_store.get_lineage_graph(uri).await?;
        Ok(self.from_lineage_descriptor(descriptor))
    }

    /// Convert LineageDescriptor to FutureCompatibleLineage
    fn from_lineage_descriptor(&self, desc: LineageDescriptor) -> FutureCompatibleLineage {
        FutureCompatibleLineage {
            id: desc.lineage_uri,
            storage_type: StorageType::RdfTriples,
            simple: Some(SimpleLineage {
                sources: desc.source_refs,
                transforms: desc.transform_refs,
                outputs: desc.output_refs,
            }),
            expression: None,
            metadata: LineageMetadata {
                created_at: desc.metadata.created_at,
                tenant_id: desc.metadata.tenant_id,
                correlation_id: desc.metadata.correlation_id,
            },
        }
    }

    /// Convert FutureCompatibleLineage to LineageDescriptor
    fn to_lineage_descriptor(&self, fcl: FutureCompatibleLineage) -> Result<LineageDescriptor> {
        let simple = fcl.simple.ok_or_else(|| {
            anyhow::anyhow!("Cannot convert expression-based lineage to descriptor")
        })?;

        Ok(LineageDescriptor {
            lineage_uri: fcl.id,
            lineage_type: crate::governance::lineage::rdf_lineage::LineageType::Hybrid,
            source_refs: simple.sources,
            transform_refs: simple.transforms,
            output_refs: simple.outputs,
            operations: vec![],
            metadata: crate::governance::lineage::rdf_lineage::LineageMetadata {
                created_at: fcl.metadata.created_at,
                created_by: "system".to_string(),
                tenant_id: fcl.metadata.tenant_id,
                run_id: uuid::Uuid::new_v4().to_string(),
                correlation_id: fcl.metadata.correlation_id,
            },
        })
    }
}

/// Trait for future LE-DAG storage backend
#[async_trait::async_trait]
pub trait LedagStore: Send + Sync {
    /// Store LE-DAG expression
    async fn store_expression(&self, lineage: FutureCompatibleLineage) -> Result<()>;

    /// Query LE-DAG expression
    async fn query_expression(&self, uri: &str) -> Result<FutureCompatibleLineage>;

    /// Expand expression to items (with limit)
    async fn expand_expression(&self, uri: &str, limit: usize) -> Result<Vec<String>>;

    /// Get expression metadata without expansion
    async fn get_expression_metadata(&self, uri: &str) -> Result<ExpressionMetadata>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExpressionMetadata {
    pub expression_type: String,
    pub cardinality: u64,
    pub depth: u32,
    pub created_at: DateTime<Utc>,
    pub storage_size_bytes: u64,
}

/// Builder for creating LE-DAG compatible lineage
pub struct LineageBuilder {
    operations: Vec<LineageOperation>,
    metadata: HashMap<String, serde_json::Value>,
}

impl LineageBuilder {
    pub fn new() -> Self {
        Self {
            operations: Vec::new(),
            metadata: HashMap::new(),
        }
    }

    /// Add a leaf node (data source)
    pub fn add_source(mut self, source_uri: &str, predicate: Option<&str>) -> Self {
        let op = LineageOperation {
            id: format!("leaf_{}", uuid::Uuid::new_v4()),
            operation: if let Some(pred) = predicate {
                OperationType::Filter {
                    predicate: pred.to_string(),
                }
            } else {
                OperationType::Custom {
                    operation: "LEAF".to_string(),
                    params: serde_json::json!({ "source": source_uri }),
                }
            },
            inputs: vec![],
            metadata: HashMap::new(),
        };
        self.operations.push(op);
        self
    }

    /// Add a union operation
    pub fn union(mut self, input1: &str, input2: &str) -> Self {
        let op = LineageOperation {
            id: format!("union_{}", uuid::Uuid::new_v4()),
            operation: OperationType::Union,
            inputs: vec![input1.to_string(), input2.to_string()],
            metadata: HashMap::new(),
        };
        self.operations.push(op);
        self
    }

    /// Add a filter operation
    pub fn filter(mut self, input: &str, predicate: &str) -> Self {
        let op = LineageOperation {
            id: format!("filter_{}", uuid::Uuid::new_v4()),
            operation: OperationType::Filter {
                predicate: predicate.to_string(),
            },
            inputs: vec![input.to_string()],
            metadata: HashMap::new(),
        };
        self.operations.push(op);
        self
    }

    /// Add an aggregation
    pub fn aggregate(
        mut self,
        input: &str,
        function: &str,
        group_by: Vec<String>,
    ) -> Self {
        let op = LineageOperation {
            id: format!("agg_{}", uuid::Uuid::new_v4()),
            operation: OperationType::Aggregate {
                function: function.to_string(),
                group_by,
            },
            inputs: vec![input.to_string()],
            metadata: HashMap::new(),
        };
        self.operations.push(op);
        self
    }

    /// Build the lineage
    pub fn build(self) -> FutureCompatibleLineage {
        // For now, build simple lineage
        // In future, this will construct LE-DAG expression

        let sources: Vec<String> = self.operations.iter()
            .filter(|op| op.inputs.is_empty())
            .map(|op| op.id.clone())
            .collect();

        let outputs: Vec<String> = self.operations.iter()
            .filter(|op| {
                // Find operations that are not inputs to any other operation
                !self.operations.iter().any(|other| other.inputs.contains(&op.id))
            })
            .map(|op| op.id.clone())
            .collect();

        FutureCompatibleLineage {
            id: format!("lineage_{}", uuid::Uuid::new_v4()),
            storage_type: StorageType::RdfTriples,
            simple: Some(SimpleLineage {
                sources,
                transforms: self.operations.iter().map(|op| op.id.clone()).collect(),
                outputs,
            }),
            expression: None,
            metadata: LineageMetadata {
                created_at: Utc::now(),
                tenant_id: "default".to_string(),
                correlation_id: None,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lineage_builder() {
        let lineage = LineageBuilder::new()
            .add_source("source1", Some("date > '2024-01-01'"))
            .add_source("source2", None)
            .union("source1", "source2")
            .filter("union_result", "status = 'active'")
            .aggregate("filter_result", "SUM", vec!["customer_id".to_string()])
            .build();

        assert!(lineage.simple.is_some());
        let simple = lineage.simple.unwrap();
        assert_eq!(simple.sources.len(), 2);
        assert!(!simple.transforms.is_empty());
    }

    #[test]
    fn test_operation_types() {
        let union = OperationType::Union;
        let filter = OperationType::Filter {
            predicate: "x > 10".to_string(),
        };
        let aggregate = OperationType::Aggregate {
            function: "AVG".to_string(),
            group_by: vec!["dept".to_string()],
        };

        // Verify serialization works
        let _union_json = serde_json::to_string(&union).unwrap();
        let _filter_json = serde_json::to_string(&filter).unwrap();
        let _aggregate_json = serde_json::to_string(&aggregate).unwrap();
    }
}