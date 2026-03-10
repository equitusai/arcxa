//! Rate Limiting Middleware
//!
//! Implements rate limiting for API endpoints using tower-governor.
//! Critical for preventing brute force attacks on authentication.

use axum::http::StatusCode;
use governor::middleware::NoOpMiddleware;
use tower_governor::{
    governor::{GovernorConfig, GovernorConfigBuilder},
    key_extractor::SmartIpKeyExtractor,
    GovernorLayer,
};

/// Rate limit configuration for different endpoint types
#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    /// Authentication endpoints (login) - strict limits to prevent brute force
    pub auth_per_minute: u32,
    /// User creation endpoints - moderate limits
    pub user_creation_per_minute: u32,
    /// Setup endpoint - very strict (one-time operation)
    pub setup_per_hour: u32,
    /// General API endpoints
    pub general_per_minute: u32,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            auth_per_minute: 5,           // 5 login attempts per minute per IP
            user_creation_per_minute: 10, // 10 user creations per minute per IP
            setup_per_hour: 1,            // 1 setup attempt per hour per IP
            general_per_minute: 100,      // 100 requests per minute for general API
        }
    }
}

impl RateLimitConfig {
    /// Create rate limit config from environment variables
    pub fn from_env() -> Self {
        Self {
            auth_per_minute: std::env::var("RATE_LIMIT_AUTH_PER_MIN")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(5),
            user_creation_per_minute: std::env::var("RATE_LIMIT_USER_CREATE_PER_MIN")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(10),
            setup_per_hour: std::env::var("RATE_LIMIT_SETUP_PER_HOUR")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(1),
            general_per_minute: std::env::var("RATE_LIMIT_GENERAL_PER_MIN")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(100),
        }
    }

    /// Validate configuration
    pub fn validate(&self) -> Result<(), String> {
        if self.auth_per_minute == 0 {
            return Err("auth_per_minute must be > 0".to_string());
        }
        if self.user_creation_per_minute == 0 {
            return Err("user_creation_per_minute must be > 0".to_string());
        }
        if self.setup_per_hour == 0 {
            return Err("setup_per_hour must be > 0".to_string());
        }
        if self.general_per_minute == 0 {
            return Err("general_per_minute must be > 0".to_string());
        }
        Ok(())
    }
}

/// Create rate limiter for authentication endpoints (login)
///
/// Strict limits to prevent brute force attacks:
/// - 5 attempts per minute per IP address
/// - Burst size of 2 (allows short bursts)
/// - Uses SmartIpKeyExtractor for reverse proxy compatibility
pub fn auth_rate_limiter() -> GovernorLayer<'static, SmartIpKeyExtractor, NoOpMiddleware> {
    let config: &'static GovernorConfig<SmartIpKeyExtractor, NoOpMiddleware> = Box::leak(Box::new(
        GovernorConfigBuilder::default()
            .key_extractor(SmartIpKeyExtractor)
            .per_millisecond(12_000) // 5 per minute = 1 per 12 seconds
            .burst_size(2)
            .finish()
            .expect("Failed to build auth rate limiter config"),
    ));

    GovernorLayer { config }
}

/// Create rate limiter for user creation endpoints
///
/// Moderate limits:
/// - 10 creations per minute per IP
/// - Burst size of 3
/// - Uses SmartIpKeyExtractor for reverse proxy compatibility
pub fn user_creation_rate_limiter() -> GovernorLayer<'static, SmartIpKeyExtractor, NoOpMiddleware> {
    let config: &'static GovernorConfig<SmartIpKeyExtractor, NoOpMiddleware> = Box::leak(Box::new(
        GovernorConfigBuilder::default()
            .key_extractor(SmartIpKeyExtractor)
            .per_millisecond(6_000) // 10 per minute = 1 per 6 seconds
            .burst_size(3)
            .finish()
            .expect("Failed to build user creation rate limiter config"),
    ));

    GovernorLayer { config }
}

/// Create rate limiter for setup endpoint
///
/// Very strict limits (one-time operation):
/// - 1 attempt per hour per IP
/// - No burst allowed
/// - Uses SmartIpKeyExtractor for reverse proxy compatibility
pub fn setup_rate_limiter() -> GovernorLayer<'static, SmartIpKeyExtractor, NoOpMiddleware> {
    let config: &'static GovernorConfig<SmartIpKeyExtractor, NoOpMiddleware> = Box::leak(Box::new(
        GovernorConfigBuilder::default()
            .key_extractor(SmartIpKeyExtractor)
            .per_millisecond(3_600_000) // 1 per hour
            .burst_size(1)
            .finish()
            .expect("Failed to build setup rate limiter config"),
    ));

    GovernorLayer { config }
}

