//! Retention Policy Management
//!
//! Implements retention policies and legal holds for GDPR compliance.
//! Ensures data is retained for legally mandated periods and not deleted
//! prematurely when under legal hold.

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Retention Policy
///
/// Defines how long different categories of data should be retained.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetentionPolicy {
    /// Unique policy identifier
    pub id: String,

    /// Policy name
    pub name: String,

    /// Data category this policy applies to
    pub data_category: DataCategory,

    /// Minimum retention period (e.g., for legal compliance)
    pub min_retention_days: Option<i64>,

    /// Maximum retention period (e.g., for GDPR storage limitation)
    pub max_retention_days: Option<i64>,

    /// Legal basis for retention
    pub legal_basis: String,

    /// Jurisdiction-specific rules
    pub jurisdiction: Option<String>,

    /// Whether this policy can be overridden
    pub allow_override: bool,

    /// Policy metadata
    pub metadata: HashMap<String, String>,
}

impl RetentionPolicy {
    /// Create a new retention policy
    pub fn new(
        name: impl Into<String>,
        data_category: DataCategory,
        min_days: Option<i64>,
        max_days: Option<i64>,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.into(),
            data_category,
            min_retention_days: min_days,
            max_retention_days: max_days,
            legal_basis: String::new(),
            jurisdiction: None,
            allow_override: false,
            metadata: HashMap::new(),
        }
    }

    /// Check if data can be deleted based on its age
    pub fn can_delete(&self, data_created_at: DateTime<Utc>) -> Result<bool, String> {
        let now = Utc::now();
        let data_age_days = (now - data_created_at).num_days();

        // Check minimum retention
        if let Some(min_days) = self.min_retention_days {
            if data_age_days < min_days {
                return Err(format!(
                    "Data must be retained for at least {} days (currently {} days old)",
                    min_days, data_age_days
                ));
            }
        }

        // Check maximum retention (data must be deleted after this)
        if let Some(max_days) = self.max_retention_days {
            if data_age_days > max_days {
                return Ok(true); // Must be deleted
            }
        }

        Ok(true)
    }

    /// Get retention expiry date for data
    pub fn expiry_date(&self, data_created_at: DateTime<Utc>) -> Option<DateTime<Utc>> {
        self.max_retention_days
            .map(|days| data_created_at + Duration::days(days))
    }
}

/// Data Category for retention purposes
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DataCategory {
    /// Personal identifiable information
    PersonalIdentifiers,
    /// Financial records (invoices, payments)
    Financial,
    /// Audit logs and compliance records
    AuditLogs,
    /// User-generated content
    UserContent,
    /// System logs and diagnostics
    SystemLogs,
    /// Marketing data and preferences
    Marketing,
    /// Analytics and aggregated data
    Analytics,
    /// Backup data
    Backups,
    /// Custom category
    Custom(String),
}

/// Legal Hold
///
/// Represents a legal hold that prevents data deletion for litigation or investigation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LegalHold {
    /// Unique hold identifier
    pub id: String,

    /// Hold name/case number
    pub name: String,

    /// When the hold was placed
    pub placed_at: DateTime<Utc>,

    /// Who placed the hold
    pub placed_by: String,

    /// Reason for the hold (case description)
    pub reason: String,

    /// Data subject(s) under hold
    pub data_subjects: Vec<String>,

    /// Data categories under hold
    pub data_categories: Vec<DataCategory>,

    /// Optional expiry date for the hold
    pub expires_at: Option<DateTime<Utc>>,

    /// Whether the hold is currently active
    pub active: bool,

    /// Metadata (case number, court order, etc.)
    pub metadata: HashMap<String, String>,
}

