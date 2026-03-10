//! RDF Triple Store for Semantic Metadata (Phase 2.1)
//!
//! Simple file-based RDF store for persisting semantic metadata triples.
//! This can be upgraded to Oxigraph/Sophia in future phases.

use super::rdf_converter::{RdfConverter, Triple};
use crate::ingestion::FieldSemanticMetadata;
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// RDF triple store configuration
#[derive(Debug, Clone)]
pub struct RdfStoreConfig {
    /// Base directory for RDF files
    pub base_dir: PathBuf,

    /// Whether to use separate files per dataset
    pub separate_files: bool,

    /// Flush frequency (number of records)
    pub flush_frequency: usize,
}

impl Default for RdfStoreConfig {
    fn default() -> Self {
        Self {
            base_dir: PathBuf::from("/tmp/graphica/rdf"),
            separate_files: true,
            flush_frequency: 100,
        }
    }
}

/// Simple file-based RDF triple store
pub struct RdfStore {
    config: RdfStoreConfig,
    converter: Arc<RdfConverter>,
    writers: Arc<Mutex<HashMap<String, BufWriter<File>>>>,
    record_count: Arc<Mutex<usize>>,
}

impl RdfStore {
    /// Create a new RDF store
    pub fn new(config: RdfStoreConfig, source_id: &str) -> Result<Self> {
        // Ensure base directory exists
        std::fs::create_dir_all(&config.base_dir)
            .context("Failed to create RDF store directory")?;

        Ok(Self {
            config,
            converter: Arc::new(RdfConverter::new(source_id)),
            writers: Arc::new(Mutex::new(HashMap::new())),
            record_count: Arc::new(Mutex::new(0)),
        })
    }

    /// Persist record with semantic metadata
    pub fn persist_record_semantics(
        &self,
        record_id: &str,
        dataset: &str,
        semantic_metadata: &HashMap<String, FieldSemanticMetadata>,
        timestamp: i64,
    ) -> Result<usize> {
        // Convert to RDF triples
        let triples = self.converter.convert_record_with_semantics(
            record_id,
            dataset,
            semantic_metadata,
            timestamp,
        )?;

        if triples.is_empty() {
            return Ok(0);
        }

        // Get writer for this dataset
        let mut writers = self.writers.lock().unwrap();
        let writer = self.get_or_create_writer(dataset, &mut writers)?;

        // Write triples in Turtle format
        let turtle = self.converter.triples_to_turtle(&triples);
        writer
            .write_all(turtle.as_bytes())
            .context("Failed to write triples")?;
        writer.write_all(b"\n").context("Failed to write newline")?;

        // Update record count and flush if necessary
        let mut count = self.record_count.lock().unwrap();
        *count += 1;

        if *count % self.config.flush_frequency == 0 {
            writer.flush().context("Failed to flush writer")?;
        }

        Ok(triples.len())
    }

    /// Persist raw triples
    pub fn persist_triples(&self, dataset: &str, triples: &[Triple]) -> Result<()> {
        if triples.is_empty() {
            return Ok(());
        }

        let mut writers = self.writers.lock().unwrap();
        let writer = self.get_or_create_writer(dataset, &mut writers)?;

        let turtle = self.converter.triples_to_turtle(triples);
        writer
            .write_all(turtle.as_bytes())
            .context("Failed to write triples")?;
        writer.write_all(b"\n").context("Failed to write newline")?;

        Ok(())
    }

    /// Get or create a writer for a dataset
    fn get_or_create_writer<'a>(
        &self,
        dataset: &str,
        writers: &'a mut HashMap<String, BufWriter<File>>,
    ) -> Result<&'a mut BufWriter<File>> {
        if !writers.contains_key(dataset) {
            let file_path = if self.config.separate_files {
                self.config.base_dir.join(format!("{}.ttl", dataset))
            } else {
                self.config.base_dir.join("all_records.ttl")
            };

            let file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&file_path)
                .context(format!("Failed to open RDF file: {:?}", file_path))?;

            writers.insert(dataset.to_string(), BufWriter::new(file));
        }

        Ok(writers.get_mut(dataset).unwrap())
    }

    /// Flush all writers
    pub fn flush_all(&self) -> Result<()> {
        let mut writers = self.writers.lock().unwrap();
        for writer in writers.values_mut() {
            writer.flush().context("Failed to flush writer")?;
        }
        Ok(())
    }

    /// Get statistics
    pub fn get_statistics(&self) -> RdfStoreStatistics {
        let count = *self.record_count.lock().unwrap();
        let datasets = self.writers.lock().unwrap().len();

        RdfStoreStatistics {
            total_records_persisted: count,
            active_datasets: datasets,
            base_dir: self.config.base_dir.clone(),
        }
    }

    /// Read triples from file (for testing/debugging)
    pub fn read_dataset_triples(&self, dataset: &str) -> Result<String> {
        let file_path = if self.config.separate_files {
            self.config.base_dir.join(format!("{}.ttl", dataset))
        } else {
            self.config.base_dir.join("all_records.ttl")
        };

        std::fs::read_to_string(&file_path)
            .context(format!("Failed to read RDF file: {:?}", file_path))
    }
}

