//! Column-Level Lineage Tracking
//!
//! Fine-grained column-to-column lineage for data engineering and impact analysis.
//! Tracks transformations, dependencies, and data flow at the column level.

use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

// =============================================================================
// Core Types
// =============================================================================

/// Column reference with full qualification
///
/// Uniquely identifies a column across all datasources, schemas, and tables.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq, Eq, Hash)]
pub struct ColumnRef {
    /// Database or datasource ID (e.g., "postgres-prod", "csv-staging")
    pub datasource_id: String,

    /// Schema name (for databases with schema support, None for files/NoSQL)
    pub schema: Option<String>,

    /// Table or file name
    pub table_name: String,

    /// Column name
    pub column_name: String,

    /// Data type (e.g., "VARCHAR(255)", "INTEGER", "TIMESTAMP")
    pub data_type: String,
}

impl ColumnRef {
    /// Create a fully qualified column reference
    pub fn new(
        datasource_id: impl Into<String>,
        table_name: impl Into<String>,
        column_name: impl Into<String>,
        data_type: impl Into<String>,
    ) -> Self {
        Self {
            datasource_id: datasource_id.into(),
            schema: None,
            table_name: table_name.into(),
            column_name: column_name.into(),
            data_type: data_type.into(),
        }
    }

    /// Create with schema
    pub fn with_schema(mut self, schema: impl Into<String>) -> Self {
        self.schema = Some(schema.into());
        self
    }

    /// Get fully qualified name (datasource.schema.table.column or datasource.table.column)
    pub fn fully_qualified_name(&self) -> String {
        if let Some(ref schema) = self.schema {
            format!(
                "{}.{}.{}.{}",
                self.datasource_id, schema, self.table_name, self.column_name
            )
        } else {
            format!(
                "{}.{}.{}",
                self.datasource_id, self.table_name, self.column_name
            )
        }
    }
}

/// Column-to-column lineage event
///
/// Represents a single transformation or derivation relationship between columns.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ColumnLineageEvent {
    /// Unique event ID
    pub id: String,

    /// Source columns (can be multiple for derived columns)
    pub source_columns: Vec<ColumnRef>,

    /// Target/derived column
    pub target_column: ColumnRef,

    /// Transformation logic (SQL expression, Python code, formula, etc.)
    pub transformation_logic: String,

    /// Type of transformation applied
    pub transformation_type: TransformationType,

    /// Job/run ID that created this lineage
    pub job_id: String,

    /// Workflow or pipeline ID
    pub workflow_id: Option<String>,

    /// Tenant ID for multi-tenancy
    pub tenant_id: String,

    /// Timestamp when lineage was created
    pub created_at: DateTime<Utc>,

    /// Confidence score (0.0-1.0) for auto-detected lineage
    /// None = manually specified, Some(x) = auto-detected with confidence x
    pub confidence: Option<f64>,

    /// User or system that created this lineage
    pub created_by: String,

    /// Additional metadata
    pub metadata: Option<serde_json::Value>,
}

/// Type of transformation applied to create derived column
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TransformationType {
    /// Direct copy with no transformation (1:1 mapping)
    DirectCopy,

    /// SQL expression transformation
    SqlExpression,

    /// Python/Scala/Java UDF transformation
    UdfTransformation {
        /// UDF name or identifier
        udf_name: String,
    },

    /// Aggregation function (SUM, AVG, COUNT, etc.)
    Aggregation {
        /// Aggregation function name
        function: String,
        /// GROUP BY columns
        group_by: Option<Vec<String>>,
    },

    /// Join operation
    Join {
        /// Join type (INNER, LEFT, RIGHT, FULL, CROSS)
        join_type: String,
        /// Join condition
        join_condition: String,
    },

    /// Type cast or conversion
    TypeCast {
        /// Original data type
        from_type: String,
        /// Target data type
        to_type: String,
    },

    /// String concatenation
    Concatenation {
        /// Separator used
        separator: Option<String>,
    },

    /// Substring extraction
    Substring {
        /// Start position
        start: i32,
        /// Length (None = to end)
        length: Option<i32>,
    },

    /// Case/when conditional logic
    Conditional,

    /// Mathematical operation (+, -, *, /, etc.)
    MathOperation {
        /// Operation type
        operation: String,
    },

    /// Date/time manipulation
    DateTimeOperation {
        /// Operation type (EXTRACT, DATE_ADD, DATE_DIFF, etc.)
        operation: String,
    },

    /// Lookup/mapping from reference data
    Lookup {
        /// Reference table
        reference_table: String,
    },

    /// ML model prediction or feature extraction
    MlTransformation {
        /// Model ID
        model_id: String,
        /// Model version
        model_version: String,
    },

    /// Custom transformation (catch-all)
    Custom {
        /// Custom transformation description
        description: String,
    },
}

