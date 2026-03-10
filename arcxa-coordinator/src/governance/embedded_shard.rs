//! Embedded Shard - Subprocess-Based RDF Store for Development Mode
//!
//! This module provides an embedded RDF store implementation that spawns a
//! local `arcxa-shard` subprocess and connects to it via gRPC on localhost.
//!
//! ## Rationale
//!
//! Direct integration of Oxigraph into the coordinator is not possible due to
//! RocksDB version conflicts:
//! - Coordinator uses `rocksdb = "0.22"` (librocksdb-sys → RocksDB v9.x)
//! - Oxigraph uses `oxigraph[rocksdb]` (oxrocksdb-sys → RocksDB v6.x)
//!
//! These cannot coexist in the same binary. The subprocess-based approach:
//! - Avoids all dependency conflicts
//! - Provides full Oxigraph SPARQL 1.1 support
//! - Maintains same interface as distributed shards
//! - Works transparently for development mode
//!
//! ## Architecture
//!
//! ```text
//! arcxa-coordinator (dev mode)
//!     ├─> Spawns: arcxa-shard subprocess on localhost:PORT
//!     └─> Connects: gRPC client to localhost:PORT
//!
//! User experience: Transparent (feels like in-process)
//! Reality: Separate process with isolated dependencies
//! ```
//!
//! ## Usage
//!
//! ```ignore
//! use graphica_coordinator::governance::embedded_shard::{EmbeddedShard, RdfQueryClient};
//!
//! # async fn example() -> anyhow::Result<()> {
//! // Create in-memory embedded shard
//! let shard = EmbeddedShard::new_in_memory().await?;
//!
//! // Use as RDF query client
//! let results = shard.query("SELECT * WHERE { ?s ?p ?o } LIMIT 10").await?;
//!
//! // Shard subprocess automatically shut down on drop
//! # Ok(())
//! # }
//! ```

use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use serde_json::Value as JsonValue;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;
use tonic::transport::Channel;
use tracing::{debug, info, warn};

use graphica_core::distributed::proto::shard_service::{
    shard_service_client::ShardServiceClient, CountRequest, ExecuteUpdateRequest, HealthRequest,
    InsertBatchRequest, QueryRequest, Triple as ProtoTriple,
};

use super::distributed::ShardRegistry;
use super::rdf_store::{NamedGraph, RdfStore};
use super::shard_coordinator::ShardCoordinatingRdfStore;
use crate::app_context::AppContext;

/// Async RDF query client trait
///
/// This trait provides an async interface for RDF operations,
/// suitable for use in transformers and other async contexts.
///
/// Implemented by:
/// - `EmbeddedShard` (subprocess-based, for development)
/// - Distributed shard client (gRPC-based, for production)
#[async_trait]
pub trait RdfQueryClient: Send + Sync {
    /// Execute SPARQL SELECT/CONSTRUCT/ASK query
    async fn query(&self, sparql: &str) -> Result<Vec<JsonValue>>;

    /// Load RDF data in Turtle format
    async fn load_turtle(&self, turtle: &str, graph: Option<&str>) -> Result<()>;

    /// Execute SPARQL UPDATE operation
    async fn update(&self, sparql_update: &str) -> Result<()>;

    /// Get total triple count
    async fn count(&self) -> Result<u64>;

    /// Health check
    async fn health_check(&self) -> Result<bool>;
}

/// Storage mode for embedded shard
#[derive(Debug, Clone)]
pub enum StorageMode {
    /// In-memory (no persistence)
    InMemory,
    /// File-backed (RocksDB persistence)
    Persistent(PathBuf),
}

/// Embedded shard (subprocess-based for development mode)
///
/// Spawns an `arcxa-shard` process on localhost and connects via gRPC.
/// Automatically shuts down the subprocess when dropped.
///
/// # Examples
///
/// ```ignore
/// // In-memory mode (fast, no persistence)
/// let shard = EmbeddedShard::new_in_memory().await?;
///
/// // Persistent mode (with RocksDB backend)
/// let shard = EmbeddedShard::new_persistent("/tmp/shard-data").await?;
/// ```
pub struct EmbeddedShard {
    /// Child process handle
    process: Child,
    /// gRPC client
    client: ShardServiceClient<Channel>,
    /// Port number
    port: u16,
    /// Storage mode
    mode: StorageMode,
}

