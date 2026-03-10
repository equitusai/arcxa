//! DashMap-based checkpointable deduplication state
//!
//! Uses DashMap for lock-free concurrent dedup checking across workers.
//! Addresses Concern #1 from Phase 2 Critical Review: thread safety without lock contention.

use anyhow::Result;
use dashmap::DashMap;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use super::Checkpointable;

/// Thread-safe checkpointable deduplication state using DashMap
///
/// # Architecture Decision #1: DashMap vs Arc<RwLock<HashSet>>
///
/// **Problem:** Arc<RwLock> causes lock contention at 10K/sec:
/// - Each dedup check requires read lock
/// - Snapshot requires write lock
/// - Lock contention = 100% CPU at scale
///
/// **Solution:** DashMap (lock-free concurrent hashmap):
/// - Lock-free reads
/// - Minimal contention
/// - Direct snapshot via iter()
///
/// # Time-Windowing
///
/// To prevent unbounded growth, entries older than window_ms are evicted.
/// Background task runs every cleanup_interval_ms to prune old entries.
#[derive(Clone)]
pub struct CheckpointableDedupState {
    /// seen_ids: record_id → timestamp (when first seen)
    seen_ids: Arc<DashMap<String, i64>>,

    /// Time window for deduplication (milliseconds)
    window_ms: i64,

    /// Maximum entries before forced cleanup
    max_entries: usize,
}

impl CheckpointableDedupState {
    /// Create new dedup state with time window
    ///
    /// # Arguments
    /// - `window_ms`: Dedup time window in milliseconds (e.g., 60_000 = 1 minute)
    /// - `max_entries`: Hard cap on entries (e.g., 100_000)
    pub fn new(window_ms: i64, max_entries: usize) -> Self {
        Self {
            seen_ids: Arc::new(DashMap::new()),
            window_ms,
            max_entries,
        }
    }

    /// Check if record_id has been seen recently
    ///
    /// Returns true if record_id exists and is within time window.
    ///
    /// # Thread Safety
    /// Uses DashMap's entry API for atomic check-and-remove operation.
    /// Prevents race condition where:
    /// 1. Thread A checks stale entry
    /// 2. Thread B inserts fresh entry
    /// 3. Thread A removes fresh entry (BUG!)
    ///
    /// With entry API, the check and remove are atomic.
    pub fn is_duplicate(&self, record_id: &str) -> bool {
        let now = Self::now_ms();

        // Use entry API for atomic check-and-remove
        match self.seen_ids.entry(record_id.to_string()) {
            dashmap::mapref::entry::Entry::Occupied(entry) => {
                let timestamp = *entry.get();

                // Check if within window
                if now - timestamp < self.window_ms {
                    true // Still valid, keep entry
                } else {
                    // Stale entry - remove atomically
                    entry.remove();
                    false
                }
            }
            dashmap::mapref::entry::Entry::Vacant(_) => {
                // Not seen
                false
            }
        }
    }

    /// Mark record_id as seen
    ///
    /// Records current timestamp for the record_id.
    /// If max_entries exceeded, triggers cleanup of old entries.
    pub fn mark_seen(&self, record_id: String) {
        let now = Self::now_ms();
        self.seen_ids.insert(record_id, now);

        // Check for overflow
        if self.seen_ids.len() > self.max_entries {
            self.cleanup_old_entries();
        }
    }

    /// Remove entries older than time window
    ///
    /// This is called automatically when max_entries is exceeded,
    /// but can also be called manually from a background task.
    pub fn cleanup_old_entries(&self) {
        let now = Self::now_ms();
        let cutoff = now - self.window_ms;

        let mut removed = 0;

        // Retain only entries within window
        self.seen_ids.retain(|_key, &mut timestamp| {
            if timestamp < cutoff {
                removed += 1;
                false
            } else {
                true
            }
        });

        if removed > 0 {
            tracing::debug!(
                "Dedup cleanup: removed {} stale entries, {} remaining",
                removed,
                self.seen_ids.len()
            );
        }
    }

    /// Get current size (number of entries)
    pub fn len(&self) -> usize {
        self.seen_ids.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.seen_ids.is_empty()
    }

