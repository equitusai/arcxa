//! Application service for ontology binding lifecycle.

use anyhow::Result;
use chrono::Utc;
use graphica_core::catalog::api_types::SchemaDefinition;
use std::sync::Arc;
use uuid::Uuid;

use super::store::BindingStore;
use super::types::{
    BindingCoverageDiff, BindingStatus, OntologyPhysicalBinding, UpsertBindingRequest,
};

/// High-level use-cases for bindings.
pub struct BindingService {
    store: Arc<BindingStore>,
}

impl BindingService {
    pub fn new(store: Arc<BindingStore>) -> Self {
        Self { store }
    }

    pub fn store(&self) -> &Arc<BindingStore> {
        &self.store
    }

    pub fn upsert_binding(&self, request: UpsertBindingRequest) -> Result<OntologyPhysicalBinding> {
        let now = Utc::now().timestamp();
        let hash = OntologyPhysicalBinding::compute_hash(
            &request.table,
            &request.column,
            &request.dialect,
        );

        let existing = self.store.get_current_binding(
            &request.source_id,
            &request.entity_uri,
            &request.ontology_uri,
        )?;

        // Idempotent update when physical target is unchanged.
        if let Some(mut current) = existing {
            if current.binding_hash == hash {
                current.confidence = request.confidence;
                current.updated_at = now;
                current.updated_by = request.updated_by;
                current.provenance = request.provenance;
                self.store.put_binding(&current)?;
                return Ok(current);
            }

            let next = OntologyPhysicalBinding {
                id: format!("bind_{}", Uuid::new_v4()),
                source_id: request.source_id,
                entity_uri: request.entity_uri,
                ontology_uri: request.ontology_uri,
                table: request.table,
                column: request.column,
                dialect: request.dialect,
                confidence: request.confidence,
                status: BindingStatus::Active,
                version: current.version + 1,
                binding_hash: hash,
                created_at: current.created_at,
                updated_at: now,
                created_by: current.created_by,
                updated_by: request.updated_by,
                provenance: request.provenance,
            };
            self.store.put_binding(&next)?;
            return Ok(next);
        }

        let created = OntologyPhysicalBinding {
            id: format!("bind_{}", Uuid::new_v4()),
            source_id: request.source_id,
            entity_uri: request.entity_uri,
            ontology_uri: request.ontology_uri,
            table: request.table,
            column: request.column,
            dialect: request.dialect,
            confidence: request.confidence,
            status: BindingStatus::Active,
            version: 1,
            binding_hash: hash,
            created_at: now,
            updated_at: now,
            created_by: request.updated_by.clone(),
            updated_by: request.updated_by,
            provenance: request.provenance,
        };
        self.store.put_binding(&created)?;
        Ok(created)
    }

    pub fn get_current_bindings_for_goal(
        &self,
        source_id: &str,
        entity_uri: &str,
        required_properties: &[String],
    ) -> Result<Vec<OntologyPhysicalBinding>> {
        let mut bindings = Vec::with_capacity(required_properties.len());
        for property in required_properties {
            if let Some(binding) = self
                .store
                .get_current_binding(source_id, entity_uri, property)?
            {
                if binding.status == BindingStatus::Active {
                    bindings.push(binding);
                }
            }
        }
        Ok(bindings)
    }

    pub fn list_current_bindings(
        &self,
        source_id: &str,
        entity_uri: &str,
    ) -> Result<Vec<OntologyPhysicalBinding>> {
        self.store
            .list_current_bindings_for_entity(source_id, entity_uri)
    }

    pub fn list_binding_history(
        &self,
        source_id: &str,
        entity_uri: &str,
        ontology_uri: &str,
    ) -> Result<Vec<OntologyPhysicalBinding>> {
        self.store
            .list_binding_history(source_id, entity_uri, ontology_uri)
    }