impl EmbeddedShard {
    /// Create in-memory embedded shard (no persistence)
    ///
    /// This is the recommended mode for development and testing.
    /// Startup time: ~500ms
    ///
    /// # Errors
    /// - Returns error if `arcxa-shard` binary not found in PATH
    /// - Returns error if port binding fails
    /// - Returns error if shard fails to start within 2 seconds
    pub async fn new_in_memory() -> Result<Self> {
        Self::spawn(StorageMode::InMemory).await
    }

    /// Create file-backed embedded shard (persisted)
    ///
    /// Uses RocksDB for persistence. Data survives restarts.
    ///
    /// # Arguments
    /// * `path` - Directory path for RocksDB data
    pub async fn new_persistent(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        Self::spawn(StorageMode::Persistent(path)).await
    }

    /// Spawn shard subprocess and connect
    async fn spawn(mode: StorageMode) -> Result<Self> {
        // Find available port
        let port =
            find_available_port().context("Failed to find available port for embedded shard")?;

        // Build command
        let mut cmd = Command::new("arcxa-shard");
        cmd.args(&["--shard-id", "0"])
            .args(&["--port", &port.to_string()])
            .args(&["--bind", "127.0.0.1"])
            .stdout(Stdio::null()) // Suppress stdout for cleaner dev experience
            .stderr(Stdio::piped()); // Capture stderr for error reporting

        // Add data path if persistent mode
        if let StorageMode::Persistent(ref path) = mode {
            cmd.args(&["--data-path", path.to_str().unwrap()]);
        }

        info!("🚀 Spawning embedded shard subprocess on port {}", port);
        debug!("Embedded shard mode: {:?}", mode);

        // Spawn process
        let process = cmd
            .spawn()
            .context("Failed to spawn arcxa-shard process. Is arcxa-shard in PATH?")?;

        // Wait for startup with health checks
        let mut retries = 0;
        let max_retries = 20; // 2 seconds total (20 * 100ms)

        loop {
            sleep(Duration::from_millis(100)).await;
            retries += 1;

            // Try to connect
            match ShardServiceClient::connect(format!("http://127.0.0.1:{}", port)).await {
                Ok(mut client) => {
                    // Verify health
                    let health_req = tonic::Request::new(HealthRequest { detailed: false });
                    match client.health(health_req).await {
                        Ok(resp) => {
                            let health_status = resp.into_inner().status;
                            if health_status == 1 {
                                info!("✅ Embedded shard ready on port {}", port);
                                return Ok(Self {
                                    process,
                                    client,
                                    port,
                                    mode,
                                });
                            } else if retries >= max_retries {
                                return Err(anyhow!(
                                    "Embedded shard failed health check after {} retries",
                                    max_retries
                                ));
                            }
                        }
                        Err(_) => {
                            if retries >= max_retries {
                                return Err(anyhow!(
                                    "Embedded shard failed health check after {} retries",
                                    max_retries
                                ));
                            }
                        }
                    }
                }
                Err(_) => {
                    if retries >= max_retries {
                        return Err(anyhow!(
                            "Failed to connect to embedded shard after {} retries",
                            max_retries
                        ));
                    }
                }
            }
        }
    }

    /// Get port number
    pub fn port(&self) -> u16 {
        self.port
    }

    /// Get storage mode
    pub fn mode(&self) -> &StorageMode {
        &self.mode
    }
}

impl Drop for EmbeddedShard {
    fn drop(&mut self) {
        info!("🛑 Shutting down embedded shard on port {}", self.port);

        // Try graceful shutdown first (ignore errors, subprocess might be dead)
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = self.process.kill();
            let _ = self.process.wait();
        }));
    }
}

