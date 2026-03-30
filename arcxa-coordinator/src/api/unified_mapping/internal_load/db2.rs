use anyhow::{Context, Result};
use futures::stream;
use graphica_core::catalog::DataSourceCatalog;
use std::collections::HashMap;
use std::sync::Arc;

use crate::etl::destinations::Db2Destination;
use crate::etl::traits::{
    DataDestination, DataRecord, DataType, ErrorTolerance, FieldSchema, LoadConfig, LoadMode,
    RecordSchema,
};
use crate::mapping::loader::{
    create_db2_pool, DB2Config as LoaderDb2Config, DB2PoolConfig, LoadResult as MappingLoadResult,
    PoolTimeouts,
};
use crate::mapping::multi_source::types::TargetTableConfig;

use super::common::{
    materialize_target_records, resolve_target_datasource_from_catalog, target_table_key_fields,
};

fn qualified_db2_table_name(
    session: &crate::mapping::multi_source::UnifiedMappingSession,
    table_name: &str,
) -> String {
    if table_name.contains('.') || session.target_database.schema.trim().is_empty() {
        table_name.to_string()
    } else {
        format!("{}.{}", session.target_database.schema, table_name)
    }
}

fn build_loader_db2_config(
    config: &graphica_core::catalog::types::DB2Config,
    credentials: &graphica_core::catalog::connector::Credentials,
) -> LoaderDb2Config {
    LoaderDb2Config {
        host: config.host.clone(),
        port: config.port,
        database: config.database.clone(),
        username: credentials.username.clone(),
        password: credentials.password.clone(),
        ..LoaderDb2Config::default()
    }
}

fn parse_decimal_type(sql_type: &str) -> Option<DataType> {
    let open = sql_type.find('(')?;
    let close = sql_type.rfind(')')?;
    let inner = &sql_type[open + 1..close];
    let mut parts = inner.split(',').map(str::trim);
    let precision = parts.next()?.parse::<u8>().ok()?;
    let scale = parts
        .next()
        .and_then(|value| value.parse::<u8>().ok())
        .unwrap_or(0);
    Some(DataType::Decimal { precision, scale })
}

fn target_column_to_data_type(sql_type: &str) -> DataType {
    let normalized = sql_type.trim().to_ascii_uppercase();

    if normalized.starts_with("DECIMAL(") || normalized.starts_with("NUMERIC(") {
        return parse_decimal_type(&normalized).unwrap_or(DataType::Decimal {
            precision: 18,
            scale: 2,
        });
    }

    if normalized.starts_with("VARCHAR")
        || normalized.starts_with("CHAR")
        || normalized == "TEXT"
        || normalized == "CLOB"
        || normalized == "STRING"
    {
        return DataType::String;
    }

    if normalized == "INTEGER" || normalized == "INT" {
        return DataType::Integer;
    }

    if normalized == "BIGINT" {
        return DataType::BigInt;
    }

    if normalized == "REAL" || normalized == "FLOAT" {
        return DataType::Float;
    }

    if normalized == "DOUBLE" || normalized == "DOUBLE PRECISION" {
        return DataType::Double;
    }

    if normalized == "SMALLINT" || normalized == "BOOLEAN" || normalized == "BOOL" {
        return DataType::Boolean;
    }

    if normalized == "DATE" {
        return DataType::Date;
    }

    if normalized == "TIME" {
        return DataType::Time;
    }

    if normalized == "TIMESTAMP" || normalized == "DATETIME" {
        return DataType::DateTime;
    }

    if normalized == "BLOB" || normalized == "BINARY" || normalized == "VARBINARY" {
        return DataType::Binary;
    }

    if normalized == "JSON" || normalized == "JSONB" {
        return DataType::Json;
    }

    DataType::String
}

fn record_schema_for_table(table_config: &TargetTableConfig, create_tables: bool) -> RecordSchema {
    let mut fields = table_config
        .columns
        .values()
        .map(|column| FieldSchema {
            name: column.name.clone(),
            data_type: target_column_to_data_type(&column.data_type),
            nullable: column.nullable,
            description: None,
            metadata: HashMap::new(),
        })
        .collect::<Vec<_>>();

    fields.sort_by(|left, right| left.name.cmp(&right.name));

    let mut metadata = HashMap::new();
    metadata.insert(
        "create_table_if_not_exists".to_string(),
        serde_json::Value::Bool(create_tables),
    );

    RecordSchema { fields, metadata }
}

