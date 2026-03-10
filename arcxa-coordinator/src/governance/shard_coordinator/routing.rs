//! Hash-Based Shard Routing
//!
//! This module implements high-performance routing of RDF triples to shards
//! using consistent hashing on the triple's subject.
//!
//! ## Performance Characteristics
//! - O(1) hash calculation using DefaultHasher
//! - O(log N) shard lookup using binary search
//! - Zero-copy hash computation
//! - Lock-free routing (read-only access to shard registry)
//!
//! ## Usage
//!
//! ```ignore
//! use graphica_coordinator::governance::shard_coordinator::routing::ShardRouter;
//! use graphica_coordinator::governance::distributed::ShardRegistry;
//!
//! # fn example() -> anyhow::Result<()> {
//! let registry = ShardRegistry::new("./data/shards", 4, 60)?;
//! let router = ShardRouter::new(registry);
//!
//! // Route single triple
//! let shard = router.route_triple("http://example.com/subject", "rdf:type", "Person")?;
//! println!("Route to shard: {}", shard.shard_id);
//!
//! // Determine if query requires scatter-gather
//! let needs_scatter = router.requires_scatter_gather("SELECT * WHERE { ?s ?p ?o }");
//! # Ok(())
//! # }
//! ```

use anyhow::{Context, Result};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use crate::governance::distributed::{ShardId, ShardMetadata, ShardRegistry};

/// High-performance shard router using consistent hashing
#[derive(Clone)]
pub struct ShardRouter {
    registry: Arc<ShardRegistry>,
}

impl ShardRouter {
    /// Create a new shard router
    ///
    /// # Arguments
    /// * `registry` - Shard registry containing topology information
    ///
    /// # Performance
    /// - Construction: O(1)
    /// - Memory: Single Arc pointer (8 bytes)
    pub fn new(registry: Arc<ShardRegistry>) -> Self {
        Self { registry }
    }

    /// Calculate hash for a triple's subject
    ///
    /// Uses DefaultHasher for high performance (xxHash or similar on most platforms)
    ///
    /// # Performance
    /// - Time: O(n) where n = subject length
    /// - Allocation: Zero-copy, stack-only
    /// - Throughput: ~1-2ns per byte on modern CPUs
    ///
    /// # Arguments
    /// * `subject` - RDF subject URI
    #[inline]
    pub fn calculate_hash(subject: &str) -> u64 {
        let mut hasher = DefaultHasher::new();
        subject.hash(&mut hasher);
        hasher.finish()
    }

    /// Calculate hash for a triple (subject + predicate + object)
    ///
    /// Alternative hashing strategy that considers all triple components.
    /// Currently unused but available for future rebalancing strategies.
    ///
    /// # Performance
    /// - Time: O(n) where n = total triple string length
    #[inline]
    #[allow(dead_code)]
    pub fn calculate_triple_hash(subject: &str, predicate: &str, object: &str) -> u64 {
        let mut hasher = DefaultHasher::new();
        subject.hash(&mut hasher);
        predicate.hash(&mut hasher);
        object.hash(&mut hasher);
        hasher.finish()
    }

    /// Route a triple to the appropriate shard
    ///
    /// # Performance
    /// - Hash calculation: O(n) where n = subject length
    /// - Shard lookup: O(log N) where N = number of shards
    /// - Total: O(n + log N), typically < 100ns for small subjects
    ///
    /// # Arguments
    /// * `subject` - RDF subject URI
    /// * `predicate` - RDF predicate URI (unused in current routing, reserved for future)
    /// * `object` - RDF object value (unused in current routing, reserved for future)
    ///
    /// # Returns
    /// ShardMetadata for the target shard
    ///
    /// # Errors
    /// - If no shard is configured for the calculated hash range
    pub fn route_triple(
        &self,
        subject: &str,
        _predicate: &str,
        _object: &str,
    ) -> Result<Arc<ShardMetadata>> {
        let hash = Self::calculate_hash(subject);
        let shard = self
            .registry
            .find_shard_for_hash(hash)?
            .ok_or_else(|| anyhow::anyhow!("No shard found for subject hash: {}", hash))?;
        Ok(Arc::new(shard))
    }

    /// Route multiple triples and group by shard
    ///
    /// Optimized batch routing that minimizes hash lookups.
    ///
    /// # Performance
    /// - Per-triple: O(n + log N)
    /// - Batch of M triples: O(M * (n + log N))
    /// - Pre-allocates HashMap with capacity for expected shard count
    ///
    /// # Returns
    /// HashMap mapping ShardId -> Vec<(subject, predicate, object)>
    pub fn route_batch(
        &self,
        triples: Vec<(String, String, String)>,
    ) -> Result<std::collections::HashMap<ShardId, Vec<(String, String, String)>>> {
        use std::collections::HashMap;

        // Pre-allocate with expected shard count to minimize rehashing
        let topology = self.registry.get_topology()?;
        let mut shard_groups: HashMap<ShardId, Vec<(String, String, String)>> =
            HashMap::with_capacity(topology.total_shards as usize);

        for (subject, predicate, object) in triples {
            let shard = self.route_triple(&subject, &predicate, &object)?;
            shard_groups
                .entry(shard.shard_id)
                .or_insert_with(Vec::new)
                .push((subject, predicate, object));
        }

        Ok(shard_groups)
    }

