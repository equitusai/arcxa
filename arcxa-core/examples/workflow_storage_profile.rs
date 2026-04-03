#![cfg_attr(not(feature = "workflow-storage"), allow(dead_code))]

#[cfg(feature = "workflow-storage")]
use std::env;
#[cfg(feature = "workflow-storage")]
use std::fs;
#[cfg(feature = "workflow-storage")]
use std::fs::File;
#[cfg(feature = "workflow-storage")]
use std::path::PathBuf;
#[cfg(feature = "workflow-storage")]
use std::sync::Arc;
#[cfg(feature = "workflow-storage")]
use std::time::{Duration, Instant};

#[cfg(feature = "workflow-storage")]
use anyhow::{anyhow, Context, Result};
#[cfg(feature = "workflow-storage")]
use arrow2::array::{Array, BooleanArray, PrimitiveArray, Utf8Array};
#[cfg(feature = "workflow-storage")]
use arrow2::chunk::Chunk;
#[cfg(feature = "workflow-storage")]
use arrow2::datatypes::Schema;
#[cfg(feature = "workflow-storage")]
use arrow2::io::parquet::read;
#[cfg(feature = "workflow-storage")]
use arrow2::io::parquet::write::{
    CompressionOptions, Encoding, FileWriter, RowGroupIterator, Version, WriteOptions,
};
#[cfg(feature = "workflow-storage")]
use chrono::Utc;
#[cfg(feature = "workflow-storage")]
use graphica_core::orchestration::workflow::{
    BatchFrame, ExecutionContextV2, RowStorage, StorageManager, StorageType as WorkflowStorageType,
};
#[cfg(feature = "workflow-storage")]
use serde::Serialize;
#[cfg(feature = "workflow-storage")]
use serde_json::{json, Map, Value};
#[cfg(feature = "workflow-storage")]
use uuid::Uuid;

#[cfg(feature = "workflow-storage")]
const PROFILE_PARQUET_ROW_GROUP_SIZE: usize = 65_536;

#[cfg(feature = "workflow-storage")]
#[derive(Debug, Clone)]
struct StorageProfileCase {
    benchmark_id: String,
    case: String,
    row_count: usize,
    expected_storage_type: WorkflowStorageType,
}

#[cfg(feature = "workflow-storage")]
#[derive(Debug, Serialize)]
struct StorageProfileReport {
    generated_at: String,
    cases: Vec<StorageProfileCaseReport>,
}

#[cfg(feature = "workflow-storage")]
#[derive(Debug, Serialize)]
struct StorageProfileCaseReport {
    benchmark_id: String,
    case: String,
    row_count: usize,
    expected_storage_type: String,
    actual_storage_type: String,
    timings: StorageProfileTimingReport,
    rss: RssSnapshotReport,
    hwm: HighWaterMarkReport,
    storage: StorageProfileStorageReport,
    parquet_write_profile: Option<ParquetWriteProfileReport>,
    parquet_read_profile: Option<ParquetReadProfileReport>,
}

#[cfg(feature = "workflow-storage")]
#[derive(Debug, Serialize)]
struct StorageProfileTimingReport {
    store_ms: f64,
    batch_frame_store_ms: Option<f64>,
    materialize_ms: f64,
    batch_frame_ms: Option<f64>,
    cleanup_ms: f64,
}

#[cfg(feature = "workflow-storage")]
#[derive(Debug, Serialize)]
struct RssSnapshotReport {
    before_bytes: usize,
    after_store_bytes: usize,
    after_materialize_bytes: usize,
    after_cleanup_bytes: usize,
    max_sampled_bytes: usize,
    delta_store_bytes: isize,
    delta_materialize_bytes: isize,
    delta_cleanup_bytes: isize,
}

#[cfg(feature = "workflow-storage")]
#[derive(Debug, Serialize)]
struct HighWaterMarkReport {
    before_bytes: usize,
    after_bytes: usize,
    delta_bytes: usize,
}

#[cfg(feature = "workflow-storage")]
#[derive(Debug, Serialize)]
struct StorageProfileStorageReport {
    planned_tier: String,
    actual_storage_type: String,
    estimated_bytes: usize,
    reserved_spill_bytes: usize,
    execution_reserved_spill_bytes: usize,
    total_reserved_spill_bytes: usize,
    spill_bytes: usize,
    spill_events: usize,
    storage_location: Option<String>,
}

