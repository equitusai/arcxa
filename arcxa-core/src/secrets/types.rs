//! Secret types and data structures

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Represents a secret with its value and metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Secret {
    /// Secret path/key
    pub path: String,

    /// Secret value
    pub value: SecretValue,

    /// Secret metadata
    pub metadata: SecretMetadata,

    /// Version ID
    pub version: String,

    /// Creation timestamp
    #[serde(rename = "createdAt")]
    pub created_at: DateTime<Utc>,

    /// Last modified timestamp
    #[serde(rename = "updatedAt")]
    pub updated_at: DateTime<Utc>,
}

/// Secret value (can be string, JSON, or binary)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum SecretValue {
    /// Plain string secret
    String(String),

    /// JSON object secret
    Json(serde_json::Value),

    /// Key-value pairs (for database credentials, etc.)
    KeyValue(HashMap<String, String>),

    /// Binary data (base64 encoded)
    Binary(Vec<u8>),
}

impl SecretValue {
    /// Create a string secret
    pub fn from_string(s: impl Into<String>) -> Self {
        Self::String(s.into())
    }

    /// Create a JSON secret
    pub fn from_json(value: serde_json::Value) -> Self {
        Self::Json(value)
    }

    /// Create a key-value secret
    pub fn from_key_value(map: HashMap<String, String>) -> Self {
        Self::KeyValue(map)
    }

    /// Create credentials from username/password
    pub fn from_credentials(username: impl Into<String>, password: impl Into<String>) -> Self {
        let mut map = HashMap::new();
        map.insert("username".to_string(), username.into());
        map.insert("password".to_string(), password.into());
        Self::KeyValue(map)
    }

    /// Get value as string (if possible)
    pub fn as_string(&self) -> Option<&str> {
        match self {
            Self::String(s) => Some(s),
            _ => None,
        }
    }

    /// Get value as key-value map
    pub fn as_key_value(&self) -> Option<&HashMap<String, String>> {
        match self {
            Self::KeyValue(map) => Some(map),
            _ => None,
        }
    }

    /// Extract username from credentials
    pub fn username(&self) -> Option<&str> {
        self.as_key_value()
            .and_then(|map| map.get("username"))
            .map(|s| s.as_str())
    }

    /// Extract password from credentials
    pub fn password(&self) -> Option<&str> {
        self.as_key_value()
            .and_then(|map| map.get("password"))
            .map(|s| s.as_str())
    }
}

/// Secret metadata
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SecretMetadata {
    /// Human-readable description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Tags for organization
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,

    /// Time-to-live in seconds (for auto-expiry)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ttl_seconds: Option<u64>,

    /// Custom metadata fields
    #[serde(flatten)]
    pub custom: HashMap<String, serde_json::Value>,

    /// Owner/creator of the secret
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,

    /// Rotation policy
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rotation_policy: Option<RotationPolicy>,
}

/// Secret rotation policy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RotationPolicy {
    /// Rotation interval in days
    #[serde(rename = "intervalDays")]
    pub interval_days: u32,

    /// Last rotation timestamp
    #[serde(rename = "lastRotated")]
    pub last_rotated: Option<DateTime<Utc>>,

    /// Next scheduled rotation
    #[serde(rename = "nextRotation")]
    pub next_rotation: Option<DateTime<Utc>>,

    /// Auto-rotate enabled
    #[serde(rename = "autoRotate")]
    pub auto_rotate: bool,
}

/// Secret version information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretVersion {
    /// Version ID
    pub version: String,

    /// Creation timestamp
    #[serde(rename = "createdAt")]
    pub created_at: DateTime<Utc>,

    /// Whether this version is active
    pub active: bool,

    /// Deprecation timestamp (if deprecated)
    #[serde(rename = "deprecatedAt")]
    pub deprecated_at: Option<DateTime<Utc>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_secret_value_credentials() {
        let secret = SecretValue::from_credentials("admin", "secret123");
        assert_eq!(secret.username(), Some("admin"));
        assert_eq!(secret.password(), Some("secret123"));
    }

    #[test]
    fn test_secret_value_string() {
        let secret = SecretValue::from_string("my-secret");
        assert_eq!(secret.as_string(), Some("my-secret"));
    }

    #[test]
    fn test_secret_value_key_value() {
        let mut map = HashMap::new();
        map.insert("api_key".to_string(), "key123".to_string());
        let secret = SecretValue::from_key_value(map);
        assert!(secret.as_key_value().is_some());
    }
}
