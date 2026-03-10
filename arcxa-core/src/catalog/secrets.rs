//! Secret Provider Interface
//!
//! Abstracts secret retrieval from various backends (Vault, AWS, Azure, etc.)

use super::connector::Credentials;
use crate::errors::GraphicaError;
use async_trait::async_trait;
use std::collections::HashMap;

/// Result type for secret operations
pub type SecretResult<T> = Result<T, GraphicaError>;

/// Secret provider interface
///
/// Implementations retrieve credentials from secret stores.
#[async_trait]
pub trait SecretProvider: Send + Sync {
    /// Provider name (e.g., "Vault", "AWS Secrets Manager")
    fn name(&self) -> &'static str;

    /// Resolve secret reference to credentials
    ///
    /// # Arguments
    /// * `secret_ref` - Secret reference URI (e.g., "vault://secrets/db/prod")
    ///
    /// # Returns
    /// Credentials with username, password, and additional fields
    async fn resolve_secret(&self, secret_ref: &str) -> SecretResult<Credentials>;

    /// Check if provider supports this secret reference
    fn supports(&self, secret_ref: &str) -> bool;
}

/// In-memory secret provider (for testing)
pub struct InMemorySecretProvider {
    secrets: HashMap<String, Credentials>,
}

impl InMemorySecretProvider {
    pub fn new() -> Self {
        Self {
            secrets: HashMap::new(),
        }
    }

    pub fn add_secret(&mut self, secret_ref: String, credentials: Credentials) {
        self.secrets.insert(secret_ref, credentials);
    }
}

impl Default for InMemorySecretProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SecretProvider for InMemorySecretProvider {
    fn name(&self) -> &'static str {
        "InMemory"
    }

    async fn resolve_secret(&self, secret_ref: &str) -> SecretResult<Credentials> {
        self.secrets
            .get(secret_ref)
            .cloned()
            .ok_or_else(|| GraphicaError::NotFound(format!("Secret not found: {}", secret_ref)))
    }

    fn supports(&self, _secret_ref: &str) -> bool {
        true
    }
}

/// Environment variable secret provider
///
/// Resolves secrets from environment variables.
/// Format: "env://VAR_NAME" retrieves credentials from JSON in $VAR_NAME
pub struct EnvSecretProvider;

impl EnvSecretProvider {
    pub fn new() -> Self {
        Self
    }
}

impl Default for EnvSecretProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SecretProvider for EnvSecretProvider {
    fn name(&self) -> &'static str {
        "Environment"
    }

    async fn resolve_secret(&self, secret_ref: &str) -> SecretResult<Credentials> {
        if !secret_ref.starts_with("env://") {
            return Err(GraphicaError::Configuration(
                "Secret reference must start with env://".to_string(),
            ));
        }

        let var_name = &secret_ref[6..]; // Strip "env://"
        let value = std::env::var(var_name).map_err(|_| {
            GraphicaError::NotFound(format!("Environment variable not found: {}", var_name))
        })?;

        // Parse JSON with username/password
        let parsed: serde_json::Value = serde_json::from_str(&value).map_err(|e| {
            GraphicaError::Configuration(format!("Failed to parse credentials JSON: {}", e))
        })?;

        let username = parsed
            .get("username")
            .and_then(|v| v.as_str())
            .ok_or_else(|| GraphicaError::Configuration("Missing username field".to_string()))?
            .to_string();

        let password = parsed
            .get("password")
            .and_then(|v| v.as_str())
            .ok_or_else(|| GraphicaError::Configuration("Missing password field".to_string()))?
            .to_string();

        let mut credentials = Credentials::new(username, password);

        // Add any additional fields
        if let Some(obj) = parsed.as_object() {
            for (key, value) in obj {
                if key != "username" && key != "password" {
                    if let Some(str_val) = value.as_str() {
                        credentials = credentials.with_additional(key.clone(), str_val.to_string());
                    }
                }
            }
        }

        Ok(credentials)
    }

    fn supports(&self, secret_ref: &str) -> bool {
        secret_ref.starts_with("env://")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_in_memory_provider() {
        let mut provider = InMemorySecretProvider::new();
        let creds = Credentials::new("testuser".to_string(), "testpass".to_string());

        provider.add_secret("test://secret1".to_string(), creds.clone());

        let resolved = provider.resolve_secret("test://secret1").await.unwrap();
        assert_eq!(resolved.username, "testuser");
        assert_eq!(resolved.password, "testpass");
    }

    #[tokio::test]
    async fn test_in_memory_not_found() {
        let provider = InMemorySecretProvider::new();
        let result = provider.resolve_secret("nonexistent").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_env_provider_supports() {
        let provider = EnvSecretProvider::new();
        assert!(provider.supports("env://MY_VAR"));
        assert!(!provider.supports("vault://secret"));
    }

    #[tokio::test]
    async fn test_env_provider_parse() {
        std::env::set_var(
            "TEST_CREDS",
            r#"{"username":"user1","password":"pass1","api_key":"key123"}"#,
        );

        let provider = EnvSecretProvider::new();
        let creds = provider.resolve_secret("env://TEST_CREDS").await.unwrap();

        assert_eq!(creds.username, "user1");
        assert_eq!(creds.password, "pass1");
        assert_eq!(creds.additional.get("api_key"), Some(&"key123".to_string()));

        std::env::remove_var("TEST_CREDS");
    }
}
