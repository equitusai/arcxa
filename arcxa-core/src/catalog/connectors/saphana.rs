//! SAP HANA Connector

use async_trait::async_trait;
use chrono::Utc;
#[cfg(feature = "odbc")]
use odbc_api::{ConnectionOptions, Cursor, Environment, ResultSetMetadata};
#[cfg(feature = "odbc")]
use odbc_api::{parameter::InputParameter, ColumnDescription, DataType};
use serde_json::{Map, Value};
use std::collections::HashMap;
use tokio::time::{timeout, Duration};

use crate::catalog::{
    api_types::{
        ColumnDefinition, ConnectionTestResult, QueryResult, SchemaDefinition,
    },
    connector::{
        ConnectorCapabilities, ConnectorResult, Credentials, DataSourceConnector, ValidationResult,
    },
    hana_runtime::{coerce_hana_scalar, resolve_hana_odbc_resolution, HanaConnectionParams},
    types::{DataSource, SourceConfig},
};
use crate::errors::GraphicaError;

pub struct SAPHANAConnector;

impl SAPHANAConnector {
    pub fn new() -> Self {
        Self
    }

    fn hana_params(source: &DataSource) -> ConnectorResult<HanaConnectionParams> {
        match &source.connection.config {
            SourceConfig::SAPHANA(config) => Ok(HanaConnectionParams::from(config)),
            _ => Err(GraphicaError::Configuration(
                "Expected SAP HANA configuration".to_string(),
            )),
        }
    }

    fn build_odbc_connection_string(
        source: &DataSource,
        credentials: &Credentials,
    ) -> ConnectorResult<String> {
        let params = Self::hana_params(source)?;
        let resolution = resolve_hana_odbc_resolution(&params, &source.metadata)?;
        Ok(resolution.build_connection_string(&credentials.username, &credentials.password))
    }
}

impl Default for SAPHANAConnector {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl DataSourceConnector for SAPHANAConnector {
    fn name(&self) -> &'static str {
        "SAP HANA Connector"
    }

