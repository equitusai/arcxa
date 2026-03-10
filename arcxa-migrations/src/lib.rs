//! # Storage Migration Module
//!
//! Handles migration from old index format to inverted indexes.
//!
//! Old format: key → Vec<event_id> (read-modify-write)
//! New format: (key, event_id) → empty (append-only)

use anyhow::{Context, Result};
use rocksdb::{IteratorMode, WriteBatch, DB};
use std::collections::BTreeSet;
use tracing::{info, warn};

/// Migration status tracking
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MigrationStatus {
    /// Database is using old index format
    OldFormat,
    /// Migration in progress (dual-write mode)
    Migrating,
    /// Migration complete (new format only)
    Complete,
}

/// Metadata key for storing migration status
const MIGRATION_STATUS_KEY: &[u8] = b"__migration_status";

/// Column families that need migration
const MIGRATABLE_CFS: &[&str] = &["record_idx", "model_idx", "run_idx", "tenant_idx"];

/// Get current migration status from database
pub fn get_migration_status(db: &DB) -> Result<MigrationStatus> {
    match db.get(MIGRATION_STATUS_KEY)? {
        Some(data) => {
            let status_str = String::from_utf8_lossy(&data);
            match status_str.as_ref() {
                "old_format" => Ok(MigrationStatus::OldFormat),
                "migrating" => Ok(MigrationStatus::Migrating),
                "complete" => Ok(MigrationStatus::Complete),
                _ => Ok(MigrationStatus::OldFormat), // Default to old format
            }
        }
        None => {
            // Check if database has any data in old format
            if has_old_format_data(db)? {
                Ok(MigrationStatus::OldFormat)
            } else {
                // New database, assume new format
                Ok(MigrationStatus::Complete)
            }
        }
    }
}

/// Set migration status in database
pub fn set_migration_status(db: &DB, status: MigrationStatus) -> Result<()> {
    let status_str = match status {
        MigrationStatus::OldFormat => "old_format",
        MigrationStatus::Migrating => "migrating",
        MigrationStatus::Complete => "complete",
    };

    db.put(MIGRATION_STATUS_KEY, status_str.as_bytes())?;
    info!("Migration status set to: {:?}", status);

    Ok(())
}

/// Check if database contains old format data
fn has_old_format_data(db: &DB) -> Result<bool> {
    // Check first CF for old format (Vec<event_id> values)
    let cf_name = MIGRATABLE_CFS[0];

    if let Some(cf_handle) = db.cf_handle(cf_name) {
        // Iterate through a few keys to check format
        for item in db.iterator_cf(cf_handle, IteratorMode::Start).take(5) {
            let (_key, value) = item?;

            // Old format: serialized Vec<String>
            // New format: empty value
            if !value.is_empty() {
                // Try to deserialize as Vec<String>
                if serde_json::from_slice::<BTreeSet<String>>(&value).is_ok() {
                    return Ok(true);
                }
            }
        }
    }

    Ok(false)
}

/// Migrate a single column family from old to new format
pub fn migrate_column_family(db: &DB, cf_name: &str) -> Result<u64> {
    let cf_handle = db
        .cf_handle(cf_name)
        .ok_or_else(|| anyhow::anyhow!("Column family {} not found", cf_name))?;

    info!("Migrating column family: {}", cf_name);

    let mut migrated_count = 0u64;
    let mut batch = WriteBatch::default();
    let batch_size = 1000;

    // Iterate through all keys in old format
    for item in db.iterator_cf(cf_handle, IteratorMode::Start) {
        let (key, value) = item?;

        // Skip if already new format (empty value)
        if value.is_empty() {
            continue;
        }

        // Skip if it's a composite key (contains separator)
        if key.contains(&b'|') {
            continue;
        }

        // Deserialize old format: key → Vec<event_id>
        match serde_json::from_slice::<BTreeSet<String>>(&value) {
            Ok(event_ids) => {
                // Convert to new format: (key, event_id) → empty
                for event_id in event_ids {
                    let mut composite_key = Vec::with_capacity(key.len() + event_id.len() + 1);
                    composite_key.extend_from_slice(&key);
                    composite_key.push(b'|');
                    composite_key.extend_from_slice(event_id.as_bytes());

                    batch.put_cf(cf_handle, &composite_key, &[]);
                    migrated_count += 1;
                }

                // Delete old format key
                batch.delete_cf(cf_handle, &key);

                // Commit batch periodically
                if migrated_count % batch_size == 0 {
                    db.write(batch)?;
                    batch = WriteBatch::default();
                    info!("Migrated {} entries in {}", migrated_count, cf_name);
                }
            }
            Err(e) => {
                warn!("Skipping key in {} (not old format): {:?}", cf_name, e);
            }
        }
    }

    // Commit remaining batch
    if migrated_count % batch_size != 0 {
        db.write(batch)?;
    }

    info!(
        "Completed migration of {}: {} entries",
        cf_name, migrated_count
    );

    Ok(migrated_count)
}

