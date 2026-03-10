//! Graphica Shard Server
//!
//! Standalone shard server process with Oxigraph + RocksDB backend.
//!
//! Each shard is responsible for:
//! - Storing a partition of the RDF triple space (hash-based)
//! - Executing SPARQL queries on its partition
//! - Serving gRPC requests from coordinator
//!
//! ## Usage
//!
//! ```bash
//! # Automatic hash range (recommended - coordinator assigns range)
//! graphica-shard \
//!   --shard-id 0 \
//!   --data-path /data/shard-0 \
//!   --port 9090
//!
//! # Manual hash range (legacy mode)
//! graphica-shard \
//!   --shard-id 0 \
//!   --hash-range "0-9223372036854775807" \
//!   --data-path /data/shard-0 \
//!   --port 9090
//! ```

mod identity;
mod instructions;
mod metrics;
mod registration;

use anyhow::{Context, Result};
use clap::Parser;
use graphica_core::distributed::proto::shard_service::{
    shard_service_server::{ShardService, ShardServiceServer},
    *,
};
use oxigraph::store::Store;
use oxigraph::sparql::QueryResults;
use oxigraph::model::{NamedNode, Quad, Subject, Term, GraphName, Literal as OxiLiteral};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;
use tokio::signal;
use tonic::{transport::Server, Request, Response, Status};
use tracing::{info, warn, error};

use identity::ShardIdentity;
use metrics::ShardMetricsCollector;
use registration::RegistrationClient;

/// Graphica Shard Server CLI
#[derive(Parser, Debug)]
#[command(name = "graphica-shard")]
#[command(about = "Graphica shard server with Oxigraph + RocksDB")]
struct Args {
    /// Shard ID (unique identifier) - LEGACY MODE ONLY
    /// For auto-registration mode, use --coordinator-url instead
    #[arg(long)]
    shard_id: Option<u32>,

    /// Hash range (start-end, e.g., "0-18446744073709551615") - LEGACY MODE ONLY
    /// If not specified, defaults to full range (0 to u64::MAX)
    /// In auto-registration mode, coordinator assigns the range
    #[arg(long)]
    hash_range: Option<String>,

    /// Coordinator URL for auto-registration (e.g., "http://localhost:9091")
    /// If specified, shard will register with coordinator and receive shard_id dynamically
    #[arg(long)]
    coordinator_url: Option<String>,

    /// Heartbeat interval in seconds for auto-registration mode
    #[arg(long, default_value = "30")]
    heartbeat_interval: u64,

    /// Data directory for RocksDB storage
    #[arg(long)]
    data_path: PathBuf,

    /// gRPC server port
    #[arg(long, default_value = "9090")]
    port: u16,

    /// Bind address
    #[arg(long, default_value = "0.0.0.0")]
    bind: String,
}

/// Shard server implementation
struct GraphicaShard {
    /// Shard ID
    shard_id: u32,

    /// Hash range (start, end)
    hash_range: (u64, u64),

    /// Oxigraph store with RocksDB backend (pub for heartbeat access)
    pub(crate) store: Arc<Store>,

    /// Server start time
    start_time: std::time::Instant,

    /// Data path for disk usage calculation
    data_path: PathBuf,

    /// Shutdown flag
    is_shutting_down: Arc<AtomicBool>,

    /// Read-only mode flag (prevents writes during maintenance)
    is_readonly: Arc<AtomicBool>,

    /// Metrics collector
    metrics: ShardMetricsCollector,
}

impl GraphicaShard {
    /// Create a new shard server with existing store and state flags
    fn new(
        shard_id: u32,
        hash_range: (u64, u64),
        data_path: PathBuf,
        store: Arc<Store>,
        metrics: ShardMetricsCollector,
        is_shutting_down: Arc<AtomicBool>,
        is_readonly: Arc<AtomicBool>,
    ) -> Result<Self> {
        info!(
            "Initializing shard {} with hash range {:?} at {:?}",
            shard_id, hash_range, data_path
        );

        Ok(Self {
            shard_id,
            hash_range,
            store,
            start_time: std::time::Instant::now(),
            data_path,
            is_shutting_down,
            is_readonly,
            metrics,
        })
    }

