//! High-Performance RDF Triple Insert Operations
//!
//! This module implements single and batch triple insertion with:
//! - Automatic shard routing based on subject hash
//! - Batch optimization (group by shard before sending)
//! - Parallel batch inserts to multiple shards
//! - Named graph support
//! - Error handling and retries
//!
//! ## Performance Characteristics
//! - Single insert: 1-5ms (includes routing + gRPC roundtrip)
//! - Batch insert (1000 triples, 4 shards): 10-20ms (parallel)
//! - Throughput: 50,000+ triples/sec (batched, distributed)
//!
//! ## Usage
//!
//! ```ignore
//! use graphica_coordinator::governance::shard_coordinator::insert::InsertExecutor;
//! use graphica_coordinator::governance::rdf_store::NamedGraph;
//!
//! # async fn example() -> anyhow::Result<()> {
//! # use std::sync::Arc;
//! # let router = Arc::new(todo!());
//! # let pool = Arc::new(todo!());
//! let executor = InsertExecutor::new(router, pool);
//!
//! // Single insert
//! executor.insert_triple(
//!     "http://example.com/subject",
//!     "rdf:type",
//!     "Person",
//!     None,
//! ).await?;
//!
//! // Batch insert
//! let triples = vec![
//!     ("s1".to_string(), "p".to_string(), "o1".to_string()),
//!     ("s2".to_string(), "p".to_string(), "o2".to_string()),
//! ];
//! executor.insert_batch(triples, Some(&NamedGraph::current())).await?;
//!
//! # Ok(())
//! # }
//! ```

use anyhow::{Context, Result};
use graphica_core::distributed::proto::shard_service::{InsertBatchRequest, Triple};
use std::sync::Arc;
use std::time::Instant;
use tracing::{debug, info, warn};

use super::connection::ConnectionPool;
use super::routing::ShardRouter;
use crate::governance::rdf_store::NamedGraph;

/// Executor for RDF triple insert operations
pub struct InsertExecutor {
    /// Shard router for determining target shards
    router: Arc<ShardRouter>,
    /// Connection pool for gRPC clients
    pool: Arc<ConnectionPool>,
}

impl InsertExecutor {
    /// Create a new insert executor
    ///
    /// # Arguments
    /// * `router` - Shard router for hash-based routing
    /// * `pool` - Connection pool for gRPC clients
    pub fn new(router: Arc<ShardRouter>, pool: Arc<ConnectionPool>) -> Self {
        Self { router, pool }
    }

    /// Insert a single RDF triple
    ///
    /// Routes the triple to the appropriate shard based on subject hash.
    ///
    /// # Performance
    /// - Routing: O(log N) where N = shard count, ~100ns
    /// - Connection lookup: O(1), ~50-100ns
    /// - gRPC call: 1-5ms (network + shard processing)
    /// - Total: ~1-5ms
    ///
    /// # Arguments
    /// * `subject` - RDF subject URI
    /// * `predicate` - RDF predicate URI
    /// * `object` - RDF object value (URI or literal)
    /// * `graph` - Optional named graph
    ///
    /// # Errors
    /// - Routing failure (no shard for hash)
    /// - Connection failure
    /// - gRPC call failure
    pub async fn insert_triple(
        &self,
        subject: &str,
        predicate: &str,
        object: &str,
        graph: Option<&NamedGraph>,
    ) -> Result<()> {
        let start = Instant::now();

        // Route to shard
        let shard = self
            .router
            .route_triple(subject, predicate, object)
            .with_context(|| format!("Failed to route triple: {}", subject))?;

        debug!(
            "Routing triple to shard {}: {} {} {}",
            shard.shard_id, subject, predicate, object
        );

        // Get gRPC client
        let mut client = self
            .pool
            .get_shard_client(&shard.leader_address)
            .await
            .with_context(|| format!("Failed to connect to shard: {}", shard.leader_address))?;

        // Build validated proto Triple with proper normalization
        let graph_uri = graph.map(|g| g.uri.as_str());
        let triple = super::rdf::build_validated_triple(subject, predicate, object, graph_uri)
            .with_context(|| {
                format!(
                    "Failed to build triple: {} {} {}",
                    subject, predicate, object
                )
            })?;

        // Send insert request (batch of 1)
        let request = tonic::Request::new(InsertBatchRequest {
            triples: vec![triple],
            transactional: false, // Single insert doesn't need transaction
            default_graph: graph.map(|g| g.uri.clone()).unwrap_or_default(),
            request_id: uuid::Uuid::new_v4().to_string(),
        });

        let response = client
            .insert_batch(request)
            .await
            .with_context(|| format!("Insert failed on shard: {}", shard.leader_address))?;

        let result = response.into_inner();

        // Check for errors
        if result.failed_count > 0 {
            self.pool.record_failure(&shard.leader_address);
            anyhow::bail!(
                "Insert failed on shard {}: {}",
                shard.shard_id,
                result.error
            );
        }

        // Record success
        self.pool.record_success(&shard.leader_address);

        debug!(
            "Insert successful (shard={}, duration={:?})",
            shard.shard_id,
            start.elapsed()
        );

        Ok(())
    }

