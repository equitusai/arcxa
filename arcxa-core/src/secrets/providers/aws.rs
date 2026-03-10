//! AWS Secrets Manager store (stub - to be implemented)

use crate::secrets::{Secret, SecretError, SecretMetadata, SecretResult, SecretStore, SecretValue};
use async_trait::async_trait;

/// AWS Secrets Manager secret store
///
/// **Coming soon**: This provider will integrate with AWS Secrets Manager.
pub struct AwsSecretsManagerStore;

impl AwsSecretsManagerStore {
    pub fn new() -> Self {
        Self
    }
}

impl Default for AwsSecretsManagerStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SecretStore for AwsSecretsManagerStore {
    fn name(&self) -> &'static str {
        "aws_secrets_manager"
    }

    async fn put_secret(
        &self,
        _path: &str,
        _secret: SecretValue,
        _metadata: Option<SecretMetadata>,
    ) -> SecretResult<String> {
        Err(SecretError::Internal(
            "AwsSecretsManagerStore not yet implemented".to_string(),
        ))
    }

    async fn get_secret(&self, _path: &str, _version: Option<&str>) -> SecretResult<Secret> {
        Err(SecretError::Internal(
            "AwsSecretsManagerStore not yet implemented".to_string(),
        ))
    }

    async fn delete_secret(&self, _path: &str, _version: Option<&str>) -> SecretResult<()> {
        Err(SecretError::Internal(
            "AwsSecretsManagerStore not yet implemented".to_string(),
        ))
    }

    async fn list_secrets(&self, _prefix: Option<&str>) -> SecretResult<Vec<String>> {
        Err(SecretError::Internal(
            "AwsSecretsManagerStore not yet implemented".to_string(),
        ))
    }

    async fn exists(&self, _path: &str) -> SecretResult<bool> {
        Err(SecretError::Internal(
            "AwsSecretsManagerStore not yet implemented".to_string(),
        ))
    }

    async fn rotate_secret(&self, _path: &str, _new_secret: SecretValue) -> SecretResult<String> {
        Err(SecretError::Internal(
            "AwsSecretsManagerStore not yet implemented".to_string(),
        ))
    }

    async fn get_metadata(&self, _path: &str) -> SecretResult<SecretMetadata> {
        Err(SecretError::Internal(
            "AwsSecretsManagerStore not yet implemented".to_string(),
        ))
    }

    async fn health_check(&self) -> SecretResult<bool> {
        Ok(false)
    }
}