    /// Get triple count using SPARQL COUNT query
    fn triple_count(&self) -> u64 {
        // Use SPARQL COUNT(*) query for efficient counting
        let count_query = "SELECT (COUNT(*) as ?count) WHERE { ?s ?p ?o }";

        match self.store.query(count_query) {
            Ok(QueryResults::Solutions(mut solutions)) => {
                if let Some(Ok(solution)) = solutions.next() {
                    if let Some(Term::Literal(lit)) = solution.get("count") {
                        return lit.value().parse::<u64>().unwrap_or(0);
                    }
                }
                0
            }
            _ => {
                warn!("Failed to count triples, returning 0");
                0
            }
        }
    }

    /// Get disk usage by recursively calculating directory size
    fn disk_usage_bytes(&self) -> u64 {
        fn dir_size(path: &std::path::Path) -> u64 {
            let mut total = 0u64;

            if let Ok(entries) = std::fs::read_dir(path) {
                for entry in entries.flatten() {
                    if let Ok(metadata) = entry.metadata() {
                        if metadata.is_file() {
                            total += metadata.len();
                        } else if metadata.is_dir() {
                            total += dir_size(&entry.path());
                        }
                    }
                }
            }

            total
        }

        dir_size(&self.data_path)
    }

    /// Get process memory usage
    fn memory_usage_bytes(&self) -> u64 {
        #[cfg(target_os = "linux")]
        {
            // Read from /proc/self/status for RSS (Resident Set Size)
            if let Ok(status) = std::fs::read_to_string("/proc/self/status") {
                for line in status.lines() {
                    if line.starts_with("VmRSS:") {
                        let parts: Vec<&str> = line.split_whitespace().collect();
                        if parts.len() >= 2 {
                            if let Ok(kb) = parts[1].parse::<u64>() {
                                return kb * 1024; // Convert KB to bytes
                            }
                        }
                    }
                }
            }
        }

        // Fallback: return 0 if unable to get memory usage
        0
    }

    /// Parse and insert a single triple
    fn parse_and_insert_triple(&self, triple: &crate::Triple) -> Result<()> {
        // Parse subject
        let subject = if triple.subject.starts_with("http://") || triple.subject.starts_with("https://") {
            Subject::NamedNode(NamedNode::new(&triple.subject)?)
        } else if triple.subject.starts_with("_:") {
            Subject::BlankNode(oxigraph::model::BlankNode::new(&triple.subject[2..])?)
        } else {
            anyhow::bail!("Invalid subject: {}", triple.subject);
        };

        // Parse predicate
        let predicate = NamedNode::new(&triple.predicate)?;

        // Parse object
        let object = if triple.object.starts_with("http://") || triple.object.starts_with("https://") {
            Term::NamedNode(NamedNode::new(&triple.object)?)
        } else if triple.object.starts_with("_:") {
            Term::BlankNode(oxigraph::model::BlankNode::new(&triple.object[2..])?)
        } else {
            // Treat as literal
            Term::Literal(OxiLiteral::new_simple_literal(&triple.object))
        };

        // Parse graph (optional)
        let graph_name = if triple.graph.is_empty() {
            GraphName::DefaultGraph
        } else {
            GraphName::NamedNode(NamedNode::new(&triple.graph)?)
        };

        // Create quad and insert
        let quad = Quad::new(subject, predicate, object, graph_name);
        self.store.insert(&quad)?;

        Ok(())
    }

    /// Parse and delete a single triple
    fn parse_and_delete_triple(&self, triple: &crate::Triple) -> Result<bool> {
        // Parse subject
        let subject = if triple.subject.starts_with("http://") || triple.subject.starts_with("https://") {
            Subject::NamedNode(NamedNode::new(&triple.subject)?)
        } else if triple.subject.starts_with("_:") {
            Subject::BlankNode(oxigraph::model::BlankNode::new(&triple.subject[2..])?)
        } else {
            anyhow::bail!("Invalid subject: {}", triple.subject);
        };

        // Parse predicate
        let predicate = NamedNode::new(&triple.predicate)?;

        // Parse object
        let object = if triple.object.starts_with("http://") || triple.object.starts_with("https://") {
            Term::NamedNode(NamedNode::new(&triple.object)?)
        } else if triple.object.starts_with("_:") {
            Term::BlankNode(oxigraph::model::BlankNode::new(&triple.object[2..])?)
        } else {
            // Treat as literal
            Term::Literal(OxiLiteral::new_simple_literal(&triple.object))
        };

        // Parse graph (optional)
        let graph_name = if triple.graph.is_empty() {
            GraphName::DefaultGraph
        } else {
            GraphName::NamedNode(NamedNode::new(&triple.graph)?)
        };

        // Create quad and remove
        let quad = Quad::new(subject, predicate, object, graph_name);
        Ok(self.store.remove(&quad)?)
    }
}