#[cfg(feature = "workflow-storage")]
#[derive(Debug, Serialize)]
struct ParquetWriteProfileReport {
    row_groups_written: usize,
    rows_written: usize,
    json_to_batch_frame_ms: f64,
    row_group_chunking_ms: f64,
    row_group_iterator_setup_ms: f64,
    file_write_ms: f64,
    file_finalize_ms: f64,
    file_size_bytes: usize,
}

#[cfg(feature = "workflow-storage")]
#[derive(Debug, Serialize)]
struct ParquetReadProfileReport {
    row_groups_read: usize,
    chunks_read: usize,
    rows_materialized: usize,
    metadata_and_schema_ms: f64,
    reader_setup_ms: f64,
    row_group_decode_ms: f64,
    json_materialization_ms: f64,
}

#[cfg(feature = "workflow-storage")]
struct ProfiledParquetWrite {
    profile: ParquetWriteProfileReport,
}

#[cfg(feature = "workflow-storage")]
struct ProfiledParquetRead {
    rows: Vec<Value>,
    profile: ParquetReadProfileReport,
}

#[cfg(feature = "workflow-storage")]
fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    let mut output: Option<PathBuf> = None;
    let mut cases: Vec<StorageProfileCase> = Vec::new();

    let mut index = 1;
    while index < args.len() {
        match args[index].as_str() {
            "--output" => {
                let path = args
                    .get(index + 1)
                    .ok_or_else(|| anyhow!("missing value after --output"))?;
                output = Some(PathBuf::from(path));
                index += 2;
            }
            "--case" => {
                let raw = args
                    .get(index + 1)
                    .ok_or_else(|| anyhow!("missing value after --case"))?;
                cases.push(parse_case(raw)?);
                index += 2;
            }
            other => {
                return Err(anyhow!("unsupported argument `{other}`"));
            }
        }
    }

    let output = output.ok_or_else(|| anyhow!("--output is required"))?;
    if cases.is_empty() {
        return Err(anyhow!("at least one --case is required"));
    }

    let mut case_reports = Vec::with_capacity(cases.len());
    for case in cases {
        case_reports.push(run_case(&case)?);
    }

    let report = StorageProfileReport {
        generated_at: Utc::now().to_rfc3339(),
        cases: case_reports,
    };

    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent).with_context(|| {
            format!(
                "creating parent directory for storage profile report at {}",
                output.display()
            )
        })?;
    }
    fs::write(&output, serde_json::to_vec_pretty(&report)?)
        .with_context(|| format!("writing storage profile report to {}", output.display()))?;

    println!(
        "Wrote {} storage profile case(s) to {}",
        report.cases.len(),
        output.display()
    );

    Ok(())
}

#[cfg(not(feature = "workflow-storage"))]
fn main() {
    eprintln!("workflow_storage_profile requires the `workflow-storage` feature");
    std::process::exit(1);
}

#[cfg(feature = "workflow-storage")]
fn parse_case(raw: &str) -> Result<StorageProfileCase> {
    let mut parts = raw.split(':');
    let case = parts
        .next()
        .ok_or_else(|| anyhow!("case must include a case label"))?;
    let row_count = parts
        .next()
        .ok_or_else(|| anyhow!("case must include a row count"))?
        .parse::<usize>()
        .with_context(|| format!("parsing row count for case `{raw}`"))?;
    let storage_type = parts
        .next()
        .ok_or_else(|| anyhow!("case must include a storage type"))?;

    if parts.next().is_some() {
        return Err(anyhow!(
            "case `{raw}` has too many segments; expected case:rows:storage_type"
        ));
    }

    let expected_storage_type = match storage_type {
        "rocksdb" => WorkflowStorageType::RocksDB,
        "parquet" => WorkflowStorageType::Parquet,
        other => {
            return Err(anyhow!(
                "unsupported storage type `{other}` in case `{raw}`"
            ))
        }
    };

    Ok(StorageProfileCase {
        benchmark_id: format!("workflow_runtime/storage_tiering_round_trip/{case}/{row_count}"),
        case: case.to_string(),
        row_count,
        expected_storage_type,
    })
}

