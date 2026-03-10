//! User management with secure password storage
//!
//! Provides user authentication with argon2id password hashing
//! and RocksDB-based user storage.

use anyhow::{Context, Result};
use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::api::auth::Role;
use crate::storage::kv_store::KvStore;

/// User record with hashed password
///
/// **Security Note**: This struct is used for internal storage and includes the password hash.
/// For API responses, use UserSummary instead which excludes the password hash.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: String,
    pub username: String,
    /// Password hash (argon2id PHC string) - stored in DB, never in API responses
    pub password_hash: String,
    pub role: Role,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_login: Option<DateTime<Utc>>,
    pub failed_attempts: u32,
    pub locked_until: Option<DateTime<Utc>>,
}

/// User creation request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateUserRequest {
    pub username: String,
    pub password: String,
    pub role: Role,
}

/// User service for credential management
pub struct UserService {
    storage: Arc<KvStore>,
}

impl UserService {
    /// Create new user service with KvStore
    pub fn new(storage: Arc<KvStore>) -> Self {
        Self { storage }
    }

    /// Create a new user with hashed password
    pub async fn create_user(&self, request: CreateUserRequest) -> Result<User> {
        // Check if username already exists
        if self.get_user_by_username(&request.username).await.is_ok() {
            anyhow::bail!("Username already exists");
        }

        // Validate password strength
        self.validate_password_strength(&request.password)?;

        // Hash password with argon2id
        let password_hash = self.hash_password(&request.password)?;

        let user = User {
            id: uuid::Uuid::new_v4().to_string(),
            username: request.username,
            password_hash,
            role: request.role,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            last_login: None,
            failed_attempts: 0,
            locked_until: None,
        };

        // Store in RocksDB
        self.store_user(&user).await?;

        Ok(user)
    }

    /// Validate user credentials
    pub async fn validate_credentials(
        &self,
        username: &str,
        password: &str,
    ) -> Result<User, AuthenticationError> {
        let mut user = self
            .get_user_by_username(username)
            .await
            .map_err(|_| AuthenticationError::InvalidCredentials)?;

        // Check account lockout
        if let Some(locked_until) = user.locked_until {
            if locked_until > Utc::now() {
                return Err(AuthenticationError::AccountLocked { locked_until });
            }
        }

        // Verify password
        let parsed_hash = PasswordHash::new(&user.password_hash)
            .map_err(|e| AuthenticationError::InvalidHash(e.to_string()))?;

        match Argon2::default().verify_password(password.as_bytes(), &parsed_hash) {
            Ok(_) => {
                // Reset failed attempts on success
                user.failed_attempts = 0;
                user.locked_until = None;
                user.last_login = Some(Utc::now());
                user.updated_at = Utc::now();
                self.store_user(&user)
                    .await
                    .map_err(|e| AuthenticationError::StorageError(e.to_string()))?;
                Ok(user)
            }
            Err(_) => {
                // Increment failed attempts
                user.failed_attempts += 1;

                // Lock account after 5 failed attempts (30 minutes)
                if user.failed_attempts >= 5 {
                    user.locked_until = Some(Utc::now() + chrono::Duration::minutes(30));
                }

                user.updated_at = Utc::now();
                self.store_user(&user)
                    .await
                    .map_err(|e| AuthenticationError::StorageError(e.to_string()))?;

                Err(AuthenticationError::InvalidCredentials)
            }
        }
    }

    /// Get user by username
    async fn get_user_by_username(&self, username: &str) -> Result<User> {
        let key = format!("user:username:{}", username);
        let data = self
            .storage
            .get(key.as_bytes())
            .context("Failed to query user storage")?
            .ok_or_else(|| anyhow::anyhow!("User not found"))?;

        let user: User = serde_json::from_slice(&data).context("Failed to deserialize user")?;

        Ok(user)
    }

    /// Store user in RocksDB
    async fn store_user(&self, user: &User) -> Result<()> {
        let user_json = serde_json::to_vec(user).context("Failed to serialize user")?;

        // Store by ID
        let id_key = format!("user:id:{}", user.id);
        self.storage
            .put(id_key.as_bytes(), &user_json)
            .context("Failed to store user by ID")?;

        // Store by username (for lookup)
        let username_key = format!("user:username:{}", user.username);
        self.storage
            .put(username_key.as_bytes(), &user_json)
            .context("Failed to store user by username")?;

        Ok(())
    }

