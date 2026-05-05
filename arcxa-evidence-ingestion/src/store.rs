use anyhow::{Context, Result};
use chrono::Utc;
use graphica_core::migration_evidence::{
    ConnectorStoreBackend, ConnectorStoreHealth, ConnectorStoreRuntimeStatus,
    MigrationConnector,
};
use rocksdb::{IteratorMode, Options, WriteBatch, DB};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;

const CONNECTOR_KEY_PREFIX: &str = "connector:";

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ConnectorState {
    #[serde(default)]
    pub connectors: HashMap<String, MigrationConnector>,
}

#[derive(Clone)]
pub struct PersistedConnectorStore {
    backend: ConnectorStore,
    state: Arc<RwLock<ConnectorState>>,
    legacy_imported_at: Option<chrono::DateTime<Utc>>,
}

#[derive(Clone)]
enum ConnectorStore {
    File {
        path: PathBuf,
    },
    RocksDb {
        db: Arc<DB>,
    },
}

impl PersistedConnectorStore {
    pub async fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::open_file(path).await
    }

    pub async fn open_file(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let state = if path.exists() {
            let bytes = tokio::fs::read(&path).await?;
            serde_json::from_slice(&bytes).context("failed to deserialize connector state")?
        } else {
            ConnectorState::default()
        };
        Ok(Self {
            backend: ConnectorStore::File { path },
            state: Arc::new(RwLock::new(state)),
            legacy_imported_at: None,
        })
    }

    pub async fn open_rocksdb(
        path: impl AsRef<Path>,
        legacy_file_path: Option<PathBuf>,
    ) -> Result<Self> {
        let db = Arc::new(open_rocksdb(path.as_ref())?);
        let mut state = load_state_from_rocksdb(&db)?;
        let mut legacy_imported_at = None;
        if state.connectors.is_empty() {
            if let Some(legacy_file_path) = legacy_file_path.as_ref() {
                if legacy_file_path.exists() {
                    let bytes = tokio::fs::read(legacy_file_path).await.with_context(|| {
                        format!(
                            "failed to read legacy connector state at {}",
                            legacy_file_path.display()
                        )
                    })?;
                    state = serde_json::from_slice(&bytes)
                        .context("failed to deserialize legacy connector state")?;
                    persist_rocksdb(&db, &state)?;
                    legacy_imported_at = Some(Utc::now());
                }
            }
        }

        Ok(Self {
            backend: ConnectorStore::RocksDb { db },
            state: Arc::new(RwLock::new(state)),
            legacy_imported_at,
        })
    }

    pub async fn upsert(&self, connector: MigrationConnector) -> Result<MigrationConnector> {
        let mut state = self.state.write().await;
        state
            .connectors
            .insert(connector.connector_id.clone(), connector.clone());
        self.persist_state(&state).await?;
        Ok(connector)
    }

    pub async fn get(&self, connector_id: &str) -> Option<MigrationConnector> {
        self.state.read().await.connectors.get(connector_id).cloned()
    }

    pub async fn runtime_status(&self) -> ConnectorStoreRuntimeStatus {
        let state = self.state.read().await;
        let updated_at = state
            .connectors
            .values()
            .map(|connector| connector.updated_at)
            .max()
            .unwrap_or_else(Utc::now);
        let last_successful_write_at = state
            .connectors
            .values()
            .map(|connector| connector.updated_at)
            .max();

        ConnectorStoreRuntimeStatus {
            backend: self.backend_kind(),
            health: ConnectorStoreHealth::Healthy,
            connector_count: state.connectors.len(),
            writable: true,
            updated_at,
            last_successful_write_at,
            legacy_imported_at: self.legacy_imported_at,
            last_error: None,
        }
    }

    pub fn backend_kind(&self) -> ConnectorStoreBackend {
        match &self.backend {
            ConnectorStore::File { .. } => ConnectorStoreBackend::File,
            ConnectorStore::RocksDb { .. } => ConnectorStoreBackend::RocksDb,
        }
    }

    async fn persist_state(&self, state: &ConnectorState) -> Result<()> {
        match &self.backend {
            ConnectorStore::File { path } => persist_file(path, state).await,
            ConnectorStore::RocksDb { db } => persist_rocksdb(db, state),
        }
    }
}

async fn persist_file(path: &PathBuf, state: &ConnectorState) -> Result<()> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let bytes = serde_json::to_vec_pretty(state).context("failed to serialize connector state")?;
    tokio::fs::write(path, bytes).await?;
    Ok(())
}

fn open_rocksdb(path: &Path) -> Result<DB> {
    let mut opts = Options::default();
    opts.create_if_missing(true);
    DB::open(&opts, path)
        .with_context(|| format!("failed to open connector RocksDB at {}", path.display()))
}

fn load_state_from_rocksdb(db: &DB) -> Result<ConnectorState> {
    let mut connectors = HashMap::new();
    for entry in db.iterator(IteratorMode::Start) {
        let (key, value) = entry.context("failed to iterate connector RocksDB")?;
        let key = String::from_utf8(key.to_vec()).context("connector RocksDB key was not UTF-8")?;
        if let Some(connector_id) = key.strip_prefix(CONNECTOR_KEY_PREFIX) {
            let connector: MigrationConnector = serde_json::from_slice(&value)
                .with_context(|| format!("failed to decode connector '{connector_id}' from RocksDB"))?;
            connectors.insert(connector_id.to_string(), connector);
        }
    }
    Ok(ConnectorState { connectors })
}