#[tonic::async_trait]
impl ShardService for GraphicaShard {
    type QueryStream = tokio_stream::wrappers::ReceiverStream<Result<QueryResponse, Status>>;

    async fn query(
        &self,
        request: Request<QueryRequest>,
    ) -> Result<Response<Self::QueryStream>, Status> {
        // Reject requests during shutdown
        if self.is_shutting_down.load(Ordering::SeqCst) {
            return Err(Status::unavailable("Shard is shutting down"));
        }

        let req = request.into_inner();

        info!(
            "Shard {} received query: {} (limit: {}, offset: {})",
            self.shard_id, req.sparql, req.limit, req.offset
        );

        let (tx, rx) = tokio::sync::mpsc::channel(100);

        let store = self.store.clone();
        let shard_id = self.shard_id;
        let sparql = req.sparql.clone();
        let metrics = self.metrics.clone();

        // Use spawn_blocking for Oxigraph (blocking I/O) - collect results into Send-able types
        tokio::spawn(async move {
            use query_response::Response;
            use binding_value::Value;

            let start = Instant::now();

            // Define result types
            enum QueryResult {
                Bindings(Vec<std::collections::HashMap<String, BindingValue>>),
                Triples(Vec<Triple>),
            }

            // Execute query and collect results in blocking thread (Oxigraph is not Send)
            let collected_results = tokio::task::spawn_blocking(move || -> Result<QueryResult, oxigraph::sparql::EvaluationError> {
                match store.query(sparql.as_str())? {
                    QueryResults::Solutions(solutions) => {
                        // Collect all bindings
                        let bindings: Result<Vec<_>, _> = solutions
                            .map(|sol| {
                                sol.map(|binding| {
                                    let mut map = std::collections::HashMap::new();
                                    for (var, term) in binding.iter() {
                                        let binding_value = match term {
                                            Term::NamedNode(n) => BindingValue {
                                                value: Some(Value::Uri(n.to_string())),
                                            },
                                            Term::BlankNode(b) => BindingValue {
                                                value: Some(Value::BlankNode(b.to_string())),
                                            },
                                            Term::Literal(l) => BindingValue {
                                                value: Some(Value::Literal(Literal {
                                                    value: l.value().to_string(),
                                                    datatype: l.datatype().as_str().to_string(),
                                                    language: l.language().unwrap_or("").to_string(),
                                                })),
                                            },
                                            #[allow(unreachable_patterns)]
                                            _ => BindingValue { value: None },
                                        };
                                        map.insert(var.as_str().to_string(), binding_value);
                                    }
                                    map
                                })
                            })
                            .collect();
                        bindings.map(QueryResult::Bindings)
                    }
                    QueryResults::Boolean(b) => {
                        let mut map = std::collections::HashMap::new();
                        map.insert(
                            "result".to_string(),
                            BindingValue { value: Some(Value::Uri(b.to_string())) },
                        );
                        Ok(QueryResult::Bindings(vec![map]))
                    }
                    QueryResults::Graph(graph) => {
                        // Collect all triples
                        let triples: Result<Vec<_>, _> = graph
                            .map(|t_result| {
                                t_result.map(|t| {
                                    let (object_value, object_datatype, object_language) = match &t.object {
                                        Term::NamedNode(n) => (n.to_string(), String::new(), String::new()),
                                        Term::BlankNode(b) => (b.to_string(), String::new(), String::new()),
                                        Term::Literal(l) => (
                                            l.value().to_string(),
                                            l.datatype().as_str().to_string(),
                                            l.language().unwrap_or("").to_string(),
                                        ),
                                        #[allow(unreachable_patterns)]
                                        _ => (t.object.to_string(), String::new(), String::new()),
                                    };
                                    Triple {
                                        subject: t.subject.to_string(),
                                        predicate: t.predicate.to_string(),
                                        object: object_value,
                                        object_datatype,
                                        object_language,
                                        graph: String::new(),
                                    }
                                })
                            })
                            .collect();
                        triples.map(QueryResult::Triples)
                    }
                }
            }).await;

            let results = match collected_results {
                Ok(Ok(res)) => res,
                Ok(Err(e)) => {
                    error!("SPARQL query error: {}", e);
                    metrics.record_query_error();
                    let _ = tx.send(Err(Status::internal(format!("Query error: {}", e)))).await;
                    return;
                }
                Err(e) => {
                    error!("Task join error: {}", e);
                    metrics.record_query_error();
                    let _ = tx.send(Err(Status::internal(format!("Task error: {}", e)))).await;
                    return;
                }
            };

            let result_count = match &results {
                QueryResult::Bindings(b) => b.len() as u64,
                QueryResult::Triples(t) => t.len() as u64,
            };

            // Send results based on type
            match results {
                QueryResult::Bindings(bindings) => {
                    for binding_map in bindings {
                        let response = QueryResponse {
                            response: Some(Response::Binding(BindingResult {
                                bindings: binding_map,
                            })),
                        };
                        if let Err(e) = tx.send(Ok(response)).await {
                            warn!("Failed to send binding: {}", e);
                            break;
                        }
                    }
                }
                QueryResult::Triples(triples) => {
                    // Send triples in batches
                    const BATCH_SIZE: usize = 100;
                    for chunk in triples.chunks(BATCH_SIZE) {
                        let response = QueryResponse {
                            response: Some(Response::TripleBatch(TripleBatch {
                                triples: chunk.to_vec(),
                                has_more: false,
                                sequence: 0,
                            })),
                        };
                        if let Err(e) = tx.send(Ok(response)).await {
                            warn!("Failed to send triple batch: {}", e);
                            break;
                        }
                    }
                }
            }

            // Send end-of-stream marker
            let execution_time_ms = start.elapsed().as_millis() as u64;
            let response = QueryResponse {
                response: Some(Response::End(EndOfStream {
                    result_count,
                    execution_time_ms,
                })),
            };

            if let Err(e) = tx.send(Ok(response)).await {
                warn!("Failed to send end-of-stream: {}", e);
            }

            // Record query success metrics
            metrics.record_query_success(execution_time_ms as f64);
        });

        Ok(Response::new(tokio_stream::wrappers::ReceiverStream::new(rx)))
    }

