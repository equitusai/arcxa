//! Shared Oracle connection helpers used by workflow readers and loaders.

use anyhow::{anyhow, Result};
use graphica_core::catalog::types::OracleConfig;
use graphica_core::catalog::Credentials;
use std::collections::HashMap;

use crate::workflows::domain::DatabaseConnectionConfig;

const DEFAULT_ORACLE_ODBC_DRIVER: &str = "Oracle in OraClient19Home1";

pub fn build_catalog_connection_string(
    config: &OracleConfig,
    credentials: &Credentials,
    metadata: &HashMap<String, String>,
) -> Result<String> {
    if let Some(raw) = metadata.get("odbc_connection_string") {
        return Ok(apply_credentials_to_connection_string(
            raw,
            &credentials.username,
            &credentials.password,
        ));
    }

    let driver = metadata
        .get("odbc_driver")
        .cloned()
        .or_else(|| std::env::var("GRAPHICA_ORACLE_ODBC_DRIVER").ok())
        .unwrap_or_else(|| DEFAULT_ORACLE_ODBC_DRIVER.to_string());

    let dsn = metadata
        .get("odbc_dsn")
        .cloned()
        .or_else(|| std::env::var("GRAPHICA_ORACLE_ODBC_DSN").ok());

    let mut conn = if let Some(dsn) = dsn {
        format!(
            "DSN={};UID={};PWD={}",
            dsn, credentials.username, credentials.password
        )
    } else {
        let dbq = build_dbq(
            &config.host,
            config.port,
            config.service_name.as_deref(),
            config.sid.as_deref(),
            None,
        )?;

        format!(
            "DRIVER={{{}}};DBQ={};UID={};PWD={};",
            driver, dbq, credentials.username, credentials.password
        )
    };

    append_odbc_options(&mut conn, metadata.get("odbc_options"));
    Ok(conn)
}

pub fn build_workflow_connection_string(
    connection_config: &DatabaseConnectionConfig,
) -> Result<String> {
    if let Some(raw) = connection_config.extra_params.get("odbc_connection_string") {
        return Ok(apply_credentials_to_connection_string(
            raw,
            &connection_config.username,
            &connection_config.password,
        ));
    }

    let driver = connection_config
        .extra_params
        .get("odbc_driver")
        .cloned()
        .or_else(|| std::env::var("GRAPHICA_ORACLE_ODBC_DRIVER").ok())
        .unwrap_or_else(|| DEFAULT_ORACLE_ODBC_DRIVER.to_string());

    let dsn = connection_config
        .extra_params
        .get("odbc_dsn")
        .cloned()
        .or_else(|| std::env::var("GRAPHICA_ORACLE_ODBC_DSN").ok());

    let service_name = connection_config
        .extra_params
        .get("service_name")
        .cloned()
        .or_else(|| connection_config.extra_params.get("serviceName").cloned())
        .or_else(|| {
            (!connection_config.database.is_empty()).then(|| connection_config.database.clone())
        });
    let sid = connection_config.extra_params.get("sid").cloned();

    let mut conn = if let Some(dsn) = dsn {
        format!(
            "DSN={};UID={};PWD={}",
            dsn, connection_config.username, connection_config.password
        )
    } else {
        let dbq = build_dbq(
            &connection_config.host,
            connection_config.port,
            service_name.as_deref(),
            sid.as_deref(),
            Some("Oracle connection config requires service_name, serviceName, sid, or a database value usable as the service name"),
        )?;

        format!(
            "DRIVER={{{}}};DBQ={};UID={};PWD={};",
            driver, dbq, connection_config.username, connection_config.password
        )
    };

    append_odbc_options(
        &mut conn,
        connection_config.extra_params.get("odbc_options"),
    );
    Ok(conn)
}

pub fn sanitize_connection_string(connection_string: &str) -> String {
    connection_string
        .split(';')
        .filter_map(|segment| {
            let trimmed = segment.trim();
            if trimmed.is_empty() {
                return None;
            }

            let upper = trimmed.to_uppercase();
            if upper.starts_with("PWD=") || upper.starts_with("PASSWORD=") {
                return Some(format!(
                    "{}=***",
                    trimmed.split('=').next().unwrap_or("PWD")
                ));
            }

            Some(trimmed.to_string())
        })
        .collect::<Vec<_>>()
        .join(";")
}

fn build_dbq(
    host: &str,
    port: u16,
    service_name: Option<&str>,
    sid: Option<&str>,
    missing_target_message: Option<&str>,
) -> Result<String> {
    if let Some(service_name) = service_name {
        Ok(format!("//{}:{}/{}", host, port, service_name))
    } else if let Some(sid) = sid {
        Ok(format!("{}:{}/{}", host, port, sid))
    } else {
        Err(anyhow!(
            "{}",
            missing_target_message.unwrap_or("Oracle configuration requires serviceName or sid")
        ))
    }
}

fn apply_credentials_to_connection_string(
    connection_string: &str,
    username: &str,
    password: &str,
) -> String {
    let mut conn = connection_string.to_string();
    let upper = conn.to_uppercase();
    if !upper.contains("UID=") {
        if !conn.ends_with(';') {
            conn.push(';');
        }
        conn.push_str(&format!("UID={}", username));
    }
    if !upper.contains("PWD=") {
        if !conn.ends_with(';') {
            conn.push(';');
        }
        conn.push_str(&format!("PWD={}", password));
    }
    conn
}

fn append_odbc_options(connection_string: &mut String, options: Option<&String>) {
    if let Some(options) = options {
        if !options.is_empty() {
            if !connection_string.ends_with(';') {
                connection_string.push(';');
            }
            connection_string.push_str(options);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workflow_builder_accepts_camel_case_service_name() {
        let mut extra_params = HashMap::new();
        extra_params.insert("serviceName".to_string(), "ORCLPDB1".to_string());

        let connection_config = DatabaseConnectionConfig {
            host: "oracle.example.com".to_string(),
            port: 1521,
            database: String::new(),
            username: "svc_arcxa".to_string(),
            password: "secret".to_string(),
            ssl_mode: None,
            extra_params,
        };

        let connection_string = build_workflow_connection_string(&connection_config).unwrap();
        assert!(connection_string.contains("DBQ=//oracle.example.com:1521/ORCLPDB1"));
        assert!(connection_string.contains("UID=svc_arcxa"));
        assert!(connection_string.contains("PWD=secret"));
    }

    #[test]
    fn sanitize_connection_string_redacts_password() {
        let sanitized = sanitize_connection_string(
            "DRIVER={Oracle in OraClient19Home1};DBQ=//oracle.example.com:1521/ORCL;UID=svc_arcxa;PWD=secret;",
        );

        assert!(sanitized.contains("UID=svc_arcxa"));
        assert!(sanitized.contains("PWD=***"));
        assert!(!sanitized.contains("secret"));
    }
}
