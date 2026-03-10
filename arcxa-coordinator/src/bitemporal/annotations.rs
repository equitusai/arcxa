// src/governance/bitemporal/annotations.rs
//
// Core bitemporal annotation types for RDF-star triples.
//
// TransactionId: Unique identifier combining sequence number, timestamp, and node ID
// BitemporalAnnotations: Complete temporal metadata (tx time + valid time)

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;

/// Transaction identifier combining monotonic sequence, wall clock, and node ID.
///
/// Design rationale:
/// - **seq**: Monotonic u64 ensures total ordering even with clock skew
/// - **timestamp**: Wall clock enables business queries ("show data as of yesterday")
/// - **node_id**: Prevents conflicts in distributed deployments
///
/// # Examples
///
/// ```ignore
/// use graphica::governance::bitemporal::TransactionId;
/// use chrono::Utc;
///
/// let tx = TransactionId {
///     seq: 42,
///     timestamp: Utc::now(),
///     node_id: 1,
/// };
///
/// assert_eq!(tx.seq, 42);
/// assert_eq!(tx.node_id, 1);
/// ```ignore
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransactionId {
    /// Monotonic sequence number for total ordering
    pub seq: u64,

    /// Wall clock timestamp for human-readable queries
    pub timestamp: DateTime<Utc>,

    /// Node identifier for distributed systems (prevents conflicts)
    pub node_id: u16,
}

impl TransactionId {
    /// Create a new transaction ID with explicit values
    pub fn new(seq: u64, timestamp: DateTime<Utc>, node_id: u16) -> Self {
        Self {
            seq,
            timestamp,
            node_id,
        }
    }

    /// Get global ordering key (combines node_id and seq for distributed uniqueness)
    pub fn global_ordering_key(&self) -> u128 {
        ((self.node_id as u128) << 64) | (self.seq as u128)
    }
}

impl fmt::Display for TransactionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Tx[node={}, seq={}, ts={}]",
            self.node_id,
            self.seq,
            self.timestamp.to_rfc3339()
        )
    }
}

impl PartialOrd for TransactionId {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for TransactionId {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Primary ordering by seq (monotonic)
        match self.seq.cmp(&other.seq) {
            std::cmp::Ordering::Equal => {
                // Tie-break by node_id
                self.node_id.cmp(&other.node_id)
            }
            other => other,
        }
    }
}

/// Complete bitemporal annotations for an RDF-star triple.
///
/// Separates system-assigned transaction time from business-assigned valid time.
///
/// # System Time (Transaction Time)
/// - **tx_id**: Transaction that inserted this version
/// - **tx_from**: When the system learned about this data
/// - **tx_to**: When this version was superseded (None = current version)
///
/// # Business Time (Valid Time)
/// - **valid_from**: When this data became true in the real world
/// - **valid_to**: When this data ceased to be true (None = still valid)
///
/// # Examples
///
/// ```ignore
/// use graphica::governance::bitemporal::{BitemporalAnnotations, TransactionId};
/// use chrono::Utc;
///
/// let tx_id = TransactionId::new(42, Utc::now(), 1);
/// let valid_from = Utc::now();
///
/// let annotations = BitemporalAnnotations::new(
///     tx_id,
///     valid_from,
///     None, // Still valid
/// );
///
/// assert!(annotations.is_current_version());
/// assert!(annotations.is_currently_valid());
/// ```ignore
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BitemporalAnnotations {
    // --- System-assigned (transaction time) ---
    /// Transaction that created this version
    pub tx_id: TransactionId,

    /// When the system learned about this data
    pub tx_from: DateTime<Utc>,

    /// When this version was superseded (None = current version)
    pub tx_to: Option<DateTime<Utc>>,

    // --- Business-assigned (valid time) ---
    /// When this data became true in the real world
    pub valid_from: DateTime<Utc>,

    /// When this data ceased to be true (None = still valid)
    pub valid_to: Option<DateTime<Utc>>,
}

