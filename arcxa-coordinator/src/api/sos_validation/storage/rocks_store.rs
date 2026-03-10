//! High-performance RocksDB storage for SoS validation
//!
//! Tightly integrated with Graphica's storage layer using optimized column families,
//! batch writes, and efficient indexing for sub-millisecond lookups.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rocksdb::{IteratorMode, Options, WriteBatch, DB};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

use crate::storage::rocks_config::{self, RocksProfile};

// Column families for SoS data (optimized for high-speed access)
const CF_SYSTEMS: &str = "sos_systems"; // system_id -> System
const CF_INTERFACES: &str = "sos_interfaces"; // interface_id -> Interface
const CF_CONTRACTS: &str = "sos_contracts"; // contract_id -> Contract
const CF_VALIDATION_REPORTS: &str = "sos_val_reports"; // report_id -> ValidationReport

// Secondary indexes for fast lookups
const CF_SYSTEM_BY_TYPE: &str = "sos_sys_type_idx"; // (type, system_id) -> empty
const CF_SYSTEM_BY_VENDOR: &str = "sos_sys_vendor_idx"; // (vendor, system_id) -> empty
const CF_INTERFACE_BY_SYSTEM: &str = "sos_if_sys_idx"; // (system_id, interface_id) -> empty
const CF_CONTRACT_BY_PROVIDER: &str = "sos_con_prov_idx"; // (provider_if_id, contract_id) -> empty
const CF_CONTRACT_BY_CONSUMER: &str = "sos_con_cons_idx"; // (consumer_if_id, contract_id) -> empty