    /// Hash password with argon2id
    fn hash_password(&self, password: &str) -> Result<String> {
        let salt = SaltString::generate(&mut OsRng);
        let argon2 = Argon2::default();

        let password_hash = argon2
            .hash_password(password.as_bytes(), &salt)
            .map_err(|e| anyhow::anyhow!("Password hashing failed: {}", e))?
            .to_string();

        Ok(password_hash)
    }

    /// Validate password strength
    fn validate_password_strength(&self, password: &str) -> Result<()> {
        if password.len() < 12 {
            anyhow::bail!("Password must be at least 12 characters");
        }

        let has_lowercase = password.chars().any(|c| c.is_lowercase());
        let has_uppercase = password.chars().any(|c| c.is_uppercase());
        let has_digit = password.chars().any(|c| c.is_numeric());
        let has_special = password.chars().any(|c| !c.is_alphanumeric());

        if !has_lowercase || !has_uppercase || !has_digit || !has_special {
            anyhow::bail!(
                "Password must contain lowercase, uppercase, digit, and special character"
            );
        }

        Ok(())
    }

    /// Create default admin user (for initial setup)
    pub async fn create_default_admin(&self, password: &str) -> Result<User> {
        self.create_user(CreateUserRequest {
            username: "admin".to_string(),
            password: password.to_string(),
            role: Role::Admin,
        })
        .await
    }

    /// List users with pagination (admin only)
    ///
    /// Returns users sorted alphabetically by username. Pagination is applied after sorting
    /// to ensure consistent ordering across pages.
    ///
    /// # Arguments
    /// * `limit` - Maximum number of users to return (max: 1000)
    /// * `offset` - Number of users to skip (for pagination)
    ///
    /// # Returns
    /// * `UserListResponse` - Paginated list of users with metadata
    ///
    /// # Example
    /// ```ignore
    /// // Get first page (20 users)
    /// let page1 = service.list_users(20, 0).await?;
    ///
    /// // Get second page
    /// let page2 = service.list_users(20, 20).await?;
    /// ```ignore
    ///
    /// # Performance Note
    /// To ensure consistent sorted ordering across pages, this method loads all users into memory,
    /// sorts them, then applies pagination. This is acceptable for up to 10,000 users. For larger
    /// user bases, consider implementing cursor-based pagination with a username index.
    pub async fn list_users(&self, limit: usize, offset: usize) -> Result<UserListResponse> {
        const MAX_LIMIT: usize = 1000;
        const MAX_SCAN: usize = 10000; // Maximum users to scan and sort

        // Enforce maximum limit to prevent resource exhaustion
        let effective_limit = limit.min(MAX_LIMIT);

        // Load all users into memory (up to MAX_SCAN)
        // This is required for consistent alphabetical ordering across pages
        let mut all_users = Vec::new();

        // Scan all user records using prefix scan
        let user_records = self
            .storage
            .prefix_scan(b"user:id:")
            .context("Failed to scan user records")?;

        for (_key, value) in user_records {
            // Safety limit: stop if we've scanned too many users
            if all_users.len() >= MAX_SCAN {
                tracing::warn!(
                    "User count exceeds MAX_SCAN ({}), some users will not be returned",
                    MAX_SCAN
                );
                break;
            }

            // Deserialize user
            match serde_json::from_slice::<User>(&value) {
                Ok(user) => {
                    // Convert to summary (exclude password hash)
                    all_users.push(UserSummary {
                        id: user.id,
                        username: user.username,
                        role: user.role,
                        created_at: user.created_at,
                        last_login: user.last_login,
                    });
                }
                Err(e) => {
                    // Log but don't fail on corrupted user records
                    tracing::warn!("Failed to deserialize user record: {}", e);
                    continue;
                }
            }
        }

        // Sort by username for consistent ordering across pages
        all_users.sort_by(|a, b| a.username.cmp(&b.username));

        // Apply pagination AFTER sorting
        let total_users = all_users.len();
        let start = offset.min(total_users);
        let end = (offset + effective_limit).min(total_users);

        let users: Vec<UserSummary> = all_users[start..end].to_vec();
        let total_returned = users.len();
        let has_more = end < total_users;

        Ok(UserListResponse {
            users,
            total_returned,
            has_more,
        })
    }

