//! Integration test for checkpoint recovery
//!
//! Validates the complete checkpoint/recovery flow:
//! 1. Process records and capture state
//! 2. Write checkpoint
//! 3. Simulate restart by creating new instances
//! 4. Restore from checkpoint
//! 5. Verify: No duplicates, no data loss, correct state

use graphica_core::checkpointing::{
    Checkpoint, CheckpointStorage, Checkpointable, CheckpointableDedupState,
};
use std::collections::HashMap;
use std::path::PathBuf;
use tempfile::tempdir;

#[test]
fn test_checkpoint_dedup_state_recovery() {
    // Scenario: Process 1000 records, checkpoint, restart, process same 1000 records
    // Expected: All 1000 should be detected as duplicates after recovery

    let temp_dir = tempdir().unwrap();
    let checkpoint_storage = CheckpointStorage::new(temp_dir.path().to_path_buf()).unwrap();

    // Phase 1: Initial processing
    let dedup_state = CheckpointableDedupState::new(60_000, 100_000);

    // Process 1000 unique records
    for i in 0..1000 {
        let record_id = format!("record_{}", i);
        assert!(
            !dedup_state.is_duplicate(&record_id),
            "Record {} should be unique",
            i
        );
        dedup_state.mark_seen(record_id);
    }

    assert_eq!(dedup_state.len(), 1000, "Should have 1000 entries");

    // Phase 2: Checkpoint
    let snapshot = dedup_state.snapshot().unwrap();
    assert_eq!(snapshot.len(), 1000, "Snapshot should have 1000 entries");

    let checkpoint = Checkpoint::new(4).with_dedup_state(snapshot);

    let checkpoint_path = checkpoint_storage.write(&checkpoint).unwrap();
    assert!(checkpoint_path.exists(), "Checkpoint file should exist");

    // Phase 3: Simulate restart - create new dedup state
    let mut recovered_dedup = CheckpointableDedupState::new(60_000, 100_000);
    assert_eq!(recovered_dedup.len(), 0, "New instance should be empty");

    // Phase 4: Restore from checkpoint
    let loaded_checkpoint = checkpoint_storage.latest().unwrap().unwrap();
    recovered_dedup
        .restore(loaded_checkpoint.dedup_state)
        .unwrap();

    assert_eq!(
        recovered_dedup.len(),
        1000,
        "Restored state should have 1000 entries"
    );

    // Phase 5: Verification - all records should be detected as duplicates
    let mut duplicate_count = 0;
    for i in 0..1000 {
        let record_id = format!("record_{}", i);
        if recovered_dedup.is_duplicate(&record_id) {
            duplicate_count += 1;
        }
    }

    assert_eq!(
        duplicate_count, 1000,
        "All 1000 records should be detected as duplicates after recovery"
    );

    // Phase 6: New records should be detected as unique
    for i in 1000..1100 {
        let record_id = format!("record_{}", i);
        assert!(
            !recovered_dedup.is_duplicate(&record_id),
            "New record {} should be unique",
            i
        );
        recovered_dedup.mark_seen(record_id);
    }

    assert_eq!(
        recovered_dedup.len(),
        1100,
        "Should have 1100 entries after processing new records"
    );
}

#[test]
fn test_checkpoint_kafka_offset_recovery() {
    // Scenario: Process to offset 500, checkpoint, restart, verify offset restored

    let temp_dir = tempdir().unwrap();
    let checkpoint_storage = CheckpointStorage::new(temp_dir.path().to_path_buf()).unwrap();

    // Phase 1: Process to offset 500 across 4 partitions
    let mut kafka_offsets = HashMap::new();
    kafka_offsets.insert(0, 500);
    kafka_offsets.insert(1, 450);
    kafka_offsets.insert(2, 480);
    kafka_offsets.insert(3, 520);

    // Phase 2: Checkpoint
    let checkpoint = Checkpoint::new(4).with_offsets(kafka_offsets.clone());

    checkpoint_storage.write(&checkpoint).unwrap();

    // Phase 3: Restore and verify
    let loaded = checkpoint_storage.latest().unwrap().unwrap();

    assert_eq!(loaded.kafka_offsets.len(), 4, "Should have 4 partitions");
    assert_eq!(
        loaded.kafka_offsets.get(&0),
        Some(&500),
        "Partition 0 offset should be 500"
    );
    assert_eq!(
        loaded.kafka_offsets.get(&1),
        Some(&450),
        "Partition 1 offset should be 450"
    );
    assert_eq!(
        loaded.kafka_offsets.get(&2),
        Some(&480),
        "Partition 2 offset should be 480"
    );
    assert_eq!(
        loaded.kafka_offsets.get(&3),
        Some(&520),
        "Partition 3 offset should be 520"
    );
}