#[cfg(feature = "workflow-storage")]
fn run_case(case: &StorageProfileCase) -> Result<StorageProfileCaseReport> {
    let temp_root = env::temp_dir().join(format!(
        "arcxa-storage-profile-{}-{}",
        case.case,
        Uuid::new_v4()
    ));
    let rocks_path = temp_root.join("rocksdb");
    let temp_dir = temp_root.join("spill");
    fs::create_dir_all(&rocks_path)?;
    fs::create_dir_all(&temp_dir)?;

    let manager = Arc::new(StorageManager::new(&rocks_path, &temp_dir)?);
    let rows = generate_rows(case.row_count);
    let batch_frame = if case.expected_storage_type == WorkflowStorageType::Parquet {
        Some(BatchFrame::from_json_values(&rows)?)
    } else {
        None
    };
    let parquet_write_profile = if case.expected_storage_type == WorkflowStorageType::Parquet {
        Some(profile_parquet_write(
            &rows,
            &temp_root.join(format!("{}-write-profile.parquet", case.case)),
        )?)
    } else {
        None
    };

    let rss_before = read_proc_status_bytes("VmRSS").unwrap_or(0);
    let hwm_before = read_proc_status_bytes("VmHWM").unwrap_or(0);

    let mut ctx = ExecutionContextV2::with_storage_manager(json!({}), manager);
    ctx.resource_limits.max_row_count = case.row_count.saturating_mul(2);
    ctx.resource_limits.max_memory_bytes = usize::MAX / 4;
    ctx.set_current_step(case.case.clone());
    let store_started = Instant::now();
    ctx.set_rows(rows)?;
    let store_elapsed = store_started.elapsed();

    let rss_after_store = read_proc_status_bytes("VmRSS").unwrap_or(0);
    let decision = ctx
        .metrics
        .recent_storage_decisions
        .last()
        .cloned()
        .ok_or_else(|| anyhow!("missing storage decision metric for case `{}`", case.case))?;
    let actual_storage_type = ctx
        .row_storage
        .as_ref()
        .ok_or_else(|| anyhow!("missing row storage for case `{}`", case.case))?
        .storage_type();

    if actual_storage_type != case.expected_storage_type {
        return Err(anyhow!(
            "case `{}` expected storage type `{}`, got `{}`",
            case.case,
            storage_type_label(case.expected_storage_type),
            storage_type_label(actual_storage_type)
        ));
    }

    let row_storage = ctx
        .row_storage
        .as_ref()
        .ok_or_else(|| anyhow!("missing row storage for case `{}`", case.case))?
        .clone();
    let materialize_started = Instant::now();
    let (materialized_rows, parquet_read_profile) = match &row_storage {
        RowStorage::Parquet { path, .. } => {
            let profiled = profile_parquet_read(path)?;
            (profiled.rows, Some(profiled.profile))
        }
        _ => (ctx.get_rows()?.to_vec()?, None),
    };
    let materialize_elapsed = materialize_started.elapsed();
    if materialized_rows.len() != case.row_count {
        return Err(anyhow!(
            "case `{}` materialized {} rows, expected {}",
            case.case,
            materialized_rows.len(),
            case.row_count
        ));
    }

    let batch_frame_elapsed = if matches!(row_storage, RowStorage::Parquet { .. }) {
        let batch_frame_started = Instant::now();
        let frame = ctx.get_batch_frame()?;
        let elapsed = batch_frame_started.elapsed();
        if frame.row_count() != case.row_count {
            return Err(anyhow!(
                "case `{}` batch-frame materialized {} rows, expected {}",
                case.case,
                frame.row_count(),
                case.row_count
            ));
        }
        Some(elapsed)
    } else {
        None
    };

    let rss_after_materialize = read_proc_status_bytes("VmRSS").unwrap_or(0);

    let cleanup_started = Instant::now();
    ctx.cleanup()?;
    let cleanup_elapsed = cleanup_started.elapsed();
    let rss_after_cleanup = read_proc_status_bytes("VmRSS").unwrap_or(0);
    let hwm_after = read_proc_status_bytes("VmHWM").unwrap_or(hwm_before);

    let batch_frame_store_elapsed = if let Some(frame) = batch_frame {
        let batch_frame_rocks_path = temp_root.join("batch-frame-rocksdb");
        let batch_frame_temp_dir = temp_root.join("batch-frame-spill");
        fs::create_dir_all(&batch_frame_rocks_path)?;
        fs::create_dir_all(&batch_frame_temp_dir)?;

        let batch_frame_manager = Arc::new(StorageManager::new(
            &batch_frame_rocks_path,
            &batch_frame_temp_dir,
        )?);
        let mut batch_frame_ctx =
            ExecutionContextV2::with_storage_manager(json!({}), batch_frame_manager);
        batch_frame_ctx.resource_limits.max_row_count = case.row_count.saturating_mul(2);
        batch_frame_ctx.resource_limits.max_memory_bytes = usize::MAX / 4;
        batch_frame_ctx.set_current_step(format!("{}-batch-frame", case.case));

        let started = Instant::now();
        batch_frame_ctx.set_batch_frame(frame)?;
        let elapsed = started.elapsed();

        let actual_storage_type = batch_frame_ctx
            .row_storage
            .as_ref()
            .ok_or_else(|| anyhow!("missing batch-frame row storage for case `{}`", case.case))?
            .storage_type();
        if actual_storage_type != case.expected_storage_type {
            return Err(anyhow!(
                "case `{}` batch-frame store expected storage type `{}`, got `{}`",
                case.case,
                storage_type_label(case.expected_storage_type),
                storage_type_label(actual_storage_type)
            ));
        }

        batch_frame_ctx.cleanup()?;
        Some(elapsed)
    } else {
        None
    };

    fs::remove_dir_all(&temp_root).ok();

    Ok(StorageProfileCaseReport {
        benchmark_id: case.benchmark_id.clone(),
        case: case.case.clone(),
        row_count: case.row_count,
        expected_storage_type: storage_type_label(case.expected_storage_type).to_string(),
        actual_storage_type: storage_type_label(actual_storage_type).to_string(),
        timings: StorageProfileTimingReport {
            store_ms: duration_ms(store_elapsed),
            batch_frame_store_ms: batch_frame_store_elapsed.map(duration_ms),
            materialize_ms: duration_ms(materialize_elapsed),
            batch_frame_ms: batch_frame_elapsed.map(duration_ms),
            cleanup_ms: duration_ms(cleanup_elapsed),
        },
        rss: RssSnapshotReport {
            before_bytes: rss_before,
            after_store_bytes: rss_after_store,
            after_materialize_bytes: rss_after_materialize,
            after_cleanup_bytes: rss_after_cleanup,
            max_sampled_bytes: [
                rss_before,
                rss_after_store,
                rss_after_materialize,
                rss_after_cleanup,
            ]
            .into_iter()
            .max()
            .unwrap_or(0),
            delta_store_bytes: rss_after_store as isize - rss_before as isize,
            delta_materialize_bytes: rss_after_materialize as isize - rss_before as isize,
            delta_cleanup_bytes: rss_after_cleanup as isize - rss_before as isize,
        },
        hwm: HighWaterMarkReport {
            before_bytes: hwm_before,
            after_bytes: hwm_after,
            delta_bytes: hwm_after.saturating_sub(hwm_before),
        },
        storage: StorageProfileStorageReport {
            planned_tier: format!("{:?}", decision.planned_tier).to_lowercase(),
            actual_storage_type: storage_type_label(actual_storage_type).to_string(),
            estimated_bytes: decision.estimated_bytes,
            reserved_spill_bytes: decision.reserved_spill_bytes,
            execution_reserved_spill_bytes: decision.execution_reserved_spill_bytes,
            total_reserved_spill_bytes: decision.total_reserved_spill_bytes,
            spill_bytes: ctx.metrics.spill_bytes,
            spill_events: ctx.metrics.spill_events,
            storage_location: decision.storage_location.clone(),
        },
        parquet_write_profile: parquet_write_profile.map(|profile| profile.profile),
        parquet_read_profile,
    })
}

