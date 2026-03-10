//! LE-DAG Node Types
//!
//! FUTURE IMPLEMENTATION - Phase 3.0
//!
//! Defines the node types for Lineage Expression DAGs.
//! These will be stored in RocksDB column families as binary data.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// LE-DAG Node - Core building block
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LedagNode {
    /// Leaf node - defines base set from a data source
    Leaf(LeafNode),

    /// Operator node - transformation or aggregation
    Operator(OperatorNode),

    /// Reference node - points to another expression (for reuse)
    Reference(ReferenceNode),
}

/// Leaf node representing a base dataset
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeafNode {
    /// Unique node identifier
    pub id: String,

    /// Data source reference
    pub source: DataSourceRef,

    /// Selection predicate (SQL-like WHERE clause)
    pub predicate: Option<String>,

    /// Estimated cardinality
    pub cardinality_estimate: u64,

    /// Optional bitmap reference for flat sets
    pub bitmap_ref: Option<BitmapRef>,

    /// Metadata
    pub metadata: NodeMetadata,
}

/// Operator node for transformations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperatorNode {
    /// Unique node identifier
    pub id: String,

    /// Operation type
    pub operation: OperationType,

    /// Input node references
    pub inputs: Vec<String>,

    /// Operation parameters
    pub params: HashMap<String, serde_json::Value>,

    /// Estimated output cardinality
    pub cardinality_estimate: u64,

    /// Metadata
    pub metadata: NodeMetadata,
}

/// Reference to another expression (for reuse)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReferenceNode {
    /// Node identifier
    pub id: String,

    /// Referenced expression ID
    pub expression_ref: String,

    /// Optional filtering on the referenced expression
    pub filter: Option<String>,
}

/// Data source reference
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataSourceRef {
    /// System identifier (e.g., "kafka", "postgres", "s3")
    pub system: String,

    /// Resource path (e.g., topic name, table, bucket/key)
    pub resource: String,

    /// Time range for temporal queries
    pub time_range: Option<TimeRange>,

    /// Partition specification
    pub partitions: Option<Vec<String>>,
}

/// Time range for temporal filtering
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeRange {
    pub start: chrono::DateTime<chrono::Utc>,
    pub end: Option<chrono::DateTime<chrono::Utc>>,
}

/// Reference to bitmap data (for compressed storage)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BitmapRef {
    /// Storage key for bitmap data
    pub storage_key: String,

    /// Dictionary reference for UUID→integer mapping
    pub dictionary_ref: String,

    /// Bitmap type (Roaring, compressed bitset, etc.)
    pub bitmap_type: String,
}

/// Operation types supported by LE-DAG
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OperationType {
    // Set operations
    Union,
    Intersection,
    Difference,
    SymmetricDifference,

    // Filters
    Filter,
    Having,

    // Transformations
    Map,
    FlatMap,
    Project,

    // Aggregations
    Aggregate,
    GroupBy,
    Window,

    // Joins
    InnerJoin,
    LeftJoin,
    RightJoin,
    FullJoin,
    CrossJoin,

    // Sorting and limits
    Sort,
    Limit,
    TopK,

    // Custom operations
    Custom(String),
}

/// Node metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeMetadata {
    /// Creation timestamp
    pub created_at: chrono::DateTime<chrono::Utc>,

    /// Last accessed timestamp (for cache management)
    pub last_accessed: chrono::DateTime<chrono::Utc>,

    /// Access count (for optimization)
    pub access_count: u64,

    /// Computation cost estimate (milliseconds)
    pub cost_estimate: u64,

    /// Storage size (bytes)
    pub storage_bytes: u64,

    /// Additional metadata
    pub properties: HashMap<String, String>,
}

impl LedagNode {
    /// Get the node's unique identifier
    pub fn id(&self) -> &str {
        match self {
            LedagNode::Leaf(n) => &n.id,
            LedagNode::Operator(n) => &n.id,
            LedagNode::Reference(n) => &n.id,
        }
    }

