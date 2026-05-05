//! Shared SAP HANA normalization, connection resolution, and value coercion helpers.

use super::oracle_runtime::{
    apply_credentials_to_connection_string, normalize_optional_string, odbc_driver_registered,
    odbc_dsn_registered,
};
use super::types::SAPHANAConfig;
use crate::errors::GraphicaError;
use serde_json::{Number, Value};
use std::collections::HashMap;

pub const DEFAULT_HANA_ODBC_DRIVER: &str = "HDBODBC";
const HANA_ODBC_DRIVER_ENV: &str = "GRAPHICA_HANA_ODBC_DRIVER";
const HANA_ODBC_DSN_ENV: &str = "GRAPHICA_HANA_ODBC_DSN";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HanaConnectionParams {
    pub host: String,
    pub port: u16,
    pub database: String,
    pub schema: Option<String>,
    pub instance_number: Option<String>,
}

impl HanaConnectionParams {
    pub fn normalize(&mut self) {
        self.host = self.host.trim().to_string();
        self.database = self.database.trim().to_string();
        self.schema = normalize_optional_string(self.schema.as_deref());
        self.instance_number = normalize_optional_string(self.instance_number.as_deref());
    }

    pub fn normalized(&self) -> Self {
        let mut normalized = self.clone();
        normalized.normalize();
        normalized
    }