#[test]
fn test_checkpoint_full_recovery_cycle() {
    // Comprehensive test: Kafka offsets + dedup state + multiple checkpoints

    let temp_dir = tempdir().unwrap();
    let checkpoint_storage = CheckpointStorage::new(temp_dir.path().to_path_buf()).unwrap();

    // Phase 1: Initial state - offset 100, 50 dedup entries
    let dedup1 = CheckpointableDedupState::new(60_000, 100_000);
    for i in 0..50 {
        dedup1.mark_seen(format!("record_{}", i));
    }

    let mut offsets1 = HashMap::new();
    offsets1.insert(0, 100);

    let checkpoint1 = Checkpoint::new(4)
        .with_offsets(offsets1)
        .with_dedup_state(dedup1.snapshot().unwrap());

    checkpoint_storage.write(&checkpoint1).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(10)); // Ensure different timestamp

    // Phase 2: Progress to offset 200, 100 dedup entries
    let dedup2 = CheckpointableDedupState::new(60_000, 100_000);
    for i in 0..100 {
        dedup2.mark_seen(format!("record_{}", i));
    }

    let mut offsets2 = HashMap::new();
    offsets2.insert(0, 200);

    let checkpoint2 = Checkpoint::new(4)
        .with_offsets(offsets2)
        .with_dedup_state(dedup2.snapshot().unwrap());

    checkpoint_storage.write(&checkpoint2).unwrap();

    // Phase 3: Verify latest checkpoint is loaded (should be checkpoint2)
    let loaded = checkpoint_storage.latest().unwrap().unwrap();

    assert_eq!(
        loaded.kafka_offsets.get(&0),
        Some(&200),
        "Should load latest offset"
    );
    assert_eq!(
        loaded.dedup_state.len(),
        100,
        "Should load latest dedup state"
    );

    // Phase 4: Restore and verify all 100 records are duplicates
    let mut recovered = CheckpointableDedupState::new(60_000, 100_000);
    recovered.restore(loaded.dedup_state).unwrap();

    let mut dup_count = 0;
    for i in 0..100 {
        if recovered.is_duplicate(&format!("record_{}", i)) {
            dup_count += 1;
        }
    }

    assert_eq!(
        dup_count, 100,
        "All 100 records from checkpoint should be duplicates"
    );
}

#[test]
fn test_checkpoint_cleanup_preserves_latest() {
    // Verify that checkpoint cleanup keeps the latest checkpoint

    let temp_dir = tempdir().unwrap();
    let checkpoint_storage = CheckpointStorage::new(temp_dir.path().to_path_buf()).unwrap();

    // Write 15 checkpoints (cleanup keeps last 10)
    for i in 0..15 {
        let dedup = CheckpointableDedupState::new(60_000, 100_000);
        dedup.mark_seen(format!("checkpoint_{}", i));

        let checkpoint = Checkpoint::new(4).with_dedup_state(dedup.snapshot().unwrap());

        checkpoint_storage.write(&checkpoint).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(10));
    }

    // Verify only 10 checkpoints exist
    let checkpoints = checkpoint_storage.list().unwrap();
    assert_eq!(checkpoints.len(), 10, "Should keep only 10 checkpoints");

    // Verify latest is checkpoint 14 (newest)
    let latest = checkpoint_storage.latest().unwrap().unwrap();
    assert!(
        latest.dedup_state.contains_key("checkpoint_14"),
        "Latest checkpoint should be checkpoint_14"
    );
}

#[test]
fn test_checkpoint_with_stale_entries() {
    // Verify that stale entries are cleaned up on restore

    let temp_dir = tempdir().unwrap();
    let checkpoint_storage = CheckpointStorage::new(temp_dir.path().to_path_buf()).unwrap();

    // Create dedup state with 100ms window
    let dedup = CheckpointableDedupState::new(100, 100_000);

    // Add entries
    for i in 0..50 {
        dedup.mark_seen(format!("record_{}", i));
    }

    // Checkpoint
    let checkpoint = Checkpoint::new(4).with_dedup_state(dedup.snapshot().unwrap());

    checkpoint_storage.write(&checkpoint).unwrap();

    // Wait for entries to become stale (100ms window)
    std::thread::sleep(std::time::Duration::from_millis(150));

    // Restore
    let loaded = checkpoint_storage.latest().unwrap().unwrap();
    let mut recovered = CheckpointableDedupState::new(100, 100_000);
    recovered.restore(loaded.dedup_state).unwrap();

    // Cleanup is called during restore, so stale entries should be removed
    // All entries should now be outside window, so none should be duplicates
    let mut dup_count = 0;
    for i in 0..50 {
        if recovered.is_duplicate(&format!("record_{}", i)) {
            dup_count += 1;
        }
    }

    // Since entries are stale, they should be cleaned up
    assert_eq!(
        dup_count, 0,
        "Stale entries should be cleaned up on restore"
    );
    assert_eq!(recovered.len(), 0, "All stale entries should be removed");
}