    async fn insert_batch(
        &self,
        request: Request<InsertBatchRequest>,
    ) -> Result<Response<InsertBatchResponse>, Status> {
        // Reject requests during shutdown
        if self.is_shutting_down.load(Ordering::SeqCst) {
            return Err(Status::unavailable("Shard is shutting down"));
        }

        // Reject write requests in read-only mode
        if self.is_readonly.load(Ordering::SeqCst) {
            return Err(Status::failed_precondition("Shard is in read-only mode - writes not allowed"));
        }

        let req = request.into_inner();

        info!(
            "Shard {} inserting batch of {} triples (transactional: {})",
            self.shard_id,
            req.triples.len(),
            req.transactional
        );

        let start = Instant::now();
        let mut inserted_count = 0u64;
        let mut failed_count = 0u64;
        let mut failed_indices = vec![];

        // Parse and insert triples
        for (idx, triple_msg) in req.triples.iter().enumerate() {
            match self.parse_and_insert_triple(triple_msg) {
                Ok(_) => {
                    inserted_count += 1;
                    self.metrics.record_insert_success();
                }
                Err(e) => {
                    error!("Failed to insert triple at index {}: {}", idx, e);
                    failed_count += 1;
                    failed_indices.push(idx as u32);
                    self.metrics.record_insert_error();
                }
            }
        }

        let duration_ms = start.elapsed().as_millis() as u64;

        Ok(Response::new(InsertBatchResponse {
            inserted_count,
            failed_count,
            failed_indices,
            duration_ms,
            error: if failed_count > 0 {
                format!("{} triples failed to insert", failed_count)
            } else {
                String::new()
            },
        }))
    }

    async fn delete_batch(
        &self,
        request: Request<DeleteBatchRequest>,
    ) -> Result<Response<DeleteBatchResponse>, Status> {
        // Reject requests during shutdown
        if self.is_shutting_down.load(Ordering::SeqCst) {
            return Err(Status::unavailable("Shard is shutting down"));
        }

        // Reject write requests in read-only mode
        if self.is_readonly.load(Ordering::SeqCst) {
            return Err(Status::failed_precondition("Shard is in read-only mode - deletes not allowed"));
        }

        let req = request.into_inner();

        info!(
            "Shard {} deleting batch of {} triples",
            self.shard_id,
            req.triples.len()
        );

        let start = Instant::now();
        let mut deleted_count = 0u64;
        let mut not_found_count = 0u64;

        // Parse and delete triples
        for triple_msg in req.triples.iter() {
            match self.parse_and_delete_triple(triple_msg) {
                Ok(was_deleted) => {
                    if was_deleted {
                        deleted_count += 1;
                        self.metrics.record_delete_success();
                    } else {
                        not_found_count += 1;
                    }
                }
                Err(e) => {
                    error!("Failed to delete triple: {}", e);
                    return Err(Status::internal(format!("Delete error: {}", e)));
                }
            }
        }

        let duration_ms = start.elapsed().as_millis() as u64;

        Ok(Response::new(DeleteBatchResponse {
            deleted_count,
            not_found_count,
            duration_ms,
            error: String::new(),
        }))
    }