#[async_trait]
impl RdfQueryClient for EmbeddedShard {
    async fn query(&self, sparql: &str) -> Result<Vec<JsonValue>> {
        debug!(
            "Executing SPARQL query on embedded shard: {}",
            if sparql.len() > 100 {
                &sparql[..100]
            } else {
                sparql
            }
        );

        let request = tonic::Request::new(QueryRequest {
            sparql: sparql.to_string(),
            limit: 0, // No limit (use LIMIT in SPARQL query itself)
            offset: 0,
            projections: vec![],
            query_plan_hint: vec![],
            timeout_ms: 30000, // 30 second timeout
            request_id: uuid::Uuid::new_v4().to_string(),
            timestamp: chrono::Utc::now().timestamp_millis() as u64,
        });

        let mut client = self.client.clone();
        let response = client.query(request).await.context("gRPC query failed")?;

        // Convert streaming gRPC response to JSON
        let mut results = Vec::new();
        let mut stream = response.into_inner();

        use futures::StreamExt;
        while let Some(resp) = stream.next().await {
            let resp = resp.context("Stream error")?;

            // Handle different response types
            if let Some(response) = resp.response {
                use graphica_core::distributed::proto::shard_service::query_response::Response;
                match response {
                    Response::Binding(binding) => {
                        // Convert binding to JSON
                        let mut json_map = serde_json::Map::new();
                        for (var, value) in binding.bindings {
                            if let Some(val) = value.value {
                                use graphica_core::distributed::proto::shard_service::binding_value::Value as BValue;
                                let json_val = match val {
                                    BValue::Uri(uri) => JsonValue::String(uri),
                                    BValue::Literal(lit) => JsonValue::String(lit.value),
                                    BValue::BlankNode(bn) => JsonValue::String(format!("_:{}", bn)),
                                };
                                json_map.insert(var, json_val);
                            }
                        }
                        results.push(JsonValue::Object(json_map));
                    }
                    Response::Error(err) => {
                        return Err(anyhow!("Query error: {} - {}", err.code, err.message));
                    }
                    Response::End(_) => {
                        // End of stream, break
                        break;
                    }
                    _ => {
                        // Ignore other response types (triple_batch, etc.)
                    }
                }
            }
        }

        debug!("Query returned {} results", results.len());
        Ok(results)
    }

    async fn load_turtle(&self, turtle: &str, graph: Option<&str>) -> Result<()> {
        debug!(
            "Loading {} bytes of Turtle data into embedded shard",
            turtle.len()
        );

        // Parse Turtle to triples using simple parser
        let triples = parse_turtle_to_triples(turtle, graph)?;

        if triples.is_empty() {
            warn!("No triples parsed from Turtle data");
            return Ok(());
        }

        // Insert triples in batches
        const BATCH_SIZE: usize = 1000;
        for batch in triples.chunks(BATCH_SIZE) {
            let request = tonic::Request::new(InsertBatchRequest {
                triples: batch.to_vec(),
                transactional: false, // Best-effort insertion
                default_graph: graph.unwrap_or("").to_string(),
                request_id: uuid::Uuid::new_v4().to_string(),
            });

            let mut client = self.client.clone();
            client
                .insert_batch(request)
                .await
                .context("Failed to insert batch")?;
        }

        info!("Loaded {} triples into embedded shard", triples.len());
        Ok(())
    }

    async fn update(&self, sparql_update: &str) -> Result<()> {
        debug!("Executing SPARQL UPDATE on embedded shard");

        let request = tonic::Request::new(ExecuteUpdateRequest {
            sparql_update: sparql_update.to_string(),
            default_graph: String::new(), // Use default graph
            request_id: uuid::Uuid::new_v4().to_string(),
            timeout_ms: 30000, // 30 second timeout
        });

        let mut client = self.client.clone();
        client
            .execute_update(request)
            .await
            .context("SPARQL UPDATE failed")?;

        Ok(())
    }

    async fn count(&self) -> Result<u64> {
        let request = tonic::Request::new(CountRequest {
            pattern: String::new(), // Empty pattern counts all triples
            graph: String::new(),   // Default graph
            request_id: uuid::Uuid::new_v4().to_string(),
        });

        let mut client = self.client.clone();
        let response = client
            .count(request)
            .await
            .context("Count request failed")?;

        Ok(response.into_inner().count)
    }

    async fn health_check(&self) -> Result<bool> {
        let request = tonic::Request::new(HealthRequest { detailed: false });

        let mut client = self.client.clone();
        match client.health(request).await {
            Ok(resp) => Ok(resp.into_inner().status == 1),
            Err(_) => Ok(false),
        }
    }
}