impl Drop for RdfStore {
    fn drop(&mut self) {
        // Flush all writers on drop
        let _ = self.flush_all();
    }
}

/// RDF store statistics
#[derive(Debug, Clone)]
pub struct RdfStoreStatistics {
    pub total_records_persisted: usize,
    pub active_datasets: usize,
    pub base_dir: PathBuf,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inference::types::SemanticType;
    use tempfile::TempDir;

    #[test]
    fn test_rdf_store_creation() {
        let temp_dir = TempDir::new().unwrap();
        let config = RdfStoreConfig {
            base_dir: temp_dir.path().to_path_buf(),
            separate_files: true,
            flush_frequency: 10,
        };

        let store = RdfStore::new(config, "test_source").unwrap();
        let stats = store.get_statistics();

        assert_eq!(stats.total_records_persisted, 0);
        assert_eq!(stats.active_datasets, 0);
    }

    #[test]
    fn test_persist_record_semantics() {
        let temp_dir = TempDir::new().unwrap();
        let config = RdfStoreConfig {
            base_dir: temp_dir.path().to_path_buf(),
            separate_files: true,
            flush_frequency: 1,
        };

        let store = RdfStore::new(config, "test_source").unwrap();

        let mut semantic_metadata = HashMap::new();
        semantic_metadata.insert(
            "email".to_string(),
            FieldSemanticMetadata {
                field_name: "email".to_string(),
                semantic_type: SemanticType::Email,
                confidence: 0.9,
                detection_method: "Exact match".to_string(),
            },
        );

        let triple_count = store
            .persist_record_semantics("user-123", "customers", &semantic_metadata, 1000000000000)
            .unwrap();

        assert!(triple_count > 0);

        // Flush and read back
        store.flush_all().unwrap();
        let content = store.read_dataset_triples("customers").unwrap();

        assert!(content.contains("@prefix"));
        assert!(content.contains("urn:graphica:record:customers/user-123"));
        assert!(content.contains("Email"));
    }

    #[test]
    fn test_multiple_records_same_dataset() {
        let temp_dir = TempDir::new().unwrap();
        let config = RdfStoreConfig {
            base_dir: temp_dir.path().to_path_buf(),
            separate_files: true,
            flush_frequency: 10,
        };

        let store = RdfStore::new(config, "test_source").unwrap();

        let mut metadata1 = HashMap::new();
        metadata1.insert(
            "email".to_string(),
            FieldSemanticMetadata {
                field_name: "email".to_string(),
                semantic_type: SemanticType::Email,
                confidence: 0.9,
                detection_method: "Exact match".to_string(),
            },
        );

        let mut metadata2 = HashMap::new();
        metadata2.insert(
            "phone".to_string(),
            FieldSemanticMetadata {
                field_name: "phone".to_string(),
                semantic_type: SemanticType::PhoneNumber,
                confidence: 0.85,
                detection_method: "Contains: phone".to_string(),
            },
        );

        store
            .persist_record_semantics("user-1", "customers", &metadata1, 1000)
            .unwrap();
        store
            .persist_record_semantics("user-2", "customers", &metadata2, 2000)
            .unwrap();

        store.flush_all().unwrap();
        let content = store.read_dataset_triples("customers").unwrap();

        assert!(content.contains("user-1"));
        assert!(content.contains("user-2"));
        assert!(content.contains("Email"));
        assert!(content.contains("PhoneNumber"));

        let stats = store.get_statistics();
        assert_eq!(stats.total_records_persisted, 2);
    }

    #[test]
    fn test_separate_dataset_files() {
        let temp_dir = TempDir::new().unwrap();
        let config = RdfStoreConfig {
            base_dir: temp_dir.path().to_path_buf(),
            separate_files: true,
            flush_frequency: 1,
        };

        let store = RdfStore::new(config, "test_source").unwrap();

        let metadata = HashMap::new(); // Empty metadata

        // Still creates record-level triples (type, recordId, dataset, ingestedAt)
        let triple_count = store
            .persist_record_semantics("r1", "dataset1", &metadata, 1000)
            .unwrap();
        assert_eq!(triple_count, 4); // 4 base record triples

        // Add actual metadata
        let mut real_metadata = HashMap::new();
        real_metadata.insert(
            "email".to_string(),
            FieldSemanticMetadata {
                field_name: "email".to_string(),
                semantic_type: SemanticType::Email,
                confidence: 0.9,
                detection_method: "Test".to_string(),
            },
        );

        store
            .persist_record_semantics("r2", "dataset1", &real_metadata, 2000)
            .unwrap();
        store
            .persist_record_semantics("r3", "dataset2", &real_metadata, 3000)
            .unwrap();

        store.flush_all().unwrap();

        // Both files should exist
        assert!(temp_dir.path().join("dataset1.ttl").exists());
        assert!(temp_dir.path().join("dataset2.ttl").exists());
    }
}