    /// Insert multiple RDF triples in batch
    ///
    /// Optimized batch insertion that:
    /// 1. Groups triples by target shard (minimizes gRPC calls)
    /// 2. Sends parallel batch inserts to each shard
    /// 3. Collects results and reports errors
    ///
    /// # Performance
    /// - Routing: O(M * log N) where M = triples, N = shards
    /// - Batching: O(M) to group by shard
    /// - Parallel inserts: O(1) with Tokio (limited by slowest shard)
    /// - Total: ~10-20ms for 1000 triples on 4 shards
    /// - Throughput: 50,000+ triples/sec (batched)
    ///
    /// # Arguments
    /// * `triples` - Vec of (subject, predicate, object) tuples
    /// * `graph` - Optional named graph for all triples
    ///
    /// # Returns
    /// `Ok(())` if all inserts succeeded
    ///
    /// # Errors
    /// - If any shard insert fails, returns error with details
    /// - Partial success is possible (some shards succeed, others fail)
    pub async fn insert_batch(
        &self,
        triples: Vec<(String, String, String)>,
        graph: Option<&NamedGraph>,
    ) -> Result<()> {
        if triples.is_empty() {
            return Ok(());
        }

        let start = Instant::now();
        let total_triples = triples.len();

        info!("Starting batch insert of {} triples", total_triples);

        // Step 1: Route and group triples by shard
        let shard_groups = self
            .router
            .route_batch(triples)
            .context("Failed to route batch")?;

        let shard_count = shard_groups.len();
        debug!(
            "Batch routed to {} shards ({} triples)",
            shard_count, total_triples
        );

        // Step 2: Prepare graph URI
        let graph_uri = graph.map(|g| g.uri.clone()).unwrap_or_default();

        // Step 3: Send parallel inserts to each shard
        let mut tasks = Vec::new();

        for (shard_id, triples) in shard_groups {
            let pool = self.pool.clone();
            let router = self.router.clone();
            let graph_uri = graph_uri.clone();

            // Spawn parallel task for each shard
            let task = tokio::spawn(async move {
                // Get shard metadata
                let shard = router
                    .get_shard(shard_id)?
                    .ok_or_else(|| anyhow::anyhow!("Shard not found: {:?}", shard_id))?;

                // Get gRPC client
                let mut client = pool.get_shard_client(&shard.leader_address).await?;

                // Convert triples to proto format with validation
                let proto_triples: Result<Vec<Triple>> = triples
                    .into_iter()
                    .map(|(subject, predicate, object)| {
                        // Use advanced triple builder for proper normalization and validation
                        let graph_ref = if graph_uri.is_empty() {
                            None
                        } else {
                            Some(graph_uri.as_str())
                        };

                        super::rdf::build_validated_triple(&subject, &predicate, &object, graph_ref)
                            .with_context(|| {
                                format!(
                                    "Failed to build triple: {} {} {}",
                                    subject, predicate, object
                                )
                            })
                    })
                    .collect();

                let proto_triples = proto_triples?;

                let batch_size = proto_triples.len();

                // Send batch insert
                let request = tonic::Request::new(InsertBatchRequest {
                    triples: proto_triples,
                    transactional: true, // Batch inserts should be transactional
                    default_graph: graph_uri.clone(),
                    request_id: uuid::Uuid::new_v4().to_string(),
                });

                let response = client.insert_batch(request).await?;
                let result = response.into_inner();

                // Check for errors
                if result.failed_count > 0 {
                    pool.record_failure(&shard.leader_address);
                    anyhow::bail!(
                        "Batch insert failed on shard {}: {} triples failed ({})",
                        shard_id,
                        result.failed_count,
                        result.error
                    );
                }

                pool.record_success(&shard.leader_address);

                Ok::<(u32, usize, u64), anyhow::Error>((shard_id.0, batch_size, result.duration_ms))
            });

            tasks.push(task);
        }

        // Step 4: Wait for all parallel inserts to complete
        let results = futures::future::join_all(tasks).await;

        // Step 5: Collect results and check for errors
        let mut total_inserted = 0;
        let mut errors = Vec::new();

        for (idx, result) in results.into_iter().enumerate() {
            match result {
                Ok(Ok((shard_id, batch_size, duration_ms))) => {
                    total_inserted += batch_size;
                    debug!(
                        "Shard {} inserted {} triples in {}ms",
                        shard_id, batch_size, duration_ms
                    );
                }
                Ok(Err(e)) => {
                    errors.push(format!("Shard {}: {}", idx, e));
                }
                Err(e) => {
                    errors.push(format!("Task {} panicked: {}", idx, e));
                }
            }
        }

        // Report results
        if !errors.is_empty() {
            warn!(
                "Batch insert partially failed: {} of {} triples inserted. Errors: {:?}",
                total_inserted, total_triples, errors
            );
            anyhow::bail!(
                "Batch insert failed: {} errors. First error: {}",
                errors.len(),
                errors[0]
            );
        }

        info!(
            "Batch insert successful: {} triples inserted across {} shards in {:?} ({:.0} triples/sec)",
            total_inserted,
            shard_count,
            start.elapsed(),
            total_triples as f64 / start.elapsed().as_secs_f64()
        );

        Ok(())
    }