/// Distributed shard client (production mode adapter)
///
/// This adapter wraps `ShardCoordinatingRdfStore` to implement the async
/// `RdfQueryClient` trait. It provides the same interface as `EmbeddedShard`
/// but connects to distributed shard servers instead of a local subprocess.
///
/// # Usage
///
/// ```ignore
/// use graphica_coordinator::governance::embedded_shard::DistributedShardClient;
///
/// # async fn example() -> anyhow::Result<()> {
/// let registry = ShardRegistry::new("./data/shards", 4, 60)?;
/// let context = AppContext::minimal();
/// let client = DistributedShardClient::new(registry.into(), context);
///
/// // Use as RDF query client (same interface as EmbeddedShard)
/// let results = client.query("SELECT * WHERE { ?s ?p ?o } LIMIT 10").await?;
/// # Ok(())
/// # }
/// ```
pub struct DistributedShardClient {
    /// Underlying distributed RDF store
    store: ShardCoordinatingRdfStore,
}

impl DistributedShardClient {
    /// Create a new distributed shard client
    ///
    /// # Arguments
    /// * `registry` - Shard registry containing topology
    /// * `context` - Application context for observability
    pub fn new(registry: Arc<ShardRegistry>, context: AppContext) -> Self {
        let store = ShardCoordinatingRdfStore::new(registry, context);
        Self { store }
    }

    /// Get connection pool statistics (for monitoring)
    pub fn connection_stats(&self) -> super::shard_coordinator::connection::ConnectionPoolStats {
        self.store.connection_stats()
    }

    /// Get active shard count
    pub fn active_shard_count(&self) -> Result<usize> {
        self.store.active_shard_count()
    }
}

#[async_trait]
impl RdfQueryClient for DistributedShardClient {
    async fn query(&self, sparql: &str) -> Result<Vec<JsonValue>> {
        // Delegate to sync RdfStore trait via spawn_blocking
        // This avoids blocking the async runtime
        let sparql = sparql.to_string();
        let store = self.store.clone();

        tokio::task::spawn_blocking(move || store.query(&sparql))
            .await
            .context("Task join error")?
    }

    async fn load_turtle(&self, turtle: &str, graph: Option<&str>) -> Result<()> {
        // Parse Turtle to triples
        let triples = parse_turtle_to_triples(turtle, graph)?;

        if triples.is_empty() {
            warn!("No triples parsed from Turtle data");
            return Ok(());
        }

        // Convert ProtoTriples to (String, String, String) format
        let triple_tuples: Vec<(String, String, String)> = triples
            .into_iter()
            .map(|t| (t.subject, t.predicate, t.object))
            .collect();

        // Build NamedGraph if specified
        let named_graph = graph.map(|g| NamedGraph { uri: g.to_string() });

        let store = self.store.clone();
        tokio::task::spawn_blocking(move || {
            store.insert_triples(triple_tuples, named_graph.as_ref())
        })
        .await
        .context("Task join error")?
    }

    async fn update(&self, sparql_update: &str) -> Result<()> {
        let sparql_update = sparql_update.to_string();
        let store = self.store.clone();

        tokio::task::spawn_blocking(move || store.update(&sparql_update))
            .await
            .context("Task join error")?
    }

    async fn count(&self) -> Result<u64> {
        // Use SPARQL query to count triples
        // This is more efficient than loading all triples
        let store = self.store.clone();

        let count_sparql = "SELECT (COUNT(*) as ?count) WHERE { ?s ?p ?o }";
        let results = tokio::task::spawn_blocking(move || store.query(count_sparql))
            .await
            .context("Task join error")??;

        // Parse count from results
        if let Some(result) = results.first() {
            if let Some(count_value) = result.get("count") {
                if let Some(count_str) = count_value.as_str() {
                    return count_str.parse::<u64>().context("Failed to parse count");
                } else if let Some(count_num) = count_value.as_u64() {
                    return Ok(count_num);
                }
            }
        }

        Ok(0)
    }

    async fn health_check(&self) -> Result<bool> {
        // Check if we have active shards
        let store = self.store.clone();

        let result = tokio::task::spawn_blocking(move || store.active_shard_count())
            .await
            .context("Task join error")?;

        Ok(result.map(|count| count > 0).unwrap_or(false))
    }
}