/// System entity (optimized for serialization speed)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct System {
    pub system_id: String,
    pub system_name: String,
    pub system_type: String,
    pub vendor: String,
    pub version: String,
    pub classification: String,
    pub description: Option<String>,
    pub deployment: HashMap<String, serde_json::Value>,
    pub capabilities: HashMap<String, serde_json::Value>,
    pub tags: Vec<String>,
    pub active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Interface entity
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Interface {
    pub interface_id: String,
    pub system_id: String,
    pub interface_name: String,
    pub direction: String,
    pub protocol: String,
    pub data_format: String,
    pub schema: serde_json::Value,
    pub coordinate_system: Option<String>,
    pub unit_system: Option<String>,
    pub metadata: HashMap<String, serde_json::Value>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Data contract entity
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Contract {
    pub contract_id: String,
    pub contract_name: String,
    pub provider_interface_id: String,
    pub consumer_interface_id: String,
    pub sla_metrics: Vec<SlaMetric>,
    pub transformation_rules: HashMap<String, serde_json::Value>,
    pub description: Option<String>,
    pub tags: Vec<String>,
    pub approved: bool,
    pub signed: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlaMetric {
    pub name: String,
    pub value: f64,
    pub operator: String,
    pub unit: Option<String>,
}

/// High-performance SoS RocksDB store
pub struct SosStore {
    db: Arc<DB>,
}

impl SosStore {
    /// Create new SoS store with HighThroughput profile for maximum performance
    pub fn new(path: &str) -> Result<Self> {
        Self::with_profile(path, RocksProfile::HighThroughput)
    }

    /// Create SoS store with specified performance profile
    pub fn with_profile(path: &str, profile: RocksProfile) -> Result<Self> {
        // Print configuration summary
        rocks_config::print_config_summary(profile);

        // Get optimized options for profile
        let opts = rocks_config::create_options(profile);

        // Define all column families
        let cfs = vec![
            CF_SYSTEMS,
            CF_INTERFACES,
            CF_CONTRACTS,
            CF_VALIDATION_REPORTS,
            CF_SYSTEM_BY_TYPE,
            CF_SYSTEM_BY_VENDOR,
            CF_INTERFACE_BY_SYSTEM,
            CF_CONTRACT_BY_PROVIDER,
            CF_CONTRACT_BY_CONSUMER,
        ];

        let db = DB::open_cf(&opts, path, cfs).context("Failed to open SoS RocksDB")?;

        Ok(Self { db: Arc::new(db) })
    }

    /// Get column family handle (optimized inline for hot path)
    #[inline(always)]
    fn cf(&self, name: &str) -> Result<&rocksdb::ColumnFamily> {
        self.db
            .cf_handle(name)
            .ok_or_else(|| anyhow::anyhow!("Column family {} not found", name))
    }

    /// Get reference to underlying DB (for sharing across repositories)
    pub fn db(&self) -> &Arc<DB> {
        &self.db
    }

    // ========================================================================
    // System Operations (High-speed CRUD)
    // ========================================================================

    /// Store system (optimized for write throughput)
    pub fn put_system(&self, system: &System) -> Result<()> {
        let cf_systems = self.cf(CF_SYSTEMS)?;
        let cf_type_idx = self.cf(CF_SYSTEM_BY_TYPE)?;
        let cf_vendor_idx = self.cf(CF_SYSTEM_BY_VENDOR)?;

        // Load existing system (if any) to update secondary indexes correctly
        let existing = self
            .db
            .get_cf(cf_systems, system.system_id.as_bytes())?
            .map(|data| serde_json::from_slice::<System>(&data))
            .transpose()
            .context("Failed to deserialize existing system")?;

        // Serialize once for efficiency
        let data = serde_json::to_vec(system).context("Failed to serialize system")?;

        // Use batch writes for atomicity + performance
        let mut batch = WriteBatch::default();

        // Remove old secondary indexes if type/vendor changed
        if let Some(existing_system) = existing {
            if existing_system.system_type != system.system_type {
                let old_type_key = format!("{}|{}", existing_system.system_type, system.system_id);
                batch.delete_cf(cf_type_idx, old_type_key.as_bytes());
            }

            if existing_system.vendor != system.vendor {
                let old_vendor_key = format!("{}|{}", existing_system.vendor, system.system_id);
                batch.delete_cf(cf_vendor_idx, old_vendor_key.as_bytes());
            }
        }

        // Primary storage
        batch.put_cf(cf_systems, system.system_id.as_bytes(), &data);

        // Secondary indexes (empty values for space efficiency)
        let type_key = format!("{}|{}", system.system_type, system.system_id);
        batch.put_cf(cf_type_idx, type_key.as_bytes(), b"");

        let vendor_key = format!("{}|{}", system.vendor, system.system_id);
        batch.put_cf(cf_vendor_idx, vendor_key.as_bytes(), b"");

        self.db.write(batch).context("Failed to write system batch")
    }

    /// Get system by ID (sub-millisecond lookup)
    pub fn get_system(&self, system_id: &str) -> Result<Option<System>> {
        let cf = self.cf(CF_SYSTEMS)?;

        match self.db.get_cf(cf, system_id.as_bytes())? {
            Some(data) => {
                let system =
                    serde_json::from_slice(&data).context("Failed to deserialize system")?;
                Ok(Some(system))
            }
            None => Ok(None),
        }
    }

    /// List systems by type (uses secondary index for speed)
    pub fn list_systems_by_type(&self, system_type: &str, limit: usize) -> Result<Vec<System>> {
        let cf_type_idx = self.cf(CF_SYSTEM_BY_TYPE)?;
        let cf_systems = self.cf(CF_SYSTEMS)?;

        let prefix = format!("{}|", system_type);
        let mut systems = Vec::new();

        // Prefix scan on index (very fast)
        for item in self.db.iterator_cf(
            cf_type_idx,
            IteratorMode::From(prefix.as_bytes(), rocksdb::Direction::Forward),
        ) {
            let (key, _) = item?;
            let key_str = std::str::from_utf8(&key)?;

            if !key_str.starts_with(&prefix) {
                break;
            }

            // Extract system_id from "type|system_id" key
            if let Some(system_id) = key_str.split('|').nth(1) {
                if let Some(data) = self.db.get_cf(cf_systems, system_id.as_bytes())? {
                    let system: System = serde_json::from_slice(&data)?;
                    systems.push(system);

                    if systems.len() >= limit {
                        break;
                    }
                }
            }
        }

        Ok(systems)
    }

    /// Delete system (with index cleanup)
    pub fn delete_system(&self, system_id: &str, system_type: &str, vendor: &str) -> Result<()> {
        let cf_systems = self.cf(CF_SYSTEMS)?;
        let cf_type_idx = self.cf(CF_SYSTEM_BY_TYPE)?;
        let cf_vendor_idx = self.cf(CF_SYSTEM_BY_VENDOR)?;

        let mut batch = WriteBatch::default();

        // Delete from primary storage
        batch.delete_cf(cf_systems, system_id.as_bytes());

        // Delete from secondary indexes
        let type_key = format!("{}|{}", system_type, system_id);
        batch.delete_cf(cf_type_idx, type_key.as_bytes());

        let vendor_key = format!("{}|{}", vendor, system_id);
        batch.delete_cf(cf_vendor_idx, vendor_key.as_bytes());

        self.db.write(batch).context("Failed to delete system")
    }

    // ========================================================================
    // Interface Operations
    // ========================================================================

    /// Store interface
    pub fn put_interface(&self, interface: &Interface) -> Result<()> {
        let cf_interfaces = self.cf(CF_INTERFACES)?;
        let cf_sys_idx = self.cf(CF_INTERFACE_BY_SYSTEM)?;

        let data = serde_json::to_vec(interface).context("Failed to serialize interface")?;

        let mut batch = WriteBatch::default();

        // Primary storage
        batch.put_cf(cf_interfaces, interface.interface_id.as_bytes(), &data);

        // System index
        let sys_key = format!("{}|{}", interface.system_id, interface.interface_id);
        batch.put_cf(cf_sys_idx, sys_key.as_bytes(), b"");

        self.db
            .write(batch)
            .context("Failed to write interface batch")
    }

    /// Get interface by ID
    pub fn get_interface(&self, interface_id: &str) -> Result<Option<Interface>> {
        let cf = self.cf(CF_INTERFACES)?;

        match self.db.get_cf(cf, interface_id.as_bytes())? {
            Some(data) => {
                let interface =
                    serde_json::from_slice(&data).context("Failed to deserialize interface")?;
                Ok(Some(interface))
            }
            None => Ok(None),
        }
    }

    /// List interfaces by system (fast indexed lookup)
    pub fn list_interfaces_by_system(&self, system_id: &str) -> Result<Vec<Interface>> {
        let cf_sys_idx = self.cf(CF_INTERFACE_BY_SYSTEM)?;
        let cf_interfaces = self.cf(CF_INTERFACES)?;

        let prefix = format!("{}|", system_id);
        let mut interfaces = Vec::new();

        for item in self.db.iterator_cf(
            cf_sys_idx,
            IteratorMode::From(prefix.as_bytes(), rocksdb::Direction::Forward),
        ) {
            let (key, _) = item?;
            let key_str = std::str::from_utf8(&key)?;

            if !key_str.starts_with(&prefix) {
                break;
            }

            if let Some(interface_id) = key_str.split('|').nth(1) {
                if let Some(data) = self.db.get_cf(cf_interfaces, interface_id.as_bytes())? {
                    let interface: Interface = serde_json::from_slice(&data)?;
                    interfaces.push(interface);
                }
            }
        }

        Ok(interfaces)
    }

    /// Delete interface (with index cleanup)
    pub fn delete_interface(&self, interface_id: &str, system_id: &str) -> Result<()> {
        let cf_interfaces = self.cf(CF_INTERFACES)?;
        let cf_sys_idx = self.cf(CF_INTERFACE_BY_SYSTEM)?;

        let mut batch = WriteBatch::default();

        // Delete from primary storage
        batch.delete_cf(cf_interfaces, interface_id.as_bytes());

        // Delete from system index
        let sys_key = format!("{}|{}", system_id, interface_id);
        batch.delete_cf(cf_sys_idx, sys_key.as_bytes());

        self.db.write(batch).context("Failed to delete interface")
    }

    /// List all interfaces (with pagination for large datasets)
    pub fn list_all_interfaces(&self, offset: usize, limit: usize) -> Result<Vec<Interface>> {
        let cf = self.cf(CF_INTERFACES)?;
        let mut interfaces = Vec::new();
        let mut count = 0;

        for item in self.db.iterator_cf(cf, IteratorMode::Start) {
            let (_key, value) = item?;

            // Skip offset
            if count < offset {
                count += 1;
                continue;
            }

            let interface: Interface = serde_json::from_slice(&value)?;
            interfaces.push(interface);

            if interfaces.len() >= limit {
                break;
            }
        }

        Ok(interfaces)
    }

    // ========================================================================
    // Contract Operations
    // ========================================================================

    /// Store contract
    pub fn put_contract(&self, contract: &Contract) -> Result<()> {
        let cf_contracts = self.cf(CF_CONTRACTS)?;
        let cf_prov_idx = self.cf(CF_CONTRACT_BY_PROVIDER)?;
        let cf_cons_idx = self.cf(CF_CONTRACT_BY_CONSUMER)?;

        let data = serde_json::to_vec(contract).context("Failed to serialize contract")?;

        let mut batch = WriteBatch::default();

        // Primary storage
        batch.put_cf(cf_contracts, contract.contract_id.as_bytes(), &data);

        // Provider index
        let prov_key = format!(
            "{}|{}",
            contract.provider_interface_id, contract.contract_id
        );
        batch.put_cf(cf_prov_idx, prov_key.as_bytes(), b"");

        // Consumer index
        let cons_key = format!(
            "{}|{}",
            contract.consumer_interface_id, contract.contract_id
        );
        batch.put_cf(cf_cons_idx, cons_key.as_bytes(), b"");

        self.db
            .write(batch)
            .context("Failed to write contract batch")
    }

    /// Get contract by ID
    pub fn get_contract(&self, contract_id: &str) -> Result<Option<Contract>> {
        let cf = self.cf(CF_CONTRACTS)?;

        match self.db.get_cf(cf, contract_id.as_bytes())? {
            Some(data) => {
                let contract =
                    serde_json::from_slice(&data).context("Failed to deserialize contract")?;
                Ok(Some(contract))
            }
            None => Ok(None),
        }
    }

    /// List contracts by provider interface (fast indexed lookup)
    pub fn list_contracts_by_provider(&self, provider_interface_id: &str) -> Result<Vec<Contract>> {
        let cf_prov_idx = self.cf(CF_CONTRACT_BY_PROVIDER)?;
        let cf_contracts = self.cf(CF_CONTRACTS)?;

        let prefix = format!("{}|", provider_interface_id);
        let mut contracts = Vec::new();

        for item in self.db.iterator_cf(
            cf_prov_idx,
            IteratorMode::From(prefix.as_bytes(), rocksdb::Direction::Forward),
        ) {
            let (key, _) = item?;
            let key_str = std::str::from_utf8(&key)?;

            if !key_str.starts_with(&prefix) {
                break;
            }

            if let Some(contract_id) = key_str.split('|').nth(1) {
                if let Some(data) = self.db.get_cf(cf_contracts, contract_id.as_bytes())? {
                    let contract: Contract = serde_json::from_slice(&data)?;
                    contracts.push(contract);
                }
            }
        }

        Ok(contracts)
    }

    /// List contracts by consumer interface (fast indexed lookup)
    pub fn list_contracts_by_consumer(&self, consumer_interface_id: &str) -> Result<Vec<Contract>> {
        let cf_cons_idx = self.cf(CF_CONTRACT_BY_CONSUMER)?;
        let cf_contracts = self.cf(CF_CONTRACTS)?;

        let prefix = format!("{}|", consumer_interface_id);
        let mut contracts = Vec::new();

        for item in self.db.iterator_cf(
            cf_cons_idx,
            IteratorMode::From(prefix.as_bytes(), rocksdb::Direction::Forward),
        ) {
            let (key, _) = item?;
            let key_str = std::str::from_utf8(&key)?;

            if !key_str.starts_with(&prefix) {
                break;
            }

            if let Some(contract_id) = key_str.split('|').nth(1) {
                if let Some(data) = self.db.get_cf(cf_contracts, contract_id.as_bytes())? {
                    let contract: Contract = serde_json::from_slice(&data)?;
                    contracts.push(contract);
                }
            }
        }

        Ok(contracts)
    }

    /// List all contracts (with pagination for large datasets)
    pub fn list_all_contracts(&self, offset: usize, limit: usize) -> Result<Vec<Contract>> {
        let cf = self.cf(CF_CONTRACTS)?;
        let mut contracts = Vec::new();
        let mut count = 0;

        for item in self.db.iterator_cf(cf, IteratorMode::Start) {
            let (_key, value) = item?;

            // Skip offset
            if count < offset {
                count += 1;
                continue;
            }

            let contract: Contract = serde_json::from_slice(&value)?;
            contracts.push(contract);

            if contracts.len() >= limit {
                break;
            }
        }

        Ok(contracts)
    }

    /// Delete contract (with index cleanup)
    pub fn delete_contract(
        &self,
        contract_id: &str,
        provider_interface_id: &str,
        consumer_interface_id: &str,
    ) -> Result<()> {
        let cf_contracts = self.cf(CF_CONTRACTS)?;
        let cf_prov_idx = self.cf(CF_CONTRACT_BY_PROVIDER)?;
        let cf_cons_idx = self.cf(CF_CONTRACT_BY_CONSUMER)?;

        let mut batch = WriteBatch::default();

        // Delete from primary storage
        batch.delete_cf(cf_contracts, contract_id.as_bytes());

        // Delete from provider index
        let prov_key = format!("{}|{}", provider_interface_id, contract_id);
        batch.delete_cf(cf_prov_idx, prov_key.as_bytes());

        // Delete from consumer index
        let cons_key = format!("{}|{}", consumer_interface_id, contract_id);
        batch.delete_cf(cf_cons_idx, cons_key.as_bytes());

        self.db.write(batch).context("Failed to delete contract")
    }

    /// List all systems (with pagination for large datasets)
    pub fn list_all_systems(&self, offset: usize, limit: usize) -> Result<Vec<System>> {
        let cf = self.cf(CF_SYSTEMS)?;
        let mut systems = Vec::new();
        let mut count = 0;

        for item in self.db.iterator_cf(cf, IteratorMode::Start) {
            let (_key, value) = item?;

            // Skip offset
            if count < offset {
                count += 1;
                continue;
            }

            let system: System = serde_json::from_slice(&value)?;
            systems.push(system);

            if systems.len() >= limit {
                break;
            }
        }

        Ok(systems)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_system_crud() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let store = SosStore::new(temp_dir.path().to_str().unwrap())?;

        let system = System {
            system_id: "test-sys-1".to_string(),
            system_name: "Test System".to_string(),
            system_type: "radar".to_string(),
            vendor: "Test Vendor".to_string(),
            version: "1.0".to_string(),
            classification: "UNCLASSIFIED".to_string(),
            description: Some("Test description".to_string()),
            deployment: HashMap::new(),
            capabilities: HashMap::new(),
            tags: vec!["test".to_string()],
            active: true,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        // Test put
        store.put_system(&system)?;

        // Test get
        let retrieved = store.get_system("test-sys-1")?;
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().system_name, "Test System");

        // Test list by type
        let systems = store.list_systems_by_type("radar", 10)?;
        assert_eq!(systems.len(), 1);

        Ok(())
    }
}