impl BitemporalAnnotations {
    /// Create new bitemporal annotations
    ///
    /// # Arguments
    /// * `tx_id` - Transaction identifier
    /// * `valid_from` - When data became valid
    /// * `valid_to` - When data became invalid (None = still valid)
    pub fn new(
        tx_id: TransactionId,
        valid_from: DateTime<Utc>,
        valid_to: Option<DateTime<Utc>>,
    ) -> Self {
        Self {
            tx_from: tx_id.timestamp,
            tx_id,
            tx_to: None, // New versions are always current
            valid_from,
            valid_to,
        }
    }

    /// Check if this is the current version (not superseded)
    pub fn is_current_version(&self) -> bool {
        self.tx_to.is_none()
    }

    /// Check if this data is currently valid (business time)
    pub fn is_currently_valid(&self) -> bool {
        self.valid_to.is_none()
    }

    /// Check if this version is visible at a specific transaction time
    pub fn visible_at_tx_time(&self, tx_time: DateTime<Utc>) -> bool {
        self.tx_from <= tx_time && self.tx_to.map_or(true, |to| tx_time < to)
    }

    /// Check if this data is valid at a specific business time
    pub fn valid_at_time(&self, valid_time: DateTime<Utc>) -> bool {
        self.valid_from <= valid_time && self.valid_to.map_or(true, |to| valid_time < to)
    }

    /// Check if this version is visible in both dimensions
    pub fn visible_at(&self, tx_time: DateTime<Utc>, valid_time: DateTime<Utc>) -> bool {
        self.visible_at_tx_time(tx_time) && self.valid_at_time(valid_time)
    }

    /// Close this version (mark as superseded)
    pub fn close_version(&mut self, closing_tx: &TransactionId) {
        self.tx_to = Some(closing_tx.timestamp);
    }

    /// Close the valid time range (mark data as no longer valid)
    pub fn close_valid_time(&mut self, end_time: DateTime<Utc>) {
        self.valid_to = Some(end_time);
    }
}

impl fmt::Display for BitemporalAnnotations {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Bitemporal[tx: {} -> {}, valid: {} -> {}]",
            self.tx_from.to_rfc3339(),
            self.tx_to
                .map(|t| t.to_rfc3339())
                .unwrap_or_else(|| "MAX".to_string()),
            self.valid_from.to_rfc3339(),
            self.valid_to
                .map(|t| t.to_rfc3339())
                .unwrap_or_else(|| "MAX".to_string()),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    #[test]
    fn test_transaction_id_creation() {
        let now = Utc::now();
        let tx = TransactionId::new(100, now, 5);

        assert_eq!(tx.seq, 100);
        assert_eq!(tx.timestamp, now);
        assert_eq!(tx.node_id, 5);
    }

    #[test]
    fn test_transaction_id_ordering() {
        let now = Utc::now();
        let tx1 = TransactionId::new(1, now, 1);
        let tx2 = TransactionId::new(2, now, 1);
        let tx3 = TransactionId::new(2, now, 2);

        assert!(tx1 < tx2);
        assert!(tx2 < tx3);
    }

    #[test]
    fn test_transaction_id_global_ordering() {
        let now = Utc::now();
        let tx1 = TransactionId::new(100, now, 1);
        let tx2 = TransactionId::new(200, now, 2);

        let key1 = tx1.global_ordering_key();
        let key2 = tx2.global_ordering_key();

        assert_ne!(key1, key2);
        assert!(key1 > 0);
        assert!(key2 > 0);
    }

    #[test]
    fn test_transaction_id_display() {
        let now = Utc::now();
        let tx = TransactionId::new(42, now, 1);
        let display = format!("{}", tx);

        assert!(display.contains("node=1"));
        assert!(display.contains("seq=42"));
    }

    #[test]
    fn test_bitemporal_annotations_creation() {
        let now = Utc::now();
        let tx_id = TransactionId::new(1, now, 1);

        let annotations = BitemporalAnnotations::new(tx_id, now, None);

        assert_eq!(annotations.tx_id.seq, 1);
        assert_eq!(annotations.tx_from, now);
        assert!(annotations.tx_to.is_none());
        assert_eq!(annotations.valid_from, now);
        assert!(annotations.valid_to.is_none());
    }

