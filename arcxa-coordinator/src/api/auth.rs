//! Authentication and Authorization Middleware
//!
//! Provides JWT-based authentication and role-based access control (RBAC)
//! for all API endpoints.

use axum::{
    extract::{Request, State},
    http::{header, StatusCode},
    middleware::Next,
    response::Response,
};
use chrono::{DateTime, Duration, Utc};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// JWT claims structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    /// Subject (user ID or service account ID)
    pub sub: String,
    /// Issued at timestamp
    pub iat: i64,
    /// Expiration timestamp
    pub exp: i64,
    /// Role for RBAC
    pub role: Role,
    /// Optional additional scopes
    #[serde(default)]
    pub scopes: Vec<String>,
}

/// User roles for RBAC
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    /// Full access to all operations
    Admin,
    /// Can execute workflows and view data
    Operator,
    /// Read-only access
    Viewer,
    /// Service account for automation
    Service,
}

impl Role {
    /// Check if role has permission for operation
    pub fn can_write(&self) -> bool {
        matches!(self, Role::Admin | Role::Operator | Role::Service)
    }

    pub fn can_admin(&self) -> bool {
        matches!(self, Role::Admin)
    }

    pub fn can_read(&self) -> bool {
        true // All roles can read
    }
}

/// Authentication configuration with secret rotation support
#[derive(Clone)]
pub struct AuthConfig {
    /// Current JWT encoding key (for signing new tokens)
    encoding_key: Arc<EncodingKey>,
    /// Current JWT decoding key (for validating tokens)
    decoding_key: Arc<DecodingKey>,
    /// Previous decoding key (for rotation overlap period)
    previous_decoding_key: Option<Arc<DecodingKey>>,
    /// Token expiration duration (default: 24 hours)
    pub token_expiry: Duration,
    /// Whether auth is enabled
    pub enabled: bool,
}

/// Configuration errors
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("JWT_SECRET environment variable not set")]
    MissingSecret,

    #[error("JWT secret must be at least 32 bytes (256 bits), got {0} bytes")]
    WeakSecret(usize),

    #[error("JWT secret has insufficient entropy: {0:.2} bits (minimum 128 required)")]
    LowEntropy(f64),

    #[error("JWT secret must be base64 encoded")]
    InvalidEncoding,

    #[error("Invalid secret format: {0}")]
    InvalidFormat(String),
}

impl AuthConfig {
    /// Create auth config from environment variable with strict validation
    ///
    /// Expects JWT_SECRET environment variable with base64-encoded secret (minimum 32 bytes)
    /// Optional JWT_SECRET_PREVIOUS for rotation support
    pub fn from_env() -> Result<Self, ConfigError> {
        let secret_b64 = std::env::var("JWT_SECRET").map_err(|_| ConfigError::MissingSecret)?;

        let secret_bytes = base64::decode(&secret_b64).map_err(|_| ConfigError::InvalidEncoding)?;

        // Validate secret strength
        Self::validate_secret_strength(&secret_bytes)?;

        // Load previous secret for rotation (optional)
        let previous_decoding_key = if let Ok(prev_b64) = std::env::var("JWT_SECRET_PREVIOUS") {
            let prev_bytes = base64::decode(&prev_b64).map_err(|_| ConfigError::InvalidEncoding)?;
            Self::validate_secret_strength(&prev_bytes)?;
            Some(Arc::new(DecodingKey::from_secret(&prev_bytes)))
        } else {
            None
        };

        Ok(Self {
            encoding_key: Arc::new(EncodingKey::from_secret(&secret_bytes)),
            decoding_key: Arc::new(DecodingKey::from_secret(&secret_bytes)),
            previous_decoding_key,
            token_expiry: Duration::hours(24),
            enabled: true,
        })
    }

    /// Create auth config from raw bytes (for testing)
    ///
    /// # Security Warning
    /// **TEST USE ONLY**. This method is provided for testing and should never be
    /// used in production code. Production deployments must use `from_env()` to load
    /// secrets from environment variables with proper validation.
    pub fn from_secret_bytes(secret: &[u8]) -> Result<Self, ConfigError> {
        Self::validate_secret_strength(secret)?;

        Ok(Self {
            encoding_key: Arc::new(EncodingKey::from_secret(secret)),
            decoding_key: Arc::new(DecodingKey::from_secret(secret)),
            previous_decoding_key: None,
            token_expiry: Duration::hours(24),
            enabled: true,
        })
    }

