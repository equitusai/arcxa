//! Shared Oracle normalization and ODBC runtime resolution helpers.

use super::types::OracleConfig;
use crate::errors::GraphicaError;
use std::collections::HashMap;
use std::process::Command;

pub const DEFAULT_ORACLE_ODBC_DRIVER: &str = "Oracle in OraClient19Home1";
const ORACLE_ODBC_DRIVER_ENV: &str = "GRAPHICA_ORACLE_ODBC_DRIVER";
const ORACLE_ODBC_DSN_ENV: &str = "GRAPHICA_ORACLE_ODBC_DSN";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OracleTargetKind {
    ServiceName,
    Sid,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OracleTarget {
    pub kind: OracleTargetKind,
    pub value: String,
}

impl OracleTarget {
    pub fn connection_type(&self) -> &'static str {
        match self.kind {
            OracleTargetKind::ServiceName => "service_name",
            OracleTargetKind::Sid => "sid",
        }
    }

    pub fn dbq(&self, host: &str, port: u16) -> String {
        match self.kind {
            OracleTargetKind::ServiceName => format!("//{}:{}/{}", host, port, self.value),
            OracleTargetKind::Sid => format!("{}:{}/{}", host, port, self.value),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OracleOdbcResolution {
    pub raw_connection_string: Option<String>,
    pub driver: String,
    pub dsn: Option<String>,
    pub options: Option<String>,
    pub target: OracleTarget,
    pub dbq: String,
    pub driver_registered: Option<bool>,
    pub dsn_registered: Option<bool>,
}

impl OracleOdbcResolution {
    pub fn build_connection_string(&self, username: &str, password: &str) -> String {
        if let Some(raw) = &self.raw_connection_string {
            return apply_credentials_to_connection_string(raw, username, password);
        }

        let mut connection_string = if let Some(dsn) = &self.dsn {
            format!("DSN={};UID={};PWD={}", dsn, username, password)
        } else {
            format!(
                "DRIVER={{{}}};DBQ={};UID={};PWD={};",
                self.driver, self.dbq, username, password
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

impl OracleConfig {
    pub fn normalize(&mut self) {
        self.service_name = normalize_optional_string(self.service_name.as_deref());
        self.sid = normalize_optional_string(self.sid.as_deref());
        self.schema = normalize_optional_string(self.schema.as_deref());
    }

    pub fn normalized(&self) -> Self {
        let mut normalized = self.clone();
        normalized.normalize();
        normalized
    }

    pub fn resolved_target(&self) -> Option<OracleTarget> {
        resolve_oracle_target(self.service_name.as_deref(), self.sid.as_deref())
    }
}

pub fn normalize_optional_string(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

pub fn resolve_oracle_target(
    service_name: Option<&str>,
    sid: Option<&str>,
) -> Option<OracleTarget> {
    if let Some(service_name) = normalize_optional_string(service_name) {
        return Some(OracleTarget {
            kind: OracleTargetKind::ServiceName,
            value: service_name,
        });
    }

    normalize_optional_string(sid).map(|sid| OracleTarget {
        kind: OracleTargetKind::Sid,
        value: sid,
    })
}

pub fn resolve_oracle_odbc_resolution(
    config: &OracleConfig,
    metadata: &HashMap<String, String>,
) -> Result<OracleOdbcResolution, GraphicaError> {
    let normalized = config.normalized();
    let target = normalized.resolved_target().ok_or_else(|| {
        GraphicaError::Configuration(
            "Oracle configuration requires a non-empty serviceName or sid".to_string(),
        )
    })?;

    let raw_connection_string = metadata_value(metadata, "odbc_connection_string");
    let dsn = metadata_value(metadata, "odbc_dsn")
        .or_else(|| normalize_optional_string(std::env::var(ORACLE_ODBC_DSN_ENV).ok().as_deref()));
    let driver = metadata_value(metadata, "odbc_driver")
        .or_else(|| {
            normalize_optional_string(std::env::var(ORACLE_ODBC_DRIVER_ENV).ok().as_deref())
        })
        .unwrap_or_else(|| DEFAULT_ORACLE_ODBC_DRIVER.to_string());
    let options = metadata_value(metadata, "odbc_options");
    let dbq = target.dbq(&normalized.host, normalized.port);
    let driver_registered = if raw_connection_string.is_some() || dsn.is_some() {
        None
    } else {
        odbc_driver_registered(&driver)
    };
    let dsn_registered = dsn.as_deref().and_then(odbc_dsn_registered);

    Ok(OracleOdbcResolution {
        raw_connection_string,
        driver,
        dsn,
        options,
        target,
        dbq,
        driver_registered,
        dsn_registered,
    })
}

pub fn apply_credentials_to_connection_string(
    connection_string: &str,
    username: &str,
    password: &str,
) -> String {
    let mut connection_string = connection_string.to_string();
    let upper = connection_string.to_uppercase();
    if !upper.contains("UID=") {
        if !connection_string.ends_with(';') {
            connection_string.push(';');
        }
        connection_string.push_str(&format!("UID={}", username));
    }
    if !upper.contains("PWD=") {
        if !connection_string.ends_with(';') {
            connection_string.push(';');
        }
        connection_string.push_str(&format!("PWD={}", password));
    }
    connection_string
}

pub fn odbc_driver_registered(driver: &str) -> Option<bool> {
    let registered_drivers = query_odbcinst("-d")?;
    Some(
        registered_drivers
            .iter()
            .any(|entry| entry.trim_matches(['[', ']']) == driver),
    )
}

pub fn odbc_dsn_registered(dsn: &str) -> Option<bool> {
    let registered_dsns = query_odbcinst("-s")?;
    Some(
        registered_dsns
            .iter()
            .any(|entry| entry.trim_matches(['[', ']']) == dsn),
    )
}

fn metadata_value(metadata: &HashMap<String, String>, key: &str) -> Option<String> {
    normalize_optional_string(metadata.get(key).map(String::as_str))
}

fn query_odbcinst(query_flag: &str) -> Option<Vec<String>> {
    let output = Command::new("odbcinst")
        .args(["-q", query_flag])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8(output.stdout).ok()?;
    Some(
        stdout
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(ToString::to_string)
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_oracle_config_clears_blank_optional_values() {
        let mut config = OracleConfig {
            host: "localhost".to_string(),
            port: 1521,
            service_name: Some("   ".to_string()),
            sid: Some(" XE ".to_string()),
            schema: Some("  ".to_string()),
        };

        config.normalize();

        assert_eq!(config.service_name, None);
        assert_eq!(config.sid, Some("XE".to_string()));
        assert_eq!(config.schema, None);
    }

    #[test]
    fn resolve_oracle_target_prefers_non_empty_service_name() {
        let target = resolve_oracle_target(Some(" ORCL "), Some("XE")).unwrap();

        assert_eq!(target.kind, OracleTargetKind::ServiceName);
        assert_eq!(target.value, "ORCL");
        assert_eq!(target.connection_type(), "service_name");
        assert_eq!(target.dbq("localhost", 1521), "//localhost:1521/ORCL");
    }

    #[test]
    fn resolve_oracle_target_falls_back_to_sid_when_service_name_blank() {
        let target = resolve_oracle_target(Some(""), Some(" XE ")).unwrap();

        assert_eq!(target.kind, OracleTargetKind::Sid);
        assert_eq!(target.value, "XE");
        assert_eq!(target.connection_type(), "sid");
        assert_eq!(target.dbq("localhost", 1521), "localhost:1521/XE");
    }

    #[test]
    fn resolve_oracle_odbc_resolution_uses_sid_when_service_name_blank() {
        let config = OracleConfig {
            host: "localhost".to_string(),
            port: 1521,
            service_name: Some(String::new()),
            sid: Some("XE".to_string()),
            schema: None,
        };

        let resolution = resolve_oracle_odbc_resolution(&config, &HashMap::new()).unwrap();

        assert_eq!(resolution.target.connection_type(), "sid");
        assert_eq!(resolution.dbq, "localhost:1521/XE");
    }

    #[test]
    fn apply_credentials_to_connection_string_adds_missing_uid_and_pwd() {
        let connection_string =
            apply_credentials_to_connection_string("DSN=OracleDemo", "demo_user", "secret");

        assert!(connection_string.contains("UID=demo_user"));
        assert!(connection_string.contains("PWD=secret"));
    }
}