    async fn execute_update(
        &self,
        request: Request<ExecuteUpdateRequest>,
    ) -> Result<Response<ExecuteUpdateResponse>, Status> {
        // Reject requests during shutdown
        if self.is_shutting_down.load(Ordering::SeqCst) {
            return Err(Status::unavailable("Shard is shutting down"));
        }

        let req = request.into_inner();

        info!(
            "Shard {} executing SPARQL UPDATE: {}",
            self.shard_id, req.sparql_update
        );

        let start = Instant::now();
        let store = self.store.clone();
        let sparql_update = req.sparql_update.clone();

        // Execute SPARQL UPDATE in blocking thread
        let update_result = tokio::task::spawn_blocking(move || -> Result<(u64, u64, u64), String> {
            use oxigraph::sparql::Update;

            // Parse SPARQL UPDATE
            match Update::parse(&sparql_update, None) {
                Ok(update) => {
                    // Oxigraph's update() method executes the update and returns ()
                    // We'll track changes by querying before/after triple counts
                    let count_before = match store.query("SELECT (COUNT(*) as ?count) WHERE { ?s ?p ?o }") {
                        Ok(QueryResults::Solutions(mut solutions)) => {
                            if let Some(Ok(solution)) = solutions.next() {
                                if let Some(Term::Literal(lit)) = solution.get("count") {
                                    lit.value().parse::<u64>().unwrap_or(0)
                                } else {
                                    0
                                }
                            } else {
                                0
                            }
                        }
                        _ => 0,
                    };

                    // Execute UPDATE
                    match store.update(update) {
                        Ok(_) => {
                            // Count after
                            let count_after = match store.query("SELECT (COUNT(*) as ?count) WHERE { ?s ?p ?o }") {
                                Ok(QueryResults::Solutions(mut solutions)) => {
                                    if let Some(Ok(solution)) = solutions.next() {
                                        if let Some(Term::Literal(lit)) = solution.get("count") {
                                            lit.value().parse::<u64>().unwrap_or(0)
                                        } else {
                                            0
                                        }
                                    } else {
                                        0
                                    }
                                }
                                _ => 0,
                            };

                            // Calculate changes
                            let (inserted, deleted) = if count_after > count_before {
                                (count_after - count_before, 0)
                            } else if count_before > count_after {
                                (0, count_before - count_after)
                            } else {
                                (0, 0) // Modified in place or no changes
                            };

                            Ok((inserted, deleted, 0))
                        }
                        Err(e) => Err(format!("Update execution error: {}", e)),
                    }
                }
                Err(e) => Err(format!("SPARQL UPDATE parse error: {}", e)),
            }
        }).await;

        let (inserted_count, deleted_count, modified_count) = match update_result {
            Ok(Ok(counts)) => counts,
            Ok(Err(e)) => {
                error!("SPARQL UPDATE failed: {}", e);
                return Ok(Response::new(ExecuteUpdateResponse {
                    inserted_count: 0,
                    deleted_count: 0,
                    modified_count: 0,
                    duration_ms: start.elapsed().as_millis() as u64,
                    error: e,
                    success: false,
                }));
            }
            Err(e) => {
                error!("Task error: {}", e);
                return Err(Status::internal(format!("Task error: {}", e)));
            }
        };

        let duration_ms = start.elapsed().as_millis() as u64;

        info!(
            "Shard {} UPDATE complete: +{} -{} modified:{} in {}ms",
            self.shard_id, inserted_count, deleted_count, modified_count, duration_ms
        );

        Ok(Response::new(ExecuteUpdateResponse {
            inserted_count,
            deleted_count,
            modified_count,
            duration_ms,
            error: String::new(),
            success: true,
        }))
    }

