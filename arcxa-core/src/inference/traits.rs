// graphica-core/src/inference/traits.rs
//! Trait hierarchy for progressive schema inference.
//!
//! Each tier is a separate trait, allowing connectors to implement
//! only the capabilities their database supports.

use crate::inference::types::*;
use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};

/// Tier 0: Basic structure inference (REQUIRED for all connectors)
#[async_trait]
pub trait BasicInference: Send + Sync {
    /// List all schemas in the data source
    async fn list_schemas(&self) -> Result<Vec<String>>;

    /// Get basic table structure for a schema
    async fn infer_basic_structure(&self, schema: &str) -> Result<Vec<TableMetadata>>;

    /// Get basic column metadata
    async fn infer_columns(&self, schema: &str, table: &str) -> Result<Vec<ColumnMetadata>>;

    /// Estimate row count (fast approximation)
    async fn estimate_row_count(&self, schema: &str, table: &str) -> Result<u64>;
}

/// Tier 1: Relationship inference (foreign keys, indexes, constraints)
#[async_trait]
pub trait RelationshipInference: BasicInference {
    /// Discover foreign key relationships
    async fn infer_foreign_keys(
        &self,
        schema: &str,
        table: &str,
    ) -> Result<Vec<ForeignKeyMetadata>>;

    /// Discover reverse foreign keys (tables that reference this one)
    async fn infer_reverse_foreign_keys(
        &self,
        schema: &str,
        table: &str,
    ) -> Result<Vec<ForeignKeyMetadata>>;

    /// Discover indexes
    async fn infer_indexes(&self, schema: &str, table: &str) -> Result<Vec<IndexMetadata>>;

    /// Discover constraints
    async fn infer_constraints(&self, schema: &str, table: &str)
        -> Result<Vec<ConstraintMetadata>>;

    /// Discover view dependencies
    async fn infer_view_dependencies(&self, schema: &str, view: &str) -> Result<Vec<String>>;
}

/// Tier 2: Statistical inference (column stats, distributions, partitioning)
#[async_trait]
pub trait StatisticalInference: RelationshipInference {
    /// Get accurate row count (may be slow)
    async fn get_exact_row_count(&self, schema: &str, table: &str) -> Result<u64>;

    /// Get table-level statistics
    async fn infer_table_statistics(&self, schema: &str, table: &str) -> Result<TableStatistics>;

    /// Get column statistics
    async fn infer_column_statistics(
        &self,
        schema: &str,
        table: &str,
        column: &str,
    ) -> Result<ColumnStatistics>;

    /// Get histogram for a column
    async fn infer_histogram(
        &self,
        schema: &str,
        table: &str,
        column: &str,
    ) -> Result<Option<Histogram>>;

    /// Get partitioning metadata
    async fn infer_partitioning(
        &self,
        schema: &str,
        table: &str,
    ) -> Result<Option<PartitioningMetadata>>;

    /// Get storage metrics
    async fn infer_storage_metrics(
        &self,
        schema: &str,
        table: &str,
    ) -> Result<(u64, u64, Option<f64>)>; // (data_size, index_size, compression_ratio)
}

/// Tier 3: Governance inference (PII detection, classification, access patterns)
#[async_trait]
pub trait GovernanceInference: StatisticalInference {
    /// Detect PII/PHI in columns
    async fn detect_pii(&self, schema: &str, table: &str) -> Result<Vec<(String, PiiDetection)>>;

    /// Classify data sensitivity
    async fn classify_data(&self, schema: &str, table: &str) -> Result<DataClassification>;

    /// Get access patterns
    async fn infer_access_patterns(&self, schema: &str, table: &str) -> Result<AccessPatterns>;

    /// Calculate data quality metrics
    async fn calculate_quality_metrics(
        &self,
        schema: &str,
        table: &str,
    ) -> Result<DataQualityMetrics>;

    /// Get freshness information
    async fn get_freshness(&self, schema: &str, table: &str) -> Result<Option<DateTime<Utc>>>;

    /// Map to business glossary (extensible)
    async fn map_to_glossary(&self, schema: &str, table: &str) -> Result<Vec<String>> {
        // Default: no mapping
        Ok(vec![])
    }
}

/// Tier 4: Deep profiling (value-level analysis)
#[async_trait]
pub trait DeepProfiling: GovernanceInference {
    /// Profile column values (with sampling)
    async fn profile_column_values(
        &self,
        schema: &str,
        table: &str,
        column: &str,
        sample_size: Option<u64>,
    ) -> Result<ValueProfile>;

    /// Validate referential integrity
    async fn validate_referential_integrity(
        &self,
        schema: &str,
        table: &str,
    ) -> Result<Vec<IntegrityViolation>>;

