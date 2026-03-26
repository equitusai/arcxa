//! Shared Oracle connection helpers used by workflow readers and loaders.

use anyhow::Result;
use graphica_core::catalog::types::OracleConfig;
use graphica_core::catalog::{resolve_oracle_odbc_resolution, Credentials};
use std::collections::HashMap;

use crate::workflows::domain::DatabaseConnectionConfig;

pub fn build_catalog_connection_string(
    config: &OracleConfig,
    credentials: &Credentials,
    metadata: &HashMap<String, String>,
) -> Result<String> {
    let resolution = resolve_oracle_odbc_resolution(config, metadata)?;
    Ok(resolution.build_connection_string(&credentials.username, &credentials.password))
}

pub fn build_workflow_connection_string(
    connection_config: &DatabaseConnectionConfig,
) -> Result<String> {
    let service_name = connection_config
        .extra_params
        .get("service_name")
        .cloned()
        .or_else(|| connection_config.extra_params.get("serviceName").cloned())
        .or_else(|| {
            (!connection_config.database.is_empty()).then(|| connection_config.database.clone())
        });
    let sid = connection_config.extra_params.get("sid").cloned();
    let oracle_config = OracleConfig {
        host: connection_config.host.clone(),
        port: connection_config.port,
        service_name,
        sid,
        schema: None,
    };

    let resolution =
        resolve_oracle_odbc_resolution(&oracle_config, &connection_config.extra_params)?;
    Ok(
        resolution
            .build_connection_string(&connection_config.username, &connection_config.password),
    )
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
    fn workflow_builder_falls_back_to_sid_when_service_name_blank() {
        let mut extra_params = HashMap::new();
        extra_params.insert("serviceName".to_string(), "   ".to_string());
        extra_params.insert("sid".to_string(), "XE".to_string());

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
        assert!(connection_string.contains("DBQ=oracle.example.com:1521/XE"));
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