/// Create rate limiter for general API endpoints
///
/// - 100 requests per minute per IP
/// - Burst size of 20 for short traffic spikes
/// - Uses SmartIpKeyExtractor for reverse proxy compatibility
pub fn general_rate_limiter() -> GovernorLayer<'static, SmartIpKeyExtractor, NoOpMiddleware> {
    let config: &'static GovernorConfig<SmartIpKeyExtractor, NoOpMiddleware> = Box::leak(Box::new(
        GovernorConfigBuilder::default()
            .key_extractor(SmartIpKeyExtractor)
            .per_millisecond(600) // 100 per minute = 1 per 0.6 seconds
            .burst_size(20)
            .finish()
            .expect("Failed to build general rate limiter config"),
    ));

    GovernorLayer { config }
}

/// Create rate limiter for schema discovery endpoints
///
/// Discovery operations are resource-intensive, so stricter limits:
/// - 10 discoveries per minute per IP (prevents resource exhaustion)
/// - Burst size of 2 (allows a couple simultaneous discoveries)
/// - Uses SmartIpKeyExtractor for reverse proxy compatibility
///
/// **Rationale**: Schema discovery involves:
/// - Database introspection queries (expensive)
/// - Metadata extraction from system catalogs
/// - Column statistics computation
/// - Pattern detection across sample data
///
/// Each discovery can take 10-30 seconds and consume significant resources.
/// Limiting to 10/minute prevents DoS while allowing legitimate use.
pub fn discovery_rate_limiter() -> GovernorLayer<'static, SmartIpKeyExtractor, NoOpMiddleware> {
    let config: &'static GovernorConfig<SmartIpKeyExtractor, NoOpMiddleware> = Box::leak(Box::new(
        GovernorConfigBuilder::default()
            .key_extractor(SmartIpKeyExtractor)
            .per_millisecond(6_000) // 10 per minute = 1 per 6 seconds
            .burst_size(2)
            .finish()
            .expect("Failed to build discovery rate limiter config"),
    ));

    GovernorLayer { config }
}

/// Custom rate limit error response
pub fn rate_limit_error() -> (StatusCode, String) {
    (
        StatusCode::TOO_MANY_REQUESTS,
        "Rate limit exceeded. Please try again later.".to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = RateLimitConfig::default();
        assert_eq!(config.auth_per_minute, 5);
        assert_eq!(config.user_creation_per_minute, 10);
        assert_eq!(config.setup_per_hour, 1);
        assert_eq!(config.general_per_minute, 100);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_config_validation() {
        let mut config = RateLimitConfig::default();

        // Valid config
        assert!(config.validate().is_ok());

        // Invalid: zero auth rate
        config.auth_per_minute = 0;
        assert!(config.validate().is_err());

        config = RateLimitConfig::default();
        config.setup_per_hour = 0;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_from_env() {
        // Test with no env vars (should use defaults)
        let config = RateLimitConfig::from_env();
        assert_eq!(config.auth_per_minute, 5);

        // Test with env vars
        std::env::set_var("RATE_LIMIT_AUTH_PER_MIN", "10");
        std::env::set_var("RATE_LIMIT_SETUP_PER_HOUR", "2");

        let config = RateLimitConfig::from_env();
        assert_eq!(config.auth_per_minute, 10);
        assert_eq!(config.setup_per_hour, 2);

        // Clean up
        std::env::remove_var("RATE_LIMIT_AUTH_PER_MIN");
        std::env::remove_var("RATE_LIMIT_SETUP_PER_HOUR");
    }

    #[test]
    fn test_auth_rate_limiter_strictness() {
        // Auth rate limiter should be strictest
        // 5 per minute = 12,000ms per request
        // This test just ensures it compiles and creates successfully
        let _limiter = auth_rate_limiter();
        // In real usage, this would block after 5 requests within a minute
    }

    #[test]
    fn test_setup_rate_limiter_very_strict() {
        // Setup should be most restrictive: 1 per hour
        let _limiter = setup_rate_limiter();
        // This would allow only 1 request per hour per IP
    }

    #[test]
    fn test_discovery_rate_limiter() {
        // Discovery should be moderately strict: 10 per minute
        // Each discovery is expensive (introspection + profiling)
        let _limiter = discovery_rate_limiter();
        // This would allow 10 discovery requests per minute per IP
    }
}
