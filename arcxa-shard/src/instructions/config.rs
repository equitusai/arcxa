//! Configuration Update Handler
//!
//! Handles dynamic configuration updates from the coordinator.

use anyhow::Result;
use graphica_core::distributed::proto::coordinator_service::ConfigUpdateInstruction as ProtoConfigUpdate;
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use tracing::{info, warn};

/// Configuration update containing key-value pairs
#[derive(Debug, Clone)]
pub struct ConfigUpdate {
    /// Configuration entries to update
    pub entries: HashMap<String, String>,

    /// Whether to apply immediately
    pub immediate: bool,
}

impl From<ProtoConfigUpdate> for ConfigUpdate {
    fn from(proto: ProtoConfigUpdate) -> Self {
        let entries = if let Some(config) = proto.new_config {
            config.feature_flags
        } else {
            HashMap::new()
        };

        Self {
            entries,
            immediate: proto.immediate,
        }
    }
}

/// Handler for configuration updates
pub struct ConfigUpdateHandler {
    // Future: Add state for managing active configuration
}

impl ConfigUpdateHandler {
    /// Create a new config update handler
    pub fn new() -> Self {
        Self {}
    }

    /// Handle a configuration update
    pub async fn handle(&self, proto: ProtoConfigUpdate) -> Result<()> {
        let update = ConfigUpdate::from(proto);

        info!("Coordinator sent config update with {} entries", update.entries.len());

        if update.immediate {
            info!("  Config update should be applied immediately");
        } else {
            info!("  Config update will be applied at next restart");
        }

        // Log the configuration entries
        for (key, value) in &update.entries {
            info!("  {} = {}", key, value);
        }

        // TODO: Implement configuration update
        // This would involve:
        // 1. Validate configuration entries
        // 2. If immediate:
        //    - Apply changes to in-memory config
        //    - Reconfigure components as needed
        //    - Persist to config file
        // 3. If not immediate:
        //    - Persist to config file only
        //    - Will be loaded on next restart
        // 4. Notify coordinator of success/failure

        info!("Config update not yet implemented - instruction logged");

        Ok(())
    }

    /// Validate a configuration key-value pair
    pub fn validate_config_entry(key: &str, value: &str) -> bool {
        // Basic validation - non-empty key and value
        !key.is_empty() && !value.is_empty()
    }

    /// Parse a configuration value as an integer
    pub fn parse_int(value: &str) -> Option<i64> {
        value.parse::<i64>().ok()
    }

    /// Parse a configuration value as a boolean
    pub fn parse_bool(value: &str) -> Option<bool> {
        match value.to_lowercase().as_str() {
            "true" | "1" | "yes" | "on" => Some(true),
            "false" | "0" | "no" | "off" => Some(false),
            _ => None,
        }
    }

    /// Parse a configuration value as JSON
    pub fn parse_json(value: &str) -> Option<JsonValue> {
        serde_json::from_str(value).ok()
    }
}

impl Default for ConfigUpdateHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_update_from_proto() {
        use graphica_core::distributed::proto::coordinator_service::ShardConfiguration;

        let mut feature_flags = HashMap::new();
        feature_flags.insert("max_memory_mb".to_string(), "16384".to_string());
        feature_flags.insert("log_level".to_string(), "debug".to_string());

        let proto = ProtoConfigUpdate {
            new_config: Some(ShardConfiguration {
                heartbeat_interval_secs: 30,
                stats_reporting_interval_secs: 60,
                enable_compression: true,
                enable_encryption: false,
                batch_size: 100,
                max_concurrent_queries: 10,
                query_timeout_secs: 30,
                feature_flags: feature_flags.clone(),
                replication: None,
            }),
            immediate: true,
        };

        let update = ConfigUpdate::from(proto);

