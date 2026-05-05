use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Utc};
use graphica_core::migration_evidence::{
    ApprovalEvent, ControlResult, EvidencePacket, ExecutionEvent, ExceptionRecord,
    MigrationEvidenceArtifactType, MigrationEvidenceEvent, MigrationObject, MigrationProgram,
    TraceabilityReadModelCounts, TraceabilityRuntimeStatus, TraceabilityStoreBackend,
    TransformationRule,
};
use rocksdb::{ColumnFamilyDescriptor, IteratorMode, Options, WriteBatch, DB};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;

const CF_META: &str = "meta";
const CF_EVENTS: &str = "events";
const CF_PROGRAMS: &str = "programs";
const CF_OBJECTS: &str = "objects";
const CF_RULES: &str = "rules";
const CF_EXECUTIONS: &str = "executions";
const CF_EXCEPTIONS: &str = "exceptions";
const CF_CONTROLS: &str = "controls";
const CF_APPROVALS: &str = "approvals";
const CF_PACKETS: &str = "packets";
const CF_OBJECT_INDEXES: &str = "object_indexes";
const CF_PROGRAM_OBJECTS: &str = "program_objects";
const CF_EVENT_IDS: &str = "event_ids";

const META_UPDATED_AT: &str = "updated_at";
const META_LAST_EVENT_SEQUENCE: &str = "last_event_sequence";
const META_LAST_REBUILD_AT: &str = "last_rebuild_at";
const META_LEGACY_IMPORTED_AT: &str = "legacy_imported_at";

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ObjectIndex {
    #[serde(default)]
    pub value_key_rules: HashMap<String, Vec<String>>,
    #[serde(default)]
    pub value_key_execution_ids: HashMap<String, Vec<String>>,
    #[serde(default)]
    pub value_key_exception_ids: HashMap<String, Vec<String>>,
    #[serde(default)]
    pub value_key_control_ids: HashMap<String, Vec<String>>,
    #[serde(default)]
    pub value_key_approval_ids: HashMap<String, Vec<String>>,
    #[serde(default)]
    pub value_key_packet_ids: HashMap<String, Vec<String>>,
    #[serde(default)]
    pub object_level_rule_ids: Vec<String>,
    #[serde(default)]
    pub object_level_execution_ids: Vec<String>,
    #[serde(default)]
    pub object_level_exception_ids: Vec<String>,
    #[serde(default)]
    pub object_level_control_ids: Vec<String>,
    #[serde(default)]
    pub object_level_approval_ids: Vec<String>,
    #[serde(default)]
    pub object_level_packet_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceabilityState {
    #[serde(default)]
    pub programs: HashMap<String, MigrationProgram>,
    #[serde(default)]
    pub objects: HashMap<String, MigrationObject>,
    #[serde(default)]
    pub rules: HashMap<String, TransformationRule>,
    #[serde(default)]
    pub executions: HashMap<String, ExecutionEvent>,
    #[serde(default)]
    pub exceptions: HashMap<String, ExceptionRecord>,
    #[serde(default)]
    pub controls: HashMap<String, ControlResult>,
    #[serde(default)]
    pub approvals: HashMap<String, ApprovalEvent>,
    #[serde(default)]
    pub packets: HashMap<String, EvidencePacket>,
    #[serde(default)]
    pub object_indexes: HashMap<String, ObjectIndex>,
    #[serde(default)]
    pub program_to_objects: HashMap<String, Vec<String>>,
    #[serde(default)]
    pub processed_event_ids: HashSet<String>,
    pub updated_at: DateTime<Utc>,
    #[serde(default)]
    pub last_event_sequence: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_rebuild_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub legacy_imported_at: Option<DateTime<Utc>>,
}

impl Default for TraceabilityState {
    fn default() -> Self {
        Self {
            programs: HashMap::new(),
            objects: HashMap::new(),
            rules: HashMap::new(),
            executions: HashMap::new(),
            exceptions: HashMap::new(),
            controls: HashMap::new(),
            approvals: HashMap::new(),
            packets: HashMap::new(),
            object_indexes: HashMap::new(),
            program_to_objects: HashMap::new(),
            processed_event_ids: HashSet::new(),
            updated_at: Utc::now(),
            last_event_sequence: 0,
            last_rebuild_at: None,
            legacy_imported_at: None,
        }
    }
}

impl TraceabilityState {
    fn counts(&self, event_log_entries: usize) -> TraceabilityReadModelCounts {
        TraceabilityReadModelCounts {
            programs: self.programs.len(),
            objects: self.objects.len(),
            rules: self.rules.len(),
            executions: self.executions.len(),
            exceptions: self.exceptions.len(),
            controls: self.controls.len(),
            approvals: self.approvals.len(),
            packets: self.packets.len(),
            object_indexes: self.object_indexes.len(),
            program_object_links: self.program_to_objects.values().map(Vec::len).sum(),
            event_log_entries,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LoggedMigrationEvidenceEvent {
    sequence: u64,
    persisted_at: DateTime<Utc>,
    event: MigrationEvidenceEvent,
}

#[derive(Clone)]
pub struct PersistedTraceabilityStore {
    backend: TraceabilityStore,
    state: Arc<RwLock<TraceabilityState>>,
}

#[derive(Clone)]
enum TraceabilityStore {
    File { path: PathBuf },
    RocksDb { db: Arc<DB> },
}

impl PersistedTraceabilityStore {
    pub async fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::open_file(path).await
    }

    pub async fn open_file(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let state = if path.exists() {
            let bytes = tokio::fs::read(&path)
                .await
                .with_context(|| format!("failed to read traceability state at {}", path.display()))?;
            serde_json::from_slice(&bytes).context("failed to deserialize traceability state")?
        } else {
            TraceabilityState::default()
        };

        Ok(Self {
            backend: TraceabilityStore::File { path },
            state: Arc::new(RwLock::new(state)),
        })
    }

    pub async fn open_rocksdb(
        path: impl AsRef<Path>,
        legacy_file_path: Option<PathBuf>,
    ) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let db = open_rocksdb(&path)?;
        let is_empty = rocksdb_is_empty(&db)?;
        let initial_state = load_rocksdb_state(&db)?;
        let store = Self {
            backend: TraceabilityStore::RocksDb { db: Arc::new(db) },
            state: Arc::new(RwLock::new(initial_state)),
        };

        if is_empty {
            if let Some(legacy_file_path) = legacy_file_path {
                if legacy_file_path.exists() {
                    store.import_legacy_file(&legacy_file_path).await?;
                }
            }
        }

        Ok(store)
    }

    pub fn backend_kind(&self) -> TraceabilityStoreBackend {
        match &self.backend {
            TraceabilityStore::File { .. } => TraceabilityStoreBackend::File,
            TraceabilityStore::RocksDb { .. } => TraceabilityStoreBackend::RocksDb,
        }
    }

    pub async fn snapshot(&self) -> TraceabilityState {
        self.state.read().await.clone()
    }

    pub async fn mutate<F, T>(&self, mutator: F) -> Result<T>
    where
        F: FnOnce(&mut TraceabilityState) -> T,
    {
        let mut state = self.state.write().await;
        let result = mutator(&mut state);
        state.updated_at = Utc::now();
        self.persist_state(&state, &[]).await?;
        Ok(result)
    }

    pub async fn append_events_and_mutate<F, T>(
        &self,
        events: &[MigrationEvidenceEvent],
        mutator: F,
    ) -> Result<T>
    where
        F: FnOnce(&mut TraceabilityState, &[MigrationEvidenceEvent]) -> T,
    {
        let mut state = self.state.write().await;
        let accepted_events = events
            .iter()
            .filter(|event| state.processed_event_ids.insert(event.event_id.clone()))
            .cloned()
            .collect::<Vec<_>>();
        let start_sequence = state.last_event_sequence;
        let logged_events = accepted_events
            .iter()
            .enumerate()
            .map(|(index, event)| LoggedMigrationEvidenceEvent {
                sequence: start_sequence + index as u64 + 1,
                persisted_at: Utc::now(),
                event: event.clone(),
            })
            .collect::<Vec<_>>();
        let result = mutator(&mut state, &accepted_events);
        state.last_event_sequence += accepted_events.len() as u64;
        state.updated_at = Utc::now();
        self.persist_state(&state, &logged_events).await?;
        Ok(result)
    }

    pub async fn replay_events(&self) -> Result<Vec<MigrationEvidenceEvent>> {
        match &self.backend {
            TraceabilityStore::File { .. } => Err(anyhow!(
                "traceability event replay is not available for file-backed stores"
            )),
            TraceabilityStore::RocksDb { db, .. } => read_event_log(db),
        }
    }

    pub async fn replace_state_from_replay(&self, mut state: TraceabilityState) -> Result<()> {
        state.updated_at = Utc::now();
        state.last_rebuild_at = Some(Utc::now());
        let mut guard = self.state.write().await;
        *guard = state;
        self.persist_state(&guard, &[]).await
    }

    pub async fn runtime_status(&self) -> Result<TraceabilityRuntimeStatus> {
        let state = self.snapshot().await;
        let event_log_entries = match &self.backend {
            TraceabilityStore::File { .. } => 0,
            TraceabilityStore::RocksDb { db, .. } => count_events(db)?,
        };

        Ok(TraceabilityRuntimeStatus {
            backend: self.backend_kind(),
            replay_supported: matches!(&self.backend, TraceabilityStore::RocksDb { .. }),
            event_log_available: matches!(&self.backend, TraceabilityStore::RocksDb { .. }),
            read_models: state.counts(event_log_entries),
            event_bus: Default::default(),
            last_event_sequence: state.last_event_sequence,
            updated_at: state.updated_at,
            last_rebuild_at: state.last_rebuild_at,
            legacy_imported_at: state.legacy_imported_at,
        })
    }

    async fn import_legacy_file(&self, legacy_file_path: &Path) -> Result<()> {
        let bytes = tokio::fs::read(legacy_file_path)
            .await
            .with_context(|| format!("failed to read legacy traceability state at {}", legacy_file_path.display()))?;
        let mut state: TraceabilityState =
            serde_json::from_slice(&bytes).context("failed to deserialize legacy traceability state")?;
        let synthetic_events = legacy_state_to_events(&state)?;
        state.processed_event_ids = synthetic_events
            .iter()
            .map(|event| event.event_id.clone())
            .collect();
        state.updated_at = Utc::now();
        state.legacy_imported_at = Some(Utc::now());
        state.last_event_sequence = synthetic_events.len() as u64;
        let logged_events = synthetic_events
            .into_iter()
            .enumerate()
            .map(|(index, event)| LoggedMigrationEvidenceEvent {
                sequence: index as u64 + 1,
                persisted_at: state.updated_at,
                event,
            })
            .collect::<Vec<_>>();

        {
            let mut guard = self.state.write().await;
            *guard = state;
            self.persist_state(&guard, &logged_events).await?;
        }
        Ok(())
    }

    async fn persist_state(
        &self,
        state: &TraceabilityState,
        appended_events: &[LoggedMigrationEvidenceEvent],
    ) -> Result<()> {
        match &self.backend {
            TraceabilityStore::File { path } => persist_file(path, state).await,
            TraceabilityStore::RocksDb { db, .. } => persist_rocksdb(db, state, appended_events),
        }
    }
}

fn open_rocksdb(path: &Path) -> Result<DB> {
    let mut opts = Options::default();
    opts.create_if_missing(true);
    opts.create_missing_column_families(true);

    let cfs = vec![
        ColumnFamilyDescriptor::new(CF_META, Options::default()),
        ColumnFamilyDescriptor::new(CF_EVENTS, Options::default()),
        ColumnFamilyDescriptor::new(CF_PROGRAMS, Options::default()),
        ColumnFamilyDescriptor::new(CF_OBJECTS, Options::default()),
        ColumnFamilyDescriptor::new(CF_RULES, Options::default()),
        ColumnFamilyDescriptor::new(CF_EXECUTIONS, Options::default()),
        ColumnFamilyDescriptor::new(CF_EXCEPTIONS, Options::default()),
        ColumnFamilyDescriptor::new(CF_CONTROLS, Options::default()),
        ColumnFamilyDescriptor::new(CF_APPROVALS, Options::default()),
        ColumnFamilyDescriptor::new(CF_PACKETS, Options::default()),
        ColumnFamilyDescriptor::new(CF_OBJECT_INDEXES, Options::default()),
        ColumnFamilyDescriptor::new(CF_PROGRAM_OBJECTS, Options::default()),
        ColumnFamilyDescriptor::new(CF_EVENT_IDS, Options::default()),
    ];

    DB::open_cf_descriptors(&opts, path, cfs)
        .with_context(|| format!("failed to open traceability RocksDB at {}", path.display()))
}

fn rocksdb_is_empty(db: &DB) -> Result<bool> {
    let cf = cf(db, CF_META)?;
    Ok(db.iterator_cf(&cf, IteratorMode::Start).next().is_none())
}

fn load_rocksdb_state(db: &DB) -> Result<TraceabilityState> {
    let mut state = TraceabilityState::default();
    state.programs = load_map_cf::<MigrationProgram>(db, CF_PROGRAMS)?;
    state.objects = load_map_cf::<MigrationObject>(db, CF_OBJECTS)?;
    state.rules = load_map_cf::<TransformationRule>(db, CF_RULES)?;
    state.executions = load_map_cf::<ExecutionEvent>(db, CF_EXECUTIONS)?;
    state.exceptions = load_map_cf::<ExceptionRecord>(db, CF_EXCEPTIONS)?;
    state.controls = load_map_cf::<ControlResult>(db, CF_CONTROLS)?;
    state.approvals = load_map_cf::<ApprovalEvent>(db, CF_APPROVALS)?;
    state.packets = load_map_cf::<EvidencePacket>(db, CF_PACKETS)?;
    state.object_indexes = load_map_cf::<ObjectIndex>(db, CF_OBJECT_INDEXES)?;
    state.program_to_objects = load_map_cf::<Vec<String>>(db, CF_PROGRAM_OBJECTS)?;
    state.processed_event_ids = load_set_cf(db, CF_EVENT_IDS)?;
    state.updated_at = load_meta_datetime(db, META_UPDATED_AT)?.unwrap_or_else(Utc::now);
    state.last_event_sequence = load_meta_u64(db, META_LAST_EVENT_SEQUENCE)?.unwrap_or_default();
    state.last_rebuild_at = load_meta_datetime(db, META_LAST_REBUILD_AT)?;
    state.legacy_imported_at = load_meta_datetime(db, META_LEGACY_IMPORTED_AT)?;
    Ok(state)
}

async fn persist_file(path: &PathBuf, state: &TraceabilityState) -> Result<()> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let bytes = serde_json::to_vec_pretty(state).context("failed to serialize traceability state")?;
    tokio::fs::write(path, bytes)
        .await
        .with_context(|| format!("failed to persist traceability state to {}", path.display()))?;
    Ok(())
}

fn persist_rocksdb(
    db: &DB,
    state: &TraceabilityState,
    appended_events: &[LoggedMigrationEvidenceEvent],
) -> Result<()> {
    let mut batch = WriteBatch::default();

    for cf_name in [
        CF_PROGRAMS,
        CF_OBJECTS,
        CF_RULES,
        CF_EXECUTIONS,
        CF_EXCEPTIONS,
        CF_CONTROLS,
        CF_APPROVALS,
        CF_PACKETS,
        CF_OBJECT_INDEXES,
        CF_PROGRAM_OBJECTS,
        CF_EVENT_IDS,
        CF_META,
    ] {
        clear_cf(db, &mut batch, cf_name)?;
    }

    write_map_cf(db, &mut batch, CF_PROGRAMS, &state.programs)?;
    write_map_cf(db, &mut batch, CF_OBJECTS, &state.objects)?;
    write_map_cf(db, &mut batch, CF_RULES, &state.rules)?;
    write_map_cf(db, &mut batch, CF_EXECUTIONS, &state.executions)?;
    write_map_cf(db, &mut batch, CF_EXCEPTIONS, &state.exceptions)?;
    write_map_cf(db, &mut batch, CF_CONTROLS, &state.controls)?;
    write_map_cf(db, &mut batch, CF_APPROVALS, &state.approvals)?;
    write_map_cf(db, &mut batch, CF_PACKETS, &state.packets)?;
    write_map_cf(db, &mut batch, CF_OBJECT_INDEXES, &state.object_indexes)?;
    write_map_cf(db, &mut batch, CF_PROGRAM_OBJECTS, &state.program_to_objects)?;
    write_set_cf(db, &mut batch, CF_EVENT_IDS, &state.processed_event_ids)?;

    write_meta_string(db, &mut batch, META_UPDATED_AT, &state.updated_at.to_rfc3339())?;
    write_meta_string(
        db,
        &mut batch,
        META_LAST_EVENT_SEQUENCE,
        &state.last_event_sequence.to_string(),
    )?;
    if let Some(value) = state.last_rebuild_at.as_ref() {
        write_meta_string(db, &mut batch, META_LAST_REBUILD_AT, &value.to_rfc3339())?;
    }
    if let Some(value) = state.legacy_imported_at.as_ref() {
        write_meta_string(db, &mut batch, META_LEGACY_IMPORTED_AT, &value.to_rfc3339())?;
    }

    if !appended_events.is_empty() {
        let events_cf = cf(db, CF_EVENTS)?;
        for event in appended_events {
            batch.put_cf(
                &events_cf,
                sequence_key(event.sequence),
                serde_json::to_vec(event).context("failed to serialize traceability event log entry")?,
            );
        }
    }

    db.write(batch).context("failed to persist traceability RocksDB state")
}

fn clear_cf(db: &DB, batch: &mut WriteBatch, cf_name: &str) -> Result<()> {
    let handle = cf(db, cf_name)?;
    let keys = db
        .iterator_cf(&handle, IteratorMode::Start)
        .map(|item| item.map(|(key, _)| key.to_vec()))
        .collect::<std::result::Result<Vec<_>, _>>()
        .with_context(|| format!("failed to iterate column family '{cf_name}'"))?;
    for key in keys {
        batch.delete_cf(&handle, key);
    }
    Ok(())
}

fn load_map_cf<T>(db: &DB, cf_name: &str) -> Result<HashMap<String, T>>
where
    T: for<'de> Deserialize<'de>,
{
    let handle = cf(db, cf_name)?;
    let mut items = HashMap::new();
    for entry in db.iterator_cf(&handle, IteratorMode::Start) {
        let (key, value) = entry.with_context(|| format!("failed to iterate column family '{cf_name}'"))?;
        let key = String::from_utf8(key.to_vec()).with_context(|| format!("invalid UTF-8 key in '{cf_name}'"))?;
        let value = serde_json::from_slice(&value)
            .with_context(|| format!("failed to deserialize value from '{cf_name}' for key '{key}'"))?;
        items.insert(key, value);
    }
    Ok(items)
}

fn load_set_cf(db: &DB, cf_name: &str) -> Result<HashSet<String>> {
    let handle = cf(db, cf_name)?;
    let mut items = HashSet::new();
    for entry in db.iterator_cf(&handle, IteratorMode::Start) {
        let (key, _) = entry.with_context(|| format!("failed to iterate column family '{cf_name}'"))?;
        let key = String::from_utf8(key.to_vec()).with_context(|| format!("invalid UTF-8 key in '{cf_name}'"))?;
        items.insert(key);
    }
    Ok(items)
}

fn write_map_cf<T>(db: &DB, batch: &mut WriteBatch, cf_name: &str, items: &HashMap<String, T>) -> Result<()>
where
    T: Serialize,
{
    let handle = cf(db, cf_name)?;
    let mut keys = items.keys().cloned().collect::<Vec<_>>();
    keys.sort();
    for key in keys {
        let value = items
            .get(&key)
            .ok_or_else(|| anyhow!("missing value for key '{}' in '{}'", key, cf_name))?;
        batch.put_cf(
            &handle,
            key.as_bytes(),
            serde_json::to_vec(value)
                .with_context(|| format!("failed to serialize value for key '{}' in '{}'", key, cf_name))?,
        );
    }
    Ok(())
}

fn write_set_cf(db: &DB, batch: &mut WriteBatch, cf_name: &str, items: &HashSet<String>) -> Result<()> {
    let handle = cf(db, cf_name)?;
    let mut keys = items.iter().cloned().collect::<Vec<_>>();
    keys.sort();
    for key in keys {
        batch.put_cf(&handle, key.as_bytes(), []);
    }
    Ok(())
}

fn load_meta_string(db: &DB, key: &str) -> Result<Option<String>> {
    let handle = cf(db, CF_META)?;
    db.get_cf(&handle, key.as_bytes())
        .context("failed to read traceability metadata")?
        .map(|value| {
            String::from_utf8(value.to_vec()).context("traceability metadata contains invalid UTF-8")
        })
        .transpose()
}

fn load_meta_datetime(db: &DB, key: &str) -> Result<Option<DateTime<Utc>>> {
    load_meta_string(db, key)?
        .map(|value| {
            chrono::DateTime::parse_from_rfc3339(&value)
                .map(|value| value.with_timezone(&Utc))
                .with_context(|| format!("invalid RFC3339 timestamp for metadata key '{}'", key))
        })
        .transpose()
}

fn load_meta_u64(db: &DB, key: &str) -> Result<Option<u64>> {
    load_meta_string(db, key)?
        .map(|value| {
            value
                .parse::<u64>()
                .with_context(|| format!("invalid u64 metadata value for key '{}'", key))
        })
        .transpose()
}

fn write_meta_string(db: &DB, batch: &mut WriteBatch, key: &str, value: &str) -> Result<()> {
    let handle = cf(db, CF_META)?;
    batch.put_cf(&handle, key.as_bytes(), value.as_bytes());
    Ok(())
}

fn cf<'a>(db: &'a DB, name: &str) -> Result<&'a rocksdb::ColumnFamily> {
    db.cf_handle(name)
        .ok_or_else(|| anyhow!("missing traceability RocksDB column family '{}'", name))
}

