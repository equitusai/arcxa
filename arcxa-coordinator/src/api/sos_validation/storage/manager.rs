//! SoS Storage Manager
//!
//! High-level API for managing SoS entities with RocksDB + optional RDF integration

use super::rocks_store::{Contract, Interface, SosStore, System};
use anyhow::Result;
use std::sync::Arc;

/// SoS Storage Manager - coordinates RocksDB and RDF storage
pub struct SosStorageManager {
    /// High-performance RocksDB store
    store: Arc<SosStore>,
}

impl SosStorageManager {
    /// Create new storage manager with RocksDB backend
    pub fn new(rocks_path: &str) -> Result<Self> {
        let store = Arc::new(SosStore::new(rocks_path)?);

        Ok(Self { store })
    }

    /// Get reference to underlying RocksDB store
    pub fn store(&self) -> &Arc<SosStore> {
        &self.store
    }

    // ========================================================================
    // System Operations (delegates to RocksDB with future RDF integration)
    // ========================================================================

    pub fn put_system(&self, system: &System) -> Result<()> {
        self.store.put_system(system)
    }

    pub fn get_system(&self, system_id: &str) -> Result<Option<System>> {
        self.store.get_system(system_id)
    }

    pub fn list_systems_by_type(&self, system_type: &str, limit: usize) -> Result<Vec<System>> {
        self.store.list_systems_by_type(system_type, limit)
    }

    pub fn list_all_systems(&self, offset: usize, limit: usize) -> Result<Vec<System>> {
        self.store.list_all_systems(offset, limit)
    }

    pub fn delete_system(&self, system_id: &str, system_type: &str, vendor: &str) -> Result<()> {
        self.store.delete_system(system_id, system_type, vendor)
    }

    // ========================================================================
    // Interface Operations
    // ========================================================================

    pub fn put_interface(&self, interface: &Interface) -> Result<()> {
        self.store.put_interface(interface)
    }

    pub fn get_interface(&self, interface_id: &str) -> Result<Option<Interface>> {
        self.store.get_interface(interface_id)
    }

    pub fn list_interfaces_by_system(&self, system_id: &str) -> Result<Vec<Interface>> {
        self.store.list_interfaces_by_system(system_id)
    }

    pub fn delete_interface(&self, interface_id: &str, system_id: &str) -> Result<()> {
        self.store.delete_interface(interface_id, system_id)
    }

    pub fn list_all_interfaces(&self, offset: usize, limit: usize) -> Result<Vec<Interface>> {
        self.store.list_all_interfaces(offset, limit)
    }

    // ========================================================================
    // Contract Operations
    // ========================================================================

    pub fn put_contract(&self, contract: &Contract) -> Result<()> {
        self.store.put_contract(contract)
    }

    pub fn get_contract(&self, contract_id: &str) -> Result<Option<Contract>> {
        self.store.get_contract(contract_id)
    }

    pub fn list_contracts_by_provider(&self, provider_interface_id: &str) -> Result<Vec<Contract>> {
        self.store.list_contracts_by_provider(provider_interface_id)
    }

    pub fn list_contracts_by_consumer(&self, consumer_interface_id: &str) -> Result<Vec<Contract>> {
        self.store.list_contracts_by_consumer(consumer_interface_id)
    }

    pub fn list_all_contracts(&self, offset: usize, limit: usize) -> Result<Vec<Contract>> {
        self.store.list_all_contracts(offset, limit)
    }

    pub fn delete_contract(
        &self,
        contract_id: &str,
        provider_interface_id: &str,
        consumer_interface_id: &str,
    ) -> Result<()> {
        self.store
            .delete_contract(contract_id, provider_interface_id, consumer_interface_id)
    }
}