// =============================================================================
// Graph & Analysis Types
// =============================================================================

/// Column lineage graph response
///
/// Contains the complete dependency graph for a column, including all upstream sources.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ColumnLineageGraph {
    /// Target column being analyzed
    pub column: ColumnRef,

    /// All source columns (transitive closure - all upstream dependencies)
    pub source_columns: Vec<ColumnRef>,

    /// Direct dependencies (immediate parent columns)
    pub direct_dependencies: Vec<ColumnLineageEvent>,

    /// All transformation events in the graph
    pub all_transformations: Vec<ColumnLineageEvent>,

    /// Lineage depth (max hops from root sources to target)
    pub lineage_depth: usize,

    /// Total transformation steps
    pub total_transformations: usize,

    /// Statistics about the lineage graph
    pub statistics: ColumnLineageStatistics,
}

/// Statistics about column lineage
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ColumnLineageStatistics {
    /// Total source datasources involved
    pub source_datasources: usize,

    /// Total source tables involved
    pub source_tables: usize,

    /// Total source columns (leaf nodes)
    pub source_columns: usize,

    /// Transformation type distribution
    pub transformation_types: std::collections::HashMap<String, usize>,

    /// Has circular dependency (should be false for columns)
    pub has_circular_dependency: bool,

    /// Average confidence score for auto-detected lineage
    pub average_confidence: Option<f64>,
}

/// Column impact analysis response
///
/// Shows what would be affected if a column is changed or removed.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ColumnImpactAnalysis {
    /// Source column being analyzed
    pub source_column: ColumnRef,

    /// All downstream columns that depend on this column
    pub affected_columns: Vec<ColumnRef>,

    /// All affected workflows/pipelines
    pub affected_pipelines: Vec<String>,

    /// All affected jobs
    pub affected_jobs: Vec<String>,

    /// Estimated records affected (if available)
    pub estimated_records_affected: Option<u64>,

    /// Impact depth (max hops to furthest affected column)
    pub impact_depth: usize,

    /// Total downstream transformations
    pub total_downstream_transformations: usize,

    /// Critical dependencies (affected columns in production tables)
    pub critical_dependencies: Vec<ColumnRef>,
}

// =============================================================================
// Storage Trait
// =============================================================================

/// Column lineage storage trait
///
/// Defines the interface for storing and querying column-level lineage.
#[async_trait]
pub trait ColumnLineageSink: Send + Sync {
    /// Record a single column lineage event
    async fn record_column_lineage(&self, event: ColumnLineageEvent) -> Result<()>;

    /// Record multiple column lineage events (batch operation)
    async fn record_column_lineage_batch(&self, events: Vec<ColumnLineageEvent>) -> Result<()>;

    /// Get all lineage events for a specific column
    ///
    /// Returns all transformations that produce this column.
    async fn get_column_lineage(&self, column: &ColumnRef) -> Result<Vec<ColumnLineageEvent>>;

    /// Trace column lineage graph (upstream dependencies)
    ///
    /// Recursively follows dependencies up to max_depth hops.
    async fn trace_column_graph(
        &self,
        column: &ColumnRef,
        max_depth: usize,
    ) -> Result<ColumnLineageGraph>;

    /// Analyze column impact (downstream effects)
    ///
    /// Shows what would be affected if this column changes.
    async fn analyze_column_impact(&self, column: &ColumnRef) -> Result<ColumnImpactAnalysis>;

    /// Find columns by transformation type
    async fn find_columns_by_transformation(
        &self,
        transformation_type: &TransformationType,
    ) -> Result<Vec<ColumnRef>>;