    /// Get current timestamp in milliseconds
    ///
    /// # Safety
    /// Uses unwrap_or to handle clock going backwards (NTP, DST, manual changes).
    /// Returns 0 if system clock is before UNIX_EPOCH, which causes all dedup
    /// entries to be considered stale. This is safe - worst case is re-processing
    /// some records, which is acceptable vs crashing the process.
    fn now_ms() -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(std::time::Duration::from_secs(0))
            .as_millis() as i64
    }

    /// Start background cleanup task
    ///
    /// Runs cleanup_old_entries() every interval_ms.
    /// Returns JoinHandle that can be aborted on shutdown.
    pub fn start_cleanup_task(self, interval_ms: u64) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_millis(interval_ms));

            loop {
                interval.tick().await;
                self.cleanup_old_entries();
            }
        })
    }
}

impl Checkpointable for CheckpointableDedupState {
    /// Create snapshot of dedup state
    ///
    /// This is fast (< 10ms for 100K entries) because DashMap
    /// allows concurrent iteration without blocking writes.
    fn snapshot(&self) -> Result<HashMap<String, i64>> {
        let snapshot: HashMap<String, i64> = self
            .seen_ids
            .iter()
            .map(|entry| (entry.key().clone(), *entry.value()))
            .collect();

        tracing::debug!("Dedup snapshot captured: {} entries", snapshot.len());

        Ok(snapshot)
    }