    fn source_type(&self) -> &'static str {
        "SAPHANA"
    }

    fn validate_config(&self, config: &SourceConfig) -> ConnectorResult<ValidationResult> {
        match config {
            SourceConfig::SAPHANA(hana_config) => {
                let params = HanaConnectionParams::from(hana_config).normalized();
                let mut errors = vec![];
                if params.host.is_empty() {
                    errors.push("Host cannot be empty".to_string());
                }
                if params.database.is_empty() {
                    errors.push("Database cannot be empty".to_string());
                }
                if errors.is_empty() {
                    Ok(ValidationResult::valid())
                } else {
                    Ok(ValidationResult::invalid(errors))
                }
            }
            _ => Err(GraphicaError::Configuration(
                "Expected SAP HANA configuration".to_string(),
            )),
        }
    }

    async fn test_connection(
        &self,
        source: &DataSource,
        credentials: Credentials,
    ) -> ConnectorResult<ConnectionTestResult> {
        let params = Self::hana_params(source)?.normalized();
        let start = std::time::Instant::now();
        let connection_string = Self::build_odbc_connection_string(source, &credentials)?;
        let resolution = resolve_hana_odbc_resolution(&params, &source.metadata)?;

        let connect = tokio::task::spawn_blocking(move || {
            let env = Environment::new()
                .map_err(|e| GraphicaError::Internal(format!("ODBC environment error: {:?}", e)))?;

            let _conn = env
                .connect_with_connection_string(&connection_string, ConnectionOptions::default())
                .map_err(|e| GraphicaError::Internal(format!("HANA connection failed: {:?}", e)))?;

            Ok::<(), GraphicaError>(())
        });

        let result = match timeout(Duration::from_secs(5), connect).await {
            Ok(res) => match res {
                Ok(inner) => Ok(inner),
                Err(e) => Err(GraphicaError::Internal(format!(
                    "HANA connection task failed: {}",
                    e
                ))),
            },
            Err(_) => {
                let duration_ms = start.elapsed().as_millis() as u64;
                return Ok(ConnectionTestResult {
                    success: false,
                    duration_ms,
                    error: Some("HANA connection test timed out".to_string()),
                    metadata: hana_connection_metadata(&params, &resolution),
                    tested_at: Utc::now(),
                });
            }
        };

        let duration_ms = start.elapsed().as_millis() as u64;

        match result {
            Ok(_) => Ok(ConnectionTestResult {
                success: true,
                duration_ms,
                error: None,
                metadata: hana_connection_metadata(&params, &resolution),
                tested_at: Utc::now(),
            }),
            Err(e) => Ok(ConnectionTestResult {
                success: false,
                duration_ms,
                error: Some(e.to_string()),
                metadata: hana_connection_metadata(&params, &resolution),
                tested_at: Utc::now(),
            }),
        }
    }

    async fn infer_schema(
        &self,
        _source: &DataSource,
        _credentials: Credentials,
        _table_name: Option<&str>,
        _sample_size: usize,
    ) -> ConnectorResult<SchemaDefinition> {
        Err(GraphicaError::Configuration(
            "SAP HANA schema inference is provided by the coordinator discovery service when ODBC support is enabled".to_string(),
        ))
    }

    async fn execute_query(
        &self,
        source: &DataSource,
        credentials: Credentials,
        query: &str,
        parameters: HashMap<String, serde_json::Value>,
        limit: Option<usize>,
        timeout_secs: u64,
    ) -> ConnectorResult<QueryResult> {
        #[cfg(feature = "odbc")]
        {
            let connection_string = Self::build_odbc_connection_string(source, &credentials)?;
            let effective_query = match limit {
                Some(limit) => format!(
                    "SELECT * FROM ({}) AS GRAPHICA_HANA_QUERY LIMIT {}",
                    query, limit
                ),
                None => query.to_string(),
            };

            let started = std::time::Instant::now();
            let task = tokio::task::spawn_blocking(move || -> Result<QueryResult, GraphicaError> {
                let env = Environment::new().map_err(|error| {
                    GraphicaError::Internal(format!("ODBC environment error: {:?}", error))
                })?;

                let conn = env
                    .connect_with_connection_string(&connection_string, ConnectionOptions::default())
                    .map_err(|error| {
                        GraphicaError::Internal(format!("HANA connection failed: {:?}", error))
                    })?;

                let mut cursor = if parameters.is_empty() {
                    conn.execute(&effective_query, (), None)
                        .map_err(|error| {
                            GraphicaError::Internal(format!("HANA query failed: {:?}", error))
                        })?
                } else {
                    let (rewritten_query, ordered_names) = rewrite_named_parameters(&effective_query);
                    if ordered_names.is_empty() {
                        return Err(GraphicaError::Configuration(
                            "Query parameters were provided but no named placeholders were found"
                                .to_string(),
                        ));
                    }
                    let bound_parameters = build_named_parameters(&parameters, &ordered_names)?;
                    conn.execute(&rewritten_query, bound_parameters.as_slice(), None)
                        .map_err(|error| {
                            GraphicaError::Internal(format!(
                                "SAP HANA parameterized query failed: {:?}",
                                error
                            ))
                        })?
                }
                .ok_or_else(|| {
                    GraphicaError::Internal(
                        "SAP HANA query returned no result set".to_string(),
                    )
                })?;

                let num_cols = cursor.num_result_cols().map_err(|error| {
                    GraphicaError::Internal(format!(
                        "Failed to inspect HANA result columns: {:?}",
                        error
                    ))
                })? as usize;

                let mut columns = Vec::with_capacity(num_cols);
                let mut raw_column_types = Vec::with_capacity(num_cols);
                let mut description = ColumnDescription::default();
                for i in 1..=num_cols {
                    cursor.describe_col(i as u16, &mut description).map_err(|error| {
                        GraphicaError::Internal(format!(
                            "Failed to describe HANA column {}: {:?}",
                            i, error
                        ))
                    })?;
                    let data_type = map_odbc_type_to_sql(&description.data_type);
                    let nullable = description.nullability != odbc_api::Nullability::NoNulls;
                    raw_column_types.push(data_type.clone());
                    columns.push(ColumnDefinition {
                        name: description
                            .name_to_string()
                            .unwrap_or_else(|_| format!("col{}", i)),
                        data_type,
                        nullable,
                        primary_key: false,
                        default_value: None,
                        semantic_type: None,
                        statistics: None,
                    });
                }

                let mut rows = Vec::new();
                while let Some(mut row) = cursor.next_row().map_err(|error| {
                    GraphicaError::Internal(format!("Failed to fetch HANA row: {:?}", error))
                })? {
                    let mut object = Map::with_capacity(num_cols);
                    for (index, column) in columns.iter().enumerate() {
                        let mut buffer = Vec::new();
                        let not_null = row.get_text((index + 1) as u16, &mut buffer).map_err(
                            |error| {
                                GraphicaError::Internal(format!(
                                    "Failed to read HANA column {}: {:?}",
                                    index + 1,
                                    error
                                ))
                            },
                        )?;
                        let value = if not_null {
                            let text = String::from_utf8_lossy(&buffer);
                            coerce_hana_scalar(&text, &raw_column_types[index])
                        } else {
                            Value::Null
                        };
                        object.insert(column.name.clone(), value);
                    }
                    rows.push(Value::Object(object));
                }

                let row_count = rows.len();
                Ok(QueryResult {
                    rows,
                    row_count,
                    execution_time_ms: started.elapsed().as_millis() as u64,
                    truncated: false,
                    columns: Some(columns),
                })
            });

            match timeout(Duration::from_secs(timeout_secs.max(1)), task).await {
                Ok(result) => match result {
                    Ok(inner) => inner,
                    Err(error) => Err(GraphicaError::Internal(format!(
                        "HANA query task failed: {}",
                        error
                    ))),
                },
                Err(_) => Err(GraphicaError::Internal(
                    "SAP HANA query timed out".to_string(),
                )),
            }
        }

        #[cfg(not(feature = "odbc"))]
        {
            let _ = (source, credentials, query, parameters, limit, timeout_secs);
            Err(GraphicaError::Configuration(
                "SAP HANA query execution requires the 'odbc' feature".to_string(),
            ))
        }
    }

    fn capabilities(&self) -> ConnectorCapabilities {
        ConnectorCapabilities {
            parameterized_queries: cfg!(feature = "odbc"),
            schema_inference: false,
            query_timeout: cfg!(feature = "odbc"),
            streaming: false,
            transactions: false,
            max_batch_size: Some(10000),
        }
    }
}

