// graphica-core/src/inference/orchestrator.rs
//! Orchestrates multi-tier schema inference with caching and async job management.

use crate::inference::{rdf_converter::RdfConverter, traits::*, types::*};
use anyhow::{Context, Result};
use chrono::Utc;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Orchestrator for schema inference jobs
pub struct SchemaInferenceOrchestrator {
    cache: Arc<RwLock<InferenceCache>>,
    jobs: Arc<RwLock<HashMap<String, InferenceJob>>>,
}

impl SchemaInferenceOrchestrator {
    pub fn new() -> Self {
        Self {
            cache: Arc::new(RwLock::new(InferenceCache::new())),
            jobs: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Start an async inference job
    pub async fn start_inference_job<I: SchemaInferrer + Send + Sync + 'static>(
        &self,
        inferrer: Arc<I>,
        source_id: String,
        schemas: Vec<String>,
        tier: InferenceTier,
    ) -> Result<String> {
        let job_id = format!("inf_{}", Uuid::new_v4());

        let job = InferenceJob {
            job_id: job_id.clone(),
            source_id: source_id.clone(),
            schemas: schemas.clone(),
            tier,
            status: JobStatus::Pending,
            started_at: Utc::now(),
            completed_at: None,
            error: None,
            result_uri: None,
        };

        // Store job
        {
            let mut jobs = self.jobs.write().await;
            jobs.insert(job_id.clone(), job);
        }

        // Spawn async task
        let jobs_clone = self.jobs.clone();
        let cache_clone = self.cache.clone();
        let job_id_clone = job_id.clone();

        tokio::spawn(async move {
            Self::run_inference_job(
                jobs_clone,
                cache_clone,
                inferrer,
                job_id_clone,
                source_id,
                schemas,
                tier,
            )
            .await
        });

        Ok(job_id)
    }

    /// Internal job runner
    async fn run_inference_job<I: SchemaInferrer + Send + Sync>(
        jobs: Arc<RwLock<HashMap<String, InferenceJob>>>,
        cache: Arc<RwLock<InferenceCache>>,
        inferrer: Arc<I>,
        job_id: String,
        source_id: String,
        schemas: Vec<String>,
        tier: InferenceTier,
    ) {
        // Update status to running
        {
            let mut jobs_map = jobs.write().await;
            if let Some(job) = jobs_map.get_mut(&job_id) {
                job.status = JobStatus::Running;
            }
        }

        // Run inference
        let result = Self::execute_inference(&*inferrer, &source_id, &schemas, tier, &cache).await;

        // Update job with result
        let mut jobs_map = jobs.write().await;
        if let Some(job) = jobs_map.get_mut(&job_id) {
            job.completed_at = Some(Utc::now());

            match result {
                Ok(result_uri) => {
                    job.status = JobStatus::Completed;
                    job.result_uri = Some(result_uri);
                }
                Err(e) => {
                    job.status = JobStatus::Failed;
                    job.error = Some(e.to_string());
                }
            }
        }
    }

    /// Execute inference with caching
    async fn execute_inference<I: SchemaInferrer + Send + Sync>(
        inferrer: &I,
        source_id: &str,
        schemas: &[String],
        tier: InferenceTier,
        cache: &Arc<RwLock<InferenceCache>>,
    ) -> Result<String> {
        let mut all_metadata = Vec::new();

        let schemas_to_infer = if schemas.is_empty() {
            inferrer.list_schemas().await?
        } else {
            schemas.to_vec()
        };

        for schema in &schemas_to_infer {
            // Check cache first
            let cache_key = format!("{}:{}:{:?}", source_id, schema, tier);

            let cached = {
                let cache_read = cache.read().await;
                cache_read.get(&cache_key)
            };

            let metadata = if let Some(cached_meta) = cached {
                // Use cached result
                cached_meta
            } else {
                // Run inference
                let meta = inferrer
                    .infer_complete(source_id.to_string(), schema, tier)
                    .await
                    .context(format!("Failed to infer schema: {}", schema))?;

                // Cache result
                {
                    let mut cache_write = cache.write().await;
                    cache_write.put(cache_key, meta.clone());
                }

                meta
            };

            all_metadata.push(metadata);
        }

        // Convert to RDF
        let converter = RdfConverter::new(source_id);
        let mut all_triples = Vec::new();

        for meta in &all_metadata {
            let triples = converter.convert_schema_metadata(meta)?;
            all_triples.extend(triples);
        }

        // Generate result URI (would be inserted into RDF store in practice)
        let result_uri = format!(
            "urn:graphica:inference:{}:{}",
            source_id,
            Utc::now().timestamp()
        );

        Ok(result_uri)
    }

    /// Get job status
    pub async fn get_job_status(&self, job_id: &str) -> Option<InferenceJob> {
        let jobs = self.jobs.read().await;
        jobs.get(job_id).cloned()
    }

    /// Cancel a running job
    pub async fn cancel_job(&self, job_id: &str) -> Result<()> {
        let mut jobs = self.jobs.write().await;
        if let Some(job) = jobs.get_mut(job_id) {
            if matches!(job.status, JobStatus::Pending | JobStatus::Running) {
                job.status = JobStatus::Cancelled;
                job.completed_at = Some(Utc::now());
            }
        }
        Ok(())
    }

    /// List all jobs for a source
    pub async fn list_jobs(&self, source_id: &str) -> Vec<InferenceJob> {
        let jobs = self.jobs.read().await;
        jobs.values()
            .filter(|j| j.source_id == source_id)
            .cloned()
            .collect()
    }

    /// Synchronous inference (convenience method)
    pub async fn infer_sync<I: SchemaInferrer + Send + Sync>(
        &self,
        inferrer: &I,
        source_id: String,
        schema: &str,
        tier: InferenceTier,
    ) -> Result<SchemaMetadata> {
        let cache_key = format!("{}:{}:{:?}", source_id, schema, tier);

        // Check cache
        {
            let cache = self.cache.read().await;
            if let Some(cached) = cache.get(&cache_key) {
                return Ok(cached);
            }
        }

        // Run inference
        let metadata = inferrer
            .infer_complete(source_id.clone(), schema, tier)
            .await?;

        // Cache result
        {
            let mut cache = self.cache.write().await;
            cache.put(cache_key, metadata.clone());
        }

        Ok(metadata)
    }

    /// Invalidate cache for a source
    pub async fn invalidate_cache(&self, source_id: &str) {
        let mut cache = self.cache.write().await;
        cache.invalidate_prefix(source_id);
    }
}

impl Default for SchemaInferenceOrchestrator {
    fn default() -> Self {
        Self::new()
    }
}

/// Cache for inference results
struct InferenceCache {
    entries: HashMap<String, CachedMetadata>,
    max_size: usize,
}

struct CachedMetadata {
    metadata: SchemaMetadata,
    cached_at: chrono::DateTime<Utc>,
    hits: u64,
}

impl InferenceCache {
    fn new() -> Self {
        Self {
            entries: HashMap::new(),
            max_size: 1000,
        }
    }