    async fn count(
        &self,
        request: Request<CountRequest>,
    ) -> Result<Response<CountResponse>, Status> {
        let req = request.into_inner();

        info!("Shard {} counting pattern: {}", self.shard_id, req.pattern);

        let start = Instant::now();
        let store = self.store.clone();
        let pattern = req.pattern.clone();

        // Execute count query in blocking thread
        let count_result = tokio::task::spawn_blocking(move || -> Result<u64, String> {
            // Build SPARQL COUNT query from pattern
            let count_query = if pattern.is_empty() {
                // Count all triples
                "SELECT (COUNT(*) as ?count) WHERE { ?s ?p ?o }".to_string()
            } else {
                // Use provided pattern
                format!("SELECT (COUNT(*) as ?count) WHERE {{ {} }}", pattern)
            };

            match store.query(&count_query) {
                Ok(QueryResults::Solutions(mut solutions)) => {
                    if let Some(Ok(solution)) = solutions.next() {
                        if let Some(Term::Literal(lit)) = solution.get("count") {
                            return lit.value().parse::<u64>()
                                .map_err(|e| format!("Failed to parse count: {}", e));
                        }
                    }
                    Ok(0)
                }
                Err(e) => Err(format!("Query error: {}", e)),
                _ => Ok(0),
            }
        }).await;

        let count = match count_result {
            Ok(Ok(c)) => c,
            Ok(Err(e)) => {
                error!("Count query failed: {}", e);
                return Err(Status::internal(e));
            }
            Err(e) => {
                error!("Task error: {}", e);
                return Err(Status::internal(format!("Task error: {}", e)));
            }
        };

        let execution_time_ms = start.elapsed().as_millis() as u64;

        Ok(Response::new(CountResponse {
            count,
            execution_time_ms,
        }))
    }

    async fn exists(
        &self,
        request: Request<ExistsRequest>,
    ) -> Result<Response<ExistsResponse>, Status> {
        let req = request.into_inner();

        if let Some(triple) = req.triple {
            info!(
                "Shard {} checking existence: {} {} {}",
                self.shard_id, triple.subject, triple.predicate, triple.object
            );

            let store = self.store.clone();

            // Execute ASK query in blocking thread
            let exists_result = tokio::task::spawn_blocking(move || -> Result<bool, String> {
                // Parse subject
                let subject_str = if triple.subject.starts_with("http://") || triple.subject.starts_with("https://") {
                    format!("<{}>", triple.subject)
                } else if triple.subject.starts_with("_:") {
                    format!("_:{}", &triple.subject[2..])
                } else {
                    format!("?s") // Variable
                };

                // Parse predicate
                let predicate_str = if triple.predicate.starts_with("http://") || triple.predicate.starts_with("https://") {
                    format!("<{}>", triple.predicate)
                } else {
                    format!("?p") // Variable
                };

                // Parse object
                let object_str = if triple.object.starts_with("http://") || triple.object.starts_with("https://") {
                    format!("<{}>", triple.object)
                } else if triple.object.starts_with("_:") {
                    format!("_:{}", &triple.object[2..])
                } else {
                    format!("\"{}\"", triple.object.replace('\"', "\\\"")) // Literal
                };

                // Build ASK query
                let ask_query = format!(
                    "ASK {{ {} {} {} }}",
                    subject_str, predicate_str, object_str
                );

                match store.query(&ask_query) {
                    Ok(QueryResults::Boolean(exists)) => Ok(exists),
                    Err(e) => Err(format!("Query error: {}", e)),
                    _ => Ok(false),
                }
            }).await;

            let exists = match exists_result {
                Ok(Ok(e)) => e,
                Ok(Err(e)) => {
                    error!("Exists query failed: {}", e);
                    return Err(Status::internal(e));
                }
                Err(e) => {
                    error!("Task error: {}", e);
                    return Err(Status::internal(format!("Task error: {}", e)));
                }
            };

            Ok(Response::new(ExistsResponse { exists }))
        } else {
            Err(Status::invalid_argument("Triple is required"))
        }
    }

    async fn health(
        &self,
        _request: Request<HealthRequest>,
    ) -> Result<Response<HealthResponse>, Status> {
        Ok(Response::new(HealthResponse {
            status: health_response::Status::Healthy as i32,
            message: format!("Shard {} operational", self.shard_id),
            shard_id: self.shard_id,
            triple_count: self.triple_count(),
            disk_usage_bytes: self.disk_usage_bytes(),
            memory_usage_bytes: self.memory_usage_bytes(),
            last_query_timestamp: 0,
            uptime_seconds: self.start_time.elapsed().as_secs(),
        }))
    }

