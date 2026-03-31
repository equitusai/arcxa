use std::borrow::Cow;

use crate::secrets::{Secret, SecretMetadata, SecretResult, SecretStore, SecretValue};

/// Convert a secret reference URI into the underlying secret store path/key.
pub fn secret_ref_to_store_path(secret_ref_or_path: &str) -> Cow<'_, str> {
    if let Some(path) = secret_ref_or_path.strip_prefix("vault://") {
        Cow::Borrowed(path)
    } else if let Some(path) = secret_ref_or_path.strip_prefix("aws://") {
        Cow::Borrowed(path)
    } else if let Some(path) = secret_ref_or_path.strip_prefix("env://") {
        Cow::Borrowed(path)
    } else {
        Cow::Borrowed(secret_ref_or_path)
    }
}

/// Retrieve a secret using the normalized store path, with compatibility
/// fallback to the original ref when the normalized lookup misses.
pub async fn get_secret_by_ref<S: SecretStore + ?Sized>(
    store: &S,
    secret_ref_or_path: &str,
    version: Option<&str>,
) -> SecretResult<Secret> {
    let normalized_path = secret_ref_to_store_path(secret_ref_or_path);
    match store.get_secret(normalized_path.as_ref(), version).await {
        Ok(secret) => Ok(secret),
        Err(err) if normalized_path.as_ref() != secret_ref_or_path => {
            match store.get_secret(secret_ref_or_path, version).await {
                Ok(secret) => Ok(secret),
                Err(_) => Err(err),
            }
        }
        Err(err) => Err(err),
    }
}

/// Check existence using the normalized store path, with compatibility
/// fallback to the original ref when the normalized lookup misses.
pub async fn secret_exists_by_ref<S: SecretStore + ?Sized>(
    store: &S,
    secret_ref_or_path: &str,
) -> SecretResult<bool> {
    let normalized_path = secret_ref_to_store_path(secret_ref_or_path);
    match store.exists(normalized_path.as_ref()).await {
        Ok(true) => Ok(true),
        Ok(false) if normalized_path.as_ref() != secret_ref_or_path => {
            store.exists(secret_ref_or_path).await
        }
        Ok(false) => Ok(false),
        Err(err) if normalized_path.as_ref() != secret_ref_or_path => {
            match store.exists(secret_ref_or_path).await {
                Ok(exists) => Ok(exists),
                Err(_) => Err(err),
            }
        }
        Err(err) => Err(err),
    }
}

/// Store a secret using the normalized store path.
pub async fn put_secret_by_ref<S: SecretStore + ?Sized>(
    store: &S,
    secret_ref_or_path: &str,
    secret: SecretValue,
    metadata: Option<SecretMetadata>,
) -> SecretResult<String> {
    let normalized_path = secret_ref_to_store_path(secret_ref_or_path);
    store
        .put_secret(normalized_path.as_ref(), secret, metadata)
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secrets::providers::InlineSecretStore;
    use std::collections::HashMap;

    #[test]
    fn test_secret_ref_to_store_path_vault() {
        assert_eq!(
            secret_ref_to_store_path("vault://datasources/postgres-prod/credentials"),
            "datasources/postgres-prod/credentials"
        );
    }

    #[test]
    fn test_secret_ref_to_store_path_aws() {
        assert_eq!(
            secret_ref_to_store_path("aws://my-db-secret"),
            "my-db-secret"
        );
    }

    #[test]
    fn test_secret_ref_to_store_path_env() {
        assert_eq!(
            secret_ref_to_store_path("env://DB_CREDENTIALS"),
            "DB_CREDENTIALS"
        );
    }

    #[test]
    fn test_secret_ref_to_store_path_direct() {
        assert_eq!(
            secret_ref_to_store_path("datasources/postgres-prod/credentials"),
            "datasources/postgres-prod/credentials"
        );
    }

    #[tokio::test]
    async fn test_put_secret_by_ref_stores_normalized_path() {
        let store = InlineSecretStore::new();
        let mut creds = HashMap::new();
        creds.insert("username".to_string(), "demo_user".to_string());
        creds.insert("password".to_string(), "demo_pass".to_string());

        put_secret_by_ref(
            &store,
            "vault://datasources/oracle-d8yg/credentials",
            SecretValue::KeyValue(creds),
            None,
        )
        .await
        .unwrap();

        assert!(store
            .exists("datasources/oracle-d8yg/credentials")
            .await
            .unwrap());
    }

    #[tokio::test]
    async fn test_get_secret_by_ref_falls_back_to_legacy_raw_key() {
        let store = InlineSecretStore::new();
        let mut creds = HashMap::new();
        creds.insert("username".to_string(), "demo_user".to_string());
        creds.insert("password".to_string(), "demo_pass".to_string());

        store
            .put_secret(
                "vault://credentials/oracle-d8yg",
                SecretValue::KeyValue(creds),
                None,
            )
            .await
            .unwrap();

        let secret = get_secret_by_ref(&store, "vault://credentials/oracle-d8yg", None)
            .await
            .unwrap();

        match secret.value {
            SecretValue::KeyValue(values) => {
                assert_eq!(
                    values.get("username").map(String::as_str),
                    Some("demo_user")
                );
            }
            other => panic!("unexpected secret value: {other:?}"),
        }
    }
}