    /// Mark bindings stale when their physical targets are not present in the inferred schema.
    ///
    /// Returns number of bindings transitioned to `stale`.
    pub fn mark_stale_bindings_for_schema(
        &self,
        source_id: &str,
        entity_uri: &str,
        schema: &SchemaDefinition,
        updated_by: &str,
    ) -> Result<usize> {
        let now = Utc::now().timestamp();
        let current = self.list_current_bindings(source_id, entity_uri)?;
        let mut stale_count = 0usize;

        for binding in current {
            if !binding_target_exists(schema, &binding.table, &binding.column)
                && binding.status != BindingStatus::Stale
            {
                let stale = OntologyPhysicalBinding {
                    id: format!("bind_{}", Uuid::new_v4()),
                    source_id: binding.source_id.clone(),
                    entity_uri: binding.entity_uri.clone(),
                    ontology_uri: binding.ontology_uri.clone(),
                    table: binding.table.clone(),
                    column: binding.column.clone(),
                    dialect: binding.dialect.clone(),
                    confidence: binding.confidence,
                    status: BindingStatus::Stale,
                    version: binding.version + 1,
                    binding_hash: binding.binding_hash.clone(),
                    created_at: binding.created_at,
                    updated_at: now,
                    created_by: binding.created_by.clone(),
                    updated_by: updated_by.to_string(),
                    provenance: binding.provenance.clone(),
                };
                self.store.put_binding(&stale)?;
                stale_count += 1;
            }
        }

        Ok(stale_count)
    }

    /// Diff ontology requirements against active bindings and optional schema presence.
    pub fn diff_coverage(
        &self,
        source_id: &str,
        entity_uri: &str,
        required_properties: &[String],
        schema: Option<&SchemaDefinition>,
    ) -> Result<BindingCoverageDiff> {
        let mut covered = Vec::new();
        let mut stale = Vec::new();
        let mut unmapped = Vec::new();

        for ontology_uri in required_properties {
            let current = self
                .store
                .get_current_binding(source_id, entity_uri, ontology_uri)?;

            match current {
                None => unmapped.push(ontology_uri.clone()),
                Some(binding) => {
                    if binding.status != BindingStatus::Active {
                        stale.push(ontology_uri.clone());
                        continue;
                    }

                    if let Some(schema) = schema {
                        if !binding_target_exists(schema, &binding.table, &binding.column) {
                            stale.push(ontology_uri.clone());
                            continue;
                        }
                    }

                    covered.push(ontology_uri.clone());
                }
            }
        }

        let mut missing = Vec::new();
        missing.extend(unmapped.iter().cloned());
        missing.extend(stale.iter().cloned());
        missing.sort();
        missing.dedup();

        let coverage_ratio = if required_properties.is_empty() {
            1.0
        } else {
            covered.len() as f64 / required_properties.len() as f64
        };

        Ok(BindingCoverageDiff {
            required_properties: required_properties.to_vec(),
            covered_properties: covered,
            missing_properties: missing,
            stale_properties: stale,
            unmapped_properties: unmapped,
            coverage_ratio,
        })
    }
}

