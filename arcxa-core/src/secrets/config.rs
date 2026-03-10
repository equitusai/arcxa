//! Secret store configuration

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Secret store type
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SecretStoreType {
    /// Inline storage in metadata (dev only)
    Inline,
    /// Environment variables
    Env,
    /// File-based storage (local dev)
    File,
    /// HashiCorp Vault
    Vault,
    /// AWS Secrets Manager
    AwsSecretsManager,
    /// Azure Key Vault
    AzureKeyVault,
    /// Google Cloud Secret Manager
    GcpSecretManager,
}

/// Secret store configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretStoreConfig {
    /// Store type
    #[serde(rename = "type")]
    pub store_type: SecretStoreType,

    /// Enable caching
    #[serde(default)]
    pub enable_cache: bool,

    /// Cache TTL in seconds
    #[serde(default = "default_cache_ttl")]
    pub cache_ttl_seconds: u64,

    /// Provider-specific configuration
    #[serde(flatten)]
    pub provider_config: ProviderConfig,
}

fn default_cache_ttl() -> u64 {
    300 // 5 minutes
}

/// Provider-specific configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "provider")]
pub enum ProviderConfig {
    /// Inline provider config (no additional config needed)
    #[serde(rename = "inline")]
    Inline,

    /// Environment variable provider
    #[serde(rename = "env")]
    Env {
        /// Prefix for environment variables (e.g., "GRAPHICA_SECRET_")
        #[serde(default)]
        prefix: Option<String>,
    },

    /// File-based provider
    #[serde(rename = "file")]
    File {
        /// Directory path for secrets
        directory: String,
        /// File format (json, yaml, toml)
        #[serde(default = "default_file_format")]
        format: String,
    },

    /// HashiCorp Vault provider
    #[serde(rename = "vault")]
    Vault {
        /// Vault server address
        address: String,
        /// Authentication method
        auth: VaultAuth,
        /// KV mount path
        #[serde(default = "default_vault_mount")]
        mount_path: String,
        /// KV version (v1 or v2)
        #[serde(default = "default_vault_version")]
        kv_version: String,
        /// Namespace (for Vault Enterprise)
        #[serde(skip_serializing_if = "Option::is_none")]
        namespace: Option<String>,
    },

    /// AWS Secrets Manager provider
    #[serde(rename = "aws")]
    AwsSecretsManager {
        /// AWS region
        region: String,
        /// Optional endpoint URL (for LocalStack, etc.)
        #[serde(skip_serializing_if = "Option::is_none")]
        endpoint: Option<String>,
        /// Use IAM role for authentication
        #[serde(default)]
        use_iam_role: bool,
        /// Optional access key ID
        #[serde(skip_serializing_if = "Option::is_none")]
        access_key_id: Option<String>,
        /// Optional secret access key
        #[serde(skip_serializing_if = "Option::is_none")]
        secret_access_key: Option<String>,
    },

    /// Azure Key Vault provider
    #[serde(rename = "azure")]
    AzureKeyVault {
        /// Vault URL
        vault_url: String,
        /// Tenant ID
        tenant_id: String,
        /// Client ID (for service principal auth)
        #[serde(skip_serializing_if = "Option::is_none")]
        client_id: Option<String>,
        /// Client secret (for service principal auth)
        #[serde(skip_serializing_if = "Option::is_none")]
        client_secret: Option<String>,
        /// Use managed identity
        #[serde(default)]
        use_managed_identity: bool,
    },

    /// GCP Secret Manager provider
    #[serde(rename = "gcp")]
    GcpSecretManager {
        /// GCP project ID
        project_id: String,
        /// Optional service account key path
        #[serde(skip_serializing_if = "Option::is_none")]
        service_account_key: Option<String>,
        /// Use application default credentials
        #[serde(default = "default_true")]
        use_adc: bool,
    },
}

/// Vault authentication configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "method", rename_all = "snake_case")]
pub enum VaultAuth {
    /// Token authentication
    Token {
        /// Vault token
        token: String,
    },
    /// AppRole authentication
    AppRole {
        /// Role ID
        role_id: String,
        /// Secret ID
        secret_id: String,
    },
    /// Kubernetes authentication
    Kubernetes {
        /// Service account token path
        #[serde(default = "default_k8s_token_path")]
        token_path: String,
        /// Vault role
        role: String,
    },
    /// AWS IAM authentication
    AwsIam {
        /// Vault role
        role: String,
    },
}

fn default_file_format() -> String {
    "json".to_string()
}

fn default_vault_mount() -> String {
    "secret".to_string()
}

fn default_vault_version() -> String {
    "v2".to_string()
}

fn default_k8s_token_path() -> String {
    "/var/run/secrets/kubernetes.io/serviceaccount/token".to_string()
}

fn default_true() -> bool {
    true
}

impl Default for SecretStoreConfig {
    fn default() -> Self {
        Self {
            store_type: SecretStoreType::Inline,
            enable_cache: false,
            cache_ttl_seconds: default_cache_ttl(),
            provider_config: ProviderConfig::Inline,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = SecretStoreConfig::default();
        assert_eq!(config.store_type, SecretStoreType::Inline);
        assert_eq!(config.enable_cache, false);
    }

    #[test]
    fn test_vault_config_serialization() {
        let config = SecretStoreConfig {
            store_type: SecretStoreType::Vault,
            enable_cache: true,
            cache_ttl_seconds: 600,
            provider_config: ProviderConfig::Vault {
                address: "https://vault.example.com".to_string(),
                auth: VaultAuth::Token {
                    token: "s.xyz123".to_string(),
                },
                mount_path: "secret".to_string(),
                kv_version: "v2".to_string(),
                namespace: None,
            },
        };

        let json = serde_json::to_string_pretty(&config).unwrap();
        let parsed: SecretStoreConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.store_type, SecretStoreType::Vault);
    }
}
