//! Security Audit Logging
//!
//! Comprehensive audit logging for security events, user actions, and system changes.
//! Designed for compliance (SOC 2, GDPR, HIPAA) and security monitoring (SIEM integration).

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::net::IpAddr;
use std::sync::Arc;

use crate::api::auth::Role;
use crate::storage::kv_store::KvStore;

/// Security audit event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEvent {
    /// Unique event ID (UUID)
    pub id: String,
    /// Event timestamp (UTC)
    pub timestamp: DateTime<Utc>,
    /// Event type (login, logout, user_created, etc.)
    pub event_type: AuditEventType,
    /// User who performed the action (if authenticated)
    pub user_id: Option<String>,
    /// Username (for easier querying)
    pub username: Option<String>,
    /// User's role at time of action
    pub user_role: Option<Role>,
    /// Client IP address
    pub ip_address: Option<IpAddr>,
    /// User agent string (browser/client identification)
    pub user_agent: Option<String>,
    /// Resource being accessed/modified
    pub resource: Option<String>,
    /// Action performed (read, write, delete, etc.)
    pub action: String,
    /// Result of the action
    pub result: AuditResult,
    /// Additional context (JSON metadata)
    pub metadata: serde_json::Value,
    /// Session ID (for correlation)
    pub session_id: Option<String>,
}

/// Type of audit event
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AuditEventType {
    // Authentication events
    LoginSuccess,
    LoginFailure,
    Logout,
    TokenGenerated,
    TokenRefreshed,
    TokenRevoked,

    // User management events
    UserCreated,
    UserUpdated,
    UserDeleted,
    PasswordChanged,
    AccountLocked,
    AccountUnlocked,

    // Setup events
    SetupTokenGenerated,
    SetupTokenUsed,
    SetupTokenExpired,
    AdminCreated,

    // Authorization events
    AccessGranted,
    AccessDenied,
    PermissionChanged,
    RoleChanged,

    // Data access events
    DataRead,
    DataWritten,
    DataDeleted,
    QueryExecuted,

    // System events
    ConfigurationChanged,
    SystemStarted,
    SystemShutdown,
    BackupCreated,
    BackupRestored,

    // Security events
    RateLimitExceeded,
    SuspiciousActivity,
    SecurityViolation,
    EncryptionKeyRotated,
}

/// Result of audited action
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AuditResult {
    Success,
    Failure,
    PartialSuccess,
    Denied,
}

/// Audit sink trait for pluggable storage backends
#[async_trait::async_trait]
pub trait AuditSink: Send + Sync {
    /// Write audit event to storage
    async fn write(&self, event: &AuditEvent) -> Result<()>;

    /// Query audit events with filters
    async fn query(&self, filter: AuditFilter) -> Result<Vec<AuditEvent>>;

    /// Delete events older than retention period
    async fn cleanup(&self, retention_days: u32) -> Result<usize>;

    /// Export events in SIEM-compatible format (JSON Lines)
    async fn export(&self, filter: AuditFilter) -> Result<String>;
}

/// Filter for querying audit events
#[derive(Debug, Clone, Default)]
pub struct AuditFilter {
    pub user_id: Option<String>,
    pub username: Option<String>,
    pub event_types: Option<Vec<AuditEventType>>,
    pub start_time: Option<DateTime<Utc>>,
    pub end_time: Option<DateTime<Utc>>,
    pub ip_address: Option<IpAddr>,
    pub result: Option<AuditResult>,
    pub limit: Option<usize>,
}

/// RocksDB-based audit storage
pub struct RocksAuditSink {
    storage: Arc<KvStore>,
}

impl RocksAuditSink {
    /// Create new RocksDB audit sink
    pub fn new(storage: Arc<KvStore>) -> Self {
        Self { storage }
    }

    /// Generate storage key for event
    fn event_key(event: &AuditEvent) -> String {
        // Key format: audit:timestamp:event_id
        // Allows efficient time-based queries
        format!("audit:{}:{}", event.timestamp.timestamp_millis(), event.id)
    }