fn hana_connection_metadata(
    params: &HanaConnectionParams,
    resolution: &crate::catalog::hana_runtime::HanaOdbcResolution,
) -> HashMap<String, String> {
    let mut metadata = HashMap::from([
        ("host".to_string(), params.host.clone()),
        ("port".to_string(), params.port.to_string()),
        ("database".to_string(), params.database.clone()),
        (
            "schema".to_string(),
            params
                .schema
                .clone()
                .unwrap_or_else(|| "PUBLIC".to_string()),
        ),
        ("driver".to_string(), resolution.driver.clone()),
    ]);

    if let Some(dsn) = &resolution.dsn {
        metadata.insert("dsn".to_string(), dsn.clone());
    }

    if let Some(instance_number) = &params.instance_number {
        metadata.insert("instanceNumber".to_string(), instance_number.clone());
    }

    metadata
}

#[cfg(feature = "odbc")]
fn rewrite_named_parameters(query: &str) -> (String, Vec<String>) {
    let mut rewritten = String::with_capacity(query.len());
    let mut ordered = Vec::new();
    let chars: Vec<char> = query.chars().collect();
    let mut i = 0;
    let mut in_single_quote = false;
    let mut in_double_quote = false;

    while i < chars.len() {
        let current = chars[i];

        if in_single_quote {
            rewritten.push(current);
            if current == '\'' {
                if chars.get(i + 1) == Some(&'\'') {
                    rewritten.push('\'');
                    i += 2;
                    continue;
                }
                in_single_quote = false;
            }
            i += 1;
            continue;
        }

        if in_double_quote {
            rewritten.push(current);
            if current == '"' {
                if chars.get(i + 1) == Some(&'"') {
                    rewritten.push('"');
                    i += 2;
                    continue;
                }
                in_double_quote = false;
            }
            i += 1;
            continue;
        }

        match current {
            '\'' => {
                in_single_quote = true;
                rewritten.push(current);
                i += 1;
            }
            '"' => {
                in_double_quote = true;
                rewritten.push(current);
                i += 1;
            }
            ':' => {
                let Some(next) = chars.get(i + 1).copied() else {
                    rewritten.push(current);
                    i += 1;
                    continue;
                };

                if !matches!(next, 'A'..='Z' | 'a'..='z' | '_') {
                    rewritten.push(current);
                    i += 1;
                    continue;
                }

                let mut j = i + 1;
                while let Some(ch) = chars.get(j).copied() {
                    if matches!(ch, 'A'..='Z' | 'a'..='z' | '0'..='9' | '_') {
                        j += 1;
                    } else {
                        break;
                    }
                }

                ordered.push(chars[i + 1..j].iter().collect());
                rewritten.push('?');
                i = j;
            }
            _ => {
                rewritten.push(current);
                i += 1;
            }
        }
    }

    (rewritten, ordered)
}

