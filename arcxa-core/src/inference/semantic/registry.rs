//! Semantic detection strategy registry
//!
//! Provides a centralized registry for managing multiple semantic type detection
//! strategies and their configurations.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

use super::column_name::ColumnNameDetector;

/// Detection strategy metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectorMetadata {
    /// Unique identifier
    pub id: String,

    /// Human-readable name
    pub name: String,

    /// Description of detection approach
    pub description: String,

    /// Detection method type
    pub method: DetectionMethod,

    /// Priority for composite detection (higher = higher priority)
    pub priority: u8,

    /// Enabled state
    pub enabled: bool,

    /// Configuration parameters
    pub config: HashMap<String, String>,
}

/// Detection method types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum DetectionMethod {
    /// Column name pattern matching
    ColumnName,

    /// Regex value matching
    ValueRegex,

    /// Statistical analysis
    Statistical,

    /// Database metadata
    DatabaseMetadata,

    /// Machine learning
    MachineLearning,

    /// Composite (multiple strategies)
    Composite,
}

/// Registry for managing semantic detection strategies
pub struct SemanticDetectionRegistry {
    /// Registered detectors
    detectors: HashMap<String, DetectorEntry>,

    /// Default detector priority order
    priority_order: Vec<String>,
}

/// Internal detector entry
struct DetectorEntry {
    metadata: DetectorMetadata,
    detector: DetectorType,
}

/// Detector type wrapper
enum DetectorType {
    ColumnName(Arc<ColumnNameDetector>),
    // Future: ValueRegex(Arc<ValueRegexDetector>),
    // Future: Statistical(Arc<StatisticalDetector>),
}

impl SemanticDetectionRegistry {
    /// Create new registry with default detectors
    pub fn new() -> Self {
        let mut registry = Self {
            detectors: HashMap::new(),
            priority_order: Vec::new(),
        };

        // Register default column name detector
        let column_name_detector = Arc::new(ColumnNameDetector::new());
        registry
            .register_column_name_detector(
                "column_name_default",
                "Default Column Name Detector",
                "Detects semantic types from column names using pattern matching",
                column_name_detector,
            )
            .expect("Failed to register default column name detector");

        registry
    }

    /// Register a column name detector
    pub fn register_column_name_detector(
        &mut self,
        id: impl Into<String>,
        name: impl Into<String>,
        description: impl Into<String>,
        detector: Arc<ColumnNameDetector>,
    ) -> Result<()> {
        let id = id.into();

        let metadata = DetectorMetadata {
            id: id.clone(),
            name: name.into(),
            description: description.into(),
            method: DetectionMethod::ColumnName,
            priority: 80, // High priority for name-based detection
            enabled: true,
            config: HashMap::new(),
        };

        let entry = DetectorEntry {
            metadata,
            detector: DetectorType::ColumnName(detector),
        };

        self.detectors.insert(id.clone(), entry);
        self.priority_order.push(id);
        self.priority_order.sort_by(|a, b| {
            let a_priority = self
                .detectors
                .get(a)
                .map(|d| d.metadata.priority)
                .unwrap_or(0);
            let b_priority = self
                .detectors
                .get(b)
                .map(|d| d.metadata.priority)
                .unwrap_or(0);
            b_priority.cmp(&a_priority) // Descending order
        });

        Ok(())
    }

    /// Get column name detector by ID
    pub fn get_column_name_detector(&self, id: &str) -> Option<Arc<ColumnNameDetector>> {
        self.detectors
            .get(id)
            .and_then(|entry| match &entry.detector {
                DetectorType::ColumnName(detector) => Some(detector.clone()),
            })
    }

    /// Get default column name detector
    pub fn get_default_column_name_detector(&self) -> Option<Arc<ColumnNameDetector>> {
        self.get_column_name_detector("column_name_default")
    }

    /// List all registered detectors
    pub fn list_detectors(&self) -> Vec<&DetectorMetadata> {
        self.detectors
            .values()
            .map(|entry| &entry.metadata)
            .collect()
    }

    /// List enabled detectors in priority order
    pub fn list_enabled_detectors(&self) -> Vec<&DetectorMetadata> {
        self.priority_order
            .iter()
            .filter_map(|id| self.detectors.get(id))
            .filter(|entry| entry.metadata.enabled)
            .map(|entry| &entry.metadata)
            .collect()
    }

    /// Enable a detector
    pub fn enable_detector(&mut self, id: &str) -> Result<()> {
        let entry = self
            .detectors
            .get_mut(id)
            .ok_or_else(|| anyhow::anyhow!("Detector {} not found", id))?;
        entry.metadata.enabled = true;
        Ok(())
    }

    /// Disable a detector
    pub fn disable_detector(&mut self, id: &str) -> Result<()> {
        let entry = self
            .detectors
            .get_mut(id)
            .ok_or_else(|| anyhow::anyhow!("Detector {} not found", id))?;
        entry.metadata.enabled = false;
        Ok(())
    }