fn load_mode_for_table(table_config: &TargetTableConfig) -> (LoadMode, Vec<String>) {
    let key_fields = target_table_key_fields(table_config).unwrap_or_default();
    let mode = if key_fields.is_empty() {
        LoadMode::Insert
    } else {
        LoadMode::Upsert
    };
    (mode, key_fields)
}

pub async fn load_unified_session_to_db2(
    catalog: Arc<dyn DataSourceCatalog>,
    secret_store_registry: Option<Arc<graphica_core::secrets::providers::SecretStoreRegistry>>,
    session: &crate::mapping::multi_source::UnifiedMappingSession,
    source_rows: &HashMap<String, Vec<crate::mapping::loader::SourceRow>>,
    create_tables: bool,
    validate_data: bool,
    batch_size: usize,
) -> Result<MappingLoadResult> {
    if session.target_database.datasource_id.trim().is_empty() {
        return Err(anyhow::anyhow!(
            "Unified DB2 load requires target_database.datasource_id"
        ));
    }

    let (target_source, credentials) = resolve_target_datasource_from_catalog(
        catalog,
        secret_store_registry,
        &session.target_database.datasource_id,
    )
    .await?;

    let db2_config = match &target_source.connection.config {
        graphica_core::catalog::types::SourceConfig::DB2(config) => config,
        other => {
            return Err(anyhow::anyhow!(
                "Unified DB2 load requires a DB2 target datasource, found {:?}",
                other
            ));
        }
    };

    let pool = create_db2_pool(DB2PoolConfig {
        db2_config: build_loader_db2_config(db2_config, &credentials),
        max_size: 4,
        timeouts: PoolTimeouts::default(),
        health_check_enabled: true,
    })
    .await
    .context("Failed to create DB2 pool for unified session")?;

    let pool = Arc::new(pool);
    let mut rows_processed = 0usize;
    let mut rows_inserted = 0usize;
    let mut errors = Vec::new();

    for (table_name, table_config) in &session.target_database.tables {
        let qualified_table_name = qualified_db2_table_name(session, table_name);

        let records =
            match materialize_target_records(session, source_rows, table_name, validate_data) {
                Ok(records) => records,
                Err(error) => {
                    errors.push(error.to_string());
                    continue;
                }
            };

        rows_processed += records.len();
        if records.is_empty() {
            continue;
        }

        let mut destination = Db2Destination::new(pool.clone(), qualified_table_name.clone());
        let (mode, key_fields) = load_mode_for_table(table_config);
        let schema = record_schema_for_table(table_config, create_tables);
        let load_config = LoadConfig {
            mode,
            batch_size: batch_size.max(1),
            key_fields,
            parallelism: 1,
            error_tolerance: ErrorTolerance::default(),
            checkpoint_interval: None,
        };

        if let Err(error) = destination.prepare(&schema, &load_config).await {
            errors.push(format!(
                "Failed to prepare DB2 target '{}': {}",
                qualified_table_name, error
            ));
            let _ = destination.rollback().await;
            continue;
        }

        let record_stream = records.into_iter().map(|record| {
            Ok(DataRecord {
                data: serde_json::Value::Object(record),
                schema: None,
                source_location: None,
                metadata: HashMap::new(),
            })
        });

        let load_result = destination
            .load_stream(Box::pin(stream::iter(record_stream)), &load_config)
            .await;

        match load_result {
            Ok(stats) => {
                if let Err(error) = destination.finalize().await {
                    errors.push(format!(
                        "Failed to finalize DB2 target '{}': {}",
                        qualified_table_name, error
                    ));
                    let _ = destination.rollback().await;
                    continue;
                }

                rows_inserted += stats.records_loaded as usize + stats.records_updated as usize;
                if stats.records_failed > 0 {
                    errors.push(format!(
                        "DB2 load for '{}' reported {} failed records",
                        qualified_table_name, stats.records_failed
                    ));
                }
            }
            Err(error) => {
                let _ = destination.rollback().await;
                errors.push(format!(
                    "Failed to load unified table '{}' to DB2: {}",
                    qualified_table_name, error
                ));
            }
        }
    }

    Ok(MappingLoadResult {
        rows_processed,
        rows_inserted,
        rows_skipped: rows_processed.saturating_sub(rows_inserted),
        errors,
        lineage_graph_uri: format!("http://graphica.io/load/{}", session.id),
    })
}
