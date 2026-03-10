//! RocksDB store for versioned ontology bindings.

use anyhow::{Context, Result};
use rocksdb::{ColumnFamilyDescriptor, IteratorMode, Options, DB};
use std::path::Path;
use std::sync::Arc;

use super::types::OntologyPhysicalBinding;

const CF_BINDING_DATA: &str = "binding_data";
const CF_BINDING_CURRENT: &str = "binding_current";
const CF_BINDING_HISTORY: &str = "binding_history";
const CF_BINDING_BY_SOURCE: &str = "binding_by_source";
const CF_BINDING_BY_ONTOLOGY: &str = "binding_by_ontology";

/// Persistent store for ontology/physical bindings.
pub struct BindingStore {
    db: Arc<DB>,
}

impl BindingStore {
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self> {
        let mut opts = Options::default();
        opts.create_if_missing(true);
        opts.create_missing_column_families(true);

        let cfs = vec![
            ColumnFamilyDescriptor::new(CF_BINDING_DATA, Options::default()),
            ColumnFamilyDescriptor::new(CF_BINDING_CURRENT, Options::default()),
            ColumnFamilyDescriptor::new(CF_BINDING_HISTORY, Options::default()),
            ColumnFamilyDescriptor::new(CF_BINDING_BY_SOURCE, Options::default()),
            ColumnFamilyDescriptor::new(CF_BINDING_BY_ONTOLOGY, Options::default()),
        ];

        let db = DB::open_cf_descriptors(&opts, path, cfs)
            .context("Failed to open binding store RocksDB")?;
        Ok(Self { db: Arc::new(db) })
    }

    pub fn get_binding(&self, binding_id: &str) -> Result<Option<OntologyPhysicalBinding>> {
        let cf_data = self
            .db
            .cf_handle(CF_BINDING_DATA)
            .context("CF_BINDING_DATA not found")?;

        match self.db.get_cf(cf_data, binding_id.as_bytes())? {
            Some(bytes) => {
                let binding: OntologyPhysicalBinding = serde_json::from_slice(&bytes)
                    .context("Failed to deserialize binding record")?;
                Ok(Some(binding))
            }
            None => Ok(None),
        }
    }

    pub fn get_current_binding(
        &self,
        source_id: &str,
        entity_uri: &str,
        ontology_uri: &str,
    ) -> Result<Option<OntologyPhysicalBinding>> {
        let cf_current = self
            .db
            .cf_handle(CF_BINDING_CURRENT)
            .context("CF_BINDING_CURRENT not found")?;
        let lookup_key = OntologyPhysicalBinding::lookup_key(source_id, entity_uri, ontology_uri);

        match self.db.get_cf(cf_current, lookup_key.as_bytes())? {
            Some(id_bytes) => {
                let binding_id = String::from_utf8(id_bytes.to_vec())
                    .context("Invalid UTF-8 binding ID in current index")?;
                self.get_binding(&binding_id)
            }
            None => Ok(None),
        }
    }

    pub fn put_binding(&self, binding: &OntologyPhysicalBinding) -> Result<()> {
        let cf_data = self
            .db
            .cf_handle(CF_BINDING_DATA)
            .context("CF_BINDING_DATA not found")?;
        let cf_current = self
            .db
            .cf_handle(CF_BINDING_CURRENT)
            .context("CF_BINDING_CURRENT not found")?;
        let cf_history = self
            .db
            .cf_handle(CF_BINDING_HISTORY)
            .context("CF_BINDING_HISTORY not found")?;
        let cf_by_source = self
            .db
            .cf_handle(CF_BINDING_BY_SOURCE)
            .context("CF_BINDING_BY_SOURCE not found")?;
        let cf_by_ontology = self
            .db
            .cf_handle(CF_BINDING_BY_ONTOLOGY)
            .context("CF_BINDING_BY_ONTOLOGY not found")?;

        let bytes = serde_json::to_vec(binding).context("Failed to serialize binding")?;
        self.db.put_cf(cf_data, binding.id.as_bytes(), bytes)?;

        let lookup_key = OntologyPhysicalBinding::lookup_key(
            &binding.source_id,
            &binding.entity_uri,
            &binding.ontology_uri,
        );
        self.db
            .put_cf(cf_current, lookup_key.as_bytes(), binding.id.as_bytes())?;

        let history_key = format!("{}|{:08}", lookup_key, binding.version);
        self.db
            .put_cf(cf_history, history_key.as_bytes(), binding.id.as_bytes())?;

        let source_key = format!("{}|{}", binding.source_id, binding.id);
        self.db.put_cf(cf_by_source, source_key.as_bytes(), b"1")?;

        let ontology_key = format!("{}|{}", binding.ontology_uri, binding.id);
        self.db
            .put_cf(cf_by_ontology, ontology_key.as_bytes(), b"1")?;

        Ok(())
    }