fn sequence_key(sequence: u64) -> [u8; 8] {
    sequence.to_be_bytes()
}

fn read_event_log(db: &DB) -> Result<Vec<MigrationEvidenceEvent>> {
    let handle = cf(db, CF_EVENTS)?;
    let mut events = Vec::new();
    for entry in db.iterator_cf(&handle, IteratorMode::Start) {
        let (_, value) = entry.context("failed to iterate traceability event log")?;
        let logged: LoggedMigrationEvidenceEvent =
            serde_json::from_slice(&value).context("failed to deserialize traceability event log entry")?;
        events.push(logged.event);
    }
    Ok(events)
}

fn count_events(db: &DB) -> Result<usize> {
    let handle = cf(db, CF_EVENTS)?;
    db.iterator_cf(&handle, IteratorMode::Start)
        .try_fold(0usize, |count, item| item.map(|_| count + 1))
        .context("failed to count traceability event log entries")
}

fn legacy_state_to_events(state: &TraceabilityState) -> Result<Vec<MigrationEvidenceEvent>> {
    let mut events = Vec::new();
    let mut seen = HashSet::new();

    for program in state.programs.values() {
        let object_id = state
            .program_to_objects
            .get(&program.program_id)
            .and_then(|items| items.first())
            .cloned()
            .unwrap_or_default();
        push_synthetic_event(
            &mut events,
            &mut seen,
            MigrationEvidenceArtifactType::Program,
            &program.program_id,
            &object_id,
            None,
            program,
        )?;
    }

    for object in state.objects.values() {
        push_synthetic_event(
            &mut events,
            &mut seen,
            MigrationEvidenceArtifactType::Object,
            &object.program_id,
            &object.object_id,
            None,
            object,
        )?;

        if let Some(index) = state.object_indexes.get(&object.object_id) {
            push_indexed_events(
                &mut events,
                &mut seen,
                state,
                object,
                index,
                MigrationEvidenceArtifactType::TransformationRule,
            )?;
            push_indexed_events(
                &mut events,
                &mut seen,
                state,
                object,
                index,
                MigrationEvidenceArtifactType::ExecutionEvent,
            )?;
            push_indexed_events(
                &mut events,
                &mut seen,
                state,
                object,
                index,
                MigrationEvidenceArtifactType::ExceptionRecord,
            )?;
            push_indexed_events(
                &mut events,
                &mut seen,
                state,
                object,
                index,
                MigrationEvidenceArtifactType::ControlResult,
            )?;
            push_indexed_events(
                &mut events,
                &mut seen,
                state,
                object,
                index,
                MigrationEvidenceArtifactType::ApprovalEvent,
            )?;
        }
    }

    for packet in state.packets.values() {
        push_synthetic_event(
            &mut events,
            &mut seen,
            MigrationEvidenceArtifactType::EvidencePacket,
            &packet.program_id,
            &packet.object_id,
            Some(packet.value_key.clone()),
            packet,
        )?;
    }

    Ok(events)
}

