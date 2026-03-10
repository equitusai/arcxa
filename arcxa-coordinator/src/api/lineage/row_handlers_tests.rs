//! Unit tests for row-level lineage API handlers

#[cfg(test)]
mod tests {
    use crate::api::lineage::row_handlers;
    use crate::api::ApiState;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use graphica_core::core::lineage::row_level::{
        DatabaseType, JobStatistics, ProcessingOutcome, RowId, RowJourney, RowLevelLineageSink,
        RowLineageEvent, RowTransformation,
    };
    use std::sync::Arc;
    use tokio::sync::Mutex;
    use tower::ServiceExt;

    // Mock row lineage store for testing
    struct MockRowLineageStore {
        events: Arc<Mutex<Vec<RowLineageEvent>>>,
    }

    impl MockRowLineageStore {
        fn new() -> Self {
            Self {
                events: Arc::new(Mutex::new(Vec::new())),
            }
        }

        async fn add_event(&self, event: RowLineageEvent) {
            let mut events = self.events.lock().await;
            events.push(event);
        }
    }

    #[async_trait::async_trait]
    impl RowLevelLineageSink for MockRowLineageStore {
        async fn write_row(&self, event: RowLineageEvent) -> anyhow::Result<()> {
            self.add_event(event).await;
            Ok(())
        }

        async fn write_rows_batch(&self, events: Vec<RowLineageEvent>) -> anyhow::Result<()> {
            let mut store_events = self.events.lock().await;
            store_events.extend(events);
            Ok(())
        }

        async fn get_row_lineage(&self, row_id: &RowId) -> anyhow::Result<Vec<RowLineageEvent>> {
            let events = self.events.lock().await;
            Ok(events
                .iter()
                .filter(|e| &e.row_id == row_id)
                .cloned()
                .collect())
        }

        async fn get_batch_lineage(&self, batch_id: &str) -> anyhow::Result<Vec<RowLineageEvent>> {
            let events = self.events.lock().await;
            Ok(events
                .iter()
                .filter(|e| e.batch_id == batch_id)
                .cloned()
                .collect())
        }

        async fn get_filtered_rows(
            &self,
            job_id: &str,
            _start_time: chrono::DateTime<chrono::Utc>,
            _end_time: chrono::DateTime<chrono::Utc>,
        ) -> anyhow::Result<Vec<(RowId, String)>> {
            let events = self.events.lock().await;
            Ok(events
                .iter()
                .filter(|e| e.job_id == job_id && e.is_filtered())
                .map(|e| (e.row_id.clone(), format!("Filtered by rule")))
                .collect())
        }

        async fn get_row_transformations(
            &self,
            row_id: &RowId,
        ) -> anyhow::Result<Vec<RowTransformation>> {
            let events = self.get_row_lineage(row_id).await?;
            Ok(events.into_iter().flat_map(|e| e.transformations).collect())
        }

        async fn trace_row_journey(&self, row_id: &RowId) -> anyhow::Result<RowJourney> {
            use graphica_core::core::lineage::row_level::JourneyStep;

            let events = self.get_row_lineage(row_id).await?;

            let steps: Vec<JourneyStep> = events
                .iter()
                .map(|e| JourneyStep {
                    activity: format!("Process row"),
                    timestamp: e.timestamp,
                    duration_ms: 100,
                    outcome: e.outcome.clone(),
                })
                .collect();

            Ok(RowJourney {
                source: row_id.clone(),
                steps,
                destination: None,
                total_duration_ms: 500,
            })
        }