    /// Generate index key for user queries
    fn user_index_key(user_id: &str, timestamp: i64, event_id: &str) -> String {
        format!("audit_user:{}:{}:{}", user_id, timestamp, event_id)
    }
}

#[async_trait::async_trait]
impl AuditSink for RocksAuditSink {
    async fn write(&self, event: &AuditEvent) -> Result<()> {
        let event_json = serde_json::to_vec(event).context("Failed to serialize audit event")?;

        // Store event by timestamp+ID
        let key = Self::event_key(event);
        self.storage
            .put(key.as_bytes(), &event_json)
            .context("Failed to store audit event")?;

        // Create user index if user_id is present
        if let Some(user_id) = &event.user_id {
            let user_key =
                Self::user_index_key(user_id, event.timestamp.timestamp_millis(), &event.id);
            // Store pointer to main event (saves space)
            self.storage
                .put(user_key.as_bytes(), key.as_bytes())
                .context("Failed to store user audit index")?;
        }

        Ok(())
    }

    async fn query(&self, filter: AuditFilter) -> Result<Vec<AuditEvent>> {
        let mut events = Vec::new();

        // Determine which index to use
        let scan_results = if let Some(user_id) = &filter.user_id {
            // Use user index for efficient user-specific queries
            let prefix = format!("audit_user:{}:", user_id);

            if let Some(start_time) = filter.start_time {
                // Start from specific timestamp
                let start_key = format!("audit_user:{}:{}", user_id, start_time.timestamp_millis());
                self.storage
                    .prefix_scan_from(start_key.as_bytes(), prefix.as_bytes())?
            } else {
                // Scan all events for this user
                self.storage.prefix_scan(prefix.as_bytes())?
            }
        } else {
            // Use main audit index
            if let Some(start_time) = filter.start_time {
                // Start from specific timestamp
                let start_key = format!("audit:{}", start_time.timestamp_millis());
                self.storage
                    .prefix_scan_from(start_key.as_bytes(), b"audit:")?
            } else {
                // Scan all audit events
                self.storage.prefix_scan(b"audit:")?
            }
        };

        // Process scan results
        for (key, value) in scan_results {
            // If using user index, value contains pointer to main event
            let event_data = if key.starts_with(b"audit_user:") {
                // Read the actual event from the pointer
                if let Some(event_key) = self.storage.get(&value)? {
                    event_key
                } else {
                    continue; // Event was deleted
                }
            } else {
                value
            };

            // Deserialize event
            let event: AuditEvent = match serde_json::from_slice(&event_data) {
                Ok(evt) => evt,
                Err(_) => continue, // Skip corrupted events
            };

            // Apply filters
            if let Some(end_time) = filter.end_time {
                if event.timestamp > end_time {
                    break; // Events are sorted by time, so we're done
                }
            }

            if let Some(ref username) = filter.username {
                if event.username.as_ref() != Some(username) {
                    continue;
                }
            }

            if let Some(ref event_types) = filter.event_types {
                if !event_types.contains(&event.event_type) {
                    continue;
                }
            }

            if let Some(ref ip) = filter.ip_address {
                if event.ip_address.as_ref() != Some(ip) {
                    continue;
                }
            }

            if let Some(ref result) = filter.result {
                if &event.result != result {
                    continue;
                }
            }

            events.push(event);

            // Apply limit
            if let Some(limit) = filter.limit {
                if events.len() >= limit {
                    break;
                }
            }
        }

        Ok(events)
    }

    async fn cleanup(&self, retention_days: u32) -> Result<usize> {
        let cutoff = Utc::now() - chrono::Duration::days(retention_days as i64);
        let cutoff_ts = cutoff.timestamp_millis();

        let mut deleted_count = 0;

        // Scan all audit events
        let all_events = self.storage.prefix_scan(b"audit:")?;

        for (key, value) in all_events {
            // Parse the key to extract timestamp: "audit:timestamp:event_id"
            let key_str = String::from_utf8_lossy(&key);
            let parts: Vec<&str> = key_str.split(':').collect();

            if parts.len() >= 2 {
                if let Ok(timestamp) = parts[1].parse::<i64>() {
                    if timestamp < cutoff_ts {
                        // Delete main event
                        self.storage.delete(&key)?;
                        deleted_count += 1;

                        // Deserialize event to get user_id for index cleanup
                        if let Ok(event) = serde_json::from_slice::<AuditEvent>(&value) {
                            if let Some(user_id) = &event.user_id {
                                // Delete user index entry
                                let user_key = Self::user_index_key(
                                    user_id,
                                    event.timestamp.timestamp_millis(),
                                    &event.id,
                                );
                                let _ = self.storage.delete(user_key.as_bytes());
                            }
                        }
                    }
                }
            }
        }

        Ok(deleted_count)
    }

