use std::collections::{BTreeMap, HashSet};
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use arrow2::array::{Array, BooleanArray, PrimitiveArray, Utf8Array};
use arrow2::chunk::Chunk;
use arrow2::datatypes::Schema;
use arrow2::io::parquet::read;
use arrow2::io::parquet::write::{
    CompressionOptions, Encoding, FileWriter, RowGroupIterator, Version, WriteOptions,
};
use serde_json::{Map, Value};

use crate::orchestration::workflow::error::{Result, WorkflowError};
use crate::orchestration::workflow::runtime::frame::BatchFrame;

const DEFAULT_PARQUET_ROW_GROUP_SIZE: usize = 65_536;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PreparedParquetSpill {
    pub path: PathBuf,
    pub stem: String,
}

#[derive(Debug, Clone)]
pub(crate) struct ParquetStorageDescriptor {
    pub path: PathBuf,
    pub schema: Arc<Schema>,
    pub row_count: usize,
    pub index: Arc<BTreeMap<usize, u64>>,
    pub file_size_bytes: usize,
}

pub(crate) fn prepare_parquet_spill(
    temp_dir: &Path,
    execution_id: &str,
    step_id: &str,
) -> Result<PreparedParquetSpill> {
    std::fs::create_dir_all(temp_dir).map_err(|e| WorkflowError::IoError(e.to_string()))?;

    let stem = format!("{}_{}_{}", execution_id, step_id, uuid::Uuid::new_v4());
    let path = temp_dir.join(format!("{stem}.parquet"));

    Ok(PreparedParquetSpill { path, stem })
}

pub(crate) fn create_parquet_storage(
    temp_dir: &Path,
    execution_id: &str,
    step_id: &str,
    rows: &[Value],
) -> Result<ParquetStorageDescriptor> {
    let prepared = prepare_parquet_spill(temp_dir, execution_id, step_id)?;
    let batch = BatchFrame::from_json_values(rows)?;
    let schema = Arc::new(batch.schema().clone());
    let row_group_index = build_row_group_index(batch.row_count(), DEFAULT_PARQUET_ROW_GROUP_SIZE);

    let file = File::create(&prepared.path).map_err(|e| WorkflowError::IoError(e.to_string()))?;
    let options = WriteOptions {
        write_statistics: true,
        compression: CompressionOptions::Uncompressed,
        version: Version::V1,
        data_pagesize_limit: None,
    };
    let encodings = vec![vec![Encoding::Plain]; schema.fields.len()];
    let row_group_chunks = build_row_group_chunks(
        batch.columns(),
        batch.row_count(),
        DEFAULT_PARQUET_ROW_GROUP_SIZE,
    );
    let row_groups = RowGroupIterator::try_new(
        row_group_chunks
            .into_iter()
            .map(Ok::<_, arrow2::error::Error>),
        schema.as_ref(),
        options,
        encodings,
    )
    .map_err(|e| WorkflowError::Storage(e.to_string()))?;

    let mut writer = FileWriter::try_new(file, schema.as_ref().clone(), options)
        .map_err(|e| WorkflowError::Storage(e.to_string()))?;
    for row_group in row_groups {
        writer
            .write(row_group.map_err(|e| WorkflowError::Storage(e.to_string()))?)
            .map_err(|e| WorkflowError::Storage(e.to_string()))?;
    }
    writer
        .end(None)
        .map_err(|e| WorkflowError::Storage(e.to_string()))?;

    let file_size_bytes = std::fs::metadata(&prepared.path)
        .map_err(|e| WorkflowError::IoError(e.to_string()))?
        .len() as usize;

    Ok(ParquetStorageDescriptor {
        path: prepared.path,
        schema,
        row_count: rows.len(),
        index: Arc::new(row_group_index),
        file_size_bytes,
    })
}

pub(crate) fn read_parquet_rows(path: &Path) -> Result<Vec<Value>> {
    read_selected_row_groups(path, None)
}

pub(crate) fn read_parquet_range(
    path: &Path,
    index: &BTreeMap<usize, u64>,
    start: usize,
    end: usize,
) -> Result<Vec<Value>> {
    if start >= end || index.is_empty() {
        return Ok(Vec::new());
    }

    let (start_group_ordinal, start_group_row) = locate_row_group(index, start)?;
    let (end_group_ordinal, _) = locate_row_group(index, end.saturating_sub(1))?;
    let selected_groups: Vec<usize> =
        (start_group_ordinal as usize..=end_group_ordinal as usize).collect();

    let mut rows = read_selected_row_groups(path, Some(&selected_groups))?;
    let leading_offset = start.saturating_sub(start_group_row);
    if leading_offset > 0 {
        rows.drain(0..leading_offset.min(rows.len()));
    }
    rows.truncate(end.saturating_sub(start));
    Ok(rows)
}

pub(crate) fn read_parquet_row(
    path: &Path,
    index: &BTreeMap<usize, u64>,
    row_index: usize,
) -> Result<Option<Value>> {
    if index.is_empty() {
        return Ok(None);
    }

    let (row_group_ordinal, row_group_start) = locate_row_group(index, row_index)?;
    let rows = read_selected_row_groups(path, Some(&[row_group_ordinal as usize]))?;
    Ok(rows.get(row_index.saturating_sub(row_group_start)).cloned())
}