    pub fn list_current_bindings_for_entity(
        &self,
        source_id: &str,
        entity_uri: &str,
    ) -> Result<Vec<OntologyPhysicalBinding>> {
        let cf_current = self
            .db
            .cf_handle(CF_BINDING_CURRENT)
            .context("CF_BINDING_CURRENT not found")?;

        let prefix = format!("{}|{}|", source_id, entity_uri);
        let mut bindings = Vec::new();

        for item in self.db.iterator_cf(cf_current, IteratorMode::Start) {
            let (key, value) = item.context("Failed to iterate current binding index")?;
            let key_str = String::from_utf8(key.to_vec()).context("Invalid UTF-8 key")?;
            if !key_str.starts_with(&prefix) {
                continue;
            }

            let binding_id = String::from_utf8(value.to_vec())
                .context("Invalid UTF-8 binding ID in current index")?;
            if let Some(binding) = self.get_binding(&binding_id)? {
                bindings.push(binding);
            }
        }

        Ok(bindings)
    }

    pub fn list_binding_history(
        &self,
        source_id: &str,
        entity_uri: &str,
        ontology_uri: &str,
    ) -> Result<Vec<OntologyPhysicalBinding>> {
        let cf_history = self
            .db
            .cf_handle(CF_BINDING_HISTORY)
            .context("CF_BINDING_HISTORY not found")?;
        let prefix = OntologyPhysicalBinding::lookup_key(source_id, entity_uri, ontology_uri);

        let mut versions = Vec::new();
        for item in self.db.iterator_cf(cf_history, IteratorMode::Start) {
            let (key, value) = item.context("Failed to iterate binding history")?;
            let key_str = String::from_utf8(key.to_vec()).context("Invalid UTF-8 history key")?;
            if !key_str.starts_with(&prefix) {
                continue;
            }
            let binding_id = String::from_utf8(value.to_vec())
                .context("Invalid UTF-8 binding ID in history index")?;
            if let Some(binding) = self.get_binding(&binding_id)? {
                versions.push(binding);
            }
        }

        versions.sort_by_key(|b| b.version);
        Ok(versions)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mapping::bindings::types::{BindingProvenance, BindingStatus};

    fn sample_binding() -> OntologyPhysicalBinding {
        OntologyPhysicalBinding {
            id: "bind_1".to_string(),
            source_id: "src".to_string(),
            entity_uri: "http://e/Patient".to_string(),
            ontology_uri: "http://e/name".to_string(),
            table: "patients".to_string(),
            column: "name".to_string(),
            dialect: "postgresql".to_string(),
            confidence: 0.98,
            status: BindingStatus::Active,
            version: 1,
            binding_hash: OntologyPhysicalBinding::compute_hash("patients", "name", "postgresql"),
            created_at: 1,
            updated_at: 1,
            created_by: "tester".to_string(),
            updated_by: "tester".to_string(),
            provenance: BindingProvenance::default(),
        }
    }

    #[test]
    fn stores_and_reads_current_binding() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = BindingStore::new(dir.path()).expect("store");

        let binding = sample_binding();
        store.put_binding(&binding).expect("put binding");

        let loaded = store
            .get_current_binding(
                &binding.source_id,
                &binding.entity_uri,
                &binding.ontology_uri,
            )
            .expect("load current")
            .expect("binding exists");

        assert_eq!(loaded.id, binding.id);
        assert_eq!(loaded.version, 1);
    }
}
