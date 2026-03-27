//! Runtime ODBC inventory helpers for deployment diagnostics.

use graphica_core::catalog::DEFAULT_ORACLE_ODBC_DRIVER;
use std::process::Command;

const ORACLE_ODBC_DRIVER_ENV: &str = "GRAPHICA_ORACLE_ODBC_DRIVER";
const ORACLE_ODBC_DSN_ENV: &str = "GRAPHICA_ORACLE_ODBC_DSN";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OdbcRuntimeInventory {
    pub compiled: bool,
    pub odbcinst_available: Option<bool>,
    pub registered_drivers: Vec<String>,
    pub registered_dsns: Vec<String>,
    pub oracle_driver_override: Option<String>,
    pub oracle_dsn_override: Option<String>,
    pub default_oracle_driver: &'static str,
}

pub fn collect_odbc_runtime_inventory() -> OdbcRuntimeInventory {
    let oracle_driver_override = normalized_env(ORACLE_ODBC_DRIVER_ENV);
    let oracle_dsn_override = normalized_env(ORACLE_ODBC_DSN_ENV);

    if !cfg!(feature = "odbc") {
        return OdbcRuntimeInventory {
            compiled: false,
            odbcinst_available: None,
            registered_drivers: Vec::new(),
            registered_dsns: Vec::new(),
            oracle_driver_override,
            oracle_dsn_override,
            default_oracle_driver: DEFAULT_ORACLE_ODBC_DRIVER,
        };
    }

    let registered_drivers = query_odbcinst("-d");
    let registered_dsns = query_odbcinst("-s");
    let odbcinst_available = match (&registered_drivers, &registered_dsns) {
        (Some(_), _) | (_, Some(_)) => Some(true),
        (None, None) => Some(false),
    };

    OdbcRuntimeInventory {
        compiled: true,
        odbcinst_available,
        registered_drivers: registered_drivers.unwrap_or_default(),
        registered_dsns: registered_dsns.unwrap_or_default(),
        oracle_driver_override,
        oracle_dsn_override,
        default_oracle_driver: DEFAULT_ORACLE_ODBC_DRIVER,
    }
}

pub fn log_odbc_runtime_inventory() {
    let inventory = collect_odbc_runtime_inventory();

    if !inventory.compiled {
        tracing::info!(
            "ODBC runtime: connector family not compiled into this build (driverless profile)"
        );
        return;
    }

    tracing::info!(
        "ODBC runtime: enabled (registered drivers: {}, DSNs: {})",
        inventory.registered_drivers.len(),
        inventory.registered_dsns.len()
    );

    match inventory.odbcinst_available {
        Some(true) => {
            if inventory.registered_drivers.is_empty() {
                tracing::warn!(
                    "ODBC runtime: no registered drivers were found via odbcinst; Oracle/DB2/SAP HANA datasources will stay not ready until drivers are installed or mounted"
                );
            } else {
                tracing::info!(
                    "ODBC runtime drivers: {}",
                    inventory.registered_drivers.join(", ")
                );
            }

            if inventory.registered_dsns.is_empty() {
                tracing::info!("ODBC runtime DSNs: none registered");
            } else {
                tracing::info!(
                    "ODBC runtime DSNs: {}",
                    inventory.registered_dsns.join(", ")
                );
            }
        }
        Some(false) => {
            tracing::warn!(
                "ODBC runtime: 'odbcinst' is not available on this node; connector readiness can still be tested through connection attempts, but installed-driver inventory could not be enumerated"
            );
        }
        None => {}
    }

    if let Some(driver) = &inventory.oracle_driver_override {
        tracing::info!("Oracle ODBC override driver: {}", driver);
    } else {
        tracing::info!(
            "Oracle ODBC default driver: {}",
            inventory.default_oracle_driver
        );
    }

    if let Some(dsn) = &inventory.oracle_dsn_override {
        tracing::info!("Oracle ODBC override DSN: {}", dsn);
    }
}

fn normalized_env(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn query_odbcinst(flag: &str) -> Option<Vec<String>> {
    let output = Command::new("odbcinst").args(["-q", flag]).output().ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8(output.stdout).ok()?;
    Some(parse_odbcinst_entries(&stdout))
}

fn parse_odbcinst_entries(stdout: &str) -> Vec<String> {
    stdout
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|line| line.trim_matches(['[', ']']).to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::parse_odbcinst_entries;

    #[test]
    fn parses_odbcinst_wrapped_entries() {
        let parsed =
            parse_odbcinst_entries("[Oracle in OraClient19Home1]\n[IBM DB2 ODBC DRIVER]\n");

        assert_eq!(
            parsed,
            vec![
                "Oracle in OraClient19Home1".to_string(),
                "IBM DB2 ODBC DRIVER".to_string()
            ]
        );
    }

    #[test]
    fn ignores_blank_odbcinst_lines() {
        let parsed = parse_odbcinst_entries("\n[PostgreSQL Unicode]\n\n");
        assert_eq!(parsed, vec!["PostgreSQL Unicode".to_string()]);
    }
}