    /// Get all columns derived from a source column
    async fn get_derived_columns(&self, source: &ColumnRef) -> Result<Vec<ColumnRef>>;

    /// Search column lineage by pattern
    ///
    /// Supports wildcards in datasource, table, or column names.
    async fn search_column_lineage(&self, pattern: &str) -> Result<Vec<ColumnLineageEvent>>;
}

// =============================================================================
// Helper Functions
// =============================================================================

impl ColumnLineageEvent {
    /// Create a new column lineage event
    pub fn new(
        source_columns: Vec<ColumnRef>,
        target_column: ColumnRef,
        transformation_logic: String,
        transformation_type: TransformationType,
        job_id: String,
        tenant_id: String,
        created_by: String,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            source_columns,
            target_column,
            transformation_logic,
            transformation_type,
            job_id,
            workflow_id: None,
            tenant_id,
            created_at: Utc::now(),
            confidence: None,
            created_by,
            metadata: None,
        }
    }

    /// Set confidence score for auto-detected lineage
    pub fn with_confidence(mut self, confidence: f64) -> Self {
        self.confidence = Some(confidence);
        self
    }

    /// Set workflow ID
    pub fn with_workflow(mut self, workflow_id: String) -> Self {
        self.workflow_id = Some(workflow_id);
        self
    }

    /// Add metadata
    pub fn with_metadata(mut self, metadata: serde_json::Value) -> Self {
        self.metadata = Some(metadata);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_column_ref_fully_qualified_name() {
        let col = ColumnRef::new("postgres-prod", "customers", "email", "VARCHAR(255)");
        assert_eq!(col.fully_qualified_name(), "postgres-prod.customers.email");

        let col_with_schema = col.clone().with_schema("public");
        assert_eq!(
            col_with_schema.fully_qualified_name(),
            "postgres-prod.public.customers.email"
        );
    }

    #[test]
    fn test_column_lineage_event_creation() {
        let source = ColumnRef::new("db1", "table1", "col1", "INT");
        let target = ColumnRef::new("db2", "table2", "col2", "INT");

        let event = ColumnLineageEvent::new(
            vec![source],
            target,
            "col2 = col1 * 2".to_string(),
            TransformationType::MathOperation {
                operation: "multiply".to_string(),
            },
            "job-123".to_string(),
            "tenant-1".to_string(),
            "system".to_string(),
        );

        assert!(!event.id.is_empty());
        assert_eq!(event.job_id, "job-123");
        assert_eq!(event.tenant_id, "tenant-1");
        assert!(event.confidence.is_none());
    }

    #[test]
    fn test_column_lineage_event_with_confidence() {
        let source = ColumnRef::new("db1", "table1", "col1", "INT");
        let target = ColumnRef::new("db2", "table2", "col2", "INT");

        let event = ColumnLineageEvent::new(
            vec![source],
            target,
            "col2 = col1".to_string(),
            TransformationType::DirectCopy,
            "job-123".to_string(),
            "tenant-1".to_string(),
            "auto-detector".to_string(),
        )
        .with_confidence(0.95);

        assert_eq!(event.confidence, Some(0.95));
    }
}
#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_columnref_serde() {
        let col = ColumnRef::new("db1", "customers", "email", "VARCHAR(255)");
        eprintln!("ColumnRef: {:?}", col);
        eprintln!("schema is None: {}", col.schema.is_none());

        match bincode::serialize(&col) {
            Ok(bytes) => {
                eprintln!("Serialized to {} bytes", bytes.len());
                eprintln!("All bytes: {:?}", bytes);

                match bincode::deserialize::<ColumnRef>(&bytes) {
                    Ok(deser) => {
                        eprintln!("✓ Deserialized successfully!");
                        eprintln!("  datasource_id: {}", deser.datasource_id);
                        eprintln!("  schema: {:?}", deser.schema);
                        eprintln!("  table_name: {}", deser.table_name);
                        eprintln!("  column_name: {}", deser.column_name);
                    }
                    Err(e) => {
                        eprintln!("✗ Deserialization failed: {:?}", e);
                    }
                }
            }
            Err(e) => {
                eprintln!("✗ Serialization failed: {:?}", e);
            }
        }
    }
}
