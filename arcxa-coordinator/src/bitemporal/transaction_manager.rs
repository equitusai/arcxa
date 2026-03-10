// src/governance/bitemporal/transaction_manager.rs
//
// Transaction ID generation for bitemporal MVCC.
//
// The TransactionManager provides monotonic, globally-unique transaction IDs
// that combine sequence numbers with wall clock timestamps for distributed systems.

use super::annotations::TransactionId;
use chrono::Utc;
use std::sync::atomic::{AtomicU64, Ordering};

/// Transaction manager that generates unique, monotonic transaction IDs.
///
/// Design:
/// - Uses AtomicU64 for lock-free sequence generation
/// - Combines sequence with wall clock for ordering + readability
/// - Includes node_id for distributed deployment support
///
/// # Thread Safety
///
/// This type is `Send + Sync` and can be safely shared across threads.
/// The atomic sequence counter ensures thread-safe ID generation.
///
/// # Examples
///
/// ```ignore
/// use graphica::governance::bitemporal::TransactionManager;
///
/// let mgr = TransactionManager::new(1);
/// let tx1 = mgr.begin_transaction();
/// let tx2 = mgr.begin_transaction();
///
/// assert!(tx1.seq < tx2.seq);
/// assert_eq!(tx1.node_id, 1);
/// assert_eq!(tx2.node_id, 1);
/// ```ignore
#[derive(Debug)]
pub struct TransactionManager {
    /// Monotonic sequence counter (atomic for thread safety)
    next_seq: AtomicU64,

    /// Node identifier for distributed systems
    node_id: u16,
}

impl TransactionManager {
    /// Create a new transaction manager for a specific node.
    ///
    /// # Arguments
    /// * `node_id` - Unique identifier for this node in a distributed system (0-65535)
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use graphica::governance::bitemporal::TransactionManager;
    ///
    /// let mgr = TransactionManager::new(1);
    /// ```ignore
    pub fn new(node_id: u16) -> Self {
        Self {
            next_seq: AtomicU64::new(1),
            node_id,
        }
    }

    /// Create a transaction manager with a specific starting sequence.
    ///
    /// Useful for recovery scenarios or testing.
    ///
    /// # Arguments
    /// * `node_id` - Unique node identifier
    /// * `start_seq` - Starting sequence number (must be > 0)
    ///
    /// # Panics
    ///
    /// Panics if `start_seq` is 0.
    pub fn with_start_seq(node_id: u16, start_seq: u64) -> Self {
        assert!(start_seq > 0, "start_seq must be > 0");
        Self {
            next_seq: AtomicU64::new(start_seq),
            node_id,
        }
    }

    /// Begin a new transaction and return its ID.
    ///
    /// This method is thread-safe and lock-free. Each call returns a unique
    /// transaction ID with a monotonically increasing sequence number.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use graphica::governance::bitemporal::TransactionManager;
    ///
    /// let mgr = TransactionManager::new(1);
    /// let tx = mgr.begin_transaction();
    ///
    /// println!("Started transaction: {}", tx);
    /// ```ignore
    pub fn begin_transaction(&self) -> TransactionId {
        let seq = self.next_seq.fetch_add(1, Ordering::SeqCst);
        TransactionId {
            seq,
            timestamp: Utc::now(),
            node_id: self.node_id,
        }
    }

    /// Get the next sequence number without incrementing (peek).
    ///
    /// Useful for diagnostics and monitoring.
    pub fn peek_next_seq(&self) -> u64 {
        self.next_seq.load(Ordering::SeqCst)
    }

    /// Get the node ID for this transaction manager.
    pub fn node_id(&self) -> u16 {
        self.node_id
    }
}

// TransactionManager is Send + Sync because AtomicU64 is Send + Sync
unsafe impl Send for TransactionManager {}
unsafe impl Sync for TransactionManager {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::sync::Arc;
    use std::thread;

    #[test]
    fn test_new_transaction_manager() {
        let mgr = TransactionManager::new(42);
        assert_eq!(mgr.node_id(), 42);
        assert_eq!(mgr.peek_next_seq(), 1);
    }

    #[test]
    fn test_with_start_seq() {
        let mgr = TransactionManager::with_start_seq(1, 100);
        assert_eq!(mgr.peek_next_seq(), 100);

        let tx = mgr.begin_transaction();
        assert_eq!(tx.seq, 100);
        assert_eq!(mgr.peek_next_seq(), 101);
    }

    #[test]
    #[should_panic(expected = "start_seq must be > 0")]
    fn test_with_start_seq_zero_panics() {
        TransactionManager::with_start_seq(1, 0);
    }