    pub fn server_node(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}

impl From<&SAPHANAConfig> for HanaConnectionParams {
    fn from(config: &SAPHANAConfig) -> Self {
        Self {
            host: config.host.clone(),
            port: config.port,
            database: config.database.clone(),
            schema: config.schema.clone(),
            instance_number: config.instance_number.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HanaOdbcResolution {
    pub raw_connection_string: Option<String>,
    pub driver: String,
    pub dsn: Option<String>,
    pub options: Option<String>,
    pub server_node: String,
    pub database_name: String,
    pub schema: Option<String>,
    pub instance_number: Option<String>,
    pub driver_registered: Option<bool>,
    pub dsn_registered: Option<bool>,
}

impl HanaOdbcResolution {
    pub fn build_connection_string(&self, username: &str, password: &str) -> String {
        if let Some(raw) = &self.raw_connection_string {
            return apply_credentials_to_connection_string(raw, username, password);
        }

        let mut connection_string = if let Some(dsn) = &self.dsn {
            format!("DSN={};UID={};PWD={}", dsn, username, password)
        } else {
            format!(
                "DRIVER={{{}}};SERVERNODE={};UID={};PWD={};DATABASENAME={};",
                self.driver, self.server_node, username, password, self.database_name
            )
        };

        if let Some(options) = &self.options {
            if !options.is_empty() {
                if !connection_string.ends_with(';') {
                    connection_string.push(';');
                }
                connection_string.push_str(options);
            }
        }

        connection_string
    }
}

pub fn resolve_hana_odbc_resolution(
    params: &HanaConnectionParams,
    metadata: &HashMap<String, String>,
) -> Result<HanaOdbcResolution, GraphicaError> {
    let normalized = params.normalized();

    if normalized.host.is_empty() {
        return Err(GraphicaError::Configuration(
            "SAP HANA configuration requires a non-empty host".to_string(),
        ));
    }
    if normalized.database.is_empty() {
        return Err(GraphicaError::Configuration(
            "SAP HANA configuration requires a non-empty database".to_string(),
        ));
    }

    let raw_connection_string = metadata_value(metadata, "odbc_connection_string");
    let dsn = metadata_value(metadata, "odbc_dsn")
        .or_else(|| normalize_optional_string(std::env::var(HANA_ODBC_DSN_ENV).ok().as_deref()));
    let driver = metadata_value(metadata, "odbc_driver")
        .or_else(|| normalize_optional_string(std::env::var(HANA_ODBC_DRIVER_ENV).ok().as_deref()))
        .unwrap_or_else(|| DEFAULT_HANA_ODBC_DRIVER.to_string());
    let options = metadata_value(metadata, "odbc_options");
    let driver_registered = if raw_connection_string.is_some() || dsn.is_some() {
        None
    } else {
        odbc_driver_registered(&driver)
    };
    let dsn_registered = dsn.as_deref().and_then(odbc_dsn_registered);

    Ok(HanaOdbcResolution {
        raw_connection_string,
        driver,
        dsn,
        options,
        server_node: normalized.server_node(),
        database_name: normalized.database,
        schema: normalized.schema,
        instance_number: normalized.instance_number,
        driver_registered,
        dsn_registered,
    })
}

pub fn coerce_hana_scalar(text: &str, data_type: &str) -> Value {
    let normalized_type = data_type.trim().to_ascii_uppercase();

    if matches!(
        normalized_type.as_str(),
        "BIT" | "BOOLEAN" | "BOOL" | "TINYINT(1)"
    ) {
        return parse_bool(text)
            .map(Value::Bool)
            .unwrap_or_else(|| Value::String(text.to_string()));
    }

    if is_numeric_type_name(&normalized_type) {
        return parse_json_number(text)
            .map(Value::Number)
            .unwrap_or_else(|| Value::String(text.to_string()));
    }

    Value::String(text.to_string())
}

fn metadata_value(metadata: &HashMap<String, String>, key: &str) -> Option<String> {
    normalize_optional_string(metadata.get(key).map(String::as_str))
}

fn is_numeric_type_name(data_type: &str) -> bool {
    data_type == "INTEGER"
        || data_type == "SMALLINT"
        || data_type == "BIGINT"
        || data_type == "TINYINT"
        || data_type == "REAL"
        || data_type == "DOUBLE"
        || data_type.starts_with("FLOAT")
        || data_type.starts_with("NUMERIC")
        || data_type.starts_with("DECIMAL")
}

fn parse_json_number(text: &str) -> Option<Number> {
    let trimmed = text.trim();
    trimmed
        .parse::<Number>()
        .ok()
        .or_else(|| trimmed.parse::<i64>().ok().map(Number::from))
        .or_else(|| trimmed.parse::<u64>().ok().map(Number::from))
        .or_else(|| {
            trimmed
                .parse::<f64>()
                .ok()
                .and_then(Number::from_f64)
        })
}

fn parse_bool(text: &str) -> Option<bool> {
    match text.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "t" | "yes" | "y" => Some(true),
        "0" | "false" | "f" | "no" | "n" => Some(false),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolution_builds_driver_connection_string() {
        let params = HanaConnectionParams {
            host: "hana.example.internal".to_string(),
            port: 30015,
            database: "HXE".to_string(),
            schema: Some("SAPABAP1".to_string()),
            instance_number: Some("00".to_string()),
        };

        let resolution = resolve_hana_odbc_resolution(&params, &HashMap::new()).unwrap();
        let conn = resolution.build_connection_string("demo", "secret");

        assert!(conn.contains("DRIVER={HDBODBC};"));
        assert!(conn.contains("SERVERNODE=hana.example.internal:30015;"));
        assert!(conn.contains("DATABASENAME=HXE;"));
        assert!(conn.contains("UID=demo;"));
        assert!(conn.contains("PWD=secret;"));
    }

    #[test]
    fn resolution_respects_raw_connection_string() {
        let params = HanaConnectionParams {
            host: "ignored".to_string(),
            port: 30015,
            database: "ignored".to_string(),
            schema: None,
            instance_number: None,
        };
        let metadata = HashMap::from([(
            "odbc_connection_string".to_string(),
            "DRIVER={HDBODBC};SERVERNODE=hana:30015;DATABASENAME=HXE;".to_string(),
        )]);

        let resolution = resolve_hana_odbc_resolution(&params, &metadata).unwrap();
        let conn = resolution.build_connection_string("demo", "secret");

        assert!(conn.contains("UID=demo"));
        assert!(conn.contains("PWD=secret"));
    }

    #[test]
    fn resolution_prefers_dsn_when_present() {
        let params = HanaConnectionParams {
            host: "hana".to_string(),
            port: 30015,
            database: "HXE".to_string(),
            schema: None,
            instance_number: None,
        };
        let metadata = HashMap::from([("odbc_dsn".to_string(), "hana_prod".to_string())]);

        let resolution = resolve_hana_odbc_resolution(&params, &metadata).unwrap();
        let conn = resolution.build_connection_string("demo", "secret");

        assert_eq!(resolution.dsn.as_deref(), Some("hana_prod"));
        assert!(conn.starts_with("DSN=hana_prod;UID=demo;PWD=secret"));
    }

    #[test]
    fn coerce_hana_scalar_preserves_numeric_types() {
        assert_eq!(coerce_hana_scalar("42", "INTEGER"), Value::from(42));
        assert_eq!(coerce_hana_scalar("10.50", "DECIMAL(15, 2)"), Value::from(10.5));
    }

    #[test]
    fn coerce_hana_scalar_preserves_boolean_types() {
        assert_eq!(coerce_hana_scalar("true", "BOOLEAN"), Value::Bool(true));
        assert_eq!(coerce_hana_scalar("0", "BIT"), Value::Bool(false));
    }

    #[test]
    fn coerce_hana_scalar_leaves_temporal_and_text_values_as_strings() {
        assert_eq!(
            coerce_hana_scalar("2026-05-01 12:00:00", "TIMESTAMP"),
            Value::String("2026-05-01 12:00:00".to_string())
        );
        assert_eq!(
            coerce_hana_scalar("A_SalesOrder", "NVARCHAR(40)"),
            Value::String("A_SalesOrder".to_string())
        );
    }
}
