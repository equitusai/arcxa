//! # Ontology Registry Client
//!
//! High-level client for querying ontology terms from the ontology registry.
//!
//! ## Responsibilities
//!
//! - Query active ontologies from the registry
//! - Coordinate parsing of ontology content
//! - Filter terms by namespace
//! - Provide graceful fallback to default terms
//! - Cache parsed terms for performance
//!
//! ## Architecture
//!
//! The client acts as an orchestration layer between:
//! - The ontology registry (storage)
//! - The Turtle parser (low-level parsing)
//! - The mapping engine (consumer)

use anyhow::Result;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{debug, info, warn};

use super::defaults::get_default_terms;
use super::parser::TurtleParser;
use super::rdfxml_parser::RdfXmlParser;
use crate::mapping::types::OntologyTerm;
use graphica_core::catalog::OntologyRegistry;

/// Detected ontology format
#[derive(Debug, Clone, Copy, PartialEq)]
enum OntologyFormat {
    Turtle,
    RdfXml,
    Unknown,
}

/// Cached ontology terms with metadata
#[derive(Debug, Clone)]
struct CachedTerms {
    /// Parsed ontology terms
    terms: Vec<OntologyTerm>,

    /// When the cache was created
    cached_at: Instant,

    /// Number of ontologies that were parsed
    ontology_count: usize,
}

/// Client for querying ontology terms from the registry
#[derive(Clone)]
pub struct RegistryClient {
    /// Optional ontology registry (None = use defaults only)
    registry: Option<Arc<parking_lot::RwLock<OntologyRegistry>>>,

    /// Cached parsed terms (invalidated on ontology updates)
    term_cache: Arc<parking_lot::RwLock<Option<CachedTerms>>>,

    /// Cache time-to-live (default: 5 minutes)
    cache_ttl: Duration,
}

impl RegistryClient {
    /// Create a new registry client
    ///
    /// # Arguments
    ///
    /// * `registry` - Optional ontology registry. If None, will always use default terms.
    pub fn new(registry: Option<Arc<parking_lot::RwLock<OntologyRegistry>>>) -> Self {
        Self {
            registry,
            term_cache: Arc::new(parking_lot::RwLock::new(None)),
            cache_ttl: Duration::from_secs(300), // 5 minutes default
        }
    }

    /// Create a registry client with custom cache TTL
    ///
    /// # Arguments
    ///
    /// * `registry` - Optional ontology registry
    /// * `cache_ttl` - How long to cache parsed terms before re-parsing
    pub fn with_cache_ttl(
        registry: Option<Arc<parking_lot::RwLock<OntologyRegistry>>>,
        cache_ttl: Duration,
    ) -> Self {
        Self {
            registry,
            term_cache: Arc::new(parking_lot::RwLock::new(None)),
            cache_ttl,
        }
    }