    async fn export(&self, filter: AuditFilter) -> Result<String> {
        let events = self.query(filter).await?;

        // SIEM export format: JSON Lines (one JSON object per line)
        let mut output = String::new();
        for event in events {
            let json =
                serde_json::to_string(&event).context("Failed to serialize event for export")?;
            output.push_str(&json);
            output.push('\n');
        }

        Ok(output)
    }
}

/// Audit logger - high-level API for creating audit events
pub struct AuditLogger {
    pub sink: Arc<dyn AuditSink>,
}

impl AuditLogger {
    /// Create new audit logger
    pub fn new(sink: Arc<dyn AuditSink>) -> Self {
        Self { sink }
    }

    /// Log authentication success
    pub async fn log_login_success(
        &self,
        user_id: &str,
        username: &str,
        role: Role,
        ip: Option<IpAddr>,
        user_agent: Option<String>,
    ) -> Result<()> {
        let event = AuditEvent {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            event_type: AuditEventType::LoginSuccess,
            user_id: Some(user_id.to_string()),
            username: Some(username.to_string()),
            user_role: Some(role),
            ip_address: ip,
            user_agent,
            resource: None,
            action: "login".to_string(),
            result: AuditResult::Success,
            metadata: serde_json::json!({}),
            session_id: None,
        };

        self.sink.write(&event).await
    }

    /// Log authentication failure
    pub async fn log_login_failure(
        &self,
        username: &str,
        reason: &str,
        ip: Option<IpAddr>,
        user_agent: Option<String>,
    ) -> Result<()> {
        let event = AuditEvent {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            event_type: AuditEventType::LoginFailure,
            user_id: None,
            username: Some(username.to_string()),
            user_role: None,
            ip_address: ip,
            user_agent,
            resource: None,
            action: "login".to_string(),
            result: AuditResult::Failure,
            metadata: serde_json::json!({ "reason": reason }),
            session_id: None,
        };

        self.sink.write(&event).await
    }

    /// Log user creation
    pub async fn log_user_created(
        &self,
        created_by_user_id: Option<&str>,
        created_by_username: Option<&str>,
        new_user_id: &str,
        new_username: &str,
        new_user_role: Role,
        ip: Option<IpAddr>,
    ) -> Result<()> {
        let event = AuditEvent {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            event_type: AuditEventType::UserCreated,
            user_id: created_by_user_id.map(String::from),
            username: created_by_username.map(String::from),
            user_role: None, // Role of creator (would need to be passed)
            ip_address: ip,
            user_agent: None,
            resource: Some(format!("user:{}", new_user_id)),
            action: "create".to_string(),
            result: AuditResult::Success,
            metadata: serde_json::json!({
                "new_user_id": new_user_id,
                "new_username": new_username,
                "new_user_role": new_user_role,
            }),
            session_id: None,
        };

        self.sink.write(&event).await
    }

    /// Log account lockout
    pub async fn log_account_locked(
        &self,
        user_id: &str,
        username: &str,
        locked_until: DateTime<Utc>,
        failed_attempts: u32,
        ip: Option<IpAddr>,
    ) -> Result<()> {
        let event = AuditEvent {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            event_type: AuditEventType::AccountLocked,
            user_id: Some(user_id.to_string()),
            username: Some(username.to_string()),
            user_role: None,
            ip_address: ip,
            user_agent: None,
            resource: Some(format!("user:{}", user_id)),
            action: "lock".to_string(),
            result: AuditResult::Success,
            metadata: serde_json::json!({
                "locked_until": locked_until,
                "failed_attempts": failed_attempts,
            }),
            session_id: None,
        };

        self.sink.write(&event).await
    }

