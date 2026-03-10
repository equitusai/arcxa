//! # Dead Letter Queue (DLQ)
//!
//! Parquet-based dead letter queue for permanently failed events

use crate::core::lineage::LineageEvent;
use anyhow::Result;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Dead letter queue record with failure metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DlqRecord {
    pub event: LineageEvent,
    pub error: String,
    pub failed_at: chrono::DateTime<chrono::Utc>,
    pub retry_count: u32,
    pub original_timestamp: i64,
}

/// Dead letter queue writer
pub struct DeadLetterQueue {
    path: PathBuf,
    writer: Mutex<Option<std::fs::File>>,
}

impl DeadLetterQueue {
    /// Create a new DLQ at the specified path
    pub fn new<P: AsRef<Path>>(base_path: P) -> Result<Self> {
        let path = base_path.as_ref().join("dlq");
        std::fs::create_dir_all(&path)?;

        Ok(Self {
            path,
            writer: Mutex::new(None),
        })
    }

    /// Write a failed event to the DLQ
    pub fn write(&self, event: LineageEvent, error: &str, retry_count: u32) -> Result<()> {
        let dlq_record = DlqRecord {
            original_timestamp: Utc::now().timestamp_millis(),
            event,
            error: error.to_string(),
            failed_at: Utc::now(),
            retry_count,
        };

        // Write as JSON lines for simplicity (could be Parquet in production)
        let filename = format!("dlq-{}.jsonl", Utc::now().format("%Y%m%d-%H"));
        let file_path = self.path.join(filename);

        let mut writer = self.writer.lock().unwrap();
        let file = writer.get_or_insert_with(|| {
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&file_path)
                .expect("Failed to open DLQ file")
        });

        use std::io::Write;
        let json = serde_json::to_string(&dlq_record)?;
        writeln!(file, "{}", json)?;
        file.flush()?;

        tracing::warn!(
            "Wrote event to DLQ: {} (retry_count: {}, error: {})",
            dlq_record.event.id,
            retry_count,
            error
        );

        Ok(())
    }

    /// Get DLQ statistics
    pub fn stats(&self) -> Result<DlqStats> {
        let mut total_records = 0u64;
        let mut total_size_bytes = 0u64;

        for entry in std::fs::read_dir(&self.path)? {
            let entry = entry?;
            if entry.path().extension().and_then(|s| s.to_str()) == Some("jsonl") {
                let metadata = entry.metadata()?;
                total_size_bytes += metadata.len();

                // Count lines
                let content = std::fs::read_to_string(entry.path())?;
                total_records += content.lines().count() as u64;
            }
        }

        Ok(DlqStats {
            total_records,
            total_size_bytes,
            path: self.path.clone(),
        })
    }
}

/// DLQ statistics
#[derive(Debug, Clone)]
pub struct DlqStats {
    pub total_records: u64,
    pub total_size_bytes: u64,
    pub path: PathBuf,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_dlq_write() {
        let tmp = TempDir::new().unwrap();
        let dlq = DeadLetterQueue::new(tmp.path()).unwrap();

        // Create a dummy event
        let event = LineageEvent {
            id: uuid::Uuid::new_v4(),
            dataset: "test".to_string(),
            record_id: "test-123".to_string(),
            source_refs: vec![],
            transforms: vec![],
            model_refs: vec![],
            output_ref: crate::core::lineage::DataRef {
                system: "test".to_string(),
                path: "test".to_string(),
                version: None,
                extracted_at: Utc::now(),
                cdc_position: None,
            },
            ts: Utc::now(),
            run_id: "test".to_string(),
            tenant_id: "test".to_string(),
            correlation_id: Some(uuid::Uuid::new_v4().to_string()),
            metadata: std::collections::HashMap::new(),
        };

        dlq.write(event, "test error", 3).unwrap();

        let stats = dlq.stats().unwrap();
        assert_eq!(stats.total_records, 1);
    }
}
