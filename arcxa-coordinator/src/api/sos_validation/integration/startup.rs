use std::{fmt::Display, future::Future, sync::Arc};

use anyhow::Result;

use crate::{
    api::sos_validation::storage::{Interface, SosStorageManager, System},
    mapping::ontology_registry::PersistedOntologyRegistry,
};

use super::{reconcile_sos_ontology_assets, SosValidationService};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SosStartupRecoveryOutcome {
    pub ontology_sync_attempted: bool,
    pub ontology_sync_succeeded: bool,
    pub ontology_sync_error: Option<String>,
    pub graph_reconcile_attempted: bool,
    pub graph_reconcile_succeeded: bool,
    pub graph_reconcile_error: Option<String>,
}

pub async fn perform_startup_recovery(
    storage_manager: Option<&Arc<SosStorageManager>>,
    registry: Option<&Arc<PersistedOntologyRegistry>>,
    service: Option<&Arc<SosValidationService>>,
) -> SosStartupRecoveryOutcome {
    let storage_manager = storage_manager.cloned();
    let registry = registry.cloned();
    let service = service.cloned();
    let ontology_storage_manager = storage_manager.clone();
    let ontology_registry = registry.clone();
    let graph_service = service.clone();

    drive_startup_recovery(
        storage_manager.is_some() && registry.is_some(),
        move || {
            let storage_manager = ontology_storage_manager
                .clone()
                .expect("ontology sync requires storage manager");
            let registry = ontology_registry
                .clone()
                .expect("ontology sync requires registry");
            async move { reconcile_sos_ontology_assets(&storage_manager, &registry).await }
        },
        service.is_some(),
        move || {
            graph_service
                .as_ref()
                .expect("graph reconcile requires SoS service")
                .reconcile_graphs()
        },
    )
    .await
}

