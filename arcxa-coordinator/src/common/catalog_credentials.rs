//! Shared datasource credential resolution for coordinator modules.

use anyhow::{anyhow, Context, Result};
use graphica_core::catalog::{connector::Credentials, types::DataSource};
use graphica_core::secrets::providers::SecretStoreRegistry;
use graphica_core::secrets::{get_secret_by_ref, SecretValue};
use std::collections::HashMap;
use std::sync::Arc;

pub async fn resolve_catalog_credentials(
    source: &DataSource,
    registry: Option<Arc<SecretStoreRegistry>>,
) -> Result<Credentials> {
    if !source.connection.credentials.is_empty() {
        return credentials_from_map(&source.connection.credentials, "connection.credentials");
    }

    if let Some(registry) = registry {
        if !source.connection.secret_ref.trim().is_empty() {
            let store = registry
                .default()
                .or_else(|| registry.get("default"))
                .ok_or_else(|| anyhow!("No default secret store configured"))?;

            let secret = get_secret_by_ref(store.as_ref(), &source.connection.secret_ref, None)
                .await
                .with_context(|| {
                    format!(
                        "Failed to resolve secretRef '{}' for datasource '{}'",
                        source.connection.secret_ref, source.id
                    )
                })?;

            return credentials_from_secret_value(&secret.value);
        }
    }

    if !source.metadata.is_empty() {
        return credentials_from_map(&source.metadata, "metadata");
    }

    Err(anyhow!(
        "Missing credentials for datasource {} (no secretRef credentials available)",
        source.id
    ))
}

fn credentials_from_secret_value(value: &SecretValue) -> Result<Credentials> {
    match value {
        SecretValue::KeyValue(map) => credentials_from_map(map, "secret value"),
        SecretValue::String(raw) => credentials_from_json_str(raw),
        SecretValue::Json(json) => credentials_from_json_value(json),
        SecretValue::Binary(_) => Err(anyhow!(
            "Binary secret values are not supported for datasource credentials"
        )),
    }
}

fn credentials_from_json_str(raw: &str) -> Result<Credentials> {
    let value: serde_json::Value = serde_json::from_str(raw)
        .map_err(|e| anyhow!("Failed to parse secret JSON credentials: {}", e))?;
    credentials_from_json_value(&value)
}

fn credentials_from_json_value(value: &serde_json::Value) -> Result<Credentials> {
    let obj = value
        .as_object()
        .ok_or_else(|| anyhow!("Credentials JSON must be an object with credential fields"))?;

    let mut map = HashMap::new();
    for (k, v) in obj {
        if let Some(s) = v.as_str() {
            map.insert(k.clone(), s.to_string());
        } else {
            map.insert(k.clone(), v.to_string());
        }
    }

    credentials_from_map(&map, "credentials JSON")
}

fn credentials_from_map(map: &HashMap<String, String>, context: &str) -> Result<Credentials> {
    let (username, password) =
        if let (Some(user), Some(pass)) = (map.get("username"), map.get("password")) {
            (user.to_string(), pass.to_string())
        } else if let (Some(user), Some(pass)) = (map.get("user"), map.get("pass")) {
            (user.to_string(), pass.to_string())
        } else {
            return Err(anyhow!(
                "Missing credentials in {} (expected username/password or user/pass)",
                context
            ));
        };

    let mut credentials = Credentials::new(username, password);
    for (k, v) in map {
        if matches!(k.as_str(), "username" | "password" | "user" | "pass") {
            continue;
        }
        credentials.additional.insert(k.clone(), v.clone());
    }

    Ok(credentials)
}
