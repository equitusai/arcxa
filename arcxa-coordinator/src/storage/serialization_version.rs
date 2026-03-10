//! Serialization Schema Versioning
//!
//! Provides versioning for binary-serialized data formats to ensure backward/forward compatibility
//! when deserializing Bincode data from RocksDB.
//!
//! **IMPORTANT**: This is SEPARATE from:
//! - `bitemporal::version_manager` - Temporal data versioning (transaction-time, valid-time)
//! - `gitops::deployment::versioning` - Deployment/release versioning
//!
//! This module handles **data format schema evolution** for persisted structures.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fmt;

/// Current serialization format version
///
/// **Version History:**
/// - V1 (2025-01): Initial row lineage format
/// - V2 (future): Add support for schema migrations
pub const CURRENT_SERIALIZATION_VERSION: SerializationVersion = SerializationVersion::V1;

/// Serialization format version for binary data
///
/// This tracks the **schema version** of serialized data structures, not the data itself.
/// Used to detect incompatibilities and trigger migrations when deserializing from storage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[repr(u16)]
pub enum SerializationVersion {
    /// Initial format (2025-01)
    V1 = 1,
}

impl SerializationVersion {
    /// Check if this version is compatible with current code
    pub fn is_compatible_with_current(self) -> bool {
        self <= CURRENT_SERIALIZATION_VERSION
    }

    /// Get the version number as u16
    pub fn as_u16(self) -> u16 {
        self as u16
    }

    /// Parse from u16
    pub fn from_u16(value: u16) -> Option<Self> {
        match value {
            1 => Some(SerializationVersion::V1),
            _ => None,
        }
    }
}

impl fmt::Display for SerializationVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SerializationVersion::V1 => write!(f, "v1"),
        }
    }
}

/// Versioned envelope for serialized data
///
/// All data stored in RocksDB should be wrapped in this envelope to enable:
/// - Schema compatibility checks
/// - Migration support
/// - Forward/backward compatibility
///
/// # Example
/// ```ignore
/// let event = RowLineageEvent { /* ... */ };
/// let envelope = VersionedData::wrap(event)?;
/// let bytes = bincode::serialize(&envelope)?;
///
/// // Later, deserialize with version check:
/// let envelope: VersionedData<RowLineageEvent> = bincode::deserialize(&bytes)?;
/// let event = envelope.unwrap_current()?;
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionedData<T> {
    /// Schema version of the serialized data
    pub version: SerializationVersion,
    /// The actual data payload
    pub data: T,
}

impl<T> VersionedData<T>
where
    T: Serialize + for<'de> Deserialize<'de>,
{
    /// Wrap data with current serialization version
    pub fn wrap(data: T) -> Self {
        Self {
            version: CURRENT_SERIALIZATION_VERSION,
            data,
        }
    }

    /// Unwrap data, checking version compatibility
    pub fn unwrap_current(self) -> Result<T> {
        if !self.version.is_compatible_with_current() {
            anyhow::bail!(
                "Incompatible serialization version: found {}, current is {}",
                self.version,
                CURRENT_SERIALIZATION_VERSION
            );
        }
        Ok(self.data)
    }

    /// Serialize with version envelope
    pub fn serialize(&self) -> Result<Vec<u8>> {
        bincode::serialize(self).context("Failed to serialize versioned data")
    }

    /// Deserialize with version check
    pub fn deserialize(bytes: &[u8]) -> Result<Self> {
        bincode::deserialize(bytes).context("Failed to deserialize versioned data")
    }
}

/// Helper for collections (Vec, etc.)
impl<T> VersionedData<Vec<T>>
where
    T: Serialize + for<'de> Deserialize<'de>,
{
    /// Wrap a vector of items
    pub fn wrap_vec(items: Vec<T>) -> Self {
        Self::wrap(items)
    }

    /// Unwrap to vector
    pub fn unwrap_vec(self) -> Result<Vec<T>> {
        self.unwrap_current()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    struct TestData {
        id: String,
        value: i32,
    }

    #[test]
    fn test_version_ordering() {
        assert!(SerializationVersion::V1 <= CURRENT_SERIALIZATION_VERSION);
    }

    #[test]
    fn test_version_compatibility() {
        assert!(SerializationVersion::V1.is_compatible_with_current());
    }

    #[test]
    fn test_versioned_data_roundtrip() {
        let data = TestData {
            id: "test_123".to_string(),
            value: 42,
        };

        let envelope = VersionedData::wrap(data.clone());
        assert_eq!(envelope.version, CURRENT_SERIALIZATION_VERSION);

        let bytes = envelope.serialize().unwrap();
        let deserialized = VersionedData::<TestData>::deserialize(&bytes).unwrap();

        assert_eq!(deserialized.version, CURRENT_SERIALIZATION_VERSION);
        assert_eq!(deserialized.data, data);
    }

    #[test]
    fn test_versioned_vec_roundtrip() {
        let items = vec![
            TestData {
                id: "item_1".to_string(),
                value: 1,
            },
            TestData {
                id: "item_2".to_string(),
                value: 2,
            },
        ];

        let envelope = VersionedData::wrap_vec(items.clone());
        let bytes = envelope.serialize().unwrap();
        let deserialized = VersionedData::<Vec<TestData>>::deserialize(&bytes).unwrap();
        let unwrapped = deserialized.unwrap_vec().unwrap();

        assert_eq!(unwrapped, items);
    }

    #[test]
    fn test_version_from_u16() {
        assert_eq!(
            SerializationVersion::from_u16(1),
            Some(SerializationVersion::V1)
        );
        assert_eq!(SerializationVersion::from_u16(99), None);
    }

    #[test]
    fn test_version_display() {
        assert_eq!(format!("{}", SerializationVersion::V1), "v1");
    }
}