#[cfg(feature = "workflow-storage")]
fn generate_rows(count: usize) -> Vec<serde_json::Value> {
    (0..count)
        .map(|index| {
            json!({
                "id": index as i64,
                "status": if index % 2 == 0 { "ACTIVE" } else { "PENDING" },
                "amount": ((index % 1000) as f64) + 0.25,
            })
        })
        .collect()
}

#[cfg(feature = "workflow-storage")]
fn read_proc_status_bytes(key: &str) -> Option<usize> {
    let status = fs::read_to_string("/proc/self/status").ok()?;
    for line in status.lines() {
        if let Some(rest) = line.strip_prefix(key) {
            let rest = rest.trim_start_matches(':').trim();
            let mut tokens = rest.split_whitespace();
            let value = tokens.next()?.parse::<usize>().ok()?;
            let unit = tokens.next().unwrap_or("kB");
            let multiplier = match unit {
                "kB" => 1024,
                "mB" | "MB" => 1024 * 1024,
                _ => 1,
            };
            return Some(value.saturating_mul(multiplier));
        }
    }
    None
}

#[cfg(feature = "workflow-storage")]
fn storage_type_label(storage_type: WorkflowStorageType) -> &'static str {
    match storage_type {
        WorkflowStorageType::InMemory => "in_memory",
        WorkflowStorageType::Shared => "shared",
        WorkflowStorageType::RocksDB => "rocksdb",
        WorkflowStorageType::Parquet => "parquet",
    }
}

