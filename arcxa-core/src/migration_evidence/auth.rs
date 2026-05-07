use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;

use crate::secrets::providers::SecretStoreRegistry;
use crate::secrets::{get_secret_by_ref, SecretValue};

use super::{ConnectorAuth, ConnectorAuthKind};

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ConnectorAuthResolutionMetadata {
    pub secret_ref: Option<String>,
    pub secret_store: Option<String>,
    pub secret_version: Option<String>,
    pub rotation_interval_days: Option<u32>,
    pub next_rotation: Option<DateTime<Utc>>,
    pub last_rotated: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
pub struct ResolvedConnectorAuth {
    pub auth: ConnectorAuth,
    pub metadata: ConnectorAuthResolutionMetadata,
}

pub async fn resolve_connector_auth(
    auth: &ConnectorAuth,
    secret_store_registry: Option<Arc<SecretStoreRegistry>>,
) -> Result<ResolvedConnectorAuth> {
    let Some(secret_ref) = auth
        .secret_ref
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    else {
        return Ok(ResolvedConnectorAuth {
            auth: auth.clone(),
            metadata: ConnectorAuthResolutionMetadata::default(),
        });
    };

    let registry = secret_store_registry.ok_or_else(|| {
        anyhow!(
            "connector auth secret_ref '{}' requires a configured secret store registry",
            secret_ref
        )
    })?;
    let store = registry
        .default()
        .or_else(|| registry.get("default"))
        .ok_or_else(|| anyhow!("no default secret store registered for '{}'", secret_ref))?;
    let secret = get_secret_by_ref(store.as_ref(), secret_ref, None)
        .await
        .with_context(|| format!("failed to resolve connector auth secret '{}'", secret_ref))?;

    let mut resolved = auth.clone();
    apply_secret_value_to_auth(&mut resolved, &secret.value)?;

    let rotation_policy = secret.metadata.rotation_policy.as_ref();
    Ok(ResolvedConnectorAuth {
        auth: resolved,
        metadata: ConnectorAuthResolutionMetadata {
            secret_ref: Some(secret_ref.to_string()),
            secret_store: Some(store.name().to_string()),
            secret_version: Some(secret.version),
            rotation_interval_days: rotation_policy.map(|policy| policy.interval_days),
            next_rotation: rotation_policy.and_then(|policy| policy.next_rotation),
            last_rotated: rotation_policy.and_then(|policy| policy.last_rotated),
        },
    })
}

fn apply_secret_value_to_auth(auth: &mut ConnectorAuth, secret: &SecretValue) -> Result<()> {
    match auth.kind {
        ConnectorAuthKind::None => Ok(()),
        ConnectorAuthKind::Bearer => {
            auth.token = Some(extract_secret_string(
                secret,
                &["token", "bearer_token", "access_token"],
            )?);
            Ok(())
        }
        ConnectorAuthKind::ApiKey => {
            auth.api_key = Some(extract_secret_string(secret, &["api_key", "key", "token"])?);
            if auth
                .header_name
                .as_deref()
                .unwrap_or_default()
                .trim()
                .is_empty()
            {
                auth.header_name =
                    extract_optional_secret_string(secret, &["header_name", "header"])
                        .or_else(|| Some("x-api-key".to_string()));
            }
            Ok(())
        }
        ConnectorAuthKind::Basic => {
            let (username, password) = extract_secret_credentials(secret)?;
            auth.username = Some(username);
            auth.password = Some(password);
            Ok(())
        }
    }
}

fn extract_secret_credentials(secret: &SecretValue) -> Result<(String, String)> {
    match secret {
        SecretValue::KeyValue(values) => Ok((
            values
                .get("username")
                .or_else(|| values.get("user"))
                .cloned()
                .ok_or_else(|| anyhow!("secret credentials require username"))?,
            values
                .get("password")
                .or_else(|| values.get("pass"))
                .cloned()
                .ok_or_else(|| anyhow!("secret credentials require password"))?,
        )),
        SecretValue::Json(Value::Object(values)) => Ok((
            values
                .get("username")
                .or_else(|| values.get("user"))
                .and_then(Value::as_str)
                .map(str::to_string)
                .ok_or_else(|| anyhow!("secret credentials require username"))?,
            values
                .get("password")
                .or_else(|| values.get("pass"))
                .and_then(Value::as_str)
                .map(str::to_string)
                .ok_or_else(|| anyhow!("secret credentials require password"))?,
        )),
        SecretValue::String(value) => {
            let mut parts = value.splitn(2, ':');
            let username = parts
                .next()
                .filter(|part| !part.is_empty())
                .ok_or_else(|| anyhow!("string credentials must use 'username:password'"))?;
            let password = parts
                .next()
                .filter(|part| !part.is_empty())
                .ok_or_else(|| anyhow!("string credentials must use 'username:password'"))?;
            Ok((username.to_string(), password.to_string()))
        }
        SecretValue::Binary(_) | SecretValue::Json(_) => Err(anyhow!(
            "secret credentials require username/password values"
        )),
    }
}

fn extract_secret_string(secret: &SecretValue, keys: &[&str]) -> Result<String> {
    extract_optional_secret_string(secret, keys)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| anyhow!("secret is missing any of {:?}", keys))
}

