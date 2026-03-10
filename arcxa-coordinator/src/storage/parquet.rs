//! # Parquet Storage Implementation
//!
//! Warm/cold tier columnar storage for historical lineage.
//! Provides efficient scanning with partition pruning and predicate pushdown.

use anyhow::{Context, Result};
use arrow2::array::*;
use arrow2::chunk::Chunk;
use arrow2::datatypes::*;
use arrow2::io::parquet::read as parquet_read;
use arrow2::io::parquet::write as parquet_write;
use chrono::{DateTime, Datelike, Utc};
use graphica_core::core::lineage::{LineageEvent, LineageSink};
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};
use std::sync::Arc;

const BATCH_SIZE: usize = 1000; // Events per Parquet file

pub struct ParquetLineageStore {
    base_path: String,
    write_buffer: std::sync::Mutex<Vec<LineageEvent>>,
}

impl ParquetLineageStore {
    pub fn new(base_path: &str) -> Result<Self> {
        std::fs::create_dir_all(base_path)?;

        Ok(Self {
            base_path: base_path.to_string(),
            write_buffer: std::sync::Mutex::new(Vec::new()),
        })
    }

    /// Generate partition path: base_path/year=2024/month=01/day=15
    fn partition_path(&self, timestamp: DateTime<Utc>) -> String {
        format!(
            "{}/year={}/month={:02}/day={:02}",
            self.base_path,
            timestamp.year(),
            timestamp.month(),
            timestamp.day()
        )
    }

    /// Get Parquet file path for a partition
    fn parquet_file_path(&self, timestamp: DateTime<Utc>) -> PathBuf {
        let partition = self.partition_path(timestamp);
        PathBuf::from(format!("{}/events.parquet", partition))
    }

    /// Convert LineageEvent batch to Arrow schema and data
    fn events_to_arrow(
        &self,
        events: &[LineageEvent],
    ) -> Result<(Arc<Schema>, Vec<Arc<dyn Array>>)> {
        // Define Arrow schema for LineageEvent
        let schema = Arc::new(Schema::from(vec![
            Field::new("id", DataType::Utf8, false),
            Field::new("dataset", DataType::Utf8, false),
            Field::new("record_id", DataType::Utf8, false),
            Field::new(
                "ts",
                DataType::Timestamp(TimeUnit::Millisecond, Some("UTC".to_string())),
                false,
            ),
            Field::new("run_id", DataType::Utf8, false),
            Field::new("tenant_id", DataType::Utf8, false),
            Field::new("event_json", DataType::Utf8, false), // Store full event as JSON for simplicity
        ]));

        // Convert events to columnar arrays
        let ids: Vec<Option<String>> = events.iter().map(|e| Some(e.id.to_string())).collect();
        let datasets: Vec<Option<String>> =
            events.iter().map(|e| Some(e.dataset.clone())).collect();
        let record_ids: Vec<Option<String>> =
            events.iter().map(|e| Some(e.record_id.clone())).collect();
        let timestamps: Vec<Option<i64>> = events
            .iter()
            .map(|e| Some(e.ts.timestamp_millis()))
            .collect();
        let run_ids: Vec<Option<String>> = events.iter().map(|e| Some(e.run_id.clone())).collect();
        let tenant_ids: Vec<Option<String>> =
            events.iter().map(|e| Some(e.tenant_id.clone())).collect();
        let event_jsons: Vec<Option<String>> = events
            .iter()
            .map(|e| serde_json::to_string(e).ok())
            .collect();

        let columns: Vec<Arc<dyn Array>> = vec![
            Arc::new(Utf8Array::<i32>::from_iter(ids)),
            Arc::new(Utf8Array::<i32>::from_iter(datasets)),
            Arc::new(Utf8Array::<i32>::from_iter(record_ids)),
            Arc::new(
                PrimitiveArray::<i64>::from_iter(timestamps).to(DataType::Timestamp(
                    TimeUnit::Millisecond,
                    Some("UTC".to_string()),
                )),
            ),
            Arc::new(Utf8Array::<i32>::from_iter(run_ids)),
            Arc::new(Utf8Array::<i32>::from_iter(tenant_ids)),
            Arc::new(Utf8Array::<i32>::from_iter(event_jsons)),
        ];

        Ok((schema, columns))
    }