#[cfg(feature = "workflow-storage")]
fn duration_ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

#[cfg(feature = "workflow-storage")]
fn profile_parquet_write(rows: &[Value], path: &std::path::Path) -> Result<ProfiledParquetWrite> {
    let batch_started = Instant::now();
    let batch = BatchFrame::from_json_values(rows)?;
    let batch_elapsed = batch_started.elapsed();

    let chunking_started = Instant::now();
    let row_group_chunks = build_profile_row_group_chunks(
        batch.columns(),
        batch.row_count(),
        PROFILE_PARQUET_ROW_GROUP_SIZE,
    );
    let chunking_elapsed = chunking_started.elapsed();

    let file = File::create(path)
        .with_context(|| format!("creating parquet write profile file {}", path.display()))?;
    let options = WriteOptions {
        write_statistics: true,
        compression: CompressionOptions::Uncompressed,
        version: Version::V1,
        data_pagesize_limit: None,
    };
    let encodings = vec![vec![Encoding::Plain]; batch.schema().fields.len()];

    let iterator_started = Instant::now();
    let row_groups = RowGroupIterator::try_new(
        row_group_chunks
            .into_iter()
            .map(Ok::<_, arrow2::error::Error>),
        batch.schema(),
        options,
        encodings,
    )
    .map_err(|e| anyhow!("building parquet row groups: {e}"))?;
    let iterator_elapsed = iterator_started.elapsed();

    let mut writer = FileWriter::try_new(file, batch.schema().clone(), options)
        .map_err(|e| anyhow!("creating parquet file writer: {e}"))?;

    let write_started = Instant::now();
    let mut row_groups_written = 0usize;
    for row_group in row_groups {
        writer
            .write(row_group.map_err(|e| anyhow!("serializing parquet row group: {e}"))?)
            .map_err(|e| anyhow!("writing parquet row group: {e}"))?;
        row_groups_written += 1;
    }
    let write_elapsed = write_started.elapsed();

    let finalize_started = Instant::now();
    writer
        .end(None)
        .map_err(|e| anyhow!("finalizing parquet file: {e}"))?;
    let finalize_elapsed = finalize_started.elapsed();

    let file_size_bytes = fs::metadata(path)
        .with_context(|| {
            format!(
                "reading parquet write profile metadata for {}",
                path.display()
            )
        })?
        .len() as usize;

    Ok(ProfiledParquetWrite {
        profile: ParquetWriteProfileReport {
            row_groups_written,
            rows_written: rows.len(),
            json_to_batch_frame_ms: duration_ms(batch_elapsed),
            row_group_chunking_ms: duration_ms(chunking_elapsed),
            row_group_iterator_setup_ms: duration_ms(iterator_elapsed),
            file_write_ms: duration_ms(write_elapsed),
            file_finalize_ms: duration_ms(finalize_elapsed),
            file_size_bytes,
        },
    })
}

#[cfg(feature = "workflow-storage")]
fn profile_parquet_read(path: &std::path::Path) -> Result<ProfiledParquetRead> {
    let metadata_started = Instant::now();
    let mut file = File::open(path)
        .with_context(|| format!("opening parquet spill file {}", path.display()))?;
    let metadata =
        read::read_metadata(&mut file).map_err(|e| anyhow!("reading parquet metadata: {e}"))?;
    let schema =
        read::infer_schema(&metadata).map_err(|e| anyhow!("inferring parquet schema: {e}"))?;
    let metadata_elapsed = metadata_started.elapsed();

    let row_groups = metadata.row_groups;
    let row_group_count = row_groups.len();

    let reader_started = Instant::now();
    let mut reader = read::FileReader::new(file, row_groups, schema.clone(), None, None, None);
    let reader_setup_elapsed = reader_started.elapsed();

    let mut decode_elapsed = Duration::default();
    let mut json_elapsed = Duration::default();
    let mut chunk_count = 0usize;
    let mut rows = Vec::new();

    loop {
        let decode_started = Instant::now();
        let next_chunk = reader.next();
        decode_elapsed += decode_started.elapsed();

        let Some(chunk) = next_chunk else {
            break;
        };
        let chunk = chunk.map_err(|e| anyhow!("reading parquet row group: {e}"))?;
        chunk_count += 1;

        let json_started = Instant::now();
        rows.extend(profile_chunk_to_json_rows(&schema, &chunk)?);
        json_elapsed += json_started.elapsed();
    }

    Ok(ProfiledParquetRead {
        profile: ParquetReadProfileReport {
            row_groups_read: row_group_count,
            chunks_read: chunk_count,
            rows_materialized: rows.len(),
            metadata_and_schema_ms: duration_ms(metadata_elapsed),
            reader_setup_ms: duration_ms(reader_setup_elapsed),
            row_group_decode_ms: duration_ms(decode_elapsed),
            json_materialization_ms: duration_ms(json_elapsed),
        },
        rows,
    })
}