fn persist_rocksdb(db: &DB, state: &ConnectorState) -> Result<()> {
    let mut batch = WriteBatch::default();
    for entry in db.iterator(IteratorMode::Start) {
        let (key, _) = entry.context("failed to iterate existing connector RocksDB entries")?;
        let key_str = String::from_utf8(key.to_vec()).context("connector RocksDB key was not UTF-8")?;
        if key_str.starts_with(CONNECTOR_KEY_PREFIX) {
            batch.delete(key);
        }
    }
    for (connector_id, connector) in &state.connectors {
        let key = format!("{CONNECTOR_KEY_PREFIX}{connector_id}");
        let value = serde_json::to_vec(connector)
            .with_context(|| format!("failed to serialize connector '{connector_id}'"))?;
        batch.put(key.as_bytes(), value);
    }
    db.write(batch)
        .context("failed to persist connector state to RocksDB")
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphica_core::migration_evidence::{
        ConnectorAuth, ConnectorEndpoint, ConnectorTransport, MigrationConnector,
        MigrationConnectorRole, MigrationConnectorVendor,
    };
    use tempfile::tempdir;

    fn sample_connector() -> MigrationConnector {
        MigrationConnector {
            connector_id: "connector-1".to_string(),
            name: "IBM Artifacts".to_string(),
            vendor: MigrationConnectorVendor::IbmRapidMove,
            role: MigrationConnectorRole::MigrationArtifactSource,
            transport: ConnectorTransport::HttpJson,
            program_id: "program-1".to_string(),
            endpoint: ConnectorEndpoint {
                base_url: "https://example.test".to_string(),
                path: "/artifacts".to_string(),
                method: "POST".to_string(),
                headers: HashMap::new(),
            },
            auth: ConnectorAuth::default(),
            schedule: None,
            enabled: true,
            metadata: HashMap::new(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    fn assert_connectors_match(actual: &MigrationConnector, expected: &MigrationConnector) {
        assert_eq!(
            serde_json::to_value(actual).unwrap(),
            serde_json::to_value(expected).unwrap()
        );
    }

    #[tokio::test]
    async fn file_store_round_trips_connectors() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("connectors.json");
        let store = PersistedConnectorStore::open_file(&path).await.unwrap();
        let connector = sample_connector();

        store.upsert(connector.clone()).await.unwrap();

        let reopened = PersistedConnectorStore::open_file(&path).await.unwrap();
        let persisted = reopened.get("connector-1").await.unwrap();
        assert_connectors_match(&persisted, &connector);
    }

    #[tokio::test]
    async fn rocksdb_store_round_trips_connectors() {
        let temp = tempdir().unwrap();
        let rocksdb_path = temp.path().join("rocksdb");
        let store = PersistedConnectorStore::open_rocksdb(&rocksdb_path, None)
            .await
            .unwrap();
        let connector = sample_connector();

        store.upsert(connector.clone()).await.unwrap();
        drop(store);

        let reopened = PersistedConnectorStore::open_rocksdb(&rocksdb_path, None)
            .await
            .unwrap();
        let persisted = reopened.get("connector-1").await.unwrap();
        assert_connectors_match(&persisted, &connector);
    }

    #[tokio::test]
    async fn rocksdb_store_imports_legacy_file_once() {
        let temp = tempdir().unwrap();
        let legacy_path = temp.path().join("connectors.json");
        let connector = sample_connector();
        let legacy_state = ConnectorState {
            connectors: HashMap::from([(connector.connector_id.clone(), connector.clone())]),
        };
        persist_file(&legacy_path, &legacy_state).await.unwrap();

        let rocksdb_path = temp.path().join("rocksdb");
        let imported = PersistedConnectorStore::open_rocksdb(
            &rocksdb_path,
            Some(legacy_path.clone()),
        )
        .await
        .unwrap();
        let imported_connector = imported.get("connector-1").await.unwrap();
        assert_connectors_match(&imported_connector, &connector);

        let replacement = MigrationConnector {
            name: "Updated IBM Artifacts".to_string(),
            ..connector.clone()
        };
        imported.upsert(replacement.clone()).await.unwrap();
        drop(imported);

        let reopened = PersistedConnectorStore::open_rocksdb(&rocksdb_path, Some(legacy_path))
            .await
            .unwrap();
        let reopened_connector = reopened.get("connector-1").await.unwrap();
        assert_connectors_match(&reopened_connector, &replacement);
    }

    #[tokio::test]
    async fn runtime_status_reports_backend_and_connector_count() {
        let temp = tempdir().unwrap();
        let rocksdb_path = temp.path().join("rocksdb");
        let store = PersistedConnectorStore::open_rocksdb(&rocksdb_path, None)
            .await
            .unwrap();
        store.upsert(sample_connector()).await.unwrap();

        let status = store.runtime_status().await;
        assert_eq!(status.backend, ConnectorStoreBackend::RocksDb);
        assert_eq!(status.health, ConnectorStoreHealth::Healthy);
        assert_eq!(status.connector_count, 1);
        assert!(status.writable);
        assert!(status.last_successful_write_at.is_some());
    }
}