    /// Get ontology terms from registry
    ///
    /// **Phase 4 Implementation**: Queries registered ontologies dynamically
    ///
    /// This method:
    /// 1. Checks cache first (if fresh, return cached terms immediately)
    /// 2. On cache miss: Queries all active ontologies from the registry
    /// 3. Parses Turtle content to extract classes and properties
    /// 4. Converts to OntologyTerm format for matching
    /// 5. Caches result for future requests
    /// 6. Falls back to default terms if no registry available
    ///
    /// # Returns
    ///
    /// Vector of ontology terms from all active ontologies, or default terms as fallback
    pub fn get_ontology_terms(&self) -> Result<Vec<OntologyTerm>> {
        // Check cache first (fast path)
        {
            let cache_lock = self.term_cache.read();
            if let Some(cached) = cache_lock.as_ref() {
                if cached.cached_at.elapsed() < self.cache_ttl {
                    debug!(
                        "Cache HIT: Returning {} cached terms from {} ontologies (age: {:.1}s)",
                        cached.terms.len(),
                        cached.ontology_count,
                        cached.cached_at.elapsed().as_secs_f32()
                    );
                    return Ok(cached.terms.clone());
                } else {
                    debug!(
                        "Cache EXPIRED: Age {:.1}s > TTL {:.1}s",
                        cached.cached_at.elapsed().as_secs_f32(),
                        self.cache_ttl.as_secs_f32()
                    );
                }
            }
        }

        debug!("Cache MISS: Parsing ontologies from registry");

        // If ontology registry is available, query it dynamically
        if let Some(registry) = &self.registry {
            info!("Querying ontology registry for terms (Phase 4)");

            // Optimization: Clone ontology content while holding lock, then drop lock before parsing
            // This minimizes lock contention and allows concurrent parsing
            let ontology_snapshots = {
                let registry_lock = registry.read();
                let active_ontologies = registry_lock.list_active_ontologies();

                if active_ontologies.is_empty() {
                    warn!("No active ontologies in registry, falling back to defaults");
                    drop(registry_lock);
                    return Ok(get_default_terms());
                }

                // Quickly snapshot: clone metadata + content, then drop lock
                let snapshots: Vec<_> = active_ontologies
                    .iter()
                    .filter_map(|&metadata| {
                        registry_lock
                            .get_ontology(&metadata.id)
                            .map(|ontology| (metadata.clone(), ontology.content.clone()))
                    })
                    .collect();

                debug!(
                    "Snapshotted {} ontologies (lock held for ~1ms)",
                    snapshots.len()
                );

                snapshots
            }; // Lock dropped here - now safe for concurrent parsing

            if ontology_snapshots.is_empty() {
                warn!("No ontologies found, falling back to defaults");
                return Ok(get_default_terms());
            }

            let ontology_count = ontology_snapshots.len();
            let mut all_terms = Vec::new();

            // Parse ontologies outside of lock - allows concurrent parsing by multiple threads
            for (metadata, content) in ontology_snapshots {
                debug!(
                    "Extracting terms from: {} ({})",
                    metadata.name, metadata.namespace
                );

                // Detect format and use appropriate parser
                let format = Self::detect_format(&content);
                debug!("Detected format: {:?}", format);

                let parse_result = match format {
                    OntologyFormat::RdfXml => RdfXmlParser::parse(&content, &metadata.namespace),
                    OntologyFormat::Turtle => TurtleParser::parse(&content, &metadata.namespace),
                    OntologyFormat::Unknown => {
                        // Try both parsers
                        TurtleParser::parse(&content, &metadata.namespace)
                            .or_else(|_| RdfXmlParser::parse(&content, &metadata.namespace))
                    }
                };

                match parse_result {
                    Ok(terms) => {
                        debug!("Found {} terms from {}", terms.len(), metadata.id);
                        all_terms.extend(terms);
                    }
                    Err(e) => {
                        warn!("Failed to parse ontology {}: {}", metadata.id, e);
                        // Continue with other ontologies
                    }
                }
            }

            if all_terms.is_empty() {
                warn!("No terms extracted, falling back to defaults");
                return Ok(get_default_terms());
            }

            info!(
                "Loaded {} ontology terms from {} ontologies",
                all_terms.len(),
                ontology_count
            );

            // Cache the parsed terms for future requests
            let cached = CachedTerms {
                terms: all_terms.clone(),
                cached_at: Instant::now(),
                ontology_count,
            };

            *self.term_cache.write() = Some(cached);
            debug!(
                "Cached {} terms (TTL: {:.0}s)",
                all_terms.len(),
                self.cache_ttl.as_secs_f32()
            );

            Ok(all_terms)
        } else {
            // Fallback to default terms
            warn!("Ontology registry not available, using default schema.org terms");
            warn!("Wire ontology registry with mapping_engine.with_ontology_registry() for dynamic ontologies");
            Ok(get_default_terms())
        }
    }