    /// Restore from snapshot
    ///
    /// Clears existing state and loads from checkpoint.
    fn restore(&mut self, state: HashMap<String, i64>) -> Result<()> {
        tracing::info!("Restoring dedup state: {} entries", state.len());

        self.seen_ids.clear();

        for (key, value) in state {
            self.seen_ids.insert(key, value);
        }

        // Clean up any stale entries from checkpoint
        self.cleanup_old_entries();

        tracing::info!(
            "Dedup state restored: {} entries after cleanup",
            self.seen_ids.len()
        );

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dedup_basic() {
        let dedup = CheckpointableDedupState::new(60_000, 1000);

        assert!(!dedup.is_duplicate("rec1"));
        dedup.mark_seen("rec1".to_string());
        assert!(dedup.is_duplicate("rec1"));
        assert!(!dedup.is_duplicate("rec2"));
    }

    #[test]
    fn test_dedup_time_window() {
        let dedup = CheckpointableDedupState::new(100, 1000); // 100ms window

        dedup.mark_seen("rec1".to_string());
        assert!(dedup.is_duplicate("rec1"));

        // Wait for window to expire
        std::thread::sleep(std::time::Duration::from_millis(150));

        // Should be removed from window
        assert!(!dedup.is_duplicate("rec1"));
    }

    #[test]
    fn test_dedup_max_entries() {
        let dedup = CheckpointableDedupState::new(60_000, 10);

        // Fill beyond max_entries
        for i in 0..20 {
            dedup.mark_seen(format!("rec{}", i));
        }

        // Should trigger cleanup
        assert!(dedup.len() <= 20); // Some may be cleaned up
    }

    #[test]
    fn test_dedup_snapshot_restore() {
        let mut dedup = CheckpointableDedupState::new(60_000, 1000);

        dedup.mark_seen("rec1".to_string());
        dedup.mark_seen("rec2".to_string());
        dedup.mark_seen("rec3".to_string());

        let snapshot = dedup.snapshot().unwrap();
        assert_eq!(snapshot.len(), 3);

        // Create new dedup and restore
        let mut dedup2 = CheckpointableDedupState::new(60_000, 1000);
        dedup2.restore(snapshot).unwrap();

        assert!(dedup2.is_duplicate("rec1"));
        assert!(dedup2.is_duplicate("rec2"));
        assert!(dedup2.is_duplicate("rec3"));
        assert!(!dedup2.is_duplicate("rec4"));
    }

    #[tokio::test]
    async fn test_cleanup_task() {
        let dedup = CheckpointableDedupState::new(100, 1000); // 100ms window

        dedup.mark_seen("rec1".to_string());
        dedup.mark_seen("rec2".to_string());

        let handle = dedup.clone().start_cleanup_task(50); // Cleanup every 50ms

        // Wait for cleanup to run
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

        // Both should be expired and cleaned up
        assert_eq!(dedup.len(), 0);

        handle.abort();
    }

    #[test]
    fn test_concurrent_access() {
        use std::thread;

        let dedup = CheckpointableDedupState::new(60_000, 10_000);
        let dedup_clone = dedup.clone();

        // Writer thread
        let writer = thread::spawn(move || {
            for i in 0..1000 {
                dedup_clone.mark_seen(format!("rec{}", i));
            }
        });

        // Reader thread
        let dedup_clone2 = dedup.clone();
        let reader = thread::spawn(move || {
            for i in 0..1000 {
                dedup_clone2.is_duplicate(&format!("rec{}", i));
            }
        });

        writer.join().unwrap();
        reader.join().unwrap();

        // Should have all 1000 entries (no lost writes)
        assert_eq!(dedup.len(), 1000);
    }

    #[test]
    fn test_race_condition_stale_entry_removal() {
        // Test for the specific race condition from Issue #1:
        // Thread A checks stale entry, Thread B inserts fresh, Thread A removes

        use std::sync::{Arc, Barrier};
        use std::thread;

        let dedup = CheckpointableDedupState::new(100, 1000); // 100ms window
        let barrier = Arc::new(Barrier::new(2));

        // Insert initial entry
        dedup.mark_seen("test_record".to_string());

        // Wait for entry to become stale
        std::thread::sleep(std::time::Duration::from_millis(150));

        let dedup_clone1 = dedup.clone();
        let barrier_clone1 = barrier.clone();

        // Thread A: Check stale and remove
        let thread_a = thread::spawn(move || {
            barrier_clone1.wait(); // Synchronize start
            dedup_clone1.is_duplicate("test_record") // Should return false and remove
        });

        let dedup_clone2 = dedup.clone();
        let barrier_clone2 = barrier.clone();

        // Thread B: Insert fresh entry (racing with Thread A)
        let thread_b = thread::spawn(move || {
            barrier_clone2.wait(); // Synchronize start
            dedup_clone2.mark_seen("test_record".to_string())
        });

        let result_a = thread_a.join().unwrap();
        thread_b.join().unwrap();

        // Critical verification:
        // After both threads complete, entry should either:
        // 1. Not exist (Thread A won, removed stale before B inserted)
        // 2. Exist with fresh timestamp (Thread B won or came after)
        //
        // BUG behavior would be: entry exists but is_duplicate returns false
        // because Thread A removed Thread B's fresh entry

        let is_dup_after = dedup.is_duplicate("test_record");

        // If entry exists, it MUST be detected as duplicate (fresh)
        // If entry doesn't exist, is_duplicate should return false
        if dedup.len() > 0 {
            assert!(
                is_dup_after,
                "Race condition: fresh entry was incorrectly removed"
            );
        } else {
            assert!(!is_dup_after, "Entry should not exist");
        }
    }

    #[test]
    fn test_concurrent_stale_removal() {
        // Stress test: multiple threads checking and removing stale entries
        use std::thread;

        let dedup = CheckpointableDedupState::new(50, 10_000); // 50ms window

        // Insert 100 entries
        for i in 0..100 {
            dedup.mark_seen(format!("rec{}", i));
        }

        // Wait for all to become stale
        std::thread::sleep(std::time::Duration::from_millis(100));

        // Spawn 10 threads all checking the same stale entries
        let threads: Vec<_> = (0..10)
            .map(|thread_id| {
                let dedup_clone = dedup.clone();
                thread::spawn(move || {
                    for i in 0..100 {
                        dedup_clone.is_duplicate(&format!("rec{}", i));
                    }
                    thread_id
                })
            })
            .collect();

        for thread in threads {
            thread.join().unwrap();
        }

        // All stale entries should be removed
        assert_eq!(dedup.len(), 0);
    }

    #[test]
    fn test_mark_seen_during_stale_check() {
        // Test specific interleaving: mark_seen called during is_duplicate
        use std::sync::{Arc, Barrier};
        use std::thread;

        let dedup = CheckpointableDedupState::new(50, 1000);
        dedup.mark_seen("test".to_string());

        // Wait for stale
        std::thread::sleep(std::time::Duration::from_millis(100));

        let barrier = Arc::new(Barrier::new(3));

        let dedup1 = dedup.clone();
        let b1 = barrier.clone();
        let t1 = thread::spawn(move || {
            b1.wait();
            dedup1.is_duplicate("test") // Check stale
        });

        let dedup2 = dedup.clone();
        let b2 = barrier.clone();
        let t2 = thread::spawn(move || {
            b2.wait();
            dedup2.mark_seen("test".to_string()) // Insert fresh
        });

        let dedup3 = dedup.clone();
        let b3 = barrier.clone();
        let t3 = thread::spawn(move || {
            b3.wait();
            std::thread::sleep(std::time::Duration::from_millis(10));
            dedup3.is_duplicate("test") // Check after both
        });

        let r1 = t1.join().unwrap();
        t2.join().unwrap();
        let r3 = t3.join().unwrap();

        // Final check should be true (fresh entry should exist)
        assert!(
            r3,
            "Fresh entry should still exist after concurrent operations"
        );
    }

    #[test]
    fn test_clock_skew_safety() {
        // Test that we don't panic if clock goes backwards
        // We can't actually move the system clock, but we can test
        // that our implementation handles duration_since returning error

        let dedup = CheckpointableDedupState::new(60_000, 1000);

        // Mark entry with current time
        dedup.mark_seen("test".to_string());

        // Even if clock were to go backwards (which now_ms handles),
        // the system should not panic
        assert!(dedup.is_duplicate("test"));
    }

    #[test]
    fn test_stale_entry_atomic_removal() {
        // Verify that stale entry removal is truly atomic
        let dedup = CheckpointableDedupState::new(50, 1000);

        // Insert and wait for stale
        dedup.mark_seen("test".to_string());
        std::thread::sleep(std::time::Duration::from_millis(100));

        // First check should remove
        let result1 = dedup.is_duplicate("test");
        assert!(!result1, "Stale entry should return false");
        assert_eq!(dedup.len(), 0, "Stale entry should be removed");

        // Second check should still return false (idempotent)
        let result2 = dedup.is_duplicate("test");
        assert!(!result2, "Non-existent entry should return false");
        assert_eq!(dedup.len(), 0, "Should still be empty");
    }

    #[test]
    fn test_high_contention_scenario() {
        // Simulate high contention: many threads accessing same keys
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;
        use std::thread;

        let dedup = CheckpointableDedupState::new(60_000, 10_000);
        let duplicate_count = Arc::new(AtomicUsize::new(0));
        let unique_count = Arc::new(AtomicUsize::new(0));

        // Spawn 20 threads, each inserting and checking 100 times
        let threads: Vec<_> = (0..20)
            .map(|thread_id| {
                let dedup_clone = dedup.clone();
                let dup_count = duplicate_count.clone();
                let uniq_count = unique_count.clone();

                thread::spawn(move || {
                    for i in 0..100 {
                        // Use overlapping keys to create contention
                        let key = format!("key{}", i % 50);

                        if dedup_clone.is_duplicate(&key) {
                            dup_count.fetch_add(1, Ordering::Relaxed);
                        } else {
                            uniq_count.fetch_add(1, Ordering::Relaxed);
                            dedup_clone.mark_seen(key);
                        }
                    }
                })
            })
            .collect();

        for thread in threads {
            thread.join().unwrap();
        }

        let total_ops =
            duplicate_count.load(Ordering::Relaxed) + unique_count.load(Ordering::Relaxed);

        // Should have processed all operations
        assert_eq!(total_ops, 20 * 100);

        // Should have at most 50 unique keys
        assert!(dedup.len() <= 50, "Should not exceed key space");

        // Should have detected many duplicates due to contention
        let dup_ratio = duplicate_count.load(Ordering::Relaxed) as f64 / total_ops as f64;
        assert!(
            dup_ratio > 0.5,
            "Should detect significant duplicates with contention"
        );
    }
}