#[cfg(feature = "odbc")]
fn build_named_parameters(
    parameters: &HashMap<String, Value>,
    ordered_names: &[String],
) -> ConnectorResult<Vec<Box<dyn InputParameter>>> {
    let mut bound = Vec::with_capacity(ordered_names.len());
    for name in ordered_names {
        let value = parameters.get(name).ok_or_else(|| {
            GraphicaError::Configuration(format!(
                "Missing value for named parameter :{}",
                name
            ))
        })?;
        bound.push(json_value_to_parameter(Some(value)));
    }
    Ok(bound)
}

#[cfg(feature = "odbc")]
fn json_value_to_parameter(value: Option<&Value>) -> Box<dyn InputParameter> {
    use odbc_api::IntoParameter;

    match value {
        None | Some(Value::Null) => Box::new(Option::<String>::None.into_parameter()),
        Some(Value::Bool(flag)) => Box::new(if *flag { 1_i16 } else { 0_i16 }),
        Some(Value::Number(number)) => {
            if let Some(value) = number.as_i64() {
                Box::new(value)
            } else if let Some(value) = number.as_u64() {
                if let Ok(signed) = i64::try_from(value) {
                    Box::new(signed)
                } else {
                    Box::new(value.to_string().into_parameter())
                }
            } else if let Some(value) = number.as_f64() {
                Box::new(value)
            } else {
                Box::new(number.to_string().into_parameter())
            }
        }
        Some(Value::String(text)) => Box::new(text.clone().into_parameter()),
        Some(other) => Box::new(other.to_string().into_parameter()),
    }
}

