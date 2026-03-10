//! Audit DTOs
//!
//! Request and response types for audit log queries and compliance operations.

use chrono::{DateTime, Utc};
use serde::Deserialize;

// =============================================================================
// Audit Query DTOs
// =============================================================================

#[derive(Deserialize)]
pub struct AuditQueryRequest {
    pub user_id: Option<String>,
    pub username: Option<String>,
    pub event_types: Option<Vec<String>>,
    pub start_time: Option<DateTime<Utc>>,
    pub end_time: Option<DateTime<Utc>>,
    pub limit: Option<usize>,
}