fn extract_optional_secret_string(secret: &SecretValue, keys: &[&str]) -> Option<String> {
    match secret {
        SecretValue::String(value) => Some(value.clone()),
        SecretValue::KeyValue(values) => keys.iter().find_map(|key| values.get(*key).cloned()),
        SecretValue::Json(Value::Object(values)) => keys
            .iter()
            .find_map(|key| values.get(*key).and_then(Value::as_str).map(str::to_string)),
        SecretValue::Json(_) | SecretValue::Binary(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secrets::providers::{InlineSecretStore, SecretStoreRegistry};
    use crate::secrets::types::RotationPolicy;
    use crate::secrets::{put_secret_by_ref, SecretMetadata};
    use std::collections::HashMap;

    #[tokio::test]
    async fn resolves_basic_credentials_from_secret_store() {
        let registry = Arc::new(SecretStoreRegistry::new());
        let store = Arc::new(InlineSecretStore::new());
        registry.register("default", store.clone());
        registry.set_default(store.clone());

        let mut creds = HashMap::new();
        creds.insert("username".to_string(), "demo".to_string());
        creds.insert("password".to_string(), "secret".to_string());

        put_secret_by_ref(
            store.as_ref(),
            "vault://migration/ecc/basic",
            SecretValue::KeyValue(creds),
            Some(SecretMetadata {
                rotation_policy: Some(RotationPolicy {
                    interval_days: 45,
                    last_rotated: None,
                    next_rotation: None,
                    auto_rotate: false,
                }),
                ..SecretMetadata::default()
            }),
        )
        .await
        .unwrap();

        let resolved = resolve_connector_auth(
            &ConnectorAuth {
                kind: ConnectorAuthKind::Basic,
                secret_ref: Some("vault://migration/ecc/basic".to_string()),
                token: None,
                api_key: None,
                header_name: None,
                username: None,
                password: None,
            },
            Some(registry),
        )
        .await
        .unwrap();

        assert_eq!(resolved.auth.username.as_deref(), Some("demo"));
        assert_eq!(resolved.auth.password.as_deref(), Some("secret"));
        assert_eq!(resolved.metadata.secret_store.as_deref(), Some("inline"));
        assert_eq!(resolved.metadata.rotation_interval_days, Some(45));
    }

    #[tokio::test]
    async fn resolves_bearer_token_from_json_secret() {
        let registry = Arc::new(SecretStoreRegistry::new());
        let store = Arc::new(InlineSecretStore::new());
        registry.register("default", store.clone());
        registry.set_default(store.clone());

        put_secret_by_ref(
            store.as_ref(),
            "vault://migration/ecc/token",
            SecretValue::Json(serde_json::json!({
                "access_token": "demo-token"
            })),
            None,
        )
        .await
        .unwrap();

        let resolved = resolve_connector_auth(
            &ConnectorAuth {
                kind: ConnectorAuthKind::Bearer,
                secret_ref: Some("vault://migration/ecc/token".to_string()),
                token: None,
                api_key: None,
                header_name: None,
                username: None,
                password: None,
            },
            Some(registry),
        )
        .await
        .unwrap();

        assert_eq!(resolved.auth.token.as_deref(), Some("demo-token"));
        assert!(resolved.metadata.secret_version.is_some());
    }
}