async fn drive_startup_recovery<OntologySync, OntologyFuture, GraphReconcile, GraphError>(
    should_run_ontology_sync: bool,
    ontology_sync: OntologySync,
    should_run_graph_reconcile: bool,
    graph_reconcile: GraphReconcile,
) -> SosStartupRecoveryOutcome
where
    OntologySync: FnOnce() -> OntologyFuture,
    OntologyFuture: Future<Output = Result<()>>,
    GraphReconcile: FnOnce() -> Result<(), GraphError>,
    GraphError: Display,
{
    let mut outcome = SosStartupRecoveryOutcome {
        ontology_sync_attempted: false,
        ontology_sync_succeeded: false,
        ontology_sync_error: None,
        graph_reconcile_attempted: false,
        graph_reconcile_succeeded: false,
        graph_reconcile_error: None,
    };

    if should_run_ontology_sync {
        outcome.ontology_sync_attempted = true;
        match ontology_sync().await {
            Ok(()) => {
                outcome.ontology_sync_succeeded = true;
            }
            Err(error) => {
                outcome.ontology_sync_error = Some(error.to_string());
            }
        }
    }

    if should_run_graph_reconcile {
        outcome.graph_reconcile_attempted = true;
        match graph_reconcile() {
            Ok(()) => {
                outcome.graph_reconcile_succeeded = true;
            }
            Err(error) => {
                outcome.graph_reconcile_error = Some(error.to_string());
            }
        }
    }

    outcome
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        api::sos_validation::storage::SosStorageManager,
        governance::rdf_store::{GraphicaRdfStore, NamedGraph, RdfStore},
        mapping::ontology_registry::PersistedOntologyRegistry,
    };
    use serde_json::json;
    use std::collections::HashMap;
    use tempfile::TempDir;

    fn create_minimal_service() -> (
        TempDir,
        Arc<SosStorageManager>,
        Arc<GraphicaRdfStore>,
        Arc<SosValidationService>,
    ) {
        let temp_dir = TempDir::new().expect("temporary directory should be created");
        let storage_manager = Arc::new(
            SosStorageManager::new(
                temp_dir
                    .path()
                    .to_str()
                    .expect("temporary directory path should be UTF-8"),
            )
            .expect("SoS storage manager should be created"),
        );
        let rdf_store =
            Arc::new(GraphicaRdfStore::new_in_memory().expect("RDF store should be created"));
        let service = Arc::new(SosValidationService::new(
            storage_manager.clone(),
            Some(rdf_store.clone()),
            None,
        ));

        register_minimal_catalog(&storage_manager);

        (temp_dir, storage_manager, rdf_store, service)
    }

    async fn create_registry() -> (TempDir, Arc<PersistedOntologyRegistry>) {
        let temp_dir = TempDir::new().expect("temporary registry directory should be created");
        let registry_path = temp_dir.path().join("ontologies.db");
        let registry = PersistedOntologyRegistry::open(&registry_path)
            .await
            .expect("persisted ontology registry should be created");

        (temp_dir, Arc::new(registry))
    }

    fn register_minimal_catalog(storage_manager: &Arc<SosStorageManager>) {
        let system = System {
            system_id: "provider-system".to_string(),
            system_name: "Provider System".to_string(),
            system_type: "provider".to_string(),
            vendor: "Graphica".to_string(),
            version: "1.0".to_string(),
            classification: "unclassified".to_string(),
            description: None,
            deployment: HashMap::new(),
            capabilities: HashMap::new(),
            tags: vec![],
            active: true,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };
        storage_manager
            .put_system(&system)
            .expect("system should be stored");

        let provider_interface = Interface {
            system_id: "provider-system".to_string(),
            interface_id: "provider-if".to_string(),
            interface_name: "Provider Interface".to_string(),
            direction: "outbound".to_string(),
            protocol: "rest".to_string(),
            data_format: "json".to_string(),
            schema: json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string" }
                },
                "required": ["id"]
            }),
            coordinate_system: None,
            unit_system: None,
            metadata: HashMap::new(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };
        storage_manager
            .put_interface(&provider_interface)
            .expect("provider interface should be stored");
    }

    #[tokio::test]
    async fn startup_recovery_skips_ontology_sync_without_registry_and_rebuilds_graph() {
        let (_temp_dir, storage_manager, rdf_store, service) = create_minimal_service();

        let outcome = perform_startup_recovery(Some(&storage_manager), None, Some(&service)).await;

        assert_eq!(
            outcome,
            SosStartupRecoveryOutcome {
                ontology_sync_attempted: false,
                ontology_sync_succeeded: false,
                ontology_sync_error: None,
                graph_reconcile_attempted: true,
                graph_reconcile_succeeded: true,
                graph_reconcile_error: None,
            }
        );

        let graph = rdf_store
            .count_triples(Some(&NamedGraph::new(
                "http://graphica.io/graph/sos-catalog",
            )))
            .expect("catalog graph should be countable");
        assert!(
            graph > 0,
            "startup recovery should rebuild the SoS catalog graph"
        );
    }

    #[tokio::test]
    async fn startup_recovery_syncs_ontology_assets_when_graph_service_is_unavailable() {
        let (_temp_dir, storage_manager, _rdf_store, _service) = create_minimal_service();
        let (_registry_dir, registry) = create_registry().await;

        let outcome = perform_startup_recovery(Some(&storage_manager), Some(&registry), None).await;

        assert_eq!(
            outcome,
            SosStartupRecoveryOutcome {
                ontology_sync_attempted: true,
                ontology_sync_succeeded: true,
                ontology_sync_error: None,
                graph_reconcile_attempted: false,
                graph_reconcile_succeeded: false,
                graph_reconcile_error: None,
            }
        );

        let interface = storage_manager
            .get_interface("provider-if")
            .expect("interface lookup should succeed")
            .expect("interface should still exist after startup recovery");
        assert!(
            interface.metadata.contains_key("shape_ref"),
            "startup ontology sync should persist shape metadata for interfaces"
        );
        assert!(
            interface.metadata.contains_key("shape_ontology_id"),
            "startup ontology sync should persist ontology identifiers for interfaces"
        );
        assert!(
            registry.get_ontology("sos_core").is_some(),
            "startup ontology sync should ensure the SoS core ontology exists"
        );
    }

    #[tokio::test]
    async fn startup_recovery_continues_to_graph_reconcile_when_ontology_sync_fails() {
        let outcome = drive_startup_recovery(
            true,
            || async { Err(anyhow::anyhow!("ontology registry unavailable")) },
            true,
            || Ok::<(), anyhow::Error>(()),
        )
        .await;

        assert!(outcome.ontology_sync_attempted);
        assert!(!outcome.ontology_sync_succeeded);
        assert_eq!(
            outcome.ontology_sync_error.as_deref(),
            Some("ontology registry unavailable")
        );
        assert!(outcome.graph_reconcile_attempted);
        assert!(outcome.graph_reconcile_succeeded);
        assert!(outcome.graph_reconcile_error.is_none());
    }

    #[tokio::test]
    async fn startup_recovery_records_graph_reconcile_failure_without_panicking() {
        let outcome = drive_startup_recovery(
            false,
            || async { Ok::<(), anyhow::Error>(()) },
            true,
            || Err(anyhow::anyhow!("rdf projection store unavailable")),
        )
        .await;

        assert!(!outcome.ontology_sync_attempted);
        assert!(outcome.graph_reconcile_attempted);
        assert!(!outcome.graph_reconcile_succeeded);
        assert_eq!(
            outcome.graph_reconcile_error.as_deref(),
            Some("rdf projection store unavailable")
        );
    }

    #[tokio::test]
    async fn startup_recovery_noops_cleanly_when_no_prerequisites_are_available() {
        let outcome = perform_startup_recovery(None, None, None).await;

        assert_eq!(
            outcome,
            SosStartupRecoveryOutcome {
                ontology_sync_attempted: false,
                ontology_sync_succeeded: false,
                ontology_sync_error: None,
                graph_reconcile_attempted: false,
                graph_reconcile_succeeded: false,
                graph_reconcile_error: None,
            }
        );
    }
}