    /// Write events batch to Parquet file
    fn write_parquet_batch(&self, events: &[LineageEvent], file_path: &Path) -> Result<()> {
        if events.is_empty() {
            return Ok(());
        }

        let (schema, columns) = self.events_to_arrow(events)?;
        let chunk = Chunk::new(columns);

        // Create parent directory
        if let Some(parent) = file_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Write to Parquet
        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(file_path)?;

        let options = parquet_write::WriteOptions {
            write_statistics: true,
            compression: parquet_write::CompressionOptions::Snappy,
            version: parquet_write::Version::V2,
            data_pagesize_limit: None,
        };

        let encodings = schema
            .fields
            .iter()
            .map(|f| parquet_write::transverse(&f.data_type, |_| parquet_write::Encoding::Plain))
            .collect();

        let row_groups = parquet_write::RowGroupIterator::try_new(
            vec![Ok(chunk)].into_iter(),
            &schema,
            options,
            encodings,
        )?;

        let mut writer = parquet_write::FileWriter::try_new(file, (*schema).clone(), options)?;

        for group in row_groups {
            writer.write(group?)?;
        }

        writer.end(None)?;

        tracing::debug!(
            "Wrote {} events to Parquet file: {:?}",
            events.len(),
            file_path
        );

        Ok(())
    }