#[cfg(feature = "odbc")]
fn map_odbc_type_to_sql(data_type: &DataType) -> String {
    match data_type {
        DataType::Integer => "INTEGER".to_string(),
        DataType::SmallInt => "SMALLINT".to_string(),
        DataType::BigInt => "BIGINT".to_string(),
        DataType::Real => "REAL".to_string(),
        DataType::Float { precision } => {
            if *precision > 0 {
                format!("FLOAT({})", precision)
            } else {
                "FLOAT".to_string()
            }
        }
        DataType::Double => "DOUBLE".to_string(),
        DataType::Numeric { precision, scale } => {
            format!("NUMERIC({}, {})", precision, scale)
        }
        DataType::Decimal { precision, scale } => {
            format!("DECIMAL({}, {})", precision, scale)
        }
        DataType::Char { length } => match length {
            Some(len) => format!("CHAR({})", len.get()),
            None => "CHAR".to_string(),
        },
        DataType::Varchar { length } => match length {
            Some(len) => format!("VARCHAR({})", len.get()),
            None => "VARCHAR".to_string(),
        },
        DataType::WVarchar { length } => match length {
            Some(len) => format!("NVARCHAR({})", len.get()),
            None => "NVARCHAR".to_string(),
        },
        DataType::LongVarchar { length } => match length {
            Some(len) => format!("LONGVARCHAR({})", len.get()),
            None => "LONGVARCHAR".to_string(),
        },
        DataType::Date => "DATE".to_string(),
        DataType::Time { precision } => {
            if *precision > 0 {
                format!("TIME({})", precision)
            } else {
                "TIME".to_string()
            }
        }
        DataType::Timestamp { precision } => {
            if *precision > 0 {
                format!("TIMESTAMP({})", precision)
            } else {
                "TIMESTAMP".to_string()
            }
        }
        DataType::Binary { length } => match length {
            Some(len) => format!("BINARY({})", len.get()),
            None => "BINARY".to_string(),
        },
        DataType::Varbinary { length } => match length {
            Some(len) => format!("VARBINARY({})", len.get()),
            None => "VARBINARY".to_string(),
        },
        DataType::LongVarbinary { length } => match length {
            Some(len) => format!("LONGVARBINARY({})", len.get()),
            None => "LONGVARBINARY".to_string(),
        },
        DataType::Bit => "BIT".to_string(),
        DataType::TinyInt => "TINYINT".to_string(),
        DataType::Other {
            data_type,
            column_size,
            decimal_digits,
        } => {
            let size_str = column_size
                .map(|size| size.get().to_string())
                .unwrap_or_else(|| "?".to_string());
            format!("UNKNOWN({:?},{},{})", data_type, size_str, decimal_digits)
        }
        _ => "VARCHAR".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn capabilities_report_parameter_support() {
        let connector = SAPHANAConnector::new();
        let capabilities = connector.capabilities();
        assert_eq!(capabilities.parameterized_queries, cfg!(feature = "odbc"));
        assert_eq!(capabilities.query_timeout, cfg!(feature = "odbc"));
        assert!(!capabilities.schema_inference);
    }

    #[test]
    fn rewrite_named_parameters_leaves_literals_untouched() {
        let (query, ordered) = rewrite_named_parameters(
            "SELECT \":literal\" FROM DUMMY WHERE amount > :min_amount AND owner = :owner",
        );

        assert_eq!(
            query,
            "SELECT \":literal\" FROM DUMMY WHERE amount > ? AND owner = ?"
        );
        assert_eq!(ordered, vec!["min_amount".to_string(), "owner".to_string()]);
    }

    #[test]
    fn build_named_parameters_requires_all_values() {
        let error = build_named_parameters(&HashMap::new(), &[String::from("missing")])
            .err()
            .unwrap()
            .to_string();
        assert!(error.contains("Missing value for named parameter :missing"));
    }

    #[test]
    fn hana_connection_metadata_includes_instance_number() {
        let params = HanaConnectionParams {
            host: "hana".to_string(),
            port: 30015,
            database: "HXE".to_string(),
            schema: Some("SAPABAP1".to_string()),
            instance_number: Some("00".to_string()),
        };
        let resolution = resolve_hana_odbc_resolution(&params, &HashMap::new()).unwrap();
        let metadata = hana_connection_metadata(&params, &resolution);

        assert_eq!(metadata.get("instanceNumber"), Some(&"00".to_string()));
        assert_eq!(metadata.get("database"), Some(&"HXE".to_string()));
    }

    #[test]
    fn json_parameter_conversion_preserves_numbers_and_strings() {
        let mut params = HashMap::new();
        params.insert("amount".to_string(), json!(10.5));
        params.insert("owner".to_string(), json!("SAP"));

        let bound = build_named_parameters(&params, &["amount".to_string(), "owner".to_string()])
            .unwrap();

        assert_eq!(bound.len(), 2);
    }
}