        assert_eq!(update.entries.len(), 2);
        assert_eq!(update.entries.get("max_memory_mb").unwrap(), "16384");
        assert_eq!(update.entries.get("log_level").unwrap(), "debug");
        assert!(update.immediate);
    }

    #[tokio::test]
    async fn test_handle_config_update_immediate() {
        use graphica_core::distributed::proto::coordinator_service::ShardConfiguration;

        let handler = ConfigUpdateHandler::new();

        let mut feature_flags = HashMap::new();
        feature_flags.insert("heartbeat_interval".to_string(), "60".to_string());

        let proto = ProtoConfigUpdate {
            new_config: Some(ShardConfiguration {
                heartbeat_interval_secs: 60,
                stats_reporting_interval_secs: 60,
                enable_compression: true,
                enable_encryption: false,
                batch_size: 100,
                max_concurrent_queries: 10,
                query_timeout_secs: 30,
                feature_flags,
                replication: None,
            }),
            immediate: true,
        };

        let result = handler.handle(proto).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_handle_config_update_deferred() {
        use graphica_core::distributed::proto::coordinator_service::ShardConfiguration;

        let handler = ConfigUpdateHandler::new();

        let mut feature_flags = HashMap::new();
        feature_flags.insert("disk_space_mb".to_string(), "1000000".to_string());

        let proto = ProtoConfigUpdate {
            new_config: Some(ShardConfiguration {
                heartbeat_interval_secs: 30,
                stats_reporting_interval_secs: 60,
                enable_compression: true,
                enable_encryption: false,
                batch_size: 100,
                max_concurrent_queries: 10,
                query_timeout_secs: 30,
                feature_flags,
                replication: None,
            }),
            immediate: false,
        };

        let result = handler.handle(proto).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_handle_empty_config_update() {
        let handler = ConfigUpdateHandler::new();

        let proto = ProtoConfigUpdate {
            new_config: None,
            immediate: true,
        };

        let result = handler.handle(proto).await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_config_entry() {
        assert!(ConfigUpdateHandler::validate_config_entry("key", "value"));
        assert!(!ConfigUpdateHandler::validate_config_entry("", "value"));
        assert!(!ConfigUpdateHandler::validate_config_entry("key", ""));
        assert!(!ConfigUpdateHandler::validate_config_entry("", ""));
    }

    #[test]
    fn test_parse_int() {
        assert_eq!(ConfigUpdateHandler::parse_int("42"), Some(42));
        assert_eq!(ConfigUpdateHandler::parse_int("-100"), Some(-100));
        assert_eq!(ConfigUpdateHandler::parse_int("0"), Some(0));
        assert_eq!(ConfigUpdateHandler::parse_int("invalid"), None);
        assert_eq!(ConfigUpdateHandler::parse_int(""), None);
    }

    #[test]
    fn test_parse_bool() {
        // True variants
        assert_eq!(ConfigUpdateHandler::parse_bool("true"), Some(true));
        assert_eq!(ConfigUpdateHandler::parse_bool("True"), Some(true));
        assert_eq!(ConfigUpdateHandler::parse_bool("TRUE"), Some(true));
        assert_eq!(ConfigUpdateHandler::parse_bool("1"), Some(true));
        assert_eq!(ConfigUpdateHandler::parse_bool("yes"), Some(true));
        assert_eq!(ConfigUpdateHandler::parse_bool("on"), Some(true));

        // False variants
        assert_eq!(ConfigUpdateHandler::parse_bool("false"), Some(false));
        assert_eq!(ConfigUpdateHandler::parse_bool("False"), Some(false));
        assert_eq!(ConfigUpdateHandler::parse_bool("FALSE"), Some(false));
        assert_eq!(ConfigUpdateHandler::parse_bool("0"), Some(false));
        assert_eq!(ConfigUpdateHandler::parse_bool("no"), Some(false));
        assert_eq!(ConfigUpdateHandler::parse_bool("off"), Some(false));

        // Invalid
        assert_eq!(ConfigUpdateHandler::parse_bool("invalid"), None);
        assert_eq!(ConfigUpdateHandler::parse_bool(""), None);
    }

    #[test]
    fn test_parse_json() {
        // Valid JSON
        let json = ConfigUpdateHandler::parse_json(r#"{"key": "value"}"#);
        assert!(json.is_some());
        assert_eq!(json.unwrap()["key"], "value");

        let json_array = ConfigUpdateHandler::parse_json(r#"[1, 2, 3]"#);
        assert!(json_array.is_some());

        // Invalid JSON
        let invalid = ConfigUpdateHandler::parse_json("not json");
        assert!(invalid.is_none());
    }

    #[tokio::test]
    async fn test_multiple_config_entries() {
        use graphica_core::distributed::proto::coordinator_service::ShardConfiguration;

        let handler = ConfigUpdateHandler::new();

        let mut feature_flags = HashMap::new();
        feature_flags.insert("key1".to_string(), "value1".to_string());
        feature_flags.insert("key2".to_string(), "value2".to_string());
        feature_flags.insert("key3".to_string(), "value3".to_string());

        let proto = ProtoConfigUpdate {
            new_config: Some(ShardConfiguration {
                heartbeat_interval_secs: 30,
                stats_reporting_interval_secs: 60,
                enable_compression: true,
                enable_encryption: false,
                batch_size: 100,
                max_concurrent_queries: 10,
                query_timeout_secs: 30,
                feature_flags: feature_flags.clone(),
                replication: None,
            }),
            immediate: false,
        };

        let result = handler.handle(proto).await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_config_update_debug() {
        let mut entries = HashMap::new();
        entries.insert("test".to_string(), "value".to_string());

        let update = ConfigUpdate {
            entries,
            immediate: true,
        };

        let debug_str = format!("{:?}", update);
        assert!(debug_str.contains("test"));
        assert!(debug_str.contains("immediate"));
    }

    #[test]
    fn test_config_update_clone() {
        let mut entries = HashMap::new();
        entries.insert("test".to_string(), "value".to_string());

        let update = ConfigUpdate {
            entries,
            immediate: true,
        };

        let cloned = update.clone();
        assert_eq!(cloned.entries.len(), 1);
        assert_eq!(cloned.immediate, true);
    }
}