fn build_row_group_chunks(
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

fn build_row_group_index(row_count: usize, row_group_size: usize) -> BTreeMap<usize, u64> {
    let mut index = BTreeMap::new();
    let mut offset = 0;
    let mut row_group = 0u64;

    while offset < row_count {
        index.insert(offset, row_group);
        offset += row_group_size;
        row_group += 1;
    }

    index
}

fn locate_row_group(index: &BTreeMap<usize, u64>, row_index: usize) -> Result<(u64, usize)> {
    index
        .range(..=row_index)
        .next_back()
        .map(|(start_row, ordinal)| (*ordinal, *start_row))
        .ok_or_else(|| {
            WorkflowError::DataNotFound(format!(
                "No Parquet row group found for row index {}",
                row_index
            ))
        })
}

fn read_selected_row_groups(path: &Path, selected_groups: Option<&[usize]>) -> Result<Vec<Value>> {
    let mut file = File::open(path).map_err(|e| WorkflowError::IoError(e.to_string()))?;
    let metadata =
        read::read_metadata(&mut file).map_err(|e| WorkflowError::Storage(e.to_string()))?;
    let schema =
        read::infer_schema(&metadata).map_err(|e| WorkflowError::Storage(e.to_string()))?;

    let selected_set = selected_groups.map(|groups| groups.iter().copied().collect::<HashSet<_>>());
    let row_groups = metadata
        .row_groups
        .into_iter()
        .enumerate()
        .filter(|(ordinal, _)| {
            selected_set
                .as_ref()
                .map(|groups| groups.contains(ordinal))
                .unwrap_or(true)
        })
        .map(|(_, row_group)| row_group)
        .collect::<Vec<_>>();

    let mut reader = read::FileReader::new(file, row_groups, schema.clone(), None, None, None);
    let mut rows = Vec::new();
    while let Some(chunk) = reader.next() {
        let chunk = chunk.map_err(|e| WorkflowError::Storage(e.to_string()))?;
        rows.extend(chunk_to_json_rows(&schema, &chunk)?);
    }

    Ok(rows)
}

fn chunk_to_json_rows(schema: &Schema, chunk: &Chunk<Box<dyn Array>>) -> Result<Vec<Value>> {
    if schema.fields.len() != chunk.arrays().len() {
        return Err(WorkflowError::InvalidData(format!(
            "Parquet schema/column mismatch: schema has {} fields, chunk has {} arrays",
            schema.fields.len(),
            chunk.arrays().len()
        )));
    }

    let row_count = chunk.len();
    let mut rows = vec![Map::new(); row_count];

    for (field, column) in schema.fields.iter().zip(chunk.arrays().iter()) {
        match field.data_type() {
            arrow2::datatypes::DataType::Boolean => {
                let array = column
                    .as_any()
                    .downcast_ref::<BooleanArray>()
                    .ok_or_else(|| {
                        WorkflowError::InvalidData(format!(
                            "Expected BooleanArray for column '{}'",
                            field.name
                        ))
                    })?;
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
                    .ok_or_else(|| {
                        WorkflowError::InvalidData(format!(
                            "Expected Int64 array for column '{}'",
                            field.name
                        ))
                    })?;
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
                    .ok_or_else(|| {
                        WorkflowError::InvalidData(format!(
                            "Expected Float64 array for column '{}'",
                            field.name
                        ))
                    })?;
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
                    .ok_or_else(|| {
                        WorkflowError::InvalidData(format!(
                            "Expected Utf8 array for column '{}'",
                            field.name
                        ))
                    })?;
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
                return Err(WorkflowError::NotImplemented(format!(
                    "Parquet row materialization does not support Arrow type {:?}",
                    other
                )));
            }
        }
    }

    Ok(rows.into_iter().map(Value::Object).collect())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use serde_json::json;
    use tempfile::tempdir;

    use super::{
        create_parquet_storage, prepare_parquet_spill, read_parquet_range, read_parquet_row,
        read_parquet_rows,
    };

    #[test]
    fn prepares_parquet_path_inside_temp_dir() {
        let temp_dir = tempdir().unwrap();
        let prepared = prepare_parquet_spill(temp_dir.path(), "exec_1", "step_a").unwrap();

        assert!(prepared.path.starts_with(temp_dir.path()));
        assert_eq!(
            prepared.path.extension().and_then(|ext| ext.to_str()),
            Some("parquet")
        );
        assert!(prepared.stem.contains("exec_1"));
        assert!(prepared.stem.contains("step_a"));
    }

    #[test]
    fn parquet_storage_round_trips_rows_and_supports_targeted_reads() {
        let temp_dir = tempdir().unwrap();
        let rows = vec![
            json!({"id": 1, "name": "alpha", "active": true, "score": 10.5}),
            json!({"id": 2, "name": "beta", "active": false, "score": 11.25}),
            json!({"id": 3, "name": "gamma", "active": true, "score": 12.75}),
        ];

        let storage = create_parquet_storage(temp_dir.path(), "exec_1", "step_a", &rows).unwrap();
        assert_eq!(storage.row_count, rows.len());
        assert!(storage.file_size_bytes > 0);

        let materialized = read_parquet_rows(&storage.path).unwrap();
        assert_eq!(materialized, rows);

        let targeted = read_parquet_row(&storage.path, &storage.index, 1).unwrap();
        assert_eq!(targeted, Some(rows[1].clone()));

        let ranged = read_parquet_range(&storage.path, &storage.index, 1, 3).unwrap();
        assert_eq!(ranged, rows[1..3].to_vec());
    }

    #[test]
    fn read_parquet_range_returns_empty_for_empty_index() {
        let temp_dir = tempdir().unwrap();
        let storage = create_parquet_storage(temp_dir.path(), "exec_empty", "step_a", &[]).unwrap();
        let rows =
            read_parquet_range(&storage.path, &BTreeMap::new(), 0, 10).expect("empty range read");
        assert!(rows.is_empty());
    }
}
