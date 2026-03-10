//! REST API Handler Functions
//!
//! This module contains all HTTP request handlers organized by domain.
//! Each sub-module handles a specific area of functionality.

pub mod datasets; // Dataset import handlers
pub mod datasets_async; // Async import background jobs
pub mod entities;
pub mod fusion;
pub mod health;
pub mod lineage;
pub mod manual_mapping; // Manual field-to-ontology mapping handlers
pub mod mapping; // Field mapping handlers
pub mod model_registry;
pub mod models;
pub mod monitoring;
pub mod persistent_model_registry; // RDF-backed model registry with persistence
pub mod quality;
pub mod rules;
pub mod sparql;
pub mod temporal;
pub mod wal;

// Re-export health handlers
pub use health::{
    health_check, liveness_check, metrics_endpoint, readiness_check, storage_health_check,
};

// Re-export lineage handlers
pub use lineage::{
    backward_root_cause, forward_impact_analysis, get_model_impact, get_model_lineage_rdf,
    get_model_training_data_as_of, get_record_lineage, get_record_lineage_as_of, query_lineage,
    simulate_change, write_lineage_events,
};

// Re-export quality handlers
pub use quality::{create_rule, get_rule, get_scorecard, list_violations};

// Re-export models handlers
pub use models::{get_model, record_predictions, register_model};

// Re-export entities handlers
pub use entities::{
    get_attribute_timeseries, get_dataset, get_entity, get_entity_attributes, get_entity_lineage,
    list_datasets, list_entities,
};

// Re-export dataset import handlers
pub use datasets::{
    batch_import_datasources, get_import_status, import_dataset, import_from_datasource,
    list_imports,
};

// Re-export fusion handlers
pub use fusion::{
    approve_fusion_candidate, calculate_match_confidence, format_fusion_candidate_triples,
    list_fusion_candidates, propose_fusion_candidates, reject_fusion_candidate,
    resolve_entity_fusion, reverse_entity_fusion,
};

// Re-export sparql handlers
pub use sparql::{get_rdf_auto_save_stats, get_rdf_stats, sparql_query, trigger_rdf_save};

// Re-export temporal handlers
pub use temporal::{
    analyze_temporal_chains, clear_temporal_cache, compact_temporal_indexes,
    create_temporal_checkpoint, get_temporal_statistics, get_temporal_summary,
};

// Re-export wal handlers
pub use wal::{get_wal_operations, get_wal_status, trigger_wal_replay};

// Re-export model registry handlers
pub use model_registry::{
    delete_model_handler, get_model_handler, list_models_handler, register_model_handler,
    update_model_handler,
};

// Re-export persistent model registry
pub use persistent_model_registry::PersistentModelRegistry;

// Re-export rule handlers
pub use rules::{
    clear_rule_cache_handler, execute_rule_handler, load_rule_handler, unload_rule_handler,
};

// Re-export mapping handlers
pub use mapping::{
    analyze_for_mapping, analyze_schema, apply_mappings, get_candidates, get_session,
    health_check as mapping_health_check, import_from_mappings, record_feedback, review_mappings,
};

// Re-export manual mapping handlers
pub use manual_mapping::{
    bulk_export as bulk_export_manual_mappings, bulk_import as bulk_import_manual_mappings,
    create_mapping as create_manual_mapping, delete_mapping as delete_manual_mapping,
    get_mapping as get_manual_mapping, suggest_mappings as suggest_manual_mappings,
    update_mapping as update_manual_mapping,
};

// Handler modules will be added incrementally during refactoring
// pub mod quality;
// pub mod models;
// pub mod entities;
// pub mod fusion;
// pub mod sparql;
// pub mod temporal;
// pub mod wal;
// pub mod auth;
// pub mod audit;
// pub mod connectors;