    /// Discover patterns in data
    async fn discover_patterns(
        &self,
        schema: &str,
        table: &str,
        column: &str,
    ) -> Result<Vec<DataPattern>>;
}

#[derive(Debug, Clone)]
pub struct IntegrityViolation {
    pub foreign_key: String,
    pub orphan_count: u64,
    pub sample_values: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct DataPattern {
    pub pattern: String,
    pub regex: String,
    pub match_percentage: f64,
    pub examples: Vec<String>,
}

/// Unified inference interface
#[async_trait]
pub trait SchemaInferrer: DeepProfiling {
    /// Get database-specific capabilities
    fn supported_tiers(&self) -> Vec<InferenceTier>;

    /// Check if a specific tier is supported
    fn supports_tier(&self, tier: InferenceTier) -> bool {
        self.supported_tiers().contains(&tier)
    }

    /// Infer schema metadata up to specified tier
    async fn infer_schema(&self, schema: &str, tier: InferenceTier) -> Result<Vec<TableMetadata>> {
        // Default implementation: build up progressively
        let mut tables = self.infer_basic_structure(schema).await?;

        if tier >= InferenceTier::Relationships && self.supports_tier(InferenceTier::Relationships)
        {
            for table in &mut tables {
                let fks = self.infer_foreign_keys(schema, &table.name).await?;
                let reverse_fks = self.infer_reverse_foreign_keys(schema, &table.name).await?;
                table.relationships = Some(RelationshipMetadata {
                    foreign_keys: fks,
                    referenced_by: reverse_fks,
                });

                table.indexes = self.infer_indexes(schema, &table.name).await?;
                table.constraints = self.infer_constraints(schema, &table.name).await?;
            }
        }

        if tier >= InferenceTier::Statistics && self.supports_tier(InferenceTier::Statistics) {
            for table in &mut tables {
                table.statistics = self.infer_table_statistics(schema, &table.name).await.ok();
                table.partitioning = self.infer_partitioning(schema, &table.name).await?;

                for col in &mut table.columns {
                    col.statistics = self
                        .infer_column_statistics(schema, &table.name, &col.name)
                        .await
                        .ok();
                }
            }
        }

        if tier >= InferenceTier::Governance && self.supports_tier(InferenceTier::Governance) {
            for table in &mut tables {
                let pii_detections = self.detect_pii(schema, &table.name).await?;
                for (col_name, detection) in pii_detections {
                    if let Some(col) = table.columns.iter_mut().find(|c| c.name == col_name) {
                        col.pii_detected = Some(detection);
                    }
                }

                let classification = self.classify_data(schema, &table.name).await?;
                let access = self.infer_access_patterns(schema, &table.name).await?;
                let quality = self.calculate_quality_metrics(schema, &table.name).await?;

                table.governance = Some(GovernanceMetadata {
                    data_classification: classification,
                    steward: None,
                    business_glossary_terms: self.map_to_glossary(schema, &table.name).await?,
                    sensitivity_labels: vec![],
                    retention_policy: None,
                    access_patterns: access,
                    quality_metrics: quality,
                });
            }
        }

        if tier >= InferenceTier::Profiling && self.supports_tier(InferenceTier::Profiling) {
            for table in &mut tables {
                table.profiling = Some(ProfilingMetadata {
                    sample_size: 1000,
                    sampling_method: SamplingMethod::Random(0.1),
                    profiled_at: Utc::now(),
                });

                for col in &mut table.columns {
                    col.value_profile = self
                        .profile_column_values(schema, &table.name, &col.name, Some(1000))
                        .await
                        .ok();
                }
            }
        }

        Ok(tables)
    }

    /// Get complete metadata envelope
    async fn infer_complete(
        &self,
        source_id: String,
        schema: &str,
        tier: InferenceTier,
    ) -> Result<SchemaMetadata> {
        let tables = self.infer_schema(schema, tier).await?;

        Ok(SchemaMetadata {
            source_id: source_id.clone(),
            schema_name: schema.to_string(),
            inferred_at: Utc::now(),
            tier_completed: tier,
            tables,
            lineage_id: format!(
                "urn:graphica:inference:{}:{}",
                source_id,
                Utc::now().timestamp()
            ),
        })
    }
}

/// Auto-implement SchemaInferrer for any type implementing DeepProfiling
impl<T: DeepProfiling> SchemaInferrer for T {
    fn supported_tiers(&self) -> Vec<InferenceTier> {
        vec![
            InferenceTier::Basic,
            InferenceTier::Relationships,
            InferenceTier::Statistics,
            InferenceTier::Governance,
            InferenceTier::Profiling,
        ]
    }
}