#[cfg(feature = "workflow-storage")]
fn build_profile_row_group_chunks(
    columns: &Chunk<Box<dyn Array>>,
    row_count: usize,
    row_group_size: usize,
) -> Vec<Chunk<Box<dyn Array>>> {
    if row_count == 0 {
        return vec![Chunk::new(
            columns
                .arrays()
                .iter()
                .map(|array| array.sliced(0, 0))
                .collect(),
        )];
    }

    let mut chunks = Vec::new();
    let mut offset = 0;
    while offset < row_count {
        let length = std::cmp::min(row_group_size, row_count - offset);
        let arrays = columns
            .arrays()
            .iter()
            .map(|array| array.sliced(offset, length))
            .collect();
        chunks.push(Chunk::new(arrays));
        offset += length;
    }
    chunks
}

#[cfg(feature = "workflow-storage")]
fn profile_chunk_to_json_rows(
    schema: &Schema,
    chunk: &Chunk<Box<dyn Array>>,
) -> Result<Vec<Value>> {
    if schema.fields.len() != chunk.arrays().len() {
        return Err(anyhow!(
            "Parquet schema/column mismatch: schema has {} fields, chunk has {} arrays",
            schema.fields.len(),
            chunk.arrays().len()
        ));
    }

    let row_count = chunk.len();
    let mut rows = vec![Map::new(); row_count];

    for (field, column) in schema.fields.iter().zip(chunk.arrays().iter()) {
        match field.data_type() {
            arrow2::datatypes::DataType::Boolean => {
                let array = column
                    .as_any()
                    .downcast_ref::<BooleanArray>()
                    .ok_or_else(|| anyhow!("expected BooleanArray for column '{}'", field.name))?;
                for row_index in 0..row_count {
                    let value = if array.is_null(row_index) {
                        Value::Null
                    } else {
                        Value::Bool(array.value(row_index))
                    };
                    rows[row_index].insert(field.name.clone(), value);
                }
            }
            arrow2::datatypes::DataType::Int64 => {
                let array = column
                    .as_any()
                    .downcast_ref::<PrimitiveArray<i64>>()
                    .ok_or_else(|| anyhow!("expected Int64 array for column '{}'", field.name))?;
                for row_index in 0..row_count {
                    let value = if array.is_null(row_index) {
                        Value::Null
                    } else {
                        Value::Number(array.value(row_index).into())
                    };
                    rows[row_index].insert(field.name.clone(), value);
                }
            }
            arrow2::datatypes::DataType::Float64 => {
                let array = column
                    .as_any()
                    .downcast_ref::<PrimitiveArray<f64>>()
                    .ok_or_else(|| anyhow!("expected Float64 array for column '{}'", field.name))?;
                for row_index in 0..row_count {
                    let value = if array.is_null(row_index) {
                        Value::Null
                    } else {
                        serde_json::Number::from_f64(array.value(row_index))
                            .map(Value::Number)
                            .unwrap_or(Value::Null)
                    };
                    rows[row_index].insert(field.name.clone(), value);
                }
            }
            arrow2::datatypes::DataType::Utf8 => {
                let array = column
                    .as_any()
                    .downcast_ref::<Utf8Array<i32>>()
                    .ok_or_else(|| anyhow!("expected Utf8 array for column '{}'", field.name))?;
                for row_index in 0..row_count {
                    let value = if array.is_null(row_index) {
                        Value::Null
                    } else {
                        Value::String(array.value(row_index).to_string())
                    };
                    rows[row_index].insert(field.name.clone(), value);
                }
            }
            other => {
                return Err(anyhow!(
                    "Parquet profiling does not support Arrow type {:?}",
                    other
                ));
            }
        }
    }

    Ok(rows.into_iter().map(Value::Object).collect())
}