    fn get(&self, key: &str) -> Option<SchemaMetadata> {
        if let Some(entry) = self.entries.get(key) {
            // Check if cache is still valid (1 hour TTL)
            let age = Utc::now() - entry.cached_at;
            if age.num_hours() < 1 {
                return Some(entry.metadata.clone());
            }
        }
        None
    }

    fn put(&mut self, key: String, metadata: SchemaMetadata) {
        // Evict if at capacity (LRU-style)
        if self.entries.len() >= self.max_size {
            self.evict_lru();
        }

        self.entries.insert(
            key,
            CachedMetadata {
                metadata,
                cached_at: Utc::now(),
                hits: 0,
            },
        );
    }

    fn evict_lru(&mut self) {
        if let Some(key_to_remove) = self
            .entries
            .iter()
            .min_by_key(|(_, v)| (v.hits, v.cached_at))
            .map(|(k, _)| k.clone())
        {
            self.entries.remove(&key_to_remove);
        }
    }

    fn invalidate_prefix(&mut self, prefix: &str) {
        self.entries.retain(|k, _| !k.starts_with(prefix));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Mock inferrer for testing
    struct MockInferrer;

    #[async_trait::async_trait]
    impl BasicInference for MockInferrer {
        async fn list_schemas(&self) -> Result<Vec<String>> {
            Ok(vec!["public".to_string()])
        }

        async fn infer_basic_structure(&self, _schema: &str) -> Result<Vec<TableMetadata>> {
            Ok(vec![])
        }

        async fn infer_columns(&self, _schema: &str, _table: &str) -> Result<Vec<ColumnMetadata>> {
            Ok(vec![])
        }

        async fn estimate_row_count(&self, _schema: &str, _table: &str) -> Result<u64> {
            Ok(0)
        }
    }

    #[async_trait::async_trait]
    impl RelationshipInference for MockInferrer {
        async fn infer_foreign_keys(
            &self,
            _schema: &str,
            _table: &str,
        ) -> Result<Vec<ForeignKeyMetadata>> {
            Ok(vec![])
        }

        async fn infer_reverse_foreign_keys(
            &self,
            _schema: &str,
            _table: &str,
        ) -> Result<Vec<ForeignKeyMetadata>> {
            Ok(vec![])
        }

        async fn infer_indexes(&self, _schema: &str, _table: &str) -> Result<Vec<IndexMetadata>> {
            Ok(vec![])
        }

        async fn infer_constraints(
            &self,
            _schema: &str,
            _table: &str,
        ) -> Result<Vec<ConstraintMetadata>> {
            Ok(vec![])
        }

        async fn infer_view_dependencies(&self, _schema: &str, _view: &str) -> Result<Vec<String>> {
            Ok(vec![])
        }
    }

    #[async_trait::async_trait]
    impl StatisticalInference for MockInferrer {
        async fn get_exact_row_count(&self, _schema: &str, _table: &str) -> Result<u64> {
            Ok(0)
        }

        async fn infer_table_statistics(
            &self,
            _schema: &str,
            _table: &str,
        ) -> Result<TableStatistics> {
            Ok(TableStatistics {
                actual_row_count: 0,
                size_bytes: 0,
                index_size_bytes: 0,
                compression_ratio: None,
                last_analyzed: None,
                last_modified: None,
                read_count_daily: None,
                write_count_daily: None,
            })
        }

        async fn infer_column_statistics(
            &self,
            _schema: &str,
            _table: &str,
            _column: &str,
        ) -> Result<ColumnStatistics> {
            Ok(ColumnStatistics {
                distinct_count: None,
                null_count: 0,
                null_percentage: 0.0,
                min_value: None,
                max_value: None,
                avg_length: None,
                histogram: None,
                most_common_values: None,
                correlation: None,
                n_distinct: None,
                avg_width: None,
                cardinality: None,
                sample_size: None,
                last_analyzed: None,
                statistics_stale: false,
            })
        }

        async fn infer_histogram(
            &self,
            _schema: &str,
            _table: &str,
            _column: &str,
        ) -> Result<Option<Histogram>> {
            Ok(None)
        }

        async fn infer_partitioning(
            &self,
            _schema: &str,
            _table: &str,
        ) -> Result<Option<PartitioningMetadata>> {
            Ok(None)
        }

        async fn infer_storage_metrics(
            &self,
            _schema: &str,
            _table: &str,
        ) -> Result<(u64, u64, Option<f64>)> {
            Ok((0, 0, None))
        }
    }

    #[async_trait::async_trait]
    impl GovernanceInference for MockInferrer {
        async fn detect_pii(
            &self,
            _schema: &str,
            _table: &str,
        ) -> Result<Vec<(String, PiiDetection)>> {
            Ok(vec![])
        }

        async fn classify_data(&self, _schema: &str, _table: &str) -> Result<DataClassification> {
            Ok(DataClassification::Internal)
        }

        async fn infer_access_patterns(
            &self,
            _schema: &str,
            _table: &str,
        ) -> Result<AccessPatterns> {
            Ok(AccessPatterns {
                read_frequency: AccessFrequency::Moderate,
                write_frequency: AccessFrequency::Moderate,
                peak_hours: vec![],
                primary_consumers: vec![],
            })
        }

        async fn calculate_quality_metrics(
            &self,
            _schema: &str,
            _table: &str,
        ) -> Result<DataQualityMetrics> {
            Ok(DataQualityMetrics {
                completeness: 100.0,
                uniqueness: 100.0,
                validity: 100.0,
                consistency: 100.0,
                timeliness: 100.0,
                accuracy_score: None,
            })
        }

        async fn get_freshness(
            &self,
            _schema: &str,
            _table: &str,
        ) -> Result<Option<chrono::DateTime<Utc>>> {
            Ok(None)
        }
    }

    #[async_trait::async_trait]
    impl DeepProfiling for MockInferrer {
        async fn profile_column_values(
            &self,
            _schema: &str,
            _table: &str,
            _column: &str,
            _sample_size: Option<u64>,
        ) -> Result<ValueProfile> {
            Ok(ValueProfile {
                top_values: vec![],
                pattern_distribution: HashMap::new(),
                length_distribution: HashMap::new(),
                format_violations: vec![],
            })
        }

        async fn validate_referential_integrity(
            &self,
            _schema: &str,
            _table: &str,
        ) -> Result<Vec<IntegrityViolation>> {
            Ok(vec![])
        }

        async fn discover_patterns(
            &self,
            _schema: &str,
            _table: &str,
            _column: &str,
        ) -> Result<Vec<DataPattern>> {
            Ok(vec![])
        }
    }

    #[tokio::test]
    async fn test_orchestrator_caching() {
        let orchestrator = SchemaInferenceOrchestrator::new();
        let inferrer = MockInferrer;

        let result1 = orchestrator
            .infer_sync(
                &inferrer,
                "test_source".to_string(),
                "public",
                InferenceTier::Basic,
            )
            .await;

        assert!(result1.is_ok());

        // Second call should hit cache
        let result2 = orchestrator
            .infer_sync(
                &inferrer,
                "test_source".to_string(),
                "public",
                InferenceTier::Basic,
            )
            .await;

        assert!(result2.is_ok());
    }

    #[tokio::test]
    async fn test_async_job() {
        let orchestrator = SchemaInferenceOrchestrator::new();
        let inferrer = Arc::new(MockInferrer);

        let job_id = orchestrator
            .start_inference_job(
                inferrer,
                "test_source".to_string(),
                vec!["public".to_string()],
                InferenceTier::Basic,
            )
            .await;

        assert!(job_id.is_ok());

        let job_id = job_id.unwrap();
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        let status = orchestrator.get_job_status(&job_id).await;
        assert!(status.is_some());
    }
}