    /// Get ontology terms filtered by namespaces
    ///
    /// **Optimized**: Only parses ontologies matching the requested namespaces,
    /// avoiding wasteful parsing of irrelevant ontologies.
    ///
    /// # Arguments
    ///
    /// * `namespaces` - List of namespace URIs to filter by
    ///
    /// # Example
    ///
    /// ```ignore
    /// let terms = client.get_terms_by_namespaces(&[
    ///     "http://schema.org/",
    ///     "http://example.com/retail#"
    /// ])?;
    /// ```
    pub fn get_terms_by_namespaces(&self, namespaces: &[String]) -> Result<Vec<OntologyTerm>> {
        if namespaces.is_empty() {
            // No filter - return all terms (use regular cache path)
            return self.get_ontology_terms();
        }

        // Optimization: Only parse ontologies matching requested namespaces
        // This avoids parsing 99% of ontologies when filtering to 1 namespace

        if let Some(registry) = &self.registry {
            debug!(
                "Filtering ontologies to {} specific namespaces",
                namespaces.len()
            );

            // Optimization: Clone matching ontology content, drop lock, then parse
            let (matching_snapshots, total_ontologies) = {
                let registry_lock = registry.read();
                let active_ontologies = registry_lock.list_active_ontologies();

                if active_ontologies.is_empty() {
                    warn!("No active ontologies in registry, falling back to defaults");
                    drop(registry_lock);
                    return Ok(get_default_terms());
                }

                let total = active_ontologies.len();

                // Filter and snapshot only matching ontologies
                let snapshots: Vec<_> = active_ontologies
                    .iter()
                    .filter_map(|&metadata| {
                        // Check if this ontology's namespace matches any requested namespace
                        let namespace_matches = namespaces.iter().any(|ns| {
                            metadata.namespace.starts_with(ns)
                                || ns.starts_with(&metadata.namespace)
                        });

                        if !namespace_matches {
                            debug!(
                                "Skipping ontology '{}' (namespace {} doesn't match filter)",
                                metadata.id, metadata.namespace
                            );
                            return None;
                        }

                        // Snapshot matching ontology
                        registry_lock.get_ontology(&metadata.id).map(|ontology| {
                            debug!(
                                "Snapshotting ontology '{}' (namespace {} matches filter)",
                                metadata.id, metadata.namespace
                            );
                            (metadata.clone(), ontology.content.clone())
                        })
                    })
                    .collect();

                debug!(
                    "Snapshotted {} matching ontologies from {} total (lock held for ~1ms)",
                    snapshots.len(),
                    total
                );

                (snapshots, total)
            }; // Lock dropped here

            let parsed_count = matching_snapshots.len();
            let skipped_count = total_ontologies - parsed_count;
            let mut filtered_terms = Vec::new();

            // Parse outside of lock - allows concurrent parsing
            for (metadata, content) in matching_snapshots {
                let format = Self::detect_format(&content);
                let parse_result = match format {
                    OntologyFormat::RdfXml => RdfXmlParser::parse(&content, &metadata.namespace),
                    OntologyFormat::Turtle => TurtleParser::parse(&content, &metadata.namespace),
                    OntologyFormat::Unknown => TurtleParser::parse(&content, &metadata.namespace)
                        .or_else(|_| RdfXmlParser::parse(&content, &metadata.namespace)),
                };

                match parse_result {
                    Ok(terms) => {
                        filtered_terms.extend(terms);
                    }
                    Err(e) => {
                        warn!("Failed to parse ontology {}: {}", metadata.id, e);
                    }
                }
            }

            info!(
                "Namespace filter: parsed {} ontologies, skipped {} (returned {} terms)",
                parsed_count,
                skipped_count,
                filtered_terms.len()
            );

            if filtered_terms.is_empty() {
                warn!("No terms matched namespace filter, falling back to defaults");
                return Ok(get_default_terms());
            }

            Ok(filtered_terms)
        } else {
            // No registry - filter default terms
            let all_terms = get_default_terms();
            let filtered: Vec<OntologyTerm> = all_terms
                .into_iter()
                .filter(|term| namespaces.iter().any(|ns| term.uri.starts_with(ns)))
                .collect();

            Ok(filtered)
        }
    }

    /// Invalidate the term cache
    ///
    /// Call this method when ontologies are added, updated, or removed
    /// to force re-parsing on the next get_ontology_terms() call.
    ///
    /// # Example
    ///
    /// ```ignore
    /// registry.register_custom_ontology("retail", content, None)?;
    /// registry_client.invalidate_cache();  // Force re-parse
    /// ```
    pub fn invalidate_cache(&self) {
        let mut cache = self.term_cache.write();
        if cache.is_some() {
            debug!("Cache invalidated - next request will re-parse ontologies");
            *cache = None;
        }
    }

    /// Get cache statistics
    ///
    /// Returns (cached_terms_count, cache_age_seconds, is_fresh)
    pub fn cache_stats(&self) -> Option<(usize, f32, bool)> {
        let cache = self.term_cache.read();
        cache.as_ref().map(|cached| {
            let age_secs = cached.cached_at.elapsed().as_secs_f32();
            let is_fresh = cached.cached_at.elapsed() < self.cache_ttl;
            (cached.terms.len(), age_secs, is_fresh)
        })
    }

    /// Check if registry is available
    pub fn has_registry(&self) -> bool {
        self.registry.is_some()
    }