/// Find an available port on localhost
fn find_available_port() -> Result<u16> {
    // Use OS to find available port
    let listener = TcpListener::bind("127.0.0.1:0").context("Failed to bind to localhost")?;
    let port = listener
        .local_addr()
        .context("Failed to get local address")?
        .port();
    drop(listener); // Release port immediately
    Ok(port)
}

/// Parse Turtle format to proto triples
///
/// This is a simplified parser for basic Turtle syntax.
/// For production use, consider using rio_turtle for full Turtle 1.1 support.
fn parse_turtle_to_triples(turtle: &str, graph: Option<&str>) -> Result<Vec<ProtoTriple>> {
    let mut triples = Vec::new();
    let graph_uri = graph.map(|s| s.to_string()).unwrap_or_default();

    // Simple line-by-line parsing (handles basic triples)
    for line in turtle.lines() {
        let line = line.trim();

        // Skip empty lines, comments, and prefix declarations
        if line.is_empty()
            || line.starts_with("#")
            || line.starts_with("@prefix")
            || line.starts_with("@base")
        {
            continue;
        }

        // Remove trailing dot
        let line = line.trim_end_matches('.').trim();

        // Split by whitespace (simplified - doesn't handle quoted strings with spaces)
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 3 {
            let object = parts[2..].join(" ");

            // Detect if object is a literal (starts with quote)
            let (object_datatype, object_language) = if object.starts_with("\"") {
                // Check for language tag or datatype
                if object.contains("@") {
                    // Language-tagged literal (e.g., "hello"@en)
                    let lang = object
                        .split("@")
                        .nth(1)
                        .unwrap_or("")
                        .trim_end_matches("\"");
                    (String::new(), lang.to_string())
                } else if object.contains("^^") {
                    // Typed literal (e.g., "123"^^xsd:integer)
                    let dtype = object.split("^^").nth(1).unwrap_or("").trim();
                    (dtype.to_string(), String::new())
                } else {
                    // Plain literal - use xsd:string as default datatype
                    (
                        "http://www.w3.org/2001/XMLSchema#string".to_string(),
                        String::new(),
                    )
                }
            } else {
                // URI or blank node
                (String::new(), String::new())
            };

            triples.push(ProtoTriple {
                subject: parts[0].to_string(),
                predicate: parts[1].to_string(),
                object,
                object_datatype,
                object_language,
                graph: graph_uri.clone(),
            });
        }
    }

    Ok(triples)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_turtle_basic() {
        let turtle = r#"
            # Comment
            @prefix ex: <http://example.com/> .

            ex:subject ex:predicate ex:object .
            <http://example.com/s2> <http://example.com/p2> "literal" .
        "#;

        let triples = parse_turtle_to_triples(turtle, None).unwrap();
        assert_eq!(triples.len(), 2);
        assert_eq!(triples[0].subject, "ex:subject");
        assert_eq!(triples[0].predicate, "ex:predicate");
    }

    // Integration tests require arcxa-shard binary
    #[tokio::test]
    #[ignore] // Run with: cargo test -- --ignored
    async fn test_embedded_shard_spawn() {
        let shard = EmbeddedShard::new_in_memory().await;
        assert!(
            shard.is_ok(),
            "Failed to spawn embedded shard: {:?}",
            shard.err()
        );

        let shard = shard.unwrap();
        assert!(shard.port() > 0);

        // Test health check
        assert!(shard.health_check().await.unwrap());
    }

    #[tokio::test]
    #[ignore]
    async fn test_embedded_shard_query() {
        let shard = EmbeddedShard::new_in_memory().await.unwrap();

        // Count triples (should be 0 initially)
        let count = shard.count().await.unwrap();
        assert_eq!(count, 0);

        // Load some data
        let turtle = r#"
            <http://example.com/alice> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://xmlns.com/foaf/0.1/Person> .
        "#;
        shard.load_turtle(turtle, None).await.unwrap();

        // Query
        let results = shard.query("SELECT * WHERE { ?s ?p ?o }").await.unwrap();
        assert!(!results.is_empty());
    }
}