impl LegalHold {
    /// Create a new legal hold
    pub fn new(
        name: impl Into<String>,
        placed_by: impl Into<String>,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.into(),
            placed_at: Utc::now(),
            placed_by: placed_by.into(),
            reason: reason.into(),
            data_subjects: Vec::new(),
            data_categories: Vec::new(),
            expires_at: None,
            active: true,
            metadata: HashMap::new(),
        }
    }

    /// Add a data subject to the hold
    pub fn add_data_subject(mut self, subject_id: impl Into<String>) -> Self {
        self.data_subjects.push(subject_id.into());
        self
    }

    /// Add a data category to the hold
    pub fn add_data_category(mut self, category: DataCategory) -> Self {
        self.data_categories.push(category);
        self
    }

    /// Set expiry date for the hold
    pub fn with_expiry(mut self, expiry: DateTime<Utc>) -> Self {
        self.expires_at = Some(expiry);
        self
    }

    /// Check if the hold is still active
    pub fn is_active(&self) -> bool {
        if !self.active {
            return false;
        }

        if let Some(expiry) = self.expires_at {
            Utc::now() < expiry
        } else {
            true
        }
    }

    /// Check if a data subject is under this hold
    pub fn covers_subject(&self, subject_id: &str) -> bool {
        self.is_active() && self.data_subjects.iter().any(|s| s == subject_id)
    }

    /// Check if a data category is under this hold
    pub fn covers_category(&self, category: &DataCategory) -> bool {
        self.is_active() && self.data_categories.contains(category)
    }

    /// Release the hold
    pub fn release(&mut self) {
        self.active = false;
    }
}

/// Retention Policy Manager
///
/// Manages retention policies and legal holds for an organization.
#[derive(Debug, Clone)]
pub struct RetentionManager {
    policies: HashMap<DataCategory, RetentionPolicy>,
    legal_holds: HashMap<String, LegalHold>,
}

impl RetentionManager {
    /// Create a new retention manager
    pub fn new() -> Self {
        Self {
            policies: HashMap::new(),
            legal_holds: HashMap::new(),
        }
    }

    /// Add a retention policy
    pub fn add_policy(&mut self, policy: RetentionPolicy) {
        self.policies.insert(policy.data_category.clone(), policy);
    }

    /// Get retention policy for a data category
    pub fn get_policy(&self, category: &DataCategory) -> Option<&RetentionPolicy> {
        self.policies.get(category)
    }

    /// Add a legal hold
    pub fn add_legal_hold(&mut self, hold: LegalHold) {
        self.legal_holds.insert(hold.id.clone(), hold);
    }

    /// Remove a legal hold
    pub fn remove_legal_hold(&mut self, hold_id: &str) {
        self.legal_holds.remove(hold_id);
    }

    /// Get all active legal holds for a data subject
    pub fn get_active_holds_for_subject(&self, subject_id: &str) -> Vec<&LegalHold> {
        self.legal_holds
            .values()
            .filter(|hold| hold.covers_subject(subject_id))
            .collect()
    }

    /// Check if a data subject is under any legal hold
    pub fn is_subject_under_hold(&self, subject_id: &str) -> bool {
        self.legal_holds
            .values()
            .any(|hold| hold.covers_subject(subject_id))
    }

    /// Check if deletion is allowed for specific data
    pub fn can_delete(
        &self,
        data_category: &DataCategory,
        data_subject: &str,
        data_created_at: DateTime<Utc>,
    ) -> Result<bool, String> {
        // Check legal holds first (highest priority)
        if self.is_subject_under_hold(data_subject) {
            return Err(format!(
                "Data subject '{}' is under legal hold - deletion prohibited",
                data_subject
            ));
        }

        // Check category-specific holds
        let category_holds: Vec<_> = self
            .legal_holds
            .values()
            .filter(|hold| hold.covers_category(data_category))
            .collect();

        if !category_holds.is_empty() {
            return Err(format!(
                "Data category '{:?}' is under {} active legal hold(s) - deletion prohibited",
                data_category,
                category_holds.len()
            ));
        }

        // Check retention policy
        if let Some(policy) = self.get_policy(data_category) {
            policy.can_delete(data_created_at)
        } else {
            // No policy = can delete (unless hold applies)
            Ok(true)
        }
    }
}

