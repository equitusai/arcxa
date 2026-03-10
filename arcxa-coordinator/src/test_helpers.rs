//! Test helpers for graphica-coordinator tests
//!
//! Common utilities and fixtures for testing

#[cfg(test)]
use graphica_core::core::LineageEvent;

#[cfg(test)]
pub fn create_test_lineage_event(id: &str) -> LineageEvent {
    use graphica_core::core::{DataRef, TransformRef};

    LineageEvent {
        id: uuid::Uuid::new_v4(),
        dataset: "test_dataset".to_string(),
        record_id: format!("rec_{}", id),
        source_refs: vec![DataRef {
            system: "test_system".to_string(),
            path: format!("/data/source/{}", id),
            version: Some("v1".to_string()),
            extracted_at: chrono::Utc::now(),
            cdc_position: None,
        }],
        transforms: vec![TransformRef {
            id: uuid::Uuid::new_v4(),
            transform_type: "test".to_string(),
            rule_id: "test_rule".to_string(),
            version: "1.0.0".to_string(),
            parameters: std::collections::HashMap::new(),
            applied_at: chrono::Utc::now(),
            fields_modified: vec![],
        }],
        model_refs: vec![],
        output_ref: DataRef {
            system: "test_system".to_string(),
            path: format!("/data/output/{}", id),
            version: Some("v1".to_string()),
            extracted_at: chrono::Utc::now(),
            cdc_position: None,
        },
        ts: chrono::Utc::now(),
        run_id: "test_run".to_string(),
        tenant_id: "test_tenant".to_string(),
        correlation_id: Some(format!("corr_{}", id)),
        metadata: std::collections::HashMap::new(),
    }
}