    #[test]
    fn test_is_current_version() {
        let now = Utc::now();
        let tx_id = TransactionId::new(1, now, 1);
        let mut annotations = BitemporalAnnotations::new(tx_id, now, None);

        assert!(annotations.is_current_version());

        let closing_tx = TransactionId::new(2, now + Duration::seconds(1), 1);
        annotations.close_version(&closing_tx);

        assert!(!annotations.is_current_version());
        assert!(annotations.tx_to.is_some());
    }

    #[test]
    fn test_is_currently_valid() {
        let now = Utc::now();
        let tx_id = TransactionId::new(1, now, 1);
        let mut annotations = BitemporalAnnotations::new(tx_id, now, None);

        assert!(annotations.is_currently_valid());

        annotations.close_valid_time(now + Duration::days(1));

        assert!(!annotations.is_currently_valid());
        assert!(annotations.valid_to.is_some());
    }

    #[test]
    fn test_visible_at_tx_time() {
        let t0 = Utc::now();
        let t1 = t0 + Duration::seconds(10);
        let t2 = t1 + Duration::seconds(10);
        let t3 = t2 + Duration::seconds(10);

        let tx_id = TransactionId::new(1, t1, 1);
        let mut annotations = BitemporalAnnotations::new(tx_id, t1, None);

        // Should not be visible before tx_from
        assert!(!annotations.visible_at_tx_time(t0));

        // Should be visible at tx_from
        assert!(annotations.visible_at_tx_time(t1));

        // Should be visible after tx_from (no tx_to yet)
        assert!(annotations.visible_at_tx_time(t2));

        // Close the version
        let closing_tx = TransactionId::new(2, t2, 1);
        annotations.close_version(&closing_tx);

        // Should still be visible at t1
        assert!(annotations.visible_at_tx_time(t1));

        // Should not be visible at or after tx_to
        assert!(!annotations.visible_at_tx_time(t2));
        assert!(!annotations.visible_at_tx_time(t3));
    }

    #[test]
    fn test_valid_at_time() {
        let t0 = Utc::now();
        let t1 = t0 + Duration::days(1);
        let t2 = t1 + Duration::days(1);
        let t3 = t2 + Duration::days(1);

        let tx_id = TransactionId::new(1, t0, 1);
        let mut annotations = BitemporalAnnotations::new(tx_id, t1, None);

        // Should not be valid before valid_from
        assert!(!annotations.valid_at_time(t0));

        // Should be valid at valid_from
        assert!(annotations.valid_at_time(t1));

        // Should be valid after valid_from (no valid_to yet)
        assert!(annotations.valid_at_time(t2));

        // Close the valid time
        annotations.close_valid_time(t2);

        // Should still be valid at t1
        assert!(annotations.valid_at_time(t1));

        // Should not be valid at or after valid_to
        assert!(!annotations.valid_at_time(t2));
        assert!(!annotations.valid_at_time(t3));
    }

    #[test]
    fn test_visible_at_both_dimensions() {
        let tx_time = Utc::now();
        let valid_time = tx_time + Duration::days(30);

        let tx_id = TransactionId::new(1, tx_time, 1);
        let annotations = BitemporalAnnotations::new(tx_id, valid_time, None);

        // Visible at both current times
        let query_tx = tx_time + Duration::seconds(1);
        let query_valid = valid_time + Duration::days(1);
        assert!(annotations.visible_at(query_tx, query_valid));

        // Not visible if tx time too early
        assert!(!annotations.visible_at(tx_time - Duration::seconds(1), query_valid));

        // Not visible if valid time too early
        assert!(!annotations.visible_at(query_tx, valid_time - Duration::seconds(1)));
    }

    #[test]
    fn test_bitemporal_display() {
        let now = Utc::now();
        let tx_id = TransactionId::new(1, now, 1);
        let annotations = BitemporalAnnotations::new(tx_id, now, None);

        let display = format!("{}", annotations);
        assert!(display.contains("Bitemporal"));
        assert!(display.contains("MAX"));
    }
}