    #[test]
    fn test_begin_transaction_monotonic() {
        let mgr = TransactionManager::new(1);

        let tx1 = mgr.begin_transaction();
        let tx2 = mgr.begin_transaction();
        let tx3 = mgr.begin_transaction();

        assert_eq!(tx1.seq, 1);
        assert_eq!(tx2.seq, 2);
        assert_eq!(tx3.seq, 3);
        assert_eq!(tx1.node_id, 1);
        assert_eq!(tx2.node_id, 1);
        assert_eq!(tx3.node_id, 1);
    }

    #[test]
    fn test_begin_transaction_timestamps_ordered() {
        let mgr = TransactionManager::new(1);

        let tx1 = mgr.begin_transaction();
        std::thread::sleep(std::time::Duration::from_millis(10));
        let tx2 = mgr.begin_transaction();

        assert!(tx1.seq < tx2.seq);
        assert!(tx1.timestamp <= tx2.timestamp);
    }

    #[test]
    fn test_thread_safety_multiple_threads() {
        let mgr = Arc::new(TransactionManager::new(1));
        let num_threads = 10;
        let txs_per_thread = 100;

        let mut handles = vec![];

        for _ in 0..num_threads {
            let mgr_clone = Arc::clone(&mgr);
            let handle = thread::spawn(move || {
                let mut txs = Vec::new();
                for _ in 0..txs_per_thread {
                    txs.push(mgr_clone.begin_transaction());
                }
                txs
            });
            handles.push(handle);
        }

        // Collect all transaction IDs from all threads
        let mut all_seqs = HashSet::new();
        for handle in handles {
            let txs = handle.join().unwrap();
            for tx in txs {
                // Each sequence should be unique
                assert!(all_seqs.insert(tx.seq), "Duplicate sequence: {}", tx.seq);
                assert_eq!(tx.node_id, 1);
            }
        }

        // Should have exactly num_threads * txs_per_thread unique sequences
        assert_eq!(all_seqs.len(), num_threads * txs_per_thread);

        // Final sequence should be num_threads * txs_per_thread + 1
        assert_eq!(
            mgr.peek_next_seq() as usize,
            num_threads * txs_per_thread + 1
        );
    }

    #[test]
    fn test_thread_safety_no_gaps() {
        let mgr = Arc::new(TransactionManager::new(1));
        let num_threads = 10;
        let txs_per_thread = 100;

        let mut handles = vec![];

        for _ in 0..num_threads {
            let mgr_clone = Arc::clone(&mgr);
            let handle = thread::spawn(move || {
                let mut txs = Vec::new();
                for _ in 0..txs_per_thread {
                    txs.push(mgr_clone.begin_transaction());
                }
                txs
            });
            handles.push(handle);
        }

        // Collect all sequences
        let mut all_seqs = Vec::new();
        for handle in handles {
            let txs = handle.join().unwrap();
            all_seqs.extend(txs.into_iter().map(|tx| tx.seq));
        }

        // Sort sequences
        all_seqs.sort_unstable();

        // Verify no gaps (should be 1..=total_count)
        let total_count = num_threads * txs_per_thread;
        for (i, &seq) in all_seqs.iter().enumerate() {
            assert_eq!(seq, (i + 1) as u64, "Gap detected at position {}", i);
        }
    }

    #[test]
    fn test_peek_next_seq() {
        let mgr = TransactionManager::new(1);
        assert_eq!(mgr.peek_next_seq(), 1);

        mgr.begin_transaction();
        assert_eq!(mgr.peek_next_seq(), 2);

        mgr.begin_transaction();
        assert_eq!(mgr.peek_next_seq(), 3);
    }

    #[test]
    fn test_different_nodes_different_ids() {
        let mgr1 = TransactionManager::new(1);
        let mgr2 = TransactionManager::new(2);

        let tx1 = mgr1.begin_transaction();
        let tx2 = mgr2.begin_transaction();

        assert_eq!(tx1.seq, 1);
        assert_eq!(tx2.seq, 1);
        assert_eq!(tx1.node_id, 1);
        assert_eq!(tx2.node_id, 2);

        // Global ordering keys should be different
        assert_ne!(tx1.global_ordering_key(), tx2.global_ordering_key());
    }

    #[test]
    fn test_high_throughput_single_thread() {
        let mgr = TransactionManager::new(1);
        let count = 10_000;

        let start = std::time::Instant::now();
        for i in 0..count {
            let tx = mgr.begin_transaction();
            assert_eq!(tx.seq, i + 1);
        }
        let elapsed = start.elapsed();

        println!(
            "Generated {} transactions in {:?} ({:.0} tx/sec)",
            count,
            elapsed,
            count as f64 / elapsed.as_secs_f64()
        );

        // Should be very fast (millions per second)
        assert!(elapsed.as_millis() < 1000, "Too slow: {:?}", elapsed);
    }
}