    /// Determine if a SPARQL query requires scatter-gather across all shards
    ///
    /// Analyzes SPARQL query to detect if it:
    /// - Uses unbound subjects (?s)
    /// - Uses aggregations (COUNT, SUM, etc.)
    /// - Uses UNION, GRAPH, or other multi-source constructs
    ///
    /// # Performance
    /// - Time: O(n) where n = query length
    /// - Simple regex/string matching, no full SPARQL parsing
    ///
    /// # Returns
    /// `true` if query must be sent to all shards, `false` if single-shard
    pub fn requires_scatter_gather(&self, sparql: &str) -> bool {
        let sparql_upper = sparql.to_uppercase();

        // Check for unbound subjects (most common scatter-gather case)
        if sparql_upper.contains("?S ") || sparql_upper.contains("?SUBJECT") {
            return true;
        }

        // Check for aggregations
        if sparql_upper.contains("COUNT(")
            || sparql_upper.contains("SUM(")
            || sparql_upper.contains("AVG(")
            || sparql_upper.contains("MIN(")
            || sparql_upper.contains("MAX(")
        {
            return true;
        }

        // Check for UNION (requires querying multiple shards)
        if sparql_upper.contains(" UNION ") {
            return true;
        }

        // Check for GRAPH queries (may span shards)
        if sparql_upper.contains("GRAPH ") {
            return true;
        }

        // Default: assume scatter-gather needed for safety
        // Future optimization: parse bound subjects and route to specific shard
        true
    }

    /// Extract bound subject from SPARQL query for single-shard routing
    ///
    /// Attempts to extract a concrete subject URI from a SPARQL query.
    /// If successful, the query can be routed to a single shard.
    ///
    /// # Performance
    /// - Time: O(n) where n = query length
    /// - Uses simple pattern matching, not full SPARQL parsing
    ///
    /// # Returns
    /// `Some(subject_uri)` if a single bound subject is found, `None` otherwise
    ///
    /// # Example
    /// ```ignore
    /// # use graphica_coordinator::governance::shard_coordinator::routing::ShardRouter;
    /// # use graphica_coordinator::governance::distributed::ShardRegistry;
    /// # fn example() -> anyhow::Result<()> {
    /// # let registry = ShardRegistry::new("./data/shards", 4, 60)?;
    /// # let router = ShardRouter::new(registry.into());
    /// let query = "SELECT * WHERE { <http://example.com/person/123> ?p ?o }";
    /// let subject = router.extract_bound_subject(query);
    /// assert_eq!(subject, Some("http://example.com/person/123".to_string()));
    /// # Ok(())
    /// # }
    /// ```
    pub fn extract_bound_subject(&self, sparql: &str) -> Option<String> {
        // Simple regex-like extraction: find <URI> at start of WHERE clause
        // Format: WHERE { <subject> ...
        if let Some(where_idx) = sparql.to_uppercase().find("WHERE") {
            let after_where = &sparql[where_idx..];
            if let Some(open_brace) = after_where.find('{') {
                let in_pattern = &after_where[open_brace + 1..];
                // Find first <...> URI
                if let Some(open_uri) = in_pattern.find('<') {
                    if let Some(close_uri) = in_pattern[open_uri + 1..].find('>') {
                        let subject = &in_pattern[open_uri + 1..open_uri + 1 + close_uri];
                        return Some(subject.trim().to_string());
                    }
                }
            }
        }

        None
    }

    /// Get all active shards for scatter-gather operations
    ///
    /// Returns only shards in Active status, excluding Draining/Down/Provisioning.
    ///
    /// # Performance
    /// - Time: O(N) where N = total shards
    /// - Allocation: Vec of Arc pointers (8 bytes per shard)
    pub fn get_active_shards(&self) -> Result<Vec<Arc<ShardMetadata>>> {
        let shards = self.registry.get_active_shards()?;
        Ok(shards.into_iter().map(Arc::new).collect())
    }

    /// Get the number of active shards (for monitoring/metrics)
    pub fn active_shard_count(&self) -> Result<usize> {
        Ok(self.get_active_shards()?.len())
    }