    /// Log access denied (authorization failure)
    pub async fn log_access_denied(
        &self,
        user_id: &str,
        username: &str,
        role: Role,
        resource: &str,
        required_permission: &str,
        ip: Option<IpAddr>,
    ) -> Result<()> {
        let event = AuditEvent {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            event_type: AuditEventType::AccessDenied,
            user_id: Some(user_id.to_string()),
            username: Some(username.to_string()),
            user_role: Some(role),
            ip_address: ip,
            user_agent: None,
            resource: Some(resource.to_string()),
            action: "access".to_string(),
            result: AuditResult::Denied,
            metadata: serde_json::json!({
                "required_permission": required_permission,
            }),
            session_id: None,
        };

        self.sink.write(&event).await
    }

    /// Log setup token usage
    pub async fn log_setup_token_used(
        &self,
        ip: Option<IpAddr>,
        user_agent: Option<String>,
    ) -> Result<()> {
        let event = AuditEvent {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            event_type: AuditEventType::SetupTokenUsed,
            user_id: None,
            username: None,
            user_role: None,
            ip_address: ip,
            user_agent,
            resource: Some("setup_token".to_string()),
            action: "consume".to_string(),
            result: AuditResult::Success,
            metadata: serde_json::json!({}),
            session_id: None,
        };

        self.sink.write(&event).await
    }

    /// Log rate limit exceeded
    pub async fn log_rate_limit_exceeded(&self, endpoint: &str, ip: Option<IpAddr>) -> Result<()> {
        let event = AuditEvent {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            event_type: AuditEventType::RateLimitExceeded,
            user_id: None,
            username: None,
            user_role: None,
            ip_address: ip,
            user_agent: None,
            resource: Some(endpoint.to_string()),
            action: "rate_limit".to_string(),
            result: AuditResult::Denied,
            metadata: serde_json::json!({}),
            session_id: None,
        };

        self.sink.write(&event).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_rocks_audit_sink() {
        let kv_store = Arc::new(KvStore::new_in_memory().unwrap());
        let sink = RocksAuditSink::new(kv_store);

        let event = AuditEvent {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            event_type: AuditEventType::LoginSuccess,
            user_id: Some("user123".to_string()),
            username: Some("testuser".to_string()),
            user_role: Some(Role::Admin),
            ip_address: Some("127.0.0.1".parse().unwrap()),
            user_agent: Some("TestClient/1.0".to_string()),
            resource: None,
            action: "login".to_string(),
            result: AuditResult::Success,
            metadata: serde_json::json!({}),
            session_id: None,
        };

        // Write should succeed
        sink.write(&event).await.unwrap();
    }

    #[tokio::test]
    async fn test_audit_logger_login_success() {
        let kv_store = Arc::new(KvStore::new_in_memory().unwrap());
        let sink = Arc::new(RocksAuditSink::new(kv_store));
        let logger = AuditLogger::new(sink);

        let result = logger
            .log_login_success(
                "user123",
                "testuser",
                Role::Operator,
                Some("192.168.1.1".parse().unwrap()),
                Some("Mozilla/5.0".to_string()),
            )
            .await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_audit_logger_login_failure() {
        let kv_store = Arc::new(KvStore::new_in_memory().unwrap());
        let sink = Arc::new(RocksAuditSink::new(kv_store));
        let logger = AuditLogger::new(sink);

        let result = logger
            .log_login_failure(
                "baduser",
                "Invalid credentials",
                Some("10.0.0.1".parse().unwrap()),
                None,
            )
            .await;

        assert!(result.is_ok());
    }

    #[test]
    fn test_event_key_format() {
        let event = AuditEvent {
            id: "evt-123".to_string(),
            timestamp: DateTime::parse_from_rfc3339("2025-01-01T00:00:00Z")
                .unwrap()
                .with_timezone(&Utc),
            event_type: AuditEventType::LoginSuccess,
            user_id: None,
            username: None,
            user_role: None,
            ip_address: None,
            user_agent: None,
            resource: None,
            action: "test".to_string(),
            result: AuditResult::Success,
            metadata: serde_json::json!({}),
            session_id: None,
        };

        let key = RocksAuditSink::event_key(&event);
        assert!(key.starts_with("audit:"));
        assert!(key.contains("evt-123"));
    }

    #[tokio::test]
    async fn test_audit_query_all() {
        let kv_store = Arc::new(KvStore::new_in_memory().unwrap());
        let sink = RocksAuditSink::new(kv_store);

        // Create multiple events
        for i in 0..5 {
            let event = AuditEvent {
                id: format!("evt-{}", i),
                timestamp: Utc::now() + chrono::Duration::seconds(i),
                event_type: AuditEventType::LoginSuccess,
                user_id: Some("user123".to_string()),
                username: Some("testuser".to_string()),
                user_role: Some(Role::Admin),
                ip_address: Some("127.0.0.1".parse().unwrap()),
                user_agent: None,
                resource: None,
                action: "login".to_string(),
                result: AuditResult::Success,
                metadata: serde_json::json!({}),
                session_id: None,
            };
            sink.write(&event).await.unwrap();
        }

        // Query all events
        let filter = AuditFilter::default();
        let events = sink.query(filter).await.unwrap();
        assert_eq!(events.len(), 5);
    }

    #[tokio::test]
    async fn test_audit_query_by_user() {
        let kv_store = Arc::new(KvStore::new_in_memory().unwrap());
        let sink = RocksAuditSink::new(kv_store);

        // Create events for different users
        for user_id in &["alice", "bob", "charlie"] {
            let event = AuditEvent {
                id: uuid::Uuid::new_v4().to_string(),
                timestamp: Utc::now(),
                event_type: AuditEventType::LoginSuccess,
                user_id: Some(user_id.to_string()),
                username: Some(user_id.to_string()),
                user_role: Some(Role::Operator),
                ip_address: None,
                user_agent: None,
                resource: None,
                action: "login".to_string(),
                result: AuditResult::Success,
                metadata: serde_json::json!({}),
                session_id: None,
            };
            sink.write(&event).await.unwrap();
        }

        // Query only alice's events
        let filter = AuditFilter {
            user_id: Some("alice".to_string()),
            ..Default::default()
        };
        let events = sink.query(filter).await.unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].user_id.as_ref().unwrap(), "alice");
    }