impl Default for RetentionManager {
    fn default() -> Self {
        let mut manager = Self::new();

        // Add standard retention policies

        // Financial records - 7 years minimum (common legal requirement)
        manager.add_policy(RetentionPolicy {
            id: uuid::Uuid::new_v4().to_string(),
            name: "Financial Records Retention".to_string(),
            data_category: DataCategory::Financial,
            min_retention_days: Some(2555), // ~7 years
            max_retention_days: Some(3650), // 10 years
            legal_basis: "Tax law compliance".to_string(),
            jurisdiction: None,
            allow_override: false,
            metadata: HashMap::new(),
        });

        // Audit logs - 7 years minimum
        manager.add_policy(RetentionPolicy {
            id: uuid::Uuid::new_v4().to_string(),
            name: "Audit Log Retention".to_string(),
            data_category: DataCategory::AuditLogs,
            min_retention_days: Some(2555), // ~7 years
            max_retention_days: None,       // Keep indefinitely
            legal_basis: "Regulatory compliance".to_string(),
            jurisdiction: None,
            allow_override: false,
            metadata: HashMap::new(),
        });

        // Personal identifiers - no minimum, 3 years maximum (GDPR storage limitation)
        manager.add_policy(RetentionPolicy {
            id: uuid::Uuid::new_v4().to_string(),
            name: "Personal Data Retention".to_string(),
            data_category: DataCategory::PersonalIdentifiers,
            min_retention_days: None,
            max_retention_days: Some(1095), // 3 years
            legal_basis: "GDPR Article 5(1)(e) - Storage Limitation".to_string(),
            jurisdiction: Some("EU".to_string()),
            allow_override: true,
            metadata: HashMap::new(),
        });

        manager
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_retention_policy_can_delete() {
        let policy = RetentionPolicy::new(
            "Test Policy",
            DataCategory::Financial,
            Some(365),  // 1 year minimum
            Some(2555), // 7 years maximum
        );

        let now = Utc::now();

        // Data from 6 months ago - too new
        let recent_data = now - Duration::days(180);
        assert!(policy.can_delete(recent_data).is_err());

        // Data from 2 years ago - can delete
        let old_data = now - Duration::days(730);
        assert!(policy.can_delete(old_data).unwrap());

        // Data from 8 years ago - must delete
        let very_old_data = now - Duration::days(2920);
        assert!(policy.can_delete(very_old_data).unwrap());
    }

    #[test]
    fn test_legal_hold_coverage() {
        let hold = LegalHold::new("Case 123", "Legal Department", "Ongoing litigation")
            .add_data_subject("user123")
            .add_data_subject("user456")
            .add_data_category(DataCategory::Financial);

        assert!(hold.covers_subject("user123"));
        assert!(hold.covers_subject("user456"));
        assert!(!hold.covers_subject("user789"));

        assert!(hold.covers_category(&DataCategory::Financial));
        assert!(!hold.covers_category(&DataCategory::Marketing));
    }

    #[test]
    fn test_retention_manager_holds() {
        let mut manager = RetentionManager::new();

        let hold = LegalHold::new("Investigation", "Compliance", "Security incident")
            .add_data_subject("compromised_user");

        manager.add_legal_hold(hold);

        assert!(manager.is_subject_under_hold("compromised_user"));
        assert!(!manager.is_subject_under_hold("normal_user"));

        // Deletion should be blocked
        let result = manager.can_delete(
            &DataCategory::PersonalIdentifiers,
            "compromised_user",
            Utc::now() - Duration::days(1000),
        );

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("legal hold"));
    }

    #[test]
    fn test_expired_legal_hold() {
        let past = Utc::now() - Duration::days(10);
        let hold = LegalHold::new("Expired Case", "Legal", "Old investigation")
            .add_data_subject("user123")
            .with_expiry(past);

        // Hold is expired, should not be active
        assert!(!hold.is_active());
        assert!(!hold.covers_subject("user123"));
    }
}