    /// Validate secret strength (minimum 256 bits = 32 bytes)
    fn validate_secret_strength(secret: &[u8]) -> Result<(), ConfigError> {
        const MIN_SECRET_LENGTH: usize = 32; // 256 bits

        if secret.len() < MIN_SECRET_LENGTH {
            return Err(ConfigError::WeakSecret(secret.len()));
        }

        // Calculate Shannon entropy to detect weak secrets (e.g., "aaaaaaa...")
        let entropy = Self::calculate_entropy(secret);
        const MIN_ENTROPY: f64 = 128.0; // bits

        if entropy < MIN_ENTROPY {
            return Err(ConfigError::LowEntropy(entropy));
        }

        Ok(())
    }

    /// Calculate Shannon entropy in bits
    ///
    /// Good random secrets should have entropy close to 8 * length bits
    /// Weak secrets (repeated chars, patterns) will have lower entropy
    fn calculate_entropy(data: &[u8]) -> f64 {
        let mut freq = [0u32; 256];

        for &byte in data {
            freq[byte as usize] += 1;
        }

        let len = data.len() as f64;
        let mut entropy = 0.0;

        for &count in &freq {
            if count > 0 {
                let p = count as f64 / len;
                entropy -= p * p.log2();
            }
        }

        // Convert to total bits
        entropy * len
    }

    /// Create disabled auth config (for development only)
    ///
    /// # Security Warning
    /// Never use in production environments. Only for local development and testing.
    pub fn disabled() -> Self {
        // Use a valid random secret even for disabled mode
        let random_secret: [u8; 32] = [
            0x1a, 0x2b, 0x3c, 0x4d, 0x5e, 0x6f, 0x70, 0x81, 0x92, 0xa3, 0xb4, 0xc5, 0xd6, 0xe7,
            0xf8, 0x09, 0x1a, 0x2b, 0x3c, 0x4d, 0x5e, 0x6f, 0x70, 0x81, 0x92, 0xa3, 0xb4, 0xc5,
            0xd6, 0xe7, 0xf8, 0x09,
        ];

        Self {
            encoding_key: Arc::new(EncodingKey::from_secret(&random_secret)),
            decoding_key: Arc::new(DecodingKey::from_secret(&random_secret)),
            previous_decoding_key: None,
            token_expiry: Duration::hours(24),
            enabled: false,
        }
    }

    /// Generate JWT token for user
    pub fn generate_token(&self, user_id: &str, role: Role) -> Result<String, AuthError> {
        let now = Utc::now();
        let claims = Claims {
            sub: user_id.to_string(),
            iat: now.timestamp(),
            exp: (now + self.token_expiry).timestamp(),
            role,
            scopes: vec![],
        };

        encode(&Header::default(), &claims, &self.encoding_key)
            .map_err(|e| AuthError::TokenGeneration(e.to_string()))
    }

    /// Validate JWT token and extract claims
    ///
    /// Supports secret rotation by trying current key first, then previous key if available
    pub fn validate_token(&self, token: &str) -> Result<Claims, AuthError> {
        let validation = Validation::default();

        // Try current key first
        match decode::<Claims>(token, &self.decoding_key, &validation) {
            Ok(data) => Ok(data.claims),
            Err(current_err) => {
                // If we have a previous key, try it (for rotation overlap)
                if let Some(ref prev_key) = self.previous_decoding_key {
                    decode::<Claims>(token, prev_key, &validation)
                        .map(|data| data.claims)
                        .map_err(|_| AuthError::InvalidToken(current_err.to_string()))
                } else {
                    Err(AuthError::InvalidToken(current_err.to_string()))
                }
            }
        }
    }
}

/// Authentication errors
#[derive(Debug)]
pub enum AuthError {
    MissingToken,
    InvalidToken(String),
    InsufficientPermissions,
    TokenGeneration(String),
}

impl std::fmt::Display for AuthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuthError::MissingToken => write!(f, "Missing authorization token"),
            AuthError::InvalidToken(msg) => write!(f, "Invalid token: {}", msg),
            AuthError::InsufficientPermissions => write!(f, "Insufficient permissions"),
            AuthError::TokenGeneration(msg) => write!(f, "Token generation failed: {}", msg),
        }
    }
}

impl std::error::Error for AuthError {}