    #[tokio::test]
    async fn test_audit_query_with_time_range() {
        let kv_store = Arc::new(KvStore::new_in_memory().unwrap());
        let sink = RocksAuditSink::new(kv_store);

        let base_time = Utc::now();

        // Create events at different times
        for hours in &[1, 2, 3, 4, 5] {
            let event = AuditEvent {
                id: uuid::Uuid::new_v4().to_string(),
                timestamp: base_time + chrono::Duration::hours(*hours),
                event_type: AuditEventType::DataRead,
                user_id: Some("user123".to_string()),
                username: Some("testuser".to_string()),
                user_role: Some(Role::Viewer),
                ip_address: None,
                user_agent: None,
                resource: None,
                action: "read".to_string(),
                result: AuditResult::Success,
                metadata: serde_json::json!({}),
                session_id: None,
            };
            sink.write(&event).await.unwrap();
        }

        // Query events between hour 2 and 4
        let filter = AuditFilter {
            start_time: Some(base_time + chrono::Duration::hours(2)),
            end_time: Some(base_time + chrono::Duration::hours(4)),
            ..Default::default()
        };
        let events = sink.query(filter).await.unwrap();
        assert_eq!(events.len(), 3); // Hours 2, 3, 4
    }

    #[tokio::test]
    async fn test_audit_query_with_limit() {
        let kv_store = Arc::new(KvStore::new_in_memory().unwrap());
        let sink = RocksAuditSink::new(kv_store);

        // Create 10 events
        for i in 0..10 {
            let event = AuditEvent {
                id: format!("evt-{}", i),
                timestamp: Utc::now() + chrono::Duration::seconds(i),
                event_type: AuditEventType::AccessGranted,
                user_id: Some("user123".to_string()),
                username: Some("testuser".to_string()),
                user_role: Some(Role::Admin),
                ip_address: None,
                user_agent: None,
                resource: Some("resource".to_string()),
                action: "access".to_string(),
                result: AuditResult::Success,
                metadata: serde_json::json!({}),
                session_id: None,
            };
            sink.write(&event).await.unwrap();
        }

        // Query with limit of 5
        let filter = AuditFilter {
            limit: Some(5),
            ..Default::default()
        };
        let events = sink.query(filter).await.unwrap();
        assert_eq!(events.len(), 5);
    }