    async fn get_stats(
        &self,
        _request: Request<StatsRequest>,
    ) -> Result<Response<StatsResponse>, Status> {
        // Get snapshot of current metrics
        let shard_stats = self.metrics.snapshot();

        Ok(Response::new(StatsResponse {
            shard_id: self.shard_id,
            hash_range: Some(HashRange {
                start: self.hash_range.0,
                end: self.hash_range.1,
            }),
            triple_count: self.triple_count(),
            disk_usage_bytes: self.disk_usage_bytes(),
            memory_usage_bytes: self.memory_usage_bytes(),
            queries_total: shard_stats.queries_processed,
            inserts_total: shard_stats.inserts_processed,
            deletes_total: shard_stats.deletes_processed,
            p50_query_latency_ms: shard_stats.p50_latency_ms,
            p95_query_latency_ms: shard_stats.p95_latency_ms,
            p99_query_latency_ms: shard_stats.p99_latency_ms,
            queries_per_second: self.metrics.queries_per_second(),
            inserts_per_second: self.metrics.inserts_per_second(),
            query_errors: shard_stats.query_errors,
            insert_errors: shard_stats.insert_errors,
            rocksdb_stats: None,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        }))
    }

    async fn shutdown(
        &self,
        _request: Request<ShutdownRequest>,
    ) -> Result<Response<ShutdownResponse>, Status> {
        info!("Shard {} received shutdown request", self.shard_id);

        // Set shutdown flag to reject new requests
        self.is_shutting_down.store(true, Ordering::SeqCst);

        info!("Shard {} shutdown flag set, flushing store...", self.shard_id);

        // Flush the Oxigraph store to ensure all data is persisted
        let store = self.store.clone();
        let shard_id = self.shard_id;

        tokio::task::spawn_blocking(move || {
            match store.flush() {
                Ok(_) => info!("Shard {} successfully flushed store", shard_id),
                Err(e) => error!("Shard {} failed to flush store: {}", shard_id, e),
            }
        }).await.map_err(|e| {
            error!("Flush task failed: {}", e);
            Status::internal(format!("Flush task failed: {}", e))
        })?;

        info!("Shard {} shutdown complete", self.shard_id);

        Ok(Response::new(ShutdownResponse {
            success: true,
            message: "Shutdown completed successfully".to_string(),
            aborted_queries: 0,
        }))
    }
}

