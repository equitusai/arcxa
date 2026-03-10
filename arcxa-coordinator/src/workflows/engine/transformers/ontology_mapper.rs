//! Ontology Mapping Transformer
//!
//! Maps source CSV fields to ontological semantic fields using the MappingEngine.
//!
//! ## Use Case
//!
//! When ingesting CSV files with arbitrary field names (`fname1`, `first_name`, `fstname`),
//! this transformer resolves them to standard ontological fields (`customer.first_name`)
//! for:
//! - Consistent model invocation (models use ontological field names)
//! - Cross-source lineage (track by semantic meaning, not source names)
//! - Data quality rules (validate ontological fields)
//!
//! ## Configuration
//!
//! **Required Parameters:**
//! - `source_id`: Source system identifier (e.g., "csv_customers")
//! - `table_name`: Table/file name (e.g., "customers")
//!
//! **Optional Parameters:**
//! - `min_confidence`: Minimum confidence threshold for mappings (default: 0.5, range: 0.0-1.0)
//! - `top_k`: Number of candidate mappings to consider (default: 1, range: 1-10)
//! - `auto_approve_threshold`: Confidence threshold for auto-approval (default: 0.85)
//! - `session_id`: Mapping session ID to load approved mappings (default: null)
//!
//! ## Mapping Priority
//!
//! The transformer applies mappings in the following priority order:
//! 1. Manual mappings (from ManualMappingStore) - highest priority
//! 2. Session-approved mappings (from MappingSession) - priority 2
//! 3. Automatic statistical matching - priority 3
//! 4. Unmapped namespace fallback - lowest priority
//!
//! ## Example
//!
//! ```rust,ignore
//! use graphica_coordinator::workflows::engine::transformers::*;
//! use serde_json::json;
//!
//! # async fn example(mapping_engine: Arc<MappingEngine>) -> anyhow::Result<()> {
//! let transformer = OntologyMapperTransformer::new(mapping_engine);
//!
//! let mut data = json!({
//!     "rows": [
//!         {"fname1": "John", "email_addr": "john@example.com"},
//!         {"fname1": "Jane", "email_addr": "jane@example.com"}
//!     ],
//!     "schema": {
//!         "fields": [
//!             {"name": "fname1", "type": "VARCHAR"},
//!             {"name": "email_addr", "type": "VARCHAR"}
//!         ]
//!     }
//! });
//!
//! let config = json!({
//!     "source_id": "csv_customers",
//!     "table_name": "customers",
//!     "session_id": "session_abc123",  // Optional: load approved mappings
//!     "min_confidence": 0.6,
//!     "auto_approve_threshold": 0.9
//! });
//!
//! transformer.transform(&config, &mut data).await?;
//!
//! // Data now has ontological field names:
//! // {
//! //   "rows": [
//! //     {"customer.first_name": "John", "customer.email": "john@example.com"},
//! //     ...
//! //   ],
//! //   "ontology_mapping": {
//! //     "fname1": {
//! //       "ontology_field": "customer.first_name",
//! //       "confidence": 0.95,
//! //       "source": "session",
//! //       "session_id": "session_abc123"
//! //     },
//! //     "email_addr": {
//! //       "ontology_field": "customer.email",
//! //       "confidence": 0.98,
//! //       "source": "automatic"
//! //     }
//! //   }
//! // }
//! # Ok(())
//! # }
//! ```

use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use serde_json::{json, Value as JsonValue};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, info, warn};

use super::Transformer;
use crate::mapping::{types::*, MappingEngine};
use graphica_core::core::lineage::column_level::{
    ColumnLineageEvent, ColumnRef, TransformationType,
};

// ============================================================================
// Ontology Mapper Transformer
// ============================================================================

/// Transformer that maps source fields to ontological semantic fields
pub struct OntologyMapperTransformer {
    /// Mapping engine for semantic field resolution
    mapping_engine: Arc<MappingEngine>,
    /// Column lineage store for tracking field-to-ontology mappings
    column_lineage_store:
        Option<Arc<dyn graphica_core::core::lineage::column_level::ColumnLineageSink>>,
}