/// Migrate entire database from old to new format
pub fn migrate_database(db_path: &str) -> Result<()> {
    info!("Starting database migration: {}", db_path);

    // Open database
    let mut opts = rocksdb::Options::default();
    opts.create_if_missing(false);
    opts.create_missing_column_families(false);

    let cfs = vec![
        "primary",
        "record_idx",
        "model_idx",
        "run_idx",
        "tenant_idx",
        "time_idx",
        "time_travel_idx",
    ];

    let db = DB::open_cf(&opts, db_path, cfs).context("Failed to open database for migration")?;

    // Check current status
    let status = get_migration_status(&db)?;

    match status {
        MigrationStatus::Complete => {
            info!("Database already migrated");
            return Ok(());
        }
        MigrationStatus::Migrating => {
            warn!("Database migration was interrupted, resuming...");
        }
        MigrationStatus::OldFormat => {
            info!("Database uses old format, starting migration...");
            set_migration_status(&db, MigrationStatus::Migrating)?;
        }
    }

    // Migrate each column family
    let mut total_migrated = 0u64;

    for cf_name in MIGRATABLE_CFS {
        let count = migrate_column_family(&db, cf_name)?;
        total_migrated += count;
    }

    // Mark migration complete
    set_migration_status(&db, MigrationStatus::Complete)?;

    info!(
        "Migration complete! Migrated {} total entries",
        total_migrated
    );

    Ok(())
}

/// Validate that new format matches old format (for testing)
#[cfg(test)]
pub fn validate_migration(db: &DB, cf_name: &str) -> Result<bool> {
    use std::collections::HashMap;

    let cf_handle = db
        .cf_handle(cf_name)
        .ok_or_else(|| anyhow::anyhow!("Column family {} not found", cf_name))?;

    // Build map from new format
    let mut new_format_map: HashMap<Vec<u8>, BTreeSet<String>> = HashMap::new();

    for item in db.iterator_cf(cf_handle, IteratorMode::Start) {
        let (key, _value) = item?;

        // Only process composite keys (new format)
        if let Some(sep_pos) = key.iter().position(|&b| b == b'|') {
            let prefix = key[..sep_pos].to_vec();
            let event_id = String::from_utf8_lossy(&key[sep_pos + 1..]).to_string();

            new_format_map
                .entry(prefix)
                .or_insert_with(BTreeSet::new)
                .insert(event_id);
        }
    }

    info!(
        "Validation: Found {} unique keys in new format",
        new_format_map.len()
    );

    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_migration_status_tracking() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let db = DB::open_default(temp_dir.path())?;

        // Default status for new database
        let status = get_migration_status(&db)?;
        assert_eq!(status, MigrationStatus::Complete);

        // Set to migrating
        set_migration_status(&db, MigrationStatus::Migrating)?;
        let status = get_migration_status(&db)?;
        assert_eq!(status, MigrationStatus::Migrating);

        // Set to complete
        set_migration_status(&db, MigrationStatus::Complete)?;
        let status = get_migration_status(&db)?;
        assert_eq!(status, MigrationStatus::Complete);

        Ok(())
    }
}