/// Extract token from Authorization header
fn extract_token(headers: &axum::http::HeaderMap) -> Result<String, AuthError> {
    let auth_header = headers
        .get(header::AUTHORIZATION)
        .ok_or(AuthError::MissingToken)?
        .to_str()
        .map_err(|_| AuthError::InvalidToken("Invalid header encoding".to_string()))?;

    if let Some(token) = auth_header.strip_prefix("Bearer ") {
        Ok(token.to_string())
    } else {
        Err(AuthError::InvalidToken(
            "Bearer scheme required".to_string(),
        ))
    }
}

/// Authentication middleware
pub async fn auth_middleware(
    State(config): State<Arc<AuthConfig>>,
    mut request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    // Skip auth if disabled - insert dummy admin claims for downstream middleware
    if !config.enabled {
        let now = chrono::Utc::now().timestamp();
        let dummy_claims = Claims {
            sub: "dev-user".to_string(),
            iat: now,
            exp: now + 86400,  // 24 hours from now
            role: Role::Admin, // Admin role grants all permissions in dev mode
            scopes: vec![],
        };
        request.extensions_mut().insert(dummy_claims);
        return Ok(next.run(request).await);
    }

    // Extract and validate token
    let token = extract_token(request.headers()).map_err(|_| StatusCode::UNAUTHORIZED)?;

    let claims = config
        .validate_token(&token)
        .map_err(|_| StatusCode::UNAUTHORIZED)?;

    // Insert claims into request extensions for handlers to access
    request.extensions_mut().insert(claims);

    Ok(next.run(request).await)
}

/// Require specific role middleware
pub async fn require_role(
    required_role: Role,
) -> impl Fn(
    Request,
    Next,
)
    -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Response, StatusCode>> + Send>> {
    move |request: Request, next: Next| {
        let required = required_role.clone();
        Box::pin(async move {
            // Get claims from extensions
            let claims = request
                .extensions()
                .get::<Claims>()
                .ok_or(StatusCode::UNAUTHORIZED)?;

            // Check role permissions
            match required {
                Role::Admin => {
                    if !claims.role.can_admin() {
                        return Err(StatusCode::FORBIDDEN);
                    }
                }
                Role::Operator => {
                    if !claims.role.can_write() {
                        return Err(StatusCode::FORBIDDEN);
                    }
                }
                Role::Viewer => {
                    if !claims.role.can_read() {
                        return Err(StatusCode::FORBIDDEN);
                    }
                }
                Role::Service => {
                    if claims.role != Role::Service && claims.role != Role::Admin {
                        return Err(StatusCode::FORBIDDEN);
                    }
                }
            }

            Ok(next.run(request).await)
        })
    }
}

/// Extract claims from request (for handlers)
pub trait RequestExt {
    fn claims(&self) -> Option<&Claims>;
    fn require_write(&self) -> Result<&Claims, StatusCode>;
    fn require_admin(&self) -> Result<&Claims, StatusCode>;
}

impl RequestExt for Request {
    fn claims(&self) -> Option<&Claims> {
        self.extensions().get::<Claims>()
    }

    fn require_write(&self) -> Result<&Claims, StatusCode> {
        let claims = self.claims().ok_or(StatusCode::UNAUTHORIZED)?;
        if !claims.role.can_write() {
            return Err(StatusCode::FORBIDDEN);
        }
        Ok(claims)
    }