    /// List all users without pagination (legacy, for backward compatibility)
    ///
    /// **WARNING**: This method loads all users into memory and should only be used
    /// for small user bases or administrative tasks. Use `list_users()` with pagination instead.
    #[deprecated(since = "0.1.0", note = "Use list_users(limit, offset) instead")]
    pub async fn list_all_users_unpaginated(&self) -> Result<Vec<UserSummary>> {
        let response = self.list_users(10000, 0).await?;
        Ok(response.users)
    }
}

/// User summary (without sensitive data)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserSummary {
    pub id: String,
    pub username: String,
    pub role: Role,
    pub created_at: DateTime<Utc>,
    pub last_login: Option<DateTime<Utc>>,
}

/// Paginated user list response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserListResponse {
    pub users: Vec<UserSummary>,
    pub total_returned: usize,
    pub has_more: bool,
}

/// Authentication errors
#[derive(Debug, thiserror::Error)]
pub enum AuthenticationError {
    #[error("Invalid credentials")]
    InvalidCredentials,

    #[error("Account locked until {locked_until}")]
    AccountLocked { locked_until: DateTime<Utc> },

    #[error("Invalid password hash: {0}")]
    InvalidHash(String),

    #[error("Storage error: {0}")]
    StorageError(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_password_strength_validation() {
        let service = UserService::new(Arc::new(KvStore::new_in_memory().unwrap()));

        // Too short
        assert!(service.validate_password_strength("Short1!").is_err());

        // No uppercase
        assert!(service.validate_password_strength("lowercase123!").is_err());

        // No lowercase
        assert!(service.validate_password_strength("UPPERCASE123!").is_err());

        // No digit
        assert!(service.validate_password_strength("NoDigitsHere!").is_err());

        // No special character
        assert!(service.validate_password_strength("NoSpecial123").is_err());

        // Valid password
        assert!(service
            .validate_password_strength("ValidPassword123!")
            .is_ok());
    }

    #[tokio::test]
    async fn test_password_hashing() {
        let service = UserService::new(Arc::new(KvStore::new_in_memory().unwrap()));
        let password = "SecurePassword123!";

        let hash = service.hash_password(password).unwrap();
        assert!(!hash.is_empty());
        assert!(hash.starts_with("$argon2"));

        // Verify password
        let parsed_hash = PasswordHash::new(&hash).unwrap();
        assert!(Argon2::default()
            .verify_password(password.as_bytes(), &parsed_hash)
            .is_ok());

        // Wrong password should fail
        assert!(Argon2::default()
            .verify_password(b"WrongPassword", &parsed_hash)
            .is_err());
    }

    #[tokio::test]
    async fn test_list_users() {
        let service = UserService::new(Arc::new(KvStore::new_in_memory().unwrap()));

        // Initially empty
        let response = service.list_users(100, 0).await.unwrap();
        assert_eq!(response.users.len(), 0);
        assert_eq!(response.total_returned, 0);
        assert!(!response.has_more);

        // Create multiple users
        service
            .create_user(CreateUserRequest {
                username: "alice".to_string(),
                password: "AlicePassword123!".to_string(),
                role: Role::Admin,
            })
            .await
            .unwrap();

        service
            .create_user(CreateUserRequest {
                username: "bob".to_string(),
                password: "BobPassword456!".to_string(),
                role: Role::Operator,
            })
            .await
            .unwrap();

        service
            .create_user(CreateUserRequest {
                username: "charlie".to_string(),
                password: "CharliePassword789!".to_string(),
                role: Role::Viewer,
            })
            .await
            .unwrap();

        // List all users with generous limit
        let response = service.list_users(100, 0).await.unwrap();
        assert_eq!(response.users.len(), 3);
        assert_eq!(response.total_returned, 3);

        // Should be sorted by username
        assert_eq!(response.users[0].username, "alice");
        assert_eq!(response.users[1].username, "bob");
        assert_eq!(response.users[2].username, "charlie");

        // Check roles
        assert_eq!(response.users[0].role, Role::Admin);
        assert_eq!(response.users[1].role, Role::Operator);
        assert_eq!(response.users[2].role, Role::Viewer);

        // Verify password hashes are not exposed
        for user in &response.users {
            // UserSummary shouldn't have password_hash field
            assert!(user.id.len() > 0);
            assert!(user.username.len() > 0);
        }
    }

    #[tokio::test]
    async fn test_list_users_pagination() {
        let service = UserService::new(Arc::new(KvStore::new_in_memory().unwrap()));

        // Create 5 test users
        for i in 1..=5 {
            service
                .create_user(CreateUserRequest {
                    username: format!("user{}", i),
                    password: format!("Password{}123!", i),
                    role: Role::Viewer,
                })
                .await
                .unwrap();
        }

        // Test first page (limit: 2)
        let page1 = service.list_users(2, 0).await.unwrap();
        assert_eq!(page1.users.len(), 2);
        assert_eq!(page1.total_returned, 2);
        assert!(page1.has_more); // More users available

        // Test second page (limit: 2, offset: 2)
        let page2 = service.list_users(2, 2).await.unwrap();
        assert_eq!(page2.users.len(), 2);
        assert_eq!(page2.total_returned, 2);
        assert!(page2.has_more); // Still more users

        // Test third page (limit: 2, offset: 4)
        let page3 = service.list_users(2, 4).await.unwrap();
        assert_eq!(page3.users.len(), 1); // Only 1 user left
        assert_eq!(page3.total_returned, 1);
        assert!(!page3.has_more); // No more users

        // Test offset beyond total
        let page4 = service.list_users(2, 10).await.unwrap();
        assert_eq!(page4.users.len(), 0);
        assert_eq!(page4.total_returned, 0);
        assert!(!page4.has_more);
    }

    #[tokio::test]
    async fn test_list_users_max_limit_enforcement() {
        let service = UserService::new(Arc::new(KvStore::new_in_memory().unwrap()));

        // Create 10 test users
        for i in 1..=10 {
            service
                .create_user(CreateUserRequest {
                    username: format!("user{}", i),
                    password: format!("Password{}123!", i),
                    role: Role::Viewer,
                })
                .await
                .unwrap();
        }

        // Request with excessive limit (2000) should be capped at 1000
        let response = service.list_users(2000, 0).await.unwrap();
        // Should return all 10 users (less than max limit of 1000)
        assert_eq!(response.users.len(), 10);
        assert_eq!(response.total_returned, 10);
    }

    #[tokio::test]
    async fn test_list_users_pagination_consistency() {
        let service = UserService::new(Arc::new(KvStore::new_in_memory().unwrap()));

        // Create users with predictable names
        let usernames = vec!["alice", "bob", "charlie", "diana", "eve"];
        for username in &usernames {
            service
                .create_user(CreateUserRequest {
                    username: username.to_string(),
                    password: format!("{}Password123!", username),
                    role: Role::Viewer,
                })
                .await
                .unwrap();
        }

        // Collect all users via pagination
        let mut all_users = Vec::new();
        let mut offset = 0;
        let limit = 2;

        loop {
            let response = service.list_users(limit, offset).await.unwrap();
            all_users.extend(response.users);

            if !response.has_more {
                break;
            }

            offset += limit;
        }

        // Verify we got all 5 users
        assert_eq!(all_users.len(), 5);

        // Verify sorted order (alice, bob, charlie, diana, eve)
        assert_eq!(all_users[0].username, "alice");
        assert_eq!(all_users[1].username, "bob");
        assert_eq!(all_users[2].username, "charlie");
        assert_eq!(all_users[3].username, "diana");
        assert_eq!(all_users[4].username, "eve");
    }

    #[tokio::test]
    async fn test_list_users_empty_result() {
        let service = UserService::new(Arc::new(KvStore::new_in_memory().unwrap()));

        // No users created
        let response = service.list_users(10, 0).await.unwrap();

        assert_eq!(response.users.len(), 0);
        assert_eq!(response.total_returned, 0);
        assert!(!response.has_more);
    }
}