    /// Get shard metadata by ID
    pub fn get_shard(&self, shard_id: ShardId) -> Result<Option<ShardMetadata>> {
        self.registry.get_shard(shard_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::governance::distributed::{HashRange, ShardStatus};

    fn create_test_registry() -> Arc<ShardRegistry> {
        use uuid::Uuid;
        let temp_dir = std::env::temp_dir();
        let unique_id = Uuid::new_v4();
        let db_path = temp_dir.join(format!("test_shard_registry_{}", unique_id));

        let registry = ShardRegistry::new(db_path, 4, 60).unwrap();

        // Create 4 shards with equal hash ranges
        let ranges = HashRange::distribute(4);
        for (i, range) in ranges.iter().enumerate() {
            let shard_id = ShardId(i as u32);
            let shard = ShardMetadata::new(shard_id, *range, format!("shard-{}:9090", i), vec![]);
            registry.register_shard(shard).unwrap();

            // Mark shard as Active (shards start in Provisioning status)
            registry
                .update_shard_status(shard_id, ShardStatus::Active)
                .unwrap();
        }

        Arc::new(registry)
    }

    #[test]
    fn test_hash_calculation() {
        let hash1 = ShardRouter::calculate_hash("http://example.com/subject1");
        let hash2 = ShardRouter::calculate_hash("http://example.com/subject1");
        let hash3 = ShardRouter::calculate_hash("http://example.com/subject2");

        // Same subject should produce same hash
        assert_eq!(hash1, hash2);

        // Different subjects should produce different hashes (with high probability)
        assert_ne!(hash1, hash3);
    }

    #[test]
    fn test_route_triple() {
        let registry = create_test_registry();
        let router = ShardRouter::new(registry);

        let shard = router
            .route_triple("http://example.com/subject", "rdf:type", "Person")
            .unwrap();

        // Should route to one of the 4 shards
        assert!(shard.shard_id.0 < 4);
    }

    #[test]
    fn test_route_batch() {
        let registry = create_test_registry();
        let router = ShardRouter::new(registry);

        let triples = vec![
            (
                "http://example.com/s1".to_string(),
                "rdf:type".to_string(),
                "Person".to_string(),
            ),
            (
                "http://example.com/s2".to_string(),
                "rdf:type".to_string(),
                "Person".to_string(),
            ),
            (
                "http://example.com/s3".to_string(),
                "rdf:type".to_string(),
                "Person".to_string(),
            ),
        ];

        let shard_groups = router.route_batch(triples).unwrap();

        // All triples should be routed
        let total: usize = shard_groups.values().map(|v| v.len()).sum();
        assert_eq!(total, 3);
    }

    #[test]
    fn test_requires_scatter_gather() {
        let registry = create_test_registry();
        let router = ShardRouter::new(registry);

        // Queries with unbound subjects require scatter-gather
        assert!(router.requires_scatter_gather("SELECT * WHERE { ?s ?p ?o }"));

        // Aggregations require scatter-gather
        assert!(router.requires_scatter_gather("SELECT (COUNT(*) as ?c) WHERE { ?s ?p ?o }"));

        // UNION requires scatter-gather
        assert!(
            router.requires_scatter_gather("SELECT * WHERE { { ?s ?p ?o } UNION { ?s ?p ?o } }")
        );

        // Bound subjects *might* allow single-shard routing (but default to scatter for safety)
        // Future optimization: parse and route to single shard
        let bound_query = "SELECT * WHERE { <http://example.com/subject> ?p ?o }";
        assert!(router.requires_scatter_gather(bound_query)); // Conservative for now
    }

    #[test]
    fn test_extract_bound_subject() {
        let registry = create_test_registry();
        let router = ShardRouter::new(registry);

        let query = "SELECT * WHERE { <http://example.com/person/123> ?p ?o }";
        let subject = router.extract_bound_subject(query);
        assert_eq!(subject, Some("http://example.com/person/123".to_string()));

        // Query with unbound subject
        let query2 = "SELECT * WHERE { ?s ?p ?o }";
        let subject2 = router.extract_bound_subject(query2);
        assert_eq!(subject2, None);
    }

    #[test]
    fn test_get_active_shards() {
        let registry = create_test_registry();
        let router = ShardRouter::new(registry);

        let active = router.get_active_shards().unwrap();
        assert_eq!(active.len(), 4); // All 4 shards are active in test setup
    }

    #[test]
    fn test_consistent_routing() {
        let registry = create_test_registry();
        let router = ShardRouter::new(registry);

        // Same subject should always route to same shard
        let shard1 = router
            .route_triple("http://example.com/consistent", "p", "o")
            .unwrap();
        let shard2 = router
            .route_triple("http://example.com/consistent", "p", "o")
            .unwrap();

        assert_eq!(shard1.shard_id, shard2.shard_id);
    }
}