    /// Update detector priority
    pub fn update_priority(&mut self, id: &str, priority: u8) -> Result<()> {
        let entry = self
            .detectors
            .get_mut(id)
            .ok_or_else(|| anyhow::anyhow!("Detector {} not found", id))?;
        entry.metadata.priority = priority;

        // Re-sort priority order
        self.priority_order.sort_by(|a, b| {
            let a_priority = self
                .detectors
                .get(a)
                .map(|d| d.metadata.priority)
                .unwrap_or(0);
            let b_priority = self
                .detectors
                .get(b)
                .map(|d| d.metadata.priority)
                .unwrap_or(0);
            b_priority.cmp(&a_priority)
        });

        Ok(())
    }

    /// Update detector configuration
    pub fn update_config(&mut self, id: &str, config: HashMap<String, String>) -> Result<()> {
        let entry = self
            .detectors
            .get_mut(id)
            .ok_or_else(|| anyhow::anyhow!("Detector {} not found", id))?;
        entry.metadata.config = config;
        Ok(())
    }

    /// Remove a detector
    pub fn unregister_detector(&mut self, id: &str) -> Result<()> {
        self.detectors
            .remove(id)
            .ok_or_else(|| anyhow::anyhow!("Detector {} not found", id))?;
        self.priority_order.retain(|detector_id| detector_id != id);
        Ok(())
    }

    /// Get statistics about registered detectors
    pub fn get_statistics(&self) -> RegistryStatistics {
        let total_count = self.detectors.len();
        let enabled_count = self
            .detectors
            .values()
            .filter(|e| e.metadata.enabled)
            .count();

        let mut by_method = HashMap::new();
        for entry in self.detectors.values() {
            *by_method.entry(entry.metadata.method.clone()).or_insert(0) += 1;
        }

        RegistryStatistics {
            total_count,
            enabled_count,
            disabled_count: total_count - enabled_count,
            by_method,
        }
    }
}

impl Default for SemanticDetectionRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Registry statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryStatistics {
    pub total_count: usize,
    pub enabled_count: usize,
    pub disabled_count: usize,
    pub by_method: HashMap<DetectionMethod, usize>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_creation() {
        let registry = SemanticDetectionRegistry::new();

        // Default column name detector should be registered
        assert_eq!(registry.list_detectors().len(), 1);
        assert!(registry.get_default_column_name_detector().is_some());
    }

    #[test]
    fn test_register_custom_detector() {
        let mut registry = SemanticDetectionRegistry::new();

        let detector = Arc::new(ColumnNameDetector::new());
        registry
            .register_column_name_detector(
                "custom_detector",
                "Custom Detector",
                "Custom column name detector",
                detector,
            )
            .unwrap();

        assert_eq!(registry.list_detectors().len(), 2);
        assert!(registry
            .get_column_name_detector("custom_detector")
            .is_some());
    }

    #[test]
    fn test_enable_disable_detector() {
        let mut registry = SemanticDetectionRegistry::new();

        assert_eq!(registry.list_enabled_detectors().len(), 1);

        registry.disable_detector("column_name_default").unwrap();
        assert_eq!(registry.list_enabled_detectors().len(), 0);

        registry.enable_detector("column_name_default").unwrap();
        assert_eq!(registry.list_enabled_detectors().len(), 1);
    }

    #[test]
    fn test_priority_ordering() {
        let mut registry = SemanticDetectionRegistry::new();

        let detector1 = Arc::new(ColumnNameDetector::new());
        let detector2 = Arc::new(ColumnNameDetector::new());

        registry
            .register_column_name_detector("low_priority", "Low", "Low priority", detector1)
            .unwrap();
        registry
            .register_column_name_detector("high_priority", "High", "High priority", detector2)
            .unwrap();

        registry.update_priority("low_priority", 10).unwrap();
        registry.update_priority("high_priority", 90).unwrap();

        let enabled = registry.list_enabled_detectors();
        // high_priority should come first (priority 90)
        assert_eq!(enabled[0].id, "high_priority");
    }

    #[test]
    fn test_unregister_detector() {
        let mut registry = SemanticDetectionRegistry::new();

        let detector = Arc::new(ColumnNameDetector::new());
        registry
            .register_column_name_detector("temp", "Temp", "Temporary", detector)
            .unwrap();

        assert_eq!(registry.list_detectors().len(), 2);

        registry.unregister_detector("temp").unwrap();
        assert_eq!(registry.list_detectors().len(), 1);
    }

    #[test]
    fn test_registry_statistics() {
        let mut registry = SemanticDetectionRegistry::new();

        let stats = registry.get_statistics();
        assert_eq!(stats.total_count, 1);
        assert_eq!(stats.enabled_count, 1);
        assert_eq!(stats.disabled_count, 0);

        registry.disable_detector("column_name_default").unwrap();

        let stats = registry.get_statistics();
        assert_eq!(stats.enabled_count, 0);
        assert_eq!(stats.disabled_count, 1);
    }

    #[test]
    fn test_update_config() {
        let mut registry = SemanticDetectionRegistry::new();

        let mut config = HashMap::new();
        config.insert("threshold".to_string(), "0.8".to_string());
        config.insert("mode".to_string(), "strict".to_string());

        registry
            .update_config("column_name_default", config.clone())
            .unwrap();

        let detectors = registry.list_detectors();
        assert_eq!(
            detectors[0].config.get("threshold"),
            Some(&"0.8".to_string())
        );
        assert_eq!(detectors[0].config.get("mode"), Some(&"strict".to_string()));
    }
}