    /// Get registry status string
    pub fn get_status(&self) -> &'static str {
        if self.registry.is_some() {
            "Registry available (Phase 4)"
        } else {
            "Using defaults only (Phase 1)"
        }
    }

    /// Detect ontology format from content
    ///
    /// Detects whether content is Turtle or RDF/XML format by examining
    /// the content structure and common patterns.
    fn detect_format(content: &str) -> OntologyFormat {
        let trimmed = content.trim();

        // Check for XML declaration or RDF/XML root element
        if trimmed.starts_with("<?xml")
            || trimmed.contains("<rdf:RDF")
            || trimmed.contains("<owl:Ontology")
        {
            return OntologyFormat::RdfXml;
        }

        // Check for Turtle-specific patterns
        if trimmed.contains("@prefix")
            || trimmed.contains("@base")
            || (trimmed.contains("a ")
                && (trimmed.contains("rdfs:Class") || trimmed.contains("owl:Class")))
        {
            return OntologyFormat::Turtle;
        }

        // If we can't determine, return Unknown
        OntologyFormat::Unknown
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphica_core::catalog::OntologyRegistry;
    use parking_lot::RwLock;

    #[test]
    fn test_client_without_registry() {
        let client = RegistryClient::new(None);

        assert!(!client.has_registry());
        assert_eq!(client.get_status(), "Using defaults only (Phase 1)");

        let terms = client.get_ontology_terms().unwrap();
        assert_eq!(terms.len(), 4); // Default terms
    }

    #[test]
    fn test_cache_behavior() {
        let registry = Arc::new(RwLock::new(OntologyRegistry::new()));

        // Register a test ontology
        {
            let mut reg = registry.write();
            let content = r#"
                @prefix test: <http://test.com/ont#> .
                @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
                @prefix owl: <http://www.w3.org/2002/07/owl#> .

                test:TestProperty a owl:DatatypeProperty ;
                    rdfs:label "Test Property" .
            "#;
            reg.register_custom_ontology("test", content, Some("http://test.com/ont#".to_string()))
                .unwrap();
        }

        // Create client with 10-second cache TTL
        let client =
            RegistryClient::with_cache_ttl(Some(registry.clone()), Duration::from_secs(10));

        // First call - cache MISS, should parse
        assert!(
            client.cache_stats().is_none(),
            "Cache should be empty initially"
        );
        let terms1 = client.get_ontology_terms().unwrap();
        assert!(!terms1.is_empty(), "Should have parsed terms");

        // Verify cache is now populated
        let (cached_count, age, is_fresh) =
            client.cache_stats().expect("Cache should be populated");
        assert_eq!(
            cached_count,
            terms1.len(),
            "Cache should have same count as returned terms"
        );
        assert!(age < 1.0, "Cache should be fresh (< 1 second old)");
        assert!(is_fresh, "Cache should be marked as fresh");

        // Second call - cache HIT, should return cached terms immediately
        let terms2 = client.get_ontology_terms().unwrap();
        assert_eq!(
            terms1.len(),
            terms2.len(),
            "Cached terms should have same count"
        );

        // Third call - invalidate cache, should re-parse
        client.invalidate_cache();
        assert!(
            client.cache_stats().is_none(),
            "Cache should be cleared after invalidation"
        );

        let terms3 = client.get_ontology_terms().unwrap();
        assert_eq!(
            terms1.len(),
            terms3.len(),
            "Re-parsed terms should have same count"
        );

        // Verify cache is populated again
        assert!(
            client.cache_stats().is_some(),
            "Cache should be re-populated after invalidation"
        );
    }

    #[test]
    fn test_filter_by_namespaces() {
        let client = RegistryClient::new(None);

        // Filter for schema.org only
        let filtered = client
            .get_terms_by_namespaces(&["http://schema.org/".to_string()])
            .unwrap();

        assert_eq!(filtered.len(), 4); // All default terms are schema.org

        // Filter for non-existent namespace
        let empty = client
            .get_terms_by_namespaces(&["http://nonexistent.com/".to_string()])
            .unwrap();

        assert_eq!(empty.len(), 0);
    }

    #[test]
    fn test_empty_namespace_filter_returns_all() {
        let client = RegistryClient::new(None);

        let all_terms = client.get_terms_by_namespaces(&[]).unwrap();

        assert_eq!(all_terms.len(), 4); // Should return all default terms
    }
}