#[test]
fn test_checkpoint_corrupted_recovery_graceful() {
    // Verify system continues if checkpoint is corrupted

    let temp_dir = tempdir().unwrap();
    let checkpoint_storage = CheckpointStorage::new(temp_dir.path().to_path_buf()).unwrap();

    // Write valid checkpoint
    let dedup = CheckpointableDedupState::new(60_000, 100_000);
    dedup.mark_seen("valid_record".to_string());

    let checkpoint = Checkpoint::new(4).with_dedup_state(dedup.snapshot().unwrap());

    let checkpoint_path = checkpoint_storage.write(&checkpoint).unwrap();

    // Corrupt checkpoint file
    std::fs::write(&checkpoint_path, b"CORRUPTED DATA").unwrap();

    // Try to load - should return error but not panic
    let result = checkpoint_storage.latest();

    // Should either return error or None, but not panic
    match result {
        Ok(None) => {
            // No checkpoint found (acceptable)
        }
        Ok(Some(_)) => {
            panic!("Should not successfully load corrupted checkpoint");
        }
        Err(_) => {
            // Error loading (acceptable)
        }
    }

    // System should be able to continue with empty state
    let fresh_dedup = CheckpointableDedupState::new(60_000, 100_000);
    assert_eq!(fresh_dedup.len(), 0, "Fresh state should be empty");
}

#[test]
fn test_checkpoint_worker_count_validation() {
    // Verify checkpoint records correct worker count

    let temp_dir = tempdir().unwrap();
    let checkpoint_storage = CheckpointStorage::new(temp_dir.path().to_path_buf()).unwrap();

    // Create checkpoint with 4 workers
    let checkpoint = Checkpoint::new(4);
    checkpoint_storage.write(&checkpoint).unwrap();

    let loaded = checkpoint_storage.latest().unwrap().unwrap();
    assert_eq!(loaded.worker_count, 4, "Worker count should be preserved");
}

#[test]
fn test_checkpoint_no_data_loss_on_recovery() {
    // Critical test: Verify no data loss during checkpoint/recovery cycle

    let temp_dir = tempdir().unwrap();
    let checkpoint_storage = CheckpointStorage::new(temp_dir.path().to_path_buf()).unwrap();

    // Phase 1: Process first batch (0-499)
    let dedup1 = CheckpointableDedupState::new(60_000, 100_000);
    for i in 0..500 {
        dedup1.mark_seen(format!("record_{}", i));
    }

    let checkpoint1 = Checkpoint::new(4).with_dedup_state(dedup1.snapshot().unwrap());
    checkpoint_storage.write(&checkpoint1).unwrap();

    // Phase 2: Continue processing second batch (500-999)
    for i in 500..1000 {
        dedup1.mark_seen(format!("record_{}", i));
    }

    let checkpoint2 = Checkpoint::new(4).with_dedup_state(dedup1.snapshot().unwrap());
    checkpoint_storage.write(&checkpoint2).unwrap();

    // Phase 3: Simulate crash and recovery
    let loaded = checkpoint_storage.latest().unwrap().unwrap();
    let mut recovered = CheckpointableDedupState::new(60_000, 100_000);
    recovered.restore(loaded.dedup_state).unwrap();

    // Phase 4: Verify all 1000 records are in recovered state
    let mut found_count = 0;
    for i in 0..1000 {
        if recovered.is_duplicate(&format!("record_{}", i)) {
            found_count += 1;
        }
    }

    assert_eq!(
        found_count, 1000,
        "No data loss: all 1000 records should be recovered"
    );
}

#[tokio::test]
async fn test_checkpoint_manager_periodic_write() {
    // Test checkpoint manager writes periodically

    use graphica_core::checkpointing::CheckpointManager;
    use std::sync::Arc;

    let temp_dir = tempdir().unwrap();
    let checkpoint_storage = CheckpointStorage::new(temp_dir.path().to_path_buf()).unwrap();

    let dedup = CheckpointableDedupState::new(60_000, 100_000);
    let dedup_for_manager = dedup.clone();

    // Add some entries
    for i in 0..10 {
        dedup.mark_seen(format!("record_{}", i));
    }

    let manager = CheckpointManager::new(checkpoint_storage.clone(), 1); // 1 second interval

    let handle = manager.start(
        move || {
            let mut offsets = HashMap::new();
            offsets.insert(0, 100);
            offsets
        },
        move || dedup_for_manager.snapshot().unwrap_or_default(),
        4,
    );

    // Wait for at least 2 checkpoint cycles
    tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;

    // Abort manager
    handle.abort();

    // Verify checkpoint was written
    let loaded = checkpoint_storage.latest().unwrap();
    assert!(loaded.is_some(), "Checkpoint should have been written");

    let checkpoint = loaded.unwrap();
    assert_eq!(
        checkpoint.dedup_state.len(),
        10,
        "Checkpoint should contain 10 dedup entries"
    );
}