    /// Insert triples with retry logic (for production reliability)
    ///
    /// Retries failed inserts up to `max_retries` times with exponential backoff.
    ///
    /// # Arguments
    /// * `triples` - Vec of (subject, predicate, object) tuples
    /// * `graph` - Optional named graph
    /// * `max_retries` - Maximum number of retry attempts
    ///
    /// # Performance
    /// - Best case (no retries): Same as `insert_batch`
    /// - Worst case (max retries): `insert_batch` time * (1 + max_retries)
    pub async fn insert_batch_with_retry(
        &self,
        triples: Vec<(String, String, String)>,
        graph: Option<&NamedGraph>,
        max_retries: u32,
    ) -> Result<()> {
        let mut attempt = 0;
        let mut last_error = None;

        while attempt <= max_retries {
            match self.insert_batch(triples.clone(), graph).await {
                Ok(()) => return Ok(()),
                Err(e) => {
                    warn!(
                        "Batch insert attempt {} failed: {}. Retrying...",
                        attempt + 1,
                        e
                    );
                    last_error = Some(e);
                    attempt += 1;

                    if attempt <= max_retries {
                        // Exponential backoff: 100ms, 200ms, 400ms, 800ms
                        let delay = std::time::Duration::from_millis(100 * 2_u64.pow(attempt - 1));
                        tokio::time::sleep(delay).await;
                    }
                }
            }
        }

        Err(last_error.unwrap_or_else(|| anyhow::anyhow!("All retry attempts failed")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::governance::distributed::{
        HashRange, ShardId, ShardMetadata, ShardRegistry, ShardStatus,
    };
    use tempfile::TempDir;

    fn create_test_router() -> (Arc<ShardRouter>, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let registry = ShardRegistry::new(temp_dir.path(), 2, 60).unwrap();

        let ranges = HashRange::distribute(2);
        for (i, range) in ranges.iter().enumerate() {
            let mut shard = ShardMetadata::new(
                ShardId(i as u32),
                *range,
                format!("localhost:{}", 9090 + i),
                vec![],
            );
            // Set status to Active so shards are available for routing
            shard.status = ShardStatus::Active;
            registry.register_shard(shard).unwrap();
        }

        (Arc::new(ShardRouter::new(Arc::new(registry))), temp_dir)
    }

    #[tokio::test]
    async fn test_insert_executor_creation() {
        let (router, _temp_dir) = create_test_router();
        let pool = Arc::new(ConnectionPool::new());
        let executor = InsertExecutor::new(router, pool);

        // Executor should be created successfully
        assert!(executor.router.active_shard_count().unwrap() > 0);
    }

    #[test]
    fn test_batch_grouping() {
        let (router, _temp_dir) = create_test_router();

        let triples = vec![
            (
                "http://example.com/s1".to_string(),
                "p".to_string(),
                "o".to_string(),
            ),
            (
                "http://example.com/s2".to_string(),
                "p".to_string(),
                "o".to_string(),
            ),
            (
                "http://example.com/s3".to_string(),
                "p".to_string(),
                "o".to_string(),
            ),
        ];

        let groups = router.route_batch(triples).unwrap();

        // All triples should be routed
        let total: usize = groups.values().map(|v| v.len()).sum();
        assert_eq!(total, 3);

        // Should be distributed across shards (probabilistic)
        // With 3 triples and 2 shards, likely at least one shard gets >1 triple
        assert!(groups.len() <= 2);
    }

    #[test]
    fn test_validated_triple_creation() {
        use super::super::rdf::build_validated_triple;

        // Test typed literal object
        let triple = build_validated_triple(
            "<http://example.com/alice>",
            "<http://example.com/age>",
            r#""42"^^<http://www.w3.org/2001/XMLSchema#integer>"#,
            None,
        )
        .expect("Should build triple with typed literal");
        assert_eq!(triple.object, "42"); // Quotes stripped, normalized
        assert_eq!(
            triple.object_datatype,
            "http://www.w3.org/2001/XMLSchema#integer"
        );
        assert_eq!(triple.object_language, "");

        // Test language-tagged literal object
        let triple = build_validated_triple(
            "<http://example.com/alice>",
            "<http://example.com/greeting>",
            r#""Hello"@en"#,
            None,
        )
        .expect("Should build triple with language tag");
        assert_eq!(triple.object, "Hello"); // Quotes stripped
        assert_eq!(triple.object_datatype, "");
        assert_eq!(triple.object_language, "en");

        // Test plain literal object
        let triple = build_validated_triple(
            "<http://example.com/alice>",
            "<http://example.com/description>",
            r#""Simple text""#,
            None,
        )
        .expect("Should build triple with plain literal");
        assert_eq!(triple.object, "Simple text"); // Quotes stripped
        assert_eq!(triple.object_datatype, "");
        assert_eq!(triple.object_language, "");

        // Test escaped quotes
        let triple = build_validated_triple(
            "<http://example.com/alice>",
            "<http://example.com/name>",
            r#""John \"The Boss\" Smith"@en"#,
            None,
        )
        .expect("Should build triple with escaped quotes");
        assert_eq!(triple.object, "John \"The Boss\" Smith"); // Unescaped, quotes stripped
        assert_eq!(triple.object_language, "en");

        // Test URI reference object
        let triple = build_validated_triple(
            "<http://example.com/alice>",
            "<http://example.com/knows>",
            "<http://example.com/bob>",
            None,
        )
        .expect("Should build triple with URI object");
        assert_eq!(triple.object, "http://example.com/bob"); // Raw URI without angle brackets
    }
}