fn push_indexed_events(
    events: &mut Vec<MigrationEvidenceEvent>,
    seen: &mut HashSet<String>,
    state: &TraceabilityState,
    object: &MigrationObject,
    index: &ObjectIndex,
    artifact_type: MigrationEvidenceArtifactType,
) -> Result<()> {
    match artifact_type {
        MigrationEvidenceArtifactType::TransformationRule => {
            push_events_for_map(events, seen, &state.rules, &object.program_id, &object.object_id, &index.value_key_rules)?;
            push_events_for_ids(events, seen, &state.rules, &object.program_id, &object.object_id, &index.object_level_rule_ids, None, artifact_type)?;
        }
        MigrationEvidenceArtifactType::ExecutionEvent => {
            push_events_for_map(events, seen, &state.executions, &object.program_id, &object.object_id, &index.value_key_execution_ids)?;
            push_events_for_ids(events, seen, &state.executions, &object.program_id, &object.object_id, &index.object_level_execution_ids, None, artifact_type)?;
        }
        MigrationEvidenceArtifactType::ExceptionRecord => {
            push_events_for_map(events, seen, &state.exceptions, &object.program_id, &object.object_id, &index.value_key_exception_ids)?;
            push_events_for_ids(events, seen, &state.exceptions, &object.program_id, &object.object_id, &index.object_level_exception_ids, None, artifact_type)?;
        }
        MigrationEvidenceArtifactType::ControlResult => {
            push_events_for_map(events, seen, &state.controls, &object.program_id, &object.object_id, &index.value_key_control_ids)?;
            push_events_for_ids(events, seen, &state.controls, &object.program_id, &object.object_id, &index.object_level_control_ids, None, artifact_type)?;
        }
        MigrationEvidenceArtifactType::ApprovalEvent => {
            push_events_for_map(events, seen, &state.approvals, &object.program_id, &object.object_id, &index.value_key_approval_ids)?;
            push_events_for_ids(events, seen, &state.approvals, &object.program_id, &object.object_id, &index.object_level_approval_ids, None, artifact_type)?;
        }
        _ => {}
    }
    Ok(())
}