    #[tokio::test]
    async fn test_audit_query_by_event_type() {
        let kv_store = Arc::new(KvStore::new_in_memory().unwrap());
        let sink = RocksAuditSink::new(kv_store);

        // Create events of different types
        for event_type in &[
            AuditEventType::LoginSuccess,
            AuditEventType::LoginFailure,
            AuditEventType::DataRead,
        ] {
            let event = AuditEvent {
                id: uuid::Uuid::new_v4().to_string(),
                timestamp: Utc::now(),
                event_type: event_type.clone(),
                user_id: Some("user123".to_string()),
                username: Some("testuser".to_string()),
                user_role: Some(Role::Admin),
                ip_address: None,
                user_agent: None,
                resource: None,
                action: "test".to_string(),
                result: AuditResult::Success,
                metadata: serde_json::json!({}),
                session_id: None,
            };
            sink.write(&event).await.unwrap();
        }

        // Query only login events
        let filter = AuditFilter {
            event_types: Some(vec![
                AuditEventType::LoginSuccess,
                AuditEventType::LoginFailure,
            ]),
            ..Default::default()
        };
        let events = sink.query(filter).await.unwrap();
        assert_eq!(events.len(), 2);
        assert!(events.iter().all(|e| matches!(
            e.event_type,
            AuditEventType::LoginSuccess | AuditEventType::LoginFailure
        )));
    }

    #[tokio::test]
    async fn test_audit_cleanup() {
        let kv_store = Arc::new(KvStore::new_in_memory().unwrap());
        let sink = RocksAuditSink::new(kv_store);

        let now = Utc::now();

        // Create old events (31 days ago)
        for i in 0..3 {
            let event = AuditEvent {
                id: format!("old-evt-{}", i),
                timestamp: now - chrono::Duration::days(31),
                event_type: AuditEventType::LoginSuccess,
                user_id: Some("olduser".to_string()),
                username: Some("olduser".to_string()),
                user_role: Some(Role::Admin),
                ip_address: None,
                user_agent: None,
                resource: None,
                action: "login".to_string(),
                result: AuditResult::Success,
                metadata: serde_json::json!({}),
                session_id: None,
            };
            sink.write(&event).await.unwrap();
        }

        // Create recent events (1 day ago)
        for i in 0..2 {
            let event = AuditEvent {
                id: format!("recent-evt-{}", i),
                timestamp: now - chrono::Duration::days(1),
                event_type: AuditEventType::LoginSuccess,
                user_id: Some("newuser".to_string()),
                username: Some("newuser".to_string()),
                user_role: Some(Role::Admin),
                ip_address: None,
                user_agent: None,
                resource: None,
                action: "login".to_string(),
                result: AuditResult::Success,
                metadata: serde_json::json!({}),
                session_id: None,
            };
            sink.write(&event).await.unwrap();
        }

        // Verify all events exist
        let all_events = sink.query(AuditFilter::default()).await.unwrap();
        assert_eq!(all_events.len(), 5);

        // Cleanup events older than 30 days
        let deleted = sink.cleanup(30).await.unwrap();
        assert_eq!(deleted, 3);

        // Verify only recent events remain
        let remaining = sink.query(AuditFilter::default()).await.unwrap();
        assert_eq!(remaining.len(), 2);
        assert!(remaining.iter().all(|e| e.id.starts_with("recent")));
    }
}