    /// Get estimated cardinality
    pub fn cardinality(&self) -> u64 {
        match self {
            LedagNode::Leaf(n) => n.cardinality_estimate,
            LedagNode::Operator(n) => n.cardinality_estimate,
            LedagNode::Reference(_) => 0, // Would need to look up referenced expression
        }
    }

    /// Check if this is a leaf node
    pub fn is_leaf(&self) -> bool {
        matches!(self, LedagNode::Leaf(_))
    }

    /// Check if this is an operator node
    pub fn is_operator(&self) -> bool {
        matches!(self, LedagNode::Operator(_))
    }

    /// Get input dependencies
    pub fn inputs(&self) -> Vec<String> {
        match self {
            LedagNode::Leaf(_) => vec![],
            LedagNode::Operator(n) => n.inputs.clone(),
            LedagNode::Reference(n) => vec![n.expression_ref.clone()],
        }
    }
}

/// Builder for creating LE-DAG nodes
pub struct NodeBuilder {
    node_type: NodeType,
    id: Option<String>,
    inputs: Vec<String>,
    params: HashMap<String, serde_json::Value>,
}

enum NodeType {
    Leaf { source: DataSourceRef, predicate: Option<String> },
    Operator { operation: OperationType },
    Reference { expression_ref: String },
}

impl NodeBuilder {
    /// Create a leaf node builder
    pub fn leaf(source: DataSourceRef) -> Self {
        Self {
            node_type: NodeType::Leaf { source, predicate: None },
            id: None,
            inputs: vec![],
            params: HashMap::new(),
        }
    }

    /// Create an operator node builder
    pub fn operator(operation: OperationType) -> Self {
        Self {
            node_type: NodeType::Operator { operation },
            id: None,
            inputs: vec![],
            params: HashMap::new(),
        }
    }

    /// Set node ID
    pub fn with_id(mut self, id: String) -> Self {
        self.id = Some(id);
        self
    }

    /// Add input node
    pub fn with_input(mut self, input: String) -> Self {
        self.inputs.push(input);
        self
    }

    /// Add parameter
    pub fn with_param(mut self, key: String, value: serde_json::Value) -> Self {
        self.params.insert(key, value);
        self
    }

    /// Build the node
    pub fn build(self) -> LedagNode {
        let id = self.id.unwrap_or_else(|| format!("node_{}", uuid::Uuid::new_v4()));
        let now = chrono::Utc::now();

        match self.node_type {
            NodeType::Leaf { source, predicate } => {
                LedagNode::Leaf(LeafNode {
                    id,
                    source,
                    predicate,
                    cardinality_estimate: 0,
                    bitmap_ref: None,
                    metadata: NodeMetadata {
                        created_at: now,
                        last_accessed: now,
                        access_count: 0,
                        cost_estimate: 0,
                        storage_bytes: 0,
                        properties: HashMap::new(),
                    },
                })
            }
            NodeType::Operator { operation } => {
                LedagNode::Operator(OperatorNode {
                    id,
                    operation,
                    inputs: self.inputs,
                    params: self.params,
                    cardinality_estimate: 0,
                    metadata: NodeMetadata {
                        created_at: now,
                        last_accessed: now,
                        access_count: 0,
                        cost_estimate: 0,
                        storage_bytes: 0,
                        properties: HashMap::new(),
                    },
                })
            }
            NodeType::Reference { expression_ref } => {
                LedagNode::Reference(ReferenceNode {
                    id,
                    expression_ref,
                    filter: None,
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_builder() {
        let source = DataSourceRef {
            system: "kafka".to_string(),
            resource: "events".to_string(),
            time_range: None,
            partitions: None,
        };

        let leaf = NodeBuilder::leaf(source)
            .with_id("leaf1".to_string())
            .build();

        assert_eq!(leaf.id(), "leaf1");
        assert!(leaf.is_leaf());

        let union = NodeBuilder::operator(OperationType::Union)
            .with_input("leaf1".to_string())
            .with_input("leaf2".to_string())
            .build();

        assert!(union.is_operator());
        assert_eq!(union.inputs().len(), 2);
    }
}