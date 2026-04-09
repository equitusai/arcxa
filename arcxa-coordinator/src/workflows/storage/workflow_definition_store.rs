//! Durable workflow definition storage.
//!
//! Stores the executable workflow definitions and metadata for the workflow
//! engine so coordinator restarts can restore registered workflows.

use anyhow::{Context, Result};
use graphica_core::orchestration::{WorkflowEngine, WorkflowMetadata};
use rocksdb::{IteratorMode, Options, DB};
use std::path::Path;
use std::sync::Arc;

/// RocksDB-backed workflow definition store.
#[derive(Clone)]
pub struct WorkflowDefinitionStore {
    db: Arc<DB>,
}

impl WorkflowDefinitionStore {
    /// Open or create the workflow definition store.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let mut options = Options::default();
        options.create_if_missing(true);

        let db = DB::open(&options, path).context("Failed to open workflow definition store")?;

        Ok(Self { db: Arc::new(db) })
    }

    /// Persist workflow metadata.
    pub fn save(&self, metadata: &WorkflowMetadata) -> Result<()> {
        let payload = serde_json::to_vec(metadata)
            .with_context(|| format!("Failed to serialize workflow '{}'", metadata.id))?;

        self.db
            .put(metadata.id.as_bytes(), payload)
            .with_context(|| format!("Failed to persist workflow '{}'", metadata.id))?;

        Ok(())
    }

    /// Fetch persisted metadata for a workflow.
    pub fn get(&self, workflow_id: &str) -> Result<Option<WorkflowMetadata>> {
        let Some(payload) = self
            .db
            .get(workflow_id.as_bytes())
            .with_context(|| format!("Failed to read workflow '{}'", workflow_id))?
        else {
            return Ok(None);
        };

        let metadata = serde_json::from_slice(&payload)
            .with_context(|| format!("Failed to deserialize workflow '{}'", workflow_id))?;

        Ok(Some(metadata))
    }

    /// Delete a persisted workflow definition.
    pub fn delete(&self, workflow_id: &str) -> Result<()> {
        self.db
            .delete(workflow_id.as_bytes())
            .with_context(|| format!("Failed to delete workflow '{}'", workflow_id))?;

        Ok(())
    }

    /// List all persisted workflow definitions.
    pub fn list(&self) -> Result<Vec<WorkflowMetadata>> {
        let mut workflows = Vec::new();

        for entry in self.db.iterator(IteratorMode::Start) {
            let (_, payload) = entry.context("Failed to iterate workflow definition store")?;
            let metadata = serde_json::from_slice::<WorkflowMetadata>(&payload)
                .context("Failed to deserialize persisted workflow metadata")?;
            workflows.push(metadata);
        }

        workflows.sort_by(|left, right| {
            left.created_at
                .cmp(&right.created_at)
                .then_with(|| left.id.cmp(&right.id))
        });

        Ok(workflows)
    }
}

/// Restore all persisted workflows into the workflow engine.
pub async fn restore_persisted_workflows(
    engine: &WorkflowEngine,
    store: &WorkflowDefinitionStore,
) -> Result<usize> {
    let workflows = store.list()?;
    let count = workflows.len();

    for metadata in workflows {
        let workflow_id = metadata.id.clone();
        engine
            .hydrate_workflow(metadata)
            .await
            .with_context(|| format!("Failed to restore workflow '{}'", workflow_id))?;
    }

    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use graphica_core::orchestration::workflow::{
        definition::{
            FallbackStrategy, HeuristicConfig, StepConfig, StepType, WorkflowDefinition,
            WorkflowStep,
        },
        WorkflowMetadata,
    };
    use tempfile::TempDir;

    fn create_test_definition() -> WorkflowDefinition {
        WorkflowDefinition {
            steps: vec![WorkflowStep {
                id: "rule_1".to_string(),
                step_type: StepType::HeuristicRule,
                config: StepConfig::Heuristic(HeuristicConfig {
                    rule_id: "match_on_email".to_string(),
                    min_confidence: 0.8,
                }),
                depends_on: Vec::new(),
            }],
            fusion_threshold: 0.8,
            fallback: FallbackStrategy::ManualReview,
        }
    }

    fn create_test_metadata(workflow_id: &str) -> WorkflowMetadata {
        WorkflowMetadata {
            id: workflow_id.to_string(),
            name: "Persistent workflow".to_string(),
            description: Some("Persisted for restart recovery".to_string()),
            tags: vec!["demo".to_string(), "persistent".to_string()],
            definition: create_test_definition(),
            version: "2.0.0".to_string(),
            created_at: Utc::now(),
            execution_count: 3,
            last_executed_at: Some(Utc::now()),
        }
    }

    #[test]
    fn round_trips_persisted_workflow_metadata() {
        let temp_dir = TempDir::new().unwrap();
        let store = WorkflowDefinitionStore::open(temp_dir.path()).unwrap();
        let metadata = create_test_metadata("wf_round_trip");

        store.save(&metadata).unwrap();

        let restored = store.get(&metadata.id).unwrap().unwrap();
        assert_eq!(restored.id, metadata.id);
        assert_eq!(restored.name, metadata.name);
        assert_eq!(restored.version, metadata.version);
        assert_eq!(restored.execution_count, metadata.execution_count);
        assert_eq!(restored.tags, metadata.tags);
    }

    #[tokio::test]
    async fn restores_persisted_workflows_into_engine() {
        let temp_dir = TempDir::new().unwrap();
        let store = WorkflowDefinitionStore::open(temp_dir.path()).unwrap();
        let metadata = create_test_metadata("wf_restore");
        let engine = WorkflowEngine::new();

        store.save(&metadata).unwrap();

        let restored_count = restore_persisted_workflows(&engine, &store).await.unwrap();
        let restored = engine.get_workflow(&metadata.id).await.unwrap().unwrap();

        assert_eq!(restored_count, 1);
        assert_eq!(restored.id, metadata.id);
        assert_eq!(restored.execution_count, metadata.execution_count);
        assert_eq!(restored.version, metadata.version);
    }
}