impl OntologyMapperTransformer {
    /// Create a new ontology mapper transformer
    pub fn new(mapping_engine: Arc<MappingEngine>) -> Self {
        Self {
            mapping_engine,
            column_lineage_store: None,
        }
    }

    /// Set the column lineage store for tracking semantic mappings
    pub fn with_column_lineage_store(
        mut self,
        store: Arc<dyn graphica_core::core::lineage::column_level::ColumnLineageSink>,
    ) -> Self {
        self.column_lineage_store = Some(store);
        self
    }

    /// Extract field schema from data
    fn extract_schema_fields(&self, data: &JsonValue) -> Result<Vec<SchemaFieldInput>> {
        // Try to get schema from data.schema
        if let Some(schema) = data.get("schema") {
            if let Some(fields) = schema.get("fields").and_then(|f| f.as_array()) {
                let mut schema_fields = Vec::new();

                for field in fields {
                    let name = field
                        .get("name")
                        .and_then(|n| n.as_str())
                        .ok_or_else(|| anyhow!("Field missing 'name'"))?
                        .to_string();

                    let data_type = field
                        .get("type")
                        .and_then(|t| t.as_str())
                        .unwrap_or("VARCHAR")
                        .to_string();

                    let nullable = field
                        .get("nullable")
                        .and_then(|n| n.as_bool())
                        .unwrap_or(true);

                    // Extract sample values from rows if available
                    let sample_values = self.extract_sample_values(data, &name);

                    schema_fields.push(SchemaFieldInput {
                        name,
                        data_type,
                        nullable,
                        sample_values,
                        description: None,
                    });
                }

                return Ok(schema_fields);
            }
        }

        // Fallback: Infer schema from first row
        if let Some(rows) = data.get("rows").and_then(|r| r.as_array()) {
            if let Some(first_row) = rows.first() {
                if let Some(obj) = first_row.as_object() {
                    let mut schema_fields = Vec::new();

                    for (name, value) in obj.iter() {
                        let data_type = infer_type_from_value(value);
                        let sample_values = self.extract_sample_values(data, name);

                        schema_fields.push(SchemaFieldInput {
                            name: name.clone(),
                            data_type,
                            nullable: true,
                            sample_values,
                            description: None,
                        });
                    }

                    return Ok(schema_fields);
                }
            }
        }

        Err(anyhow!(
            "Could not extract schema from data - no 'schema' or 'rows' found"
        ))
    }

    /// Extract sample values for a field from rows
    fn extract_sample_values(&self, data: &JsonValue, field_name: &str) -> Option<Vec<String>> {
        let rows = data.get("rows")?.as_array()?;

        let mut samples = Vec::new();
        for row in rows.iter().take(10) {
            if let Some(value) = row.get(field_name) {
                if let Some(s) = value.as_str() {
                    samples.push(s.to_string());
                } else if !value.is_null() {
                    samples.push(value.to_string());
                }
            }
        }

        if samples.is_empty() {
            None
        } else {
            Some(samples)
        }
    }

    /// Map rows from source fields to ontological fields
    fn map_rows(
        &self,
        rows: &[JsonValue],
        field_mapping: &HashMap<String, String>,
        entity_uri: Option<&str>,
    ) -> Vec<JsonValue> {
        let mut mapped_rows = Vec::new();

        for row in rows {
            if let Some(obj) = row.as_object() {
                let mut mapped_obj = serde_json::Map::new();

                for (source_field, value) in obj.iter() {
                    let target_field = field_mapping
                        .get(source_field)
                        .cloned()
                        .unwrap_or_else(|| source_field.clone());

                    mapped_obj.insert(target_field, value.clone());
                }

                // Inject entity URI metadata if provided (enables ontology-driven loading with DDL auto-generation)
                if let Some(uri) = entity_uri {
                    tracing::debug!("Injecting __entity_uri__ metadata: {}", uri);
                    mapped_obj.insert(
                        "__entity_uri__".to_string(),
                        JsonValue::String(uri.to_string()),
                    );
                }

                mapped_rows.push(JsonValue::Object(mapped_obj));
            }
        }

        mapped_rows
    }
}