fn push_events_for_map<T>(
    events: &mut Vec<MigrationEvidenceEvent>,
    seen: &mut HashSet<String>,
    store: &HashMap<String, T>,
    program_id: &str,
    object_id: &str,
    ids_by_value_key: &HashMap<String, Vec<String>>,
) -> Result<()>
where
    T: Serialize,
{
    for (value_key, ids) in ids_by_value_key {
        push_events_for_ids(
            events,
            seen,
            store,
            program_id,
            object_id,
            ids,
            Some(value_key.as_str()),
            infer_artifact_type::<T>(),
        )?;
    }
    Ok(())
}

fn push_events_for_ids<T>(
    events: &mut Vec<MigrationEvidenceEvent>,
    seen: &mut HashSet<String>,
    store: &HashMap<String, T>,
    program_id: &str,
    object_id: &str,
    ids: &[String],
    value_key: Option<&str>,
    artifact_type: MigrationEvidenceArtifactType,
) -> Result<()>
where
    T: Serialize,
{
    for id in ids {
        if let Some(value) = store.get(id) {
            push_synthetic_event(
                events,
                seen,
                artifact_type.clone(),
                program_id,
                object_id,
                value_key.map(str::to_owned),
                value,
            )?;
        }
    }
    Ok(())
}

fn infer_artifact_type<T>() -> MigrationEvidenceArtifactType {
    let name = std::any::type_name::<T>();
    if name.ends_with("TransformationRule") {
        MigrationEvidenceArtifactType::TransformationRule
    } else if name.ends_with("ExecutionEvent") {
        MigrationEvidenceArtifactType::ExecutionEvent
    } else if name.ends_with("ExceptionRecord") {
        MigrationEvidenceArtifactType::ExceptionRecord
    } else if name.ends_with("ControlResult") {
        MigrationEvidenceArtifactType::ControlResult
    } else {
        MigrationEvidenceArtifactType::ApprovalEvent
    }
}

