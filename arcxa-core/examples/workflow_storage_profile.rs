#![cfg_attr(not(feature = "workflow-storage"), allow(dead_code))]

#[cfg(feature = "workflow-storage")]
use std::env;
#[cfg(feature = "workflow-storage")]
use std::fs;
#[cfg(feature = "workflow-storage")]
use std::path::PathBuf;
#[cfg(feature = "workflow-storage")]
use std::sync::Arc;

#[cfg(feature = "workflow-storage")]
use anyhow::{anyhow, Context, Result};
#[cfg(feature = "workflow-storage")]
use chrono::Utc;
#[cfg(feature = "workflow-storage")]
use graphica_core::orchestration::workflow::{
    ExecutionContextV2, StorageManager, StorageType as WorkflowStorageType,
};
#[cfg(feature = "workflow-storage")]
use serde::Serialize;
#[cfg(feature = "workflow-storage")]
use serde_json::json;
#[cfg(feature = "workflow-storage")]
use uuid::Uuid;

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
    rss: RssSnapshotReport,
    hwm: HighWaterMarkReport,
    storage: StorageProfileStorageReport,
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

    let rss_before = read_proc_status_bytes("VmRSS").unwrap_or(0);
    let hwm_before = read_proc_status_bytes("VmHWM").unwrap_or(0);

    let mut ctx = ExecutionContextV2::with_storage_manager(json!({}), manager);
    ctx.resource_limits.max_row_count = case.row_count.saturating_mul(2);
    ctx.resource_limits.max_memory_bytes = usize::MAX / 4;
    ctx.set_current_step(case.case.clone());
    ctx.set_rows(rows)?;

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

    let materialized_rows = ctx.get_rows()?.to_vec()?;
    if materialized_rows.len() != case.row_count {
        return Err(anyhow!(
            "case `{}` materialized {} rows, expected {}",
            case.case,
            materialized_rows.len(),
            case.row_count
        ));
    }
    let rss_after_materialize = read_proc_status_bytes("VmRSS").unwrap_or(0);

    ctx.cleanup()?;
    let rss_after_cleanup = read_proc_status_bytes("VmRSS").unwrap_or(0);
    let hwm_after = read_proc_status_bytes("VmHWM").unwrap_or(hwm_before);

    fs::remove_dir_all(&temp_root).ok();

    Ok(StorageProfileCaseReport {
        benchmark_id: case.benchmark_id.clone(),
        case: case.case.clone(),
        row_count: case.row_count,
        expected_storage_type: storage_type_label(case.expected_storage_type).to_string(),
        actual_storage_type: storage_type_label(actual_storage_type).to_string(),
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