#[async_trait]
impl Transformer for OntologyMapperTransformer {
    async fn transform(
        &self,
        config: &JsonValue,
        data: &mut JsonValue,
        context: Option<&crate::workflows::engine::executor::ExecutionContext>,
    ) -> Result<()> {
        info!("Starting ontology mapping transformation");

        // Extract configuration
        let source_id = config
            .get("source_id")
            .and_then(|s| s.as_str())
            .ok_or_else(|| anyhow!("Missing required config: source_id"))?
            .to_string();

        let table_name = config
            .get("table_name")
            .and_then(|t| t.as_str())
            .ok_or_else(|| anyhow!("Missing required config: table_name"))?
            .to_string();

        // Task 2.2: Extract configurable thresholds (with sensible defaults)
        let min_confidence = config
            .get("min_confidence")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.5);

        let top_k = config.get("top_k").and_then(|v| v.as_u64()).unwrap_or(1) as usize;

        // Task 2.3: Auto-approval threshold for high-confidence mappings
        let auto_approve_threshold = config
            .get("auto_approve_threshold")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.85);

        // Task 2.4: Optional session_id for loading approved mappings from mapping session
        let session_id = config
            .get("session_id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        // Extract optional entity_uri for ontology-driven loading
        debug!(
            "Semantic mapper config: {}",
            serde_json::to_string_pretty(config).unwrap_or_else(|_| "invalid".to_string())
        );
        let entity_uri = config
            .get("entity_uri")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        if let Some(ref uri) = entity_uri {
            info!(
                "✓ Entity URI configured for ontology-driven loading: {}",
                uri
            );
        } else {
            info!("✗ No entity URI configured - ontology-driven loading will NOT be triggered");
        }

        // Validate configuration parameters
        if min_confidence < 0.0 || min_confidence > 1.0 {
            return Err(anyhow!(
                "Invalid min_confidence: {}. Must be between 0.0 and 1.0",
                min_confidence
            ));
        }

        if top_k < 1 || top_k > 10 {
            return Err(anyhow!(
                "Invalid top_k: {}. Must be between 1 and 10",
                top_k
            ));
        }

        if auto_approve_threshold < 0.0 || auto_approve_threshold > 1.0 {
            return Err(anyhow!(
                "Invalid auto_approve_threshold: {}. Must be between 0.0 and 1.0",
                auto_approve_threshold
            ));
        }

        if auto_approve_threshold <= min_confidence {
            return Err(anyhow!(
                "Invalid configuration: auto_approve_threshold ({}) must be greater than min_confidence ({})",
                auto_approve_threshold,
                min_confidence
            ));
        }

        debug!(
            "Configuration validated: source_id={}, table={}, min_confidence={}, top_k={}, auto_approve={}, session_id={:?}",
            source_id, table_name, min_confidence, top_k, auto_approve_threshold, session_id
        );

        // Extract schema from data
        let schema_fields = self
            .extract_schema_fields(data)
            .context("Failed to extract schema from data")?;

        debug!("Extracted {} fields from schema", schema_fields.len());

        // Analyze schema using mapping engine
        let analyze_request = AnalyzeSchemaRequest {
            source_id: source_id.clone(),
            table_name: table_name.clone(),
            fields: schema_fields,
            sample_size: Some(100),
        };

        let analyze_response = self
            .mapping_engine
            .analyze_schema(analyze_request)
            .await
            .context("Failed to analyze schema with mapping engine")?;

        info!(
            "Analyzed {} fields in {}ms",
            analyze_response.fields.len(),
            analyze_response.processing_time_ms
        );

        // Task 2.4: Load approved mappings from session if session_id provided
        let mut session_mappings: HashMap<String, (String, f64, String)> = HashMap::new();
        if let Some(ref session_id_str) = session_id {
            info!("Loading approved mappings from session: {}", session_id_str);

            // Access MappingEngine storage to get session
            if let Ok(Some(mapping_session)) =
                self.mapping_engine.storage.get_session(session_id_str)
            {
                // Extract approved field mappings from the session
                for table in &mapping_session.tables {
                    if table.table_name == table_name {
                        for field_state in &table.field_mappings {
                            // Only use approved or auto-approved mappings
                            if matches!(
                                field_state.approval_status,
                                FieldApprovalStatus::Approved | FieldApprovalStatus::AutoApproved
                            ) {
                                if let Some(ref selected) = field_state.selected_mapping {
                                    // Store: field_name -> (ontology_uri, confidence, approval_status)
                                    session_mappings.insert(
                                        field_state.field_name.clone(),
                                        (
                                            selected.ontology_term_uri.clone(),
                                            selected.confidence,
                                            format!("{:?}", field_state.approval_status),
                                        ),
                                    );

                                    debug!(
                                        "Loaded session mapping: '{}' -> '{}' (confidence: {:.2}, status: {:?})",
                                        field_state.field_name,
                                        selected.ontology_term_uri,
                                        selected.confidence,
                                        field_state.approval_status
                                    );
                                }
                            }
                        }
                    }
                }

                info!(
                    "Loaded {} approved mappings from session",
                    session_mappings.len()
                );
            } else {
                warn!(
                    "Session not found: {}, will use automatic mapping",
                    session_id_str
                );
            }
        }

        // Get mapping candidates for each field (with manual mapping priority - Task 1.2)
        let mut field_mapping: HashMap<String, String> = HashMap::new();
        let mut ontology_mapping = serde_json::Map::new();

        // TODO(ontology-mapper): Add batch retrieval for manual mappings to reduce DB calls
        for field in &analyze_response.fields {
            // Task 1.2: Check manual mapping store first
            let mut manual_mapping_applied = false;
            if let Some(context_ref) = context {
                if let Some(manual_store) = &context_ref.manual_mapping_store {
                    // Build source context for manual mapping lookup
                    let source_context = crate::mapping::manual::SourceContext {
                        source_id: Some(source_id.clone()),
                        table_name: table_name.clone(),
                        field_name: field.name.clone(),
                        field_metadata: None,
                    };

                    // Query manual mapping store
                    if let Ok(Some(manual_mapping)) =
                        manual_store.find_by_source(&source_context).await
                    {
                        debug!(
                            "Manual mapping found for '{}' -> '{}' (confidence: {:.2})",
                            field.name, manual_mapping.target_field_uri, manual_mapping.confidence
                        );

                        // Extract ontology property name from URI
                        let target_field = crate::mapping::uri_utils::extract_local_name(
                            &manual_mapping.target_field_uri,
                        )
                        .unwrap_or_else(|| manual_mapping.target_field_uri.clone());

                        // Apply manual mapping with confidence 1.0
                        field_mapping.insert(field.name.clone(), target_field.clone());

                        ontology_mapping.insert(
                            field.name.clone(),
                            json!({
                                "ontology_field": target_field,
                                "confidence": manual_mapping.confidence,
                                "source": "manual",
                                "mapping_id": manual_mapping.id,
                                "explanation": format!("Manual mapping by {}", manual_mapping.created_by),
                            }),
                        );

                        manual_mapping_applied = true;

                        info!(
                            "Applied manual mapping: '{}' -> '{}' (id: {})",
                            field.name, target_field, manual_mapping.id
                        );
                    }
                }
            }

            // Task 2.4: Check session-approved mappings (priority 2, after manual)
            let mut session_mapping_applied = false;
            if !manual_mapping_applied {
                if let Some((ontology_uri, confidence, approval_status)) =
                    session_mappings.get(&field.name)
                {
                    debug!(
                        "Session mapping found for '{}' -> '{}' (confidence: {:.2}, status: {})",
                        field.name, ontology_uri, confidence, approval_status
                    );

                    // Extract ontology property name from URI
                    let target_field = crate::mapping::uri_utils::extract_local_name(ontology_uri)
                        .unwrap_or_else(|| ontology_uri.clone());

                    // Apply session mapping
                    field_mapping.insert(field.name.clone(), target_field.clone());

                    ontology_mapping.insert(
                        field.name.clone(),
                        json!({
                            "ontology_field": target_field,
                            "confidence": confidence,
                            "source": "session",
                            "status": approval_status.to_lowercase(),
                            "session_id": session_id.as_ref().unwrap(),
                            "explanation": format!("Approved mapping from session (status: {})", approval_status),
                        }),
                    );

                    session_mapping_applied = true;

                    info!(
                        "Applied session mapping: '{}' -> '{}' (status: {})",
                        field.name, target_field, approval_status
                    );
                }
            }

            // Fallback to automatic mapping if no manual or session mapping exists
            if !manual_mapping_applied && !session_mapping_applied {
                // TODO(ontology-mapper): Cache manual mappings per source_id+table_name for performance
                // Task 2.2: Use configurable thresholds instead of hardcoded values
                let response = self
                    .mapping_engine
                    .get_candidates(&field.id, top_k, min_confidence, None)
                    .await
                    .context(format!("Failed to get candidates for field {}", field.name))?;

                // Task 2.3: Smart fallback with multi-tier strategy
                if let Some(candidate) = response.candidates.first() {
                    // Determine mapping status based on confidence
                    let mapping_status = if candidate.confidence >= auto_approve_threshold {
                        "auto_approved"
                    } else if candidate.confidence >= min_confidence {
                        "requires_review"
                    } else {
                        "low_confidence"
                    };

                    // Map: source_field_name -> ontology_field_name (extract local name from URI)
                    // CRITICAL FIX: Preserve full ontology URI for proper lineage tracking
                    // Extract local name for field naming, but preserve URI in metadata
                    let target_field =
                        crate::mapping::uri_utils::extract_local_name(&candidate.ontology_term_uri)
                            .unwrap_or_else(|| candidate.ontology_term_uri.clone());
                    field_mapping.insert(field.name.clone(), target_field.clone());

                    // Store mapping metadata for lineage with status
                    ontology_mapping.insert(
                        field.name.clone(),
                        json!({
                            "ontology_field": target_field.clone(),
                            "ontology_uri": candidate.ontology_term_uri.clone(),
                            "confidence": candidate.confidence,
                            "source": "automatic",
                            "status": mapping_status,
                            "confidence_breakdown": {
                                "statistical": candidate.confidence_breakdown.statistical,
                                "semantic": candidate.confidence_breakdown.semantic,
                                "graph": candidate.confidence_breakdown.graph,
                            },
                            "explanation": candidate.explanation,
                            "alternatives": if response.candidates.len() > 1 {
                                Some(response.candidates[1..].iter().take(3).map(|c| {
                                    json!({
                                        "field": c.ontology_term_uri,
                                        "confidence": c.confidence,
                                    })
                                }).collect::<Vec<_>>())
                            } else {
                                None
                            },
                        }),
                    );

                    if mapping_status == "requires_review" {
                        info!(
                            "Mapped '{}' -> '{}' (URI: {}) (confidence: {:.2}, status: {})",
                            field.name,
                            target_field,
                            candidate.ontology_term_uri,
                            candidate.confidence,
                            mapping_status
                        );
                    } else {
                        debug!(
                            "Mapped '{}' -> '{}' (URI: {}) (confidence: {:.2}, status: {})",
                            field.name,
                            target_field,
                            candidate.ontology_term_uri,
                            candidate.confidence,
                            mapping_status
                        );
                    }
                } else {
                    // Task 2.3: Ultimate fallback - use unmapped namespace
                    warn!(
                        "No mapping found for field '{}' - using unmapped namespace",
                        field.name
                    );
                    let unmapped_field = format!("unmapped.{}", field.name);
                    field_mapping.insert(field.name.clone(), unmapped_field.clone());

                    ontology_mapping.insert(
                        field.name.clone(),
                        json!({
                            "ontology_field": unmapped_field,
                            "confidence": 0.0,
                            "source": "fallback",
                            "status": "unmapped",
                            "explanation": "No ontology mapping found - preserved in unmapped namespace",
                            "original_field": field.name,
                        }),
                    );
                }
            }
        }

        // Transform rows using field mapping
        if let Some(rows) = data.get("rows").and_then(|r| r.as_array()) {
            let row_count = rows.len();
            let mapped_rows = self.map_rows(rows, &field_mapping, entity_uri.as_deref());
            data["rows"] = JsonValue::Array(mapped_rows);
            debug!("Mapped {} rows to ontological fields", row_count);
            if let Some(ref uri) = entity_uri {
                info!(
                    "Injected entity URI metadata into {} rows: {}",
                    row_count, uri
                );
            }
        }

        // Add ontology mapping metadata to data
        data["ontology_mapping"] = JsonValue::Object(ontology_mapping.clone());

        info!(
            "Ontology mapping complete: {} fields mapped",
            field_mapping.len()
        );

        // Task 1.3: Record field-level lineage transformations
        if let Some(ctx) = context {
            if let Some(lineage_gen) = &ctx.lineage_generator {
                if let Some(exec_id) = &ctx.execution_id {
                    // Build FieldModification records for lineage
                    let mut field_modifications = Vec::new();

                    for (source_field, target_field) in &field_mapping {
                        // Only record if field name changed (actual transformation)
                        if source_field != target_field {
                            // Extract confidence and ontology URI from ontology_mapping metadata
                            let confidence = ontology_mapping
                                .get(source_field)
                                .and_then(|m| m.get("confidence"))
                                .and_then(|c| c.as_f64())
                                .unwrap_or(0.0);

                            // CRITICAL FIX: Use full ontology URI in lineage, not just local name
                            let ontology_uri = ontology_mapping
                                .get(source_field)
                                .and_then(|m| m.get("ontology_uri"))
                                .and_then(|u| u.as_str())
                                .unwrap_or(target_field.as_str());

                            field_modifications.push(
                                crate::workflows::lineage::rdf::FieldModification {
                                    field_name: source_field.clone(),
                                    old_value: json!(source_field),
                                    new_value: json!(ontology_uri), // Use full URI instead of local name
                                    confidence,
                                    is_reversible: true,
                                },
                            );
                        }
                    }

                    // Record to RDF lineage if any transformations occurred
                    if !field_modifications.is_empty() {
                        // TODO(ontology-mapper): Add field lineage chaining (track multi-step transformations)
                        // TODO(ontology-mapper): Add provenance for mapping algorithm version
                        // TODO(ontology-mapper): Link to training data used for semantic matching
                        if let Err(e) = lineage_gen.record_field_transformations(
                            exec_id,
                            &ctx.workflow_id,
                            &ctx.route_id,
                            ctx.action_index,
                            "ontology_mapper",
                            field_modifications.clone(),
                        ) {
                            warn!(
                                "Failed to record field lineage for {} transformations: {}",
                                field_modifications.len(),
                                e
                            );
                        } else {
                            debug!(
                                "Recorded {} field transformations to RDF lineage",
                                field_modifications.len()
                            );
                        }
                    }

                    // Task 2.4: Record session transformation statistics (if session_id provided)
                    if let Some(ref sid) = session_id {
                        let fields_used = session_mappings.len();
                        let success = field_modifications.len() > 0; // Success if we recorded transformations

                        if let Err(e) = self.mapping_engine.storage.record_session_transformation(
                            sid,
                            fields_used,
                            success,
                        ) {
                            warn!(
                                "Failed to record session transformation stats for {}: {}",
                                sid, e
                            );
                        } else {
                            debug!(
                                "Recorded session transformation stats: session_id={}, fields_used={}, success={}",
                                sid, fields_used, success
                            );
                        }
                    }
                }
            }
        }

        // Record column-level lineage for semantic mappings
        // Use self.column_lineage_store since it may not be in context when called via callback
        info!(
            "DEBUG: About to check column_lineage_store. Is Some: {}",
            self.column_lineage_store.is_some()
        );
        if let Some(column_store) = &self.column_lineage_store {
            info!("Column lineage store is available, preparing to record events");

            // Get execution context values from config (passed by core executor) or context, with defaults
            let job_id = config
                .get("job_id")
                .and_then(|v| v.as_str())
                .map(String::from)
                .or_else(|| context.and_then(|c| c.execution_id.clone()))
                .unwrap_or_else(|| "unknown".to_string());
            let tenant_id = config
                .get("tenant_id")
                .and_then(|v| v.as_str())
                .map(String::from)
                .or_else(|| context.map(|c| c.tenant_id.clone()))
                .unwrap_or_else(|| "default".to_string());
            let workflow_id = config
                .get("execution_id")
                .and_then(|v| v.as_str())
                .map(String::from)
                .or_else(|| context.map(|c| c.workflow_id.clone()))
                .unwrap_or_else(|| "unknown".to_string());

            info!(
                "Column lineage metadata: job_id={}, tenant_id={}, workflow_id={}",
                job_id, tenant_id, workflow_id
            );

            let mut column_events = Vec::new();

            for (source_field, target_field) in &field_mapping {
                // Only record if field name changed (actual transformation)
                if source_field != target_field {
                    // Extract confidence, mapping source, and ontology URI from ontology_mapping metadata
                    let (confidence, mapping_source, ontology_uri) = ontology_mapping
                        .get(source_field)
                        .map(|m| {
                            let conf = m.get("confidence").and_then(|c| c.as_f64()).unwrap_or(0.0);
                            let src = m
                                .get("source")
                                .and_then(|s| s.as_str())
                                .unwrap_or("automatic")
                                .to_string();
                            let uri = m
                                .get("ontology_uri")
                                .and_then(|u| u.as_str())
                                .unwrap_or(target_field)
                                .to_string();
                            (conf, src, uri)
                        })
                        .unwrap_or((0.0, "automatic".to_string(), target_field.clone()));

                    // Create source column reference
                    let source_col = ColumnRef::new(
                        source_id.clone(),
                        table_name.clone(),
                        source_field.clone(),
                        "VARCHAR", // Default type, could be enhanced with actual type info
                    );

                    // Create target column reference (ontology-mapped)
                    // CRITICAL FIX: Use full ontology URI instead of local name
                    let target_col = ColumnRef::new(
                        "ontology",
                        table_name.clone(),
                        ontology_uri.clone(), // Use full URI
                        "VARCHAR",
                    );

                    // Create column lineage event
                    let event = ColumnLineageEvent::new(
                        vec![source_col],
                        target_col,
                        format!(
                            "Semantic mapping: {} -> {} (source: {})",
                            source_field, target_field, mapping_source
                        ),
                        TransformationType::DirectCopy, // Semantic mapping is a direct copy with renaming
                        job_id.clone(),
                        tenant_id.clone(),
                        "ontology_mapper".to_string(),
                    )
                    .with_confidence(confidence)
                    .with_workflow(workflow_id.clone())
                    .with_metadata(json!({
                        "mapping_source": mapping_source,
                        "source_field": source_field,
                        "target_field": target_field,
                        "ontology_uri": ontology_uri, // CRITICAL FIX: Include full ontology URI in metadata
                    }));

                    column_events.push(event);
                }
            }

            // Record all column lineage events
            if !column_events.is_empty() {
                let event_count = column_events.len();
                info!("About to record {} column lineage events", event_count);
                match column_store
                    .record_column_lineage_batch(column_events)
                    .await
                {
                    Ok(()) => {
                        info!(
                            "SUCCESS: Recorded {} column lineage events for semantic mappings",
                            event_count
                        );
                    }
                    Err(e) => {
                        warn!(
                            "Failed to record column lineage for {} mappings: {}",
                            event_count, e
                        );
                    }
                }
            } else {
                warn!("No column lineage events to record - field_mapping might be empty");
            }
        }

        // CRITICAL FIX: Add _modifications metadata for step-level lineage tracking
        // This enables the WorkflowExecutor to record semantic mapping transformations
        let mut modifications = Vec::new();
        for (source_field, target_field) in &field_mapping {
            // Only record if field name changed (actual transformation)
            if source_field != target_field {
                let confidence = ontology_mapping
                    .get(source_field)
                    .and_then(|m| m.get("confidence"))
                    .and_then(|c| c.as_f64())
                    .unwrap_or(0.0);

                let ontology_uri = ontology_mapping
                    .get(source_field)
                    .and_then(|m| m.get("ontology_uri"))
                    .and_then(|u| u.as_str())
                    .unwrap_or(target_field.as_str());

                modifications.push(json!({
                    "field_name": source_field,
                    "old_value": source_field,
                    "new_value": ontology_uri, // Use full ontology URI
                    "confidence": confidence,
                    "is_reversible": true,
                    "operations": 1,
                }));
            }
        }

        // Add modifications array to data output
        data["_modifications"] = json!(modifications);
        info!(
            "Added {} field modifications to output for step-level lineage",
            modifications.len()
        );

        Ok(())
    }

    fn name(&self) -> &'static str {
        "ontology_mapper"
    }

    fn validate_config(&self, config: &JsonValue) -> Result<()> {
        // Validate required fields
        if config.get("source_id").and_then(|s| s.as_str()).is_none() {
            return Err(anyhow!("Missing required config field: source_id"));
        }

        if config.get("table_name").and_then(|t| t.as_str()).is_none() {
            return Err(anyhow!("Missing required config field: table_name"));
        }

        Ok(())
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Infer SQL data type from JSON value
fn infer_type_from_value(value: &JsonValue) -> String {
    match value {
        JsonValue::Null => "VARCHAR".to_string(),
        JsonValue::Bool(_) => "BOOLEAN".to_string(),
        JsonValue::Number(n) => {
            if n.is_f64() {
                "DECIMAL".to_string()
            } else {
                "INTEGER".to_string()
            }
        }
        JsonValue::String(s) => {
            // Check if it looks like a date
            if s.contains('-') && s.len() >= 10 {
                if let Ok(_) = chrono::NaiveDate::parse_from_str(&s[..10], "%Y-%m-%d") {
                    return "DATE".to_string();
                }
            }
            // Check if it looks like an email
            if s.contains('@') {
                return "VARCHAR".to_string();
            }
            "VARCHAR".to_string()
        }
        JsonValue::Array(_) => "JSON".to_string(),
        JsonValue::Object(_) => "JSON".to_string(),
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::governance::rdf_store::GraphicaRdfStore;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_ontology_mapper_basic() -> Result<()> {
        // Setup mapping engine
        let temp_dir = TempDir::new()?;
        let rdf_store = Arc::new(GraphicaRdfStore::new_in_memory()?);
        let mapping_engine =
            Arc::new(MappingEngine::new(temp_dir.path().to_str().unwrap(), rdf_store).await?);

        let transformer = OntologyMapperTransformer::new(mapping_engine);

        // Test data with CSV-like field names
        let mut data = json!({
            "rows": [
                {"fname": "John", "email_addr": "john@example.com"},
                {"fname": "Jane", "email_addr": "jane@example.com"}
            ],
            "schema": {
                "fields": [
                    {"name": "fname", "type": "VARCHAR"},
                    {"name": "email_addr", "type": "VARCHAR"}
                ]
            }
        });

        let config = json!({
            "source_id": "test_csv",
            "table_name": "customers"
        });

        transformer.transform(&config, &mut data, None).await?;

        // Verify ontology mapping was added
        assert!(data.get("ontology_mapping").is_some());
        let mapping = data["ontology_mapping"].as_object().unwrap();
        assert!(mapping.contains_key("fname"));
        assert!(mapping.contains_key("email_addr"));

        Ok(())
    }

    #[tokio::test]
    async fn test_schema_extraction() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let rdf_store = Arc::new(GraphicaRdfStore::new_in_memory()?);
        let mapping_engine =
            Arc::new(MappingEngine::new(temp_dir.path().to_str().unwrap(), rdf_store).await?);

        let transformer = OntologyMapperTransformer::new(mapping_engine);

        // Test schema extraction from explicit schema
        let data = json!({
            "rows": [{"id": 1}],
            "schema": {
                "fields": [
                    {"name": "id", "type": "INTEGER", "nullable": false}
                ]
            }
        });

        let fields = transformer.extract_schema_fields(&data)?;
        assert_eq!(fields.len(), 1);
        assert_eq!(fields[0].name, "id");
        assert_eq!(fields[0].data_type, "INTEGER");
        assert_eq!(fields[0].nullable, false);

        Ok(())
    }

    #[tokio::test]
    async fn test_type_inference() {
        assert_eq!(infer_type_from_value(&json!(42)), "INTEGER");
        assert_eq!(infer_type_from_value(&json!(3.14)), "DECIMAL");
        assert_eq!(infer_type_from_value(&json!(true)), "BOOLEAN");
        assert_eq!(infer_type_from_value(&json!("test")), "VARCHAR");
        assert_eq!(infer_type_from_value(&json!("2024-01-15")), "DATE");
        assert_eq!(infer_type_from_value(&json!("test@example.com")), "VARCHAR");
    }
}