    fn require_admin(&self) -> Result<&Claims, StatusCode> {
        let claims = self.claims().ok_or(StatusCode::UNAUTHORIZED)?;
        if !claims.role.can_admin() {
            return Err(StatusCode::FORBIDDEN);
        }
        Ok(claims)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Valid test secret (32 bytes with good entropy)
    fn test_secret() -> [u8; 32] {
        [
            0x1a, 0x2b, 0x3c, 0x4d, 0x5e, 0x6f, 0x70, 0x81, 0x92, 0xa3, 0xb4, 0xc5, 0xd6, 0xe7,
            0xf8, 0x09, 0xfe, 0xdc, 0xba, 0x98, 0x76, 0x54, 0x32, 0x10, 0x11, 0x22, 0x33, 0x44,
            0x55, 0x66, 0x77, 0x88,
        ]
    }

    #[test]
    fn test_token_generation_and_validation() {
        let config = AuthConfig::from_secret_bytes(&test_secret()).unwrap();

        // Generate token
        let token = config.generate_token("user123", Role::Admin).unwrap();
        assert!(!token.is_empty());

        // Validate token
        let claims = config.validate_token(&token).unwrap();
        assert_eq!(claims.sub, "user123");
        assert_eq!(claims.role, Role::Admin);
    }

    #[test]
    fn test_invalid_token() {
        let config = AuthConfig::from_secret_bytes(&test_secret()).unwrap();
        let result = config.validate_token("invalid.token.here");
        assert!(result.is_err());
    }

    #[test]
    fn test_role_permissions() {
        assert!(Role::Admin.can_write());
        assert!(Role::Admin.can_admin());
        assert!(Role::Admin.can_read());

        assert!(Role::Operator.can_write());
        assert!(!Role::Operator.can_admin());
        assert!(Role::Operator.can_read());

        assert!(!Role::Viewer.can_write());
        assert!(!Role::Viewer.can_admin());
        assert!(Role::Viewer.can_read());

        assert!(Role::Service.can_write());
        assert!(!Role::Service.can_admin());
        assert!(Role::Service.can_read());
    }

    #[test]
    fn test_disabled_auth() {
        let config = AuthConfig::disabled();
        assert!(!config.enabled);
    }

    #[test]
    fn test_role_based_access_control() {
        let config = AuthConfig::from_secret_bytes(&test_secret()).unwrap();

        // Test Admin role - full access
        let admin_token = config.generate_token("admin_user", Role::Admin).unwrap();
        let admin_claims = config.validate_token(&admin_token).unwrap();
        assert!(admin_claims.role.can_read());
        assert!(admin_claims.role.can_write());
        assert!(admin_claims.role.can_admin());

        // Test Operator role - read and write, no admin
        let operator_token = config
            .generate_token("operator_user", Role::Operator)
            .unwrap();
        let operator_claims = config.validate_token(&operator_token).unwrap();
        assert!(operator_claims.role.can_read());
        assert!(operator_claims.role.can_write());
        assert!(!operator_claims.role.can_admin());

        // Test Viewer role - read only
        let viewer_token = config.generate_token("viewer_user", Role::Viewer).unwrap();
        let viewer_claims = config.validate_token(&viewer_token).unwrap();
        assert!(viewer_claims.role.can_read());
        assert!(!viewer_claims.role.can_write());
        assert!(!viewer_claims.role.can_admin());

        // Test Service role - read and write, no admin
        let service_token = config.generate_token("service_bot", Role::Service).unwrap();
        let service_claims = config.validate_token(&service_token).unwrap();
        assert!(service_claims.role.can_read());
        assert!(service_claims.role.can_write());
        assert!(!service_claims.role.can_admin());
    }

    #[test]
    fn test_service_account_tokens() {
        let config = AuthConfig::from_secret_bytes(&test_secret()).unwrap();

        // Service accounts should be able to write but not perform admin operations
        let service_token = config.generate_token("ci_pipeline", Role::Service).unwrap();
        let claims = config.validate_token(&service_token).unwrap();

        assert_eq!(claims.sub, "ci_pipeline");
        assert_eq!(claims.role, Role::Service);
        assert!(
            claims.role.can_write(),
            "Service accounts should have write access"
        );
        assert!(
            !claims.role.can_admin(),
            "Service accounts should not have admin access"
        );
    }

    // === NEW SECURITY VALIDATION TESTS ===

    #[test]
    fn test_weak_secret_too_short() {
        // Test secrets < 32 bytes are rejected
        let weak = b"short_secret_only_24_chr";
        assert_eq!(weak.len(), 24);

        match AuthConfig::from_secret_bytes(weak) {
            Err(ConfigError::WeakSecret(len)) => {
                assert_eq!(len, 24);
            }
            _ => panic!("Should reject secret shorter than 32 bytes"),
        }
    }

    #[test]
    fn test_low_entropy_secret_rejected() {
        // Test repeated characters (low entropy) are rejected
        let weak = b"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"; // 32 'a's
        assert_eq!(weak.len(), 32);

        match AuthConfig::from_secret_bytes(weak) {
            Err(ConfigError::LowEntropy(entropy)) => {
                assert!(entropy < 128.0, "Entropy {} should be < 128 bits", entropy);
            }
            Ok(_) => panic!("Should reject low entropy secret"),
            Err(e) => panic!("Wrong error type: {:?}", e),
        }
    }

    #[test]
    fn test_valid_secret_accepted() {
        // Test valid random secret with good entropy is accepted
        let valid = test_secret();
        let result = AuthConfig::from_secret_bytes(&valid);
        assert!(
            result.is_ok(),
            "Should accept valid secret with good entropy"
        );
    }

    #[test]
    fn test_entropy_calculation() {
        // Test entropy calculation with known values

        // All same byte: entropy should be 0
        let uniform = [0x00u8; 32];
        let entropy = AuthConfig::calculate_entropy(&uniform);
        assert_eq!(entropy, 0.0, "Uniform data should have 0 entropy");

        // Perfectly random-like: high entropy
        let random = test_secret();
        let entropy = AuthConfig::calculate_entropy(&random);
        assert!(entropy > 128.0, "Random-like data should have high entropy");
    }

    #[test]
    fn test_secret_rotation() {
        // Create first config with secret1
        let secret1 = test_secret();
        let config1 = AuthConfig::from_secret_bytes(&secret1).unwrap();

        // Generate token with secret1
        let token = config1.generate_token("user456", Role::Operator).unwrap();

        // Create second config with secret2 as current, secret1 as previous
        let secret2: [u8; 32] = [
            0xff, 0xee, 0xdd, 0xcc, 0xbb, 0xaa, 0x99, 0x88, 0x77, 0x66, 0x55, 0x44, 0x33, 0x22,
            0x11, 0x00, 0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 0xde, 0xf0, 0xa1, 0xb2, 0xc3, 0xd4,
            0xe5, 0xf6, 0x07, 0x18,
        ];

        let config2 = AuthConfig {
            encoding_key: Arc::new(EncodingKey::from_secret(&secret2)),
            decoding_key: Arc::new(DecodingKey::from_secret(&secret2)),
            previous_decoding_key: Some(Arc::new(DecodingKey::from_secret(&secret1))),
            token_expiry: Duration::hours(24),
            enabled: true,
        };

        // Token signed with secret1 should still validate with config2 (using previous key)
        let claims = config2.validate_token(&token).unwrap();
        assert_eq!(claims.sub, "user456");
        assert_eq!(claims.role, Role::Operator);
    }

    #[test]
    fn test_rotation_fails_without_previous_key() {
        // Create token with secret1
        let secret1 = test_secret();
        let config1 = AuthConfig::from_secret_bytes(&secret1).unwrap();
        let token = config1.generate_token("user789", Role::Viewer).unwrap();

        // Create config2 with different secret and NO previous key
        let secret2: [u8; 32] = [
            0xff, 0xee, 0xdd, 0xcc, 0xbb, 0xaa, 0x99, 0x88, 0x77, 0x66, 0x55, 0x44, 0x33, 0x22,
            0x11, 0x00, 0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 0xde, 0xf0, 0xa1, 0xb2, 0xc3, 0xd4,
            0xe5, 0xf6, 0x07, 0x18,
        ];
        let config2 = AuthConfig::from_secret_bytes(&secret2).unwrap();

        // Token should fail validation (no previous key to fall back to)
        assert!(config2.validate_token(&token).is_err());
    }

    #[test]
    fn test_minimum_entropy_boundary() {
        // Test secret exactly at entropy boundary
        // Create a secret with calculated low entropy
        let mut low_entropy = vec![0x00u8; 16];
        low_entropy.extend_from_slice(&[0xFFu8; 16]);
        assert_eq!(low_entropy.len(), 32);

        // This has some entropy but likely below threshold
        let entropy = AuthConfig::calculate_entropy(&low_entropy);
        assert!(entropy < 128.0, "This pattern should have low entropy");

        match AuthConfig::from_secret_bytes(&low_entropy) {
            Err(ConfigError::LowEntropy(_)) => {
                // Expected
            }
            Ok(_) => panic!("Should reject low entropy secret"),
            Err(e) => panic!("Wrong error: {:?}", e),
        }
    }

    #[test]
    fn test_disabled_config_uses_valid_secret() {
        // Even disabled config should use a valid secret (security best practice)
        let config = AuthConfig::disabled();

        // Should still be able to generate/validate tokens (even though disabled=true)
        let token = config.generate_token("test_user", Role::Admin).unwrap();
        let claims = config.validate_token(&token).unwrap();
        assert_eq!(claims.sub, "test_user");

        // But enabled flag should be false
        assert!(!config.enabled);
    }
}