    /// Read events from a Parquet file with optional filtering
    fn read_parquet_file(
        &self,
        file_path: &Path,
        filter: Option<&Box<dyn Fn(&LineageEvent) -> bool>>,
    ) -> Result<Vec<LineageEvent>> {
        if !file_path.exists() {
            return Ok(vec![]);
        }

        let mut file = File::open(file_path)
            .with_context(|| format!("Failed to open Parquet file: {:?}", file_path))?;

        let metadata = parquet_read::read_metadata(&mut file)?;
        let schema = parquet_read::infer_schema(&metadata)?;

        let mut events = Vec::new();

        // Read all row groups
        for row_group in 0..metadata.row_groups.len() {
            let mut columns_iter = parquet_read::FileReader::new(
                file.try_clone()?,
                vec![metadata.row_groups[row_group].clone()],
                schema.clone(),
                Some(10_000),
                None,
                None,
            );

            while let Some(chunk) = columns_iter.next() {
                let chunk = chunk?;

                // Extract event_json column (index 6)
                if let Some(array) = chunk.arrays().get(6) {
                    if let Some(utf8_array) = array.as_any().downcast_ref::<Utf8Array<i32>>() {
                        for i in 0..utf8_array.len() {
                            if let Some(json_str) = utf8_array.get(i) {
                                match serde_json::from_str::<LineageEvent>(json_str) {
                                    Ok(event) => {
                                        if let Some(ref f) = filter {
                                            if f(&event) {
                                                events.push(event);
                                            }
                                        } else {
                                            events.push(event);
                                        }
                                    }
                                    Err(e) => {
                                        tracing::warn!(
                                            "Failed to deserialize event from Parquet: {}",
                                            e
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(events)
    }

    /// Scan all partitions within a date range
    fn scan_partitions(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
        filter: Option<Box<dyn Fn(&LineageEvent) -> bool>>,
    ) -> Result<Vec<LineageEvent>> {
        let mut all_events = Vec::new();

        // Iterate through all partitions (naive implementation - could be optimized)
        let mut current = start.date_naive();
        let end_date = end.date_naive();

        while current <= end_date {
            let ts = current.and_hms_opt(0, 0, 0).unwrap().and_utc();
            let file_path = self.parquet_file_path(ts);

            if file_path.exists() {
                // Read all events from file, filter will be applied inside read_parquet_file
                match self.read_parquet_file(&file_path, filter.as_ref()) {
                    Ok(mut events) => all_events.append(&mut events),
                    Err(e) => tracing::warn!("Failed to read partition {:?}: {}", file_path, e),
                }
            }

            current = current.succ_opt().unwrap_or(end_date);
        }

        Ok(all_events)
    }

    /// Flush write buffer to disk
    pub fn flush(&self) -> Result<()> {
        let mut buffer = self.write_buffer.lock().unwrap();
        if buffer.is_empty() {
            return Ok(());
        }

        // Group events by partition (date)
        let mut partitions: HashMap<String, Vec<LineageEvent>> = HashMap::new();
        for event in buffer.drain(..) {
            let partition_key = format!(
                "{}-{:02}-{:02}",
                event.ts.year(),
                event.ts.month(),
                event.ts.day()
            );
            partitions.entry(partition_key).or_default().push(event);
        }

        // Write each partition
        for (_, events) in partitions {
            if let Some(first_event) = events.first() {
                let file_path = self.parquet_file_path(first_event.ts);
                self.write_parquet_batch(&events, &file_path)?;
            }
        }

        Ok(())
    }
}

impl LineageSink for ParquetLineageStore {
    fn write(&self, event: LineageEvent) -> Result<()> {
        // Buffer writes for efficiency
        let mut buffer = self.write_buffer.lock().unwrap();
        buffer.push(event);

        // Flush if buffer is full
        if buffer.len() >= BATCH_SIZE {
            drop(buffer); // Release lock before flushing
            self.flush()?;
        }

        Ok(())
    }

    fn get_record_lineage(&self, record_id: &str) -> Result<Vec<LineageEvent>> {
        // Scan all partitions (in production, use metadata index)
        let now = Utc::now();
        let one_year_ago = now - chrono::Duration::days(365);

        let filter = Box::new({
            let record_id = record_id.to_string();
            move |event: &LineageEvent| event.record_id == record_id
        });

        self.scan_partitions(one_year_ago, now, Some(filter))
    }

    fn get_model_impact(&self, model_id: &str, version: &str) -> Result<Vec<LineageEvent>> {
        let now = Utc::now();
        let one_year_ago = now - chrono::Duration::days(365);

        let filter = Box::new({
            let model_id = model_id.to_string();
            let version = version.to_string();
            move |event: &LineageEvent| {
                event
                    .model_refs
                    .iter()
                    .any(|m| m.model_id == model_id && m.version == version)
            }
        });

        self.scan_partitions(one_year_ago, now, Some(filter))
    }

    fn query_by_time_range(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<LineageEvent>> {
        let filter = Box::new({
            let start = start;
            let end = end;
            move |event: &LineageEvent| event.ts >= start && event.ts <= end
        });

        self.scan_partitions(start, end, Some(filter))
    }

    fn get_run_lineage(&self, run_id: &str) -> Result<Vec<LineageEvent>> {
        let now = Utc::now();
        let one_year_ago = now - chrono::Duration::days(365);

        let filter = Box::new({
            let run_id = run_id.to_string();
            move |event: &LineageEvent| event.run_id == run_id
        });

        self.scan_partitions(one_year_ago, now, Some(filter))
    }

    fn get_lineage_as_of(
        &self,
        record_id: &str,
        as_of: DateTime<Utc>,
    ) -> Result<Vec<LineageEvent>> {
        // Scan partitions from 1 year before as_of up to as_of
        let one_year_before = as_of - chrono::Duration::days(365);

        let filter = Box::new({
            let record_id = record_id.to_string();
            let as_of = as_of;
            move |event: &LineageEvent| event.record_id == record_id && event.ts <= as_of
        });

        self.scan_partitions(one_year_before, as_of, Some(filter))
    }
}

impl Drop for ParquetLineageStore {
    fn drop(&mut self) {
        // Flush any remaining buffered events
        if let Err(e) = self.flush() {
            tracing::error!("Failed to flush Parquet buffer on drop: {}", e);
        }
    }
}