        async fn get_job_stats(&self, job_id: &str) -> anyhow::Result<JobStatistics> {
            let events = self.events.lock().await;
            let job_events: Vec<_> = events.iter().filter(|e| e.job_id == job_id).collect();

            let mut stats = JobStatistics {
                job_id: job_id.to_string(),
                total_rows: job_events.len() as u64,
                success_count: 0,
                filtered_count: 0,
                failed_count: 0,
                filter_reasons: Default::default(),
                avg_processing_time_ms: 100.0,
                start_time: chrono::Utc::now(),
                end_time: Some(chrono::Utc::now()),
            };

            for event in job_events {
                match &event.outcome {
                    ProcessingOutcome::Processed { .. } => stats.success_count += 1,
                    ProcessingOutcome::Filtered { reason, .. } => {
                        stats.filtered_count += 1;
                        *stats.filter_reasons.entry(reason.clone()).or_insert(0) += 1;
                    }
                    ProcessingOutcome::Failed { .. }
                    | ProcessingOutcome::ValidationFailed { .. } => stats.failed_count += 1,
                }
            }

            Ok(stats)
        }
    }

    fn create_test_api_state() -> Arc<ApiState> {
        use crate::api::auth::AuthConfig;
        use crate::api::import_jobs::ImportJobManager;
        use crate::api::setup_token::SetupTokenManager;
        use crate::storage::LineageStorage;
        use tempfile::TempDir;

        let mock_store = Arc::new(MockRowLineageStore::new());

        // Create temporary directories for test storage
        let temp_dir = TempDir::new().unwrap();
        let temp_path = temp_dir.path().to_str().unwrap();
        let rocks_path = format!("{}/rocksdb", temp_path);
        let parquet_path = format!("{}/parquet", temp_path);
        let cold_path = format!("{}/cold", temp_path);

        let lineage_storage =
            LineageStorage::new_for_tests(&rocks_path, &parquet_path, &cold_path).unwrap();

        Arc::new(ApiState {
            lineage_storage: Arc::new(lineage_storage),
            governance_brain: None,
            rdf_store: None,
            shard_registry: None,
            query_executor: None,
            workflow_engine: None,
            model_registry: None,
            model_cache: None,
            rule_executor: None,
            transformer_registry: None,
            circuit_breakers: None,
            auth_config: Arc::new(AuthConfig::disabled()),
            user_service: None,
            setup_token_manager: Arc::new(SetupTokenManager::new()),
            audit_logger: None,
            datasource_catalog: None,
            datasource_catalog_impl: None,
            persisted_ontology_registry: None,
            ontology_registry: None,
            rdf_storage: None,
            connector_registry: None,
            resolved_entity_cache: None,
            metrics_registry: None,
            import_job_manager: Arc::new(ImportJobManager::new()),
            mapping_engine: None,
            secret_store_registry: None,
            loader_job_manager: None,
            unified_mapping_coordinator: None,
            binding_service: None,
            schedule_store: None,
            workflow_store: None,
            execution_store: None,
            file_library: None,
            kafka_producer: None,
            http_client: None,
            lineage_generator: None,
            metrics: None,
            replay_coordinator: None,
            row_lineage_store: Some(mock_store),
            manual_mapping_store: None,
            db2_pool: None,
            approval_store: None,
            execution_sync: None,
            policy_checker: None,
            checkpoint_persistence: None,
            dlq_reader: None,
            dlq_reprocessor: None,
            dlq_stats_calculator: None,
            schema_version_store: None,
            column_lineage_store: None,
            schema_evolution_store: None,
            gdpr_coordinator: None,
            export_executor: None,
            progress_store: None,
            cancellation_manager: None,
            sos_storage_manager: None,
            discovery_state: None,
            discovery_orchestrator: None,
        })
    }

    #[tokio::test]
    async fn test_parse_row_key_csv() {
        let row_id = row_handlers::parse_row_key("csv:test.csv:123").unwrap();
        assert_eq!(row_id.to_key(), "csv:test.csv:123");
    }

    #[tokio::test]
    async fn test_parse_row_key_database() {
        let row_id =
            row_handlers::parse_row_key("db2:customers:customer_id=C123,order_id=O456").unwrap();
        assert!(row_id.to_key().contains("db2:customers:"));
        assert!(row_id.to_key().contains("customer_id=C123"));
    }

    #[tokio::test]
    async fn test_parse_row_key_kafka() {
        let row_id = row_handlers::parse_row_key("kafka:orders:p5:o987654").unwrap();
        assert_eq!(row_id.to_key(), "kafka:orders:p5:o987654");
    }

    #[tokio::test]
    async fn test_get_row_lineage_success() {
        let state = create_test_api_state();

        // Add test event
        if let Some(ref store) = state.row_lineage_store {
            let event = RowLineageEvent::success(
                RowId::csv("test.csv", 1),
                "batch-1".to_string(),
                "job-1".to_string(),
                "/output/test.csv".to_string(),
                "tenant-a".to_string(),
            );
            store.write_row(event).await.unwrap();
        }

        // Query row lineage
        let response = row_handlers::get_row_lineage(
            axum::extract::State(state),
            axum::extract::Path("csv:test.csv:1".to_string()),
        )
        .await;

        assert!(response.is_ok());
        let lineage = response.unwrap();
        assert_eq!(lineage.0.total_count, 1);
    }

    #[tokio::test]
    async fn test_get_batch_lineage_success() {
        let state = create_test_api_state();

        // Add multiple test events for the same batch
        if let Some(ref store) = state.row_lineage_store {
            for i in 1..=5 {
                let event = RowLineageEvent::success(
                    RowId::csv("test.csv", i),
                    "batch-123".to_string(),
                    "job-1".to_string(),
                    "/output/test.csv".to_string(),
                    "tenant-a".to_string(),
                );
                store.write_row(event).await.unwrap();
            }
        }

        // Query batch lineage
        let response = row_handlers::get_batch_lineage(
            axum::extract::State(state),
            axum::extract::Path("batch-123".to_string()),
        )
        .await;

        assert!(response.is_ok());
        let lineage = response.unwrap();
        assert_eq!(lineage.0.total_rows, 5);
    }

    #[tokio::test]
    async fn test_get_job_stats_success() {
        let state = create_test_api_state();

        // Add test events with different outcomes
        if let Some(ref store) = state.row_lineage_store {
            // 3 successful
            for i in 1..=3 {
                let event = RowLineageEvent::success(
                    RowId::csv("test.csv", i),
                    "batch-1".to_string(),
                    "job-test".to_string(),
                    "/output/test.csv".to_string(),
                    "tenant-a".to_string(),
                );
                store.write_row(event).await.unwrap();
            }

            // 2 filtered
            for i in 4..=5 {
                let event = RowLineageEvent::filtered(
                    RowId::csv("test.csv", i),
                    "batch-1".to_string(),
                    "job-test".to_string(),
                    "Invalid data".to_string(),
                    "rule-1".to_string(),
                    "tenant-a".to_string(),
                );
                store.write_row(event).await.unwrap();
            }
        }

        // Query job stats
        let response = row_handlers::get_job_stats(
            axum::extract::State(state),
            axum::extract::Path("job-test".to_string()),
        )
        .await;

        assert!(response.is_ok());
        let stats = response.unwrap();
        assert_eq!(stats.0.total_rows, 5);
        assert_eq!(stats.0.success_count, 3);
        assert_eq!(stats.0.filtered_count, 2);
    }

    #[tokio::test]
    async fn test_get_row_lineage_not_found() {
        let state = create_test_api_state();

        let response = row_handlers::get_row_lineage(
            axum::extract::State(state),
            axum::extract::Path("csv:nonexistent.csv:999".to_string()),
        )
        .await;

        assert!(response.is_err());
    }

    #[tokio::test]
    async fn test_get_row_journey_success() {
        let state = create_test_api_state();

        // Add test event
        if let Some(ref store) = state.row_lineage_store {
            let event = RowLineageEvent::success(
                RowId::csv("test.csv", 1),
                "batch-1".to_string(),
                "job-1".to_string(),
                "/output/test.csv".to_string(),
                "tenant-a".to_string(),
            );
            store.write_row(event).await.unwrap();
        }

        // Query row journey
        let response = row_handlers::get_row_journey(
            axum::extract::State(state),
            axum::extract::Path("csv:test.csv:1".to_string()),
        )
        .await;

        assert!(response.is_ok());
        let journey = response.unwrap();
        assert!(!journey.0.steps.is_empty());
    }
}