/// Parse hash range from string (e.g., "0-4294967295")
fn parse_hash_range(range_str: &str) -> Result<(u64, u64)> {
    let parts: Vec<&str> = range_str.split('-').collect();

    if parts.len() != 2 {
        anyhow::bail!("Invalid hash range format. Expected: start-end");
    }

    let start = parts[0].parse::<u64>()
        .with_context(|| format!("Invalid start value: {}", parts[0]))?;

    let end = parts[1].parse::<u64>()
        .with_context(|| format!("Invalid end value: {}", parts[1]))?;

    if start > end {
        anyhow::bail!("Invalid hash range: start ({}) > end ({})", start, end);
    }

    Ok((start, end))
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"))
        )
        .init();

    // Parse CLI arguments
    let args = Args::parse();

    info!("Starting Graphica shard server");
    info!("Data path: {:?}", args.data_path);
    info!("Port: {}", args.port);

    // Create data directory if it doesn't exist
    std::fs::create_dir_all(&args.data_path)
        .with_context(|| format!("Failed to create data directory: {:?}", args.data_path))?;

    // Initialize Oxigraph store early (needed by both registration and shard)
    info!("Opening Oxigraph store at {:?}", args.data_path);
    let store = Arc::new(Store::open(&args.data_path)
        .with_context(|| format!("Failed to open Oxigraph store at {:?}", args.data_path))?);
    info!("Store opened successfully");

    // Initialize shard state flags early (needed by both registration and shard)
    let is_shutting_down = Arc::new(AtomicBool::new(false));
    let is_readonly = Arc::new(AtomicBool::new(false));

    // Determine mode: auto-registration or legacy
    let (shard_id, hash_range, registration_client) = if let Some(coordinator_url) = args.coordinator_url {
        // AUTO-REGISTRATION MODE
        info!("=== AUTO-REGISTRATION MODE ===");
        info!("Coordinator URL: {}", coordinator_url);
        info!("Heartbeat interval: {}s", args.heartbeat_interval);

        // Load or create shard identity
        let identity = ShardIdentity::load_or_create(&args.data_path)
            .context("Failed to load or create shard identity")?;

        info!("Machine ID: {}", identity.machine_id());

        // Create registration client and register with coordinator
        let shard_address = format!("{}:{}",
            if args.bind == "0.0.0.0" { "localhost" } else { &args.bind },
            args.port
        );

        let mut client = RegistrationClient::new(
            coordinator_url,
            std::time::Duration::from_secs(args.heartbeat_interval),
            shard_address,
            identity,
            store.clone(),
            is_shutting_down.clone(),
            is_readonly.clone(),
        )
        .await
        .context("Failed to create registration client")?;

        // Register with coordinator
        let (assigned_shard_id, assigned_hash_range) = client.register()
            .await
            .context("Failed to register with coordinator")?;

        info!("Registration successful!");
        info!("  Shard ID: {}", assigned_shard_id);
        info!("  Hash range: {:?}", assigned_hash_range);

        // Store the client for heartbeat loop after server starts
        // We'll spawn the heartbeat loop after the shard is created
        let stored_client = Some(client);

        (assigned_shard_id, assigned_hash_range, stored_client)
    } else if let Some(shard_id) = args.shard_id {
        // LEGACY MODE
        info!("=== LEGACY MODE ===");
        info!("Shard ID: {}", shard_id);

        // Parse hash range or use default (full range)
        let hash_range = if let Some(range_str) = &args.hash_range {
            info!("Hash range: {}", range_str);
            parse_hash_range(range_str)?
        } else {
            info!("Hash range: auto (defaults to 0-{}, will be assigned by coordinator)", u64::MAX);
            (0, u64::MAX)
        };
        info!("Parsed hash range: {:?}", hash_range);

        (shard_id, hash_range, None)
    } else {
        // Neither mode specified - error
        anyhow::bail!(
            "Must specify either --coordinator-url (auto-registration) or --shard-id (legacy mode)"
        );
    };

    info!("Final configuration:");
    info!("  Shard ID: {}", shard_id);
    info!("  Hash range: {:?}", hash_range);

    // Create metrics collector
    let metrics = ShardMetricsCollector::new();
    info!("Metrics collector initialized");

    // Create shard server with the pre-initialized store and state flags
    let shard = GraphicaShard::new(
        shard_id,
        hash_range,
        args.data_path.clone(),
        store.clone(),
        metrics.clone(),
        is_shutting_down.clone(),
        is_readonly.clone(),
    )?;

    // Start heartbeat loop if in auto-registration mode
    if let Some(client) = registration_client {
        info!("Starting heartbeat loop in background");

        // Clone the store for triple counting in heartbeat
        let store_for_heartbeat = shard.store.clone();
        let metrics_for_heartbeat = metrics.clone();

        tokio::spawn(async move {
            let triple_count_fn = move || {
                // Use SPARQL COUNT(*) query for efficient counting
                let count_query = "SELECT (COUNT(*) as ?count) WHERE { ?s ?p ?o }";
                match store_for_heartbeat.query(count_query) {
                    Ok(QueryResults::Solutions(mut solutions)) => {
                        if let Some(Ok(solution)) = solutions.next() {
                            if let Some(Term::Literal(lit)) = solution.get("count") {
                                return lit.value().parse::<u64>().unwrap_or(0);
                            }
                        }
                        0
                    }
                    _ => 0,
                }
            };

            if let Err(e) = client.start_heartbeat_loop(triple_count_fn, metrics_for_heartbeat).await {
                error!("Heartbeat loop failed: {}", e);
            }
        });

        info!("Heartbeat loop started successfully");
    }

    // Create gRPC server
    let addr = format!("{}:{}", args.bind, args.port).parse()?;
    info!("Starting gRPC server on {}", addr);

    // Build reflection service descriptor
    let reflection_service = tonic_reflection::server::Builder::configure()
        .register_encoded_file_descriptor_set(graphica_core::distributed::proto::FILE_DESCRIPTOR_SET)
        .build()
        .context("Failed to build reflection service")?;

    // Build server with reflection support
    info!("Reflection service enabled for debugging");

    // Configure ShardServiceServer with 100MB message limits to match coordinator
    let shard_service = ShardServiceServer::new(shard)
        .max_decoding_message_size(100 * 1024 * 1024) // 100MB
        .max_encoding_message_size(100 * 1024 * 1024); // 100MB

    // Build gRPC server
    Server::builder()
        .add_service(shard_service)
        .add_service(reflection_service)
        .serve_with_shutdown(addr, shutdown_signal())
        .await?;

    Ok(())
}

/// Graceful shutdown signal handler
async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("Failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("Failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {
            info!("Received Ctrl+C, shutting down");
        }
        _ = terminate => {
            info!("Received SIGTERM, shutting down");
        }
    }
}
