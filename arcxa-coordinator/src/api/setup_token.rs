//! Secure Admin Setup with Setup Token
//!
//! Implements a secure mechanism for initial admin user creation.
//! Prevents unauthorized admin creation via setup tokens.

use anyhow::Result;
use chrono::{DateTime, Duration, Utc};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Setup token for secure admin initialization
#[derive(Clone, Debug)]
pub struct SetupToken {
    /// The actual token value (32 bytes, base64-encoded)
    pub token: String,
    /// When the token was generated
    pub created_at: DateTime<Utc>,
    /// When the token expires (default: 1 hour)
    pub expires_at: DateTime<Utc>,
    /// Whether the token has been used
    pub used: bool,
}

impl SetupToken {
    /// Generate a new cryptographically secure setup token
    pub fn generate() -> Self {
        use rand::Rng;

        let mut token_bytes = [0u8; 32];
        rand::thread_rng().fill(&mut token_bytes);

        let token = base64::encode(token_bytes);
        let created_at = Utc::now();
        let expires_at = created_at + Duration::hours(1);

        Self {
            token,
            created_at,
            expires_at,
            used: false,
        }
    }

    /// Check if token is valid (not expired, not used)
    pub fn is_valid(&self) -> bool {
        !self.used && Utc::now() < self.expires_at
    }

    /// Validate and consume the token
    pub fn consume(&mut self, provided_token: &str) -> Result<(), SetupTokenError> {
        if self.used {
            return Err(SetupTokenError::AlreadyUsed);
        }

        if Utc::now() >= self.expires_at {
            return Err(SetupTokenError::Expired);
        }

        // Constant-time comparison to prevent timing attacks
        if !constant_time_compare(&self.token, provided_token) {
            return Err(SetupTokenError::Invalid);
        }

        self.used = true;
        Ok(())
    }
}

/// Setup token errors
#[derive(Debug, thiserror::Error)]
pub enum SetupTokenError {
    #[error("Setup token has already been used")]
    AlreadyUsed,

    #[error("Setup token has expired")]
    Expired,

    #[error("Invalid setup token")]
    Invalid,

    #[error("Setup token not available (admin already exists)")]
    NotAvailable,
}

/// Constant-time string comparison to prevent timing attacks
fn constant_time_compare(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }

    let a_bytes = a.as_bytes();
    let b_bytes = b.as_bytes();

    let mut result = 0u8;
    for i in 0..a_bytes.len() {
        result |= a_bytes[i] ^ b_bytes[i];
    }

    result == 0
}

/// Setup token manager
pub struct SetupTokenManager {
    token: Arc<RwLock<Option<SetupToken>>>,
}

impl SetupTokenManager {
    /// Create a new setup token manager
    pub fn new() -> Self {
        Self {
            token: Arc::new(RwLock::new(None)),
        }
    }

    /// Generate and return a new setup token (only if no admin exists)
    pub async fn generate_token(&self) -> Result<SetupToken, SetupTokenError> {
        let mut token_guard = self.token.write().await;

        // Only generate if no token exists
        if token_guard.is_some() {
            return Err(SetupTokenError::NotAvailable);
        }

        let new_token = SetupToken::generate();
        *token_guard = Some(new_token.clone());

        Ok(new_token)
    }

    /// Validate and consume the setup token
    pub async fn validate_and_consume(&self, provided_token: &str) -> Result<(), SetupTokenError> {
        let mut token_guard = self.token.write().await;

        match token_guard.as_mut() {
            Some(token) => {
                token.consume(provided_token)?;
                Ok(())
            }
            None => Err(SetupTokenError::NotAvailable),
        }
    }

    /// Check if setup is available (token exists and is valid)
    pub async fn is_setup_available(&self) -> bool {
        let token_guard = self.token.read().await;
        match token_guard.as_ref() {
            Some(token) => token.is_valid(),
            None => false,
        }
    }

    /// Clear the token (after successful setup)
    pub async fn clear(&self) {
        let mut token_guard = self.token.write().await;
        *token_guard = None;
    }
}

impl Default for SetupTokenManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_setup_token_generation() {
        let token = SetupToken::generate();
        assert!(!token.token.is_empty());
        assert!(token.is_valid());
        assert!(!token.used);
    }

    #[test]
    fn test_token_consumption() {
        let mut token = SetupToken::generate();
        let token_value = token.token.clone();

        // Valid token should succeed
        assert!(token.consume(&token_value).is_ok());

        // Used token should fail
        assert!(matches!(
            token.consume(&token_value),
            Err(SetupTokenError::AlreadyUsed)
        ));
    }

    #[test]
    fn test_invalid_token() {
        let mut token = SetupToken::generate();

        // Wrong token should fail
        assert!(matches!(
            token.consume("wrong-token"),
            Err(SetupTokenError::Invalid)
        ));
    }

    #[test]
    fn test_token_expiration() {
        let mut token = SetupToken::generate();
        token.expires_at = Utc::now() - Duration::seconds(1);

        assert!(!token.is_valid());
        assert!(matches!(
            token.consume(&token.token.clone()),
            Err(SetupTokenError::Expired)
        ));
    }

    #[test]
    fn test_constant_time_compare() {
        let a = "test_token_abc123";
        let b = "test_token_abc123";
        let c = "test_token_xyz789";

        assert!(constant_time_compare(a, b));
        assert!(!constant_time_compare(a, c));
        assert!(!constant_time_compare(a, "short"));
    }

    #[tokio::test]
    async fn test_setup_token_manager() {
        let manager = SetupTokenManager::new();

        // Generate token
        let token = manager.generate_token().await.unwrap();
        assert!(manager.is_setup_available().await);

        // Validate and consume
        assert!(manager.validate_and_consume(&token.token).await.is_ok());

        // Should fail on second use
        assert!(manager.validate_and_consume(&token.token).await.is_err());
    }

    #[tokio::test]
    async fn test_manager_single_token() {
        let manager = SetupTokenManager::new();

        let token1 = manager.generate_token().await.unwrap();

        // Should not allow second token
        assert!(manager.generate_token().await.is_err());

        // Clear and try again
        manager.clear().await;
        let token2 = manager.generate_token().await.unwrap();

        assert_ne!(token1.token, token2.token);
    }
}