fn binding_target_exists(schema: &SchemaDefinition, table: &str, column: &str) -> bool {
    schema.tables.iter().any(|t| {
        t.name.eq_ignore_ascii_case(table)
            && t.columns
                .iter()
                .any(|c| c.name.eq_ignore_ascii_case(column))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mapping::bindings::types::BindingProvenance;
    use chrono::Utc;
    use graphica_core::catalog::api_types::{ColumnDefinition, TableDefinition};

    #[test]
    fn bumps_version_when_physical_target_changes() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = Arc::new(BindingStore::new(dir.path()).expect("store"));
        let service = BindingService::new(store);

        let v1 = service
            .upsert_binding(UpsertBindingRequest {
                source_id: "src1".to_string(),
                entity_uri: "http://e/Patient".to_string(),
                ontology_uri: "http://e/name".to_string(),
                table: "patients".to_string(),
                column: "name".to_string(),
                dialect: "postgresql".to_string(),
                confidence: 0.9,
                updated_by: "tester".to_string(),
                provenance: BindingProvenance::default(),
            })
            .expect("v1");

        let v2 = service
            .upsert_binding(UpsertBindingRequest {
                source_id: "src1".to_string(),
                entity_uri: "http://e/Patient".to_string(),
                ontology_uri: "http://e/name".to_string(),
                table: "patient_master".to_string(),
                column: "full_name".to_string(),
                dialect: "postgresql".to_string(),
                confidence: 0.92,
                updated_by: "tester2".to_string(),
                provenance: BindingProvenance::default(),
            })
            .expect("v2");

        assert_eq!(v1.version, 1);
        assert_eq!(v2.version, 2);
        assert_ne!(v1.binding_hash, v2.binding_hash);
    }

    #[test]
    fn marks_binding_stale_when_schema_target_disappears() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = Arc::new(BindingStore::new(dir.path()).expect("store"));
        let service = BindingService::new(store);

        let created = service
            .upsert_binding(UpsertBindingRequest {
                source_id: "src1".to_string(),
                entity_uri: "http://e/Patient".to_string(),
                ontology_uri: "http://e/name".to_string(),
                table: "patients".to_string(),
                column: "name".to_string(),
                dialect: "postgresql".to_string(),
                confidence: 0.9,
                updated_by: "tester".to_string(),
                provenance: BindingProvenance::default(),
            })
            .expect("binding created");

        let schema_without_name = SchemaDefinition {
            name: "public".to_string(),
            tables: vec![TableDefinition {
                name: "patients".to_string(),
                columns: vec![ColumnDefinition {
                    name: "id".to_string(),
                    data_type: "INTEGER".to_string(),
                    nullable: false,
                    primary_key: true,
                    default_value: None,
                    semantic_type: None,
                    statistics: None,
                }],
                estimated_rows: Some(10),
            }],
            relationships: vec![],
            indexes: vec![],
            inferred_at: Utc::now(),
        };

        let stale = service
            .mark_stale_bindings_for_schema(
                "src1",
                "http://e/Patient",
                &schema_without_name,
                "system:test",
            )
            .expect("mark stale");

        assert_eq!(stale, 1);
        let current = service
            .list_binding_history("src1", "http://e/Patient", "http://e/name")
            .expect("history");
        assert_eq!(current.len(), 2);
        assert_eq!(current[0].version, 1);
        assert_eq!(current[0].status, BindingStatus::Active);
        assert_eq!(current[1].version, created.version + 1);
        assert_eq!(current[1].status, BindingStatus::Stale);
    }

    #[test]
    fn computes_binding_coverage_diff() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = Arc::new(BindingStore::new(dir.path()).expect("store"));
        let service = BindingService::new(store);

        service
            .upsert_binding(UpsertBindingRequest {
                source_id: "src1".to_string(),
                entity_uri: "http://e/Patient".to_string(),
                ontology_uri: "http://e/name".to_string(),
                table: "patients".to_string(),
                column: "name".to_string(),
                dialect: "postgresql".to_string(),
                confidence: 0.95,
                updated_by: "tester".to_string(),
                provenance: BindingProvenance::default(),
            })
            .expect("binding");

        let schema_without_name = SchemaDefinition {
            name: "public".to_string(),
            tables: vec![TableDefinition {
                name: "patients".to_string(),
                columns: vec![ColumnDefinition {
                    name: "id".to_string(),
                    data_type: "INTEGER".to_string(),
                    nullable: false,
                    primary_key: true,
                    default_value: None,
                    semantic_type: None,
                    statistics: None,
                }],
                estimated_rows: Some(10),
            }],
            relationships: vec![],
            indexes: vec![],
            inferred_at: Utc::now(),
        };

        let diff = service
            .diff_coverage(
                "src1",
                "http://e/Patient",
                &["http://e/name".to_string(), "http://e/id".to_string()],
                Some(&schema_without_name),
            )
            .expect("diff");

        assert_eq!(diff.covered_properties.len(), 0);
        assert_eq!(diff.stale_properties, vec!["http://e/name".to_string()]);
        assert_eq!(diff.unmapped_properties, vec!["http://e/id".to_string()]);
        assert_eq!(diff.missing_properties.len(), 2);
    }
}