fn push_synthetic_event<T>(
    events: &mut Vec<MigrationEvidenceEvent>,
    seen: &mut HashSet<String>,
    artifact_type: MigrationEvidenceArtifactType,
    program_id: &str,
    object_id: &str,
    value_key: Option<String>,
    payload: &T,
) -> Result<()>
where
    T: Serialize,
{
    let payload = serde_json::to_value(payload).context("failed to serialize legacy traceability payload")?;
    let identity = format!(
        "{:?}|{}|{}|{}",
        artifact_type,
        payload_identity(&artifact_type, &payload)?,
        object_id,
        value_key.clone().unwrap_or_default()
    );
    if !seen.insert(identity) {
        return Ok(());
    }

    events.push(MigrationEvidenceEvent::new(
        "traceability-legacy-import",
        "legacy-import",
        graphica_core::migration_evidence::MigrationConnectorVendor::Generic,
        program_id.to_string(),
        object_id.to_string(),
        artifact_type,
        value_key,
        payload,
    ));
    Ok(())
}

fn payload_identity(artifact_type: &MigrationEvidenceArtifactType, payload: &serde_json::Value) -> Result<String> {
    let field = match artifact_type {
        MigrationEvidenceArtifactType::Program => "program_id",
        MigrationEvidenceArtifactType::Object => "object_id",
        MigrationEvidenceArtifactType::TransformationRule => "rule_id",
        MigrationEvidenceArtifactType::ExecutionEvent => "execution_id",
        MigrationEvidenceArtifactType::ExceptionRecord => "exception_id",
        MigrationEvidenceArtifactType::ControlResult => "control_id",
        MigrationEvidenceArtifactType::ApprovalEvent => "approval_id",
        MigrationEvidenceArtifactType::EvidencePacket => "packet_id",
    };

    payload
        .get(field)
        .and_then(|value| value.as_str())
        .map(str::to_owned)
        .ok_or_else(|| anyhow!("legacy migration evidence payload missing '{}'", field))
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphica_core::migration_evidence::{
        ApprovalStatus, ControlStatus, ExceptionSeverity, ExceptionStatus,
        MigrationObjectType, TransformationRuleType,
    };
    use serde_json::json;
    use tempfile::tempdir;

    fn sample_state() -> TraceabilityState {
        let mut state = TraceabilityState::default();
        state.programs.insert(
            "program-1".to_string(),
            MigrationProgram {
                program_id: "program-1".to_string(),
                name: "RISE wave 1".to_string(),
                customer_name: None,
                source_landscape: None,
                target_landscape: None,
                tags: vec![],
                metadata: HashMap::new(),
                created_at: Utc::now(),
                updated_at: Utc::now(),
            },
        );
        state.objects.insert(
            "object-1".to_string(),
            MigrationObject {
                object_id: "object-1".to_string(),
                program_id: "program-1".to_string(),
                object_type: MigrationObjectType::BusinessObject,
                name: "SalesOrder".to_string(),
                description: None,
                source_record_id: Some("SO-1".to_string()),
                target_record_id: Some("SO-1".to_string()),
                tags: vec![],
                metadata: HashMap::new(),
            },
        );
        state.program_to_objects.insert("program-1".to_string(), vec!["object-1".to_string()]);
        state.rules.insert(
            "rule-1".to_string(),
            TransformationRule {
                rule_id: "rule-1".to_string(),
                rule_type: TransformationRuleType::Mapping,
                name: "Map amount".to_string(),
                description: None,
                source_fields: vec![],
                target_fields: vec![],
                expression: Some("NETWR * 1.0".to_string()),
                filter_predicate: None,
                default_value: None,
                aggregation: None,
                metadata: HashMap::new(),
            },
        );
        state.controls.insert(
            "control-1".to_string(),
            ControlResult {
                control_id: "control-1".to_string(),
                program_id: "program-1".to_string(),
                object_id: "object-1".to_string(),
                control_name: "amount-reconciliation".to_string(),
                control_type: "verification".to_string(),
                status: ControlStatus::Passed,
                summary: "passed".to_string(),
                expected_value: Some(json!(100)),
                actual_value: Some(json!(100)),
                tolerance: Some(0.0),
                executed_at: Utc::now(),
                evidence_refs: vec![],
                metadata: HashMap::new(),
            },
        );
        state.approvals.insert(
            "approval-1".to_string(),
            ApprovalEvent {
                approval_id: "approval-1".to_string(),
                program_id: "program-1".to_string(),
                object_id: "object-1".to_string(),
                approver_role: "data_owner".to_string(),
                approver_id: "owner-1".to_string(),
                status: ApprovalStatus::Approved,
                comment: None,
                approved_at: Utc::now(),
                evidence_refs: vec![],
                attestation_ref: None,
                metadata: HashMap::new(),
            },
        );
        state.exceptions.insert(
            "exception-1".to_string(),
            ExceptionRecord {
                exception_id: "exception-1".to_string(),
                program_id: "program-1".to_string(),
                object_id: "object-1".to_string(),
                severity: ExceptionSeverity::Warning,
                status: ExceptionStatus::Accepted,
                category: "rounding".to_string(),
                message: "accepted delta".to_string(),
                source_value: Some(json!(100)),
                target_value: Some(json!(101)),
                remediation: None,
                detected_at: Utc::now(),
                resolved_at: None,
                metadata: HashMap::new(),
            },
        );
        state.object_indexes.insert(
            "object-1".to_string(),
            ObjectIndex {
                value_key_rules: HashMap::from([("SO-1::$.amount".to_string(), vec!["rule-1".to_string()])]),
                value_key_execution_ids: HashMap::new(),
                value_key_exception_ids: HashMap::from([("SO-1::$.amount".to_string(), vec!["exception-1".to_string()])]),
                value_key_control_ids: HashMap::from([("SO-1::$.amount".to_string(), vec!["control-1".to_string()])]),
                value_key_approval_ids: HashMap::new(),
                value_key_packet_ids: HashMap::new(),
                object_level_rule_ids: vec![],
                object_level_execution_ids: vec![],
                object_level_exception_ids: vec![],
                object_level_control_ids: vec![],
                object_level_approval_ids: vec!["approval-1".to_string()],
                object_level_packet_ids: vec![],
            },
        );
        state
    }

    #[tokio::test]
    async fn rocksdb_store_persists_event_log_and_runtime_status() {
        let temp = tempdir().unwrap();
        let store = PersistedTraceabilityStore::open_rocksdb(temp.path().join("traceability.db"), None)
            .await
            .unwrap();
        let event = MigrationEvidenceEvent::new(
            "connector-1",
            "run-1",
            graphica_core::migration_evidence::MigrationConnectorVendor::Generic,
            "program-1",
            "object-1",
            MigrationEvidenceArtifactType::Object,
            None,
            serde_json::to_value(MigrationObject {
                object_id: "object-1".to_string(),
                program_id: "program-1".to_string(),
                object_type: MigrationObjectType::BusinessObject,
                name: "SalesOrder".to_string(),
                description: None,
                source_record_id: None,
                target_record_id: None,
                tags: vec![],
                metadata: HashMap::new(),
            })
            .unwrap(),
        );

        store
            .append_events_and_mutate(&[event], |state, accepted_events| {
                assert_eq!(accepted_events.len(), 1);
                state.objects.insert(
                    "object-1".to_string(),
                    MigrationObject {
                        object_id: "object-1".to_string(),
                        program_id: "program-1".to_string(),
                        object_type: MigrationObjectType::BusinessObject,
                        name: "SalesOrder".to_string(),
                        description: None,
                        source_record_id: None,
                        target_record_id: None,
                        tags: vec![],
                        metadata: HashMap::new(),
                    },
                );
            })
            .await
            .unwrap();

        let status = store.runtime_status().await.unwrap();
        assert_eq!(status.backend, TraceabilityStoreBackend::RocksDb);
        assert_eq!(status.read_models.objects, 1);
        assert_eq!(status.read_models.event_log_entries, 1);
        assert_eq!(status.last_event_sequence, 1);

        let replayed = store.replay_events().await.unwrap();
        assert_eq!(replayed.len(), 1);
    }

    #[tokio::test]
    async fn rocksdb_store_imports_legacy_file_into_event_log() {
        let temp = tempdir().unwrap();
        let legacy_path = temp.path().join("legacy.json");
        tokio::fs::write(&legacy_path, serde_json::to_vec_pretty(&sample_state()).unwrap())
            .await
            .unwrap();

        let store = PersistedTraceabilityStore::open_rocksdb(
            temp.path().join("traceability.db"),
            Some(legacy_path),
        )
        .await
        .unwrap();

        let status = store.runtime_status().await.unwrap();
        assert!(status.legacy_imported_at.is_some());
        assert!(status.read_models.event_log_entries >= 5);
        assert_eq!(status.read_models.objects, 1);
        assert_eq!(status.read_models.rules, 1);
    }

    #[tokio::test]
    async fn rocksdb_store_deduplicates_event_ids_across_retries() {
        let temp = tempdir().unwrap();
        let store = PersistedTraceabilityStore::open_rocksdb(temp.path().join("traceability.db"), None)
            .await
            .unwrap();
        let event = MigrationEvidenceEvent::new(
            "connector-1",
            "run-1",
            graphica_core::migration_evidence::MigrationConnectorVendor::Generic,
            "program-1",
            "object-1",
            MigrationEvidenceArtifactType::Object,
            None,
            serde_json::to_value(MigrationObject {
                object_id: "object-1".to_string(),
                program_id: "program-1".to_string(),
                object_type: MigrationObjectType::BusinessObject,
                name: "SalesOrder".to_string(),
                description: None,
                source_record_id: None,
                target_record_id: None,
                tags: vec![],
                metadata: HashMap::new(),
            })
            .unwrap(),
        );

        for _ in 0..2 {
            store
                .append_events_and_mutate(std::slice::from_ref(&event), |state, accepted_events| {
                    for accepted in accepted_events {
                        let object: MigrationObject = serde_json::from_value(accepted.payload.clone()).unwrap();
                        state.objects.insert(object.object_id.clone(), object);
                    }
                })
                .await
                .unwrap();
        }

        let status = store.runtime_status().await.unwrap();
        assert_eq!(status.read_models.objects, 1);
        assert_eq!(status.read_models.event_log_entries, 1);
        assert_eq!(status.last_event_sequence, 1);
    }
}
