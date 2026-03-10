//! Security Stack Performance Benchmarks
//!
//! Benchmarks for authentication, authorization, audit logging, and user management.

use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use std::sync::Arc;
use tokio::runtime::Runtime;

use graphica::api::{
    auth::{AuthConfig, Role},
    users::{UserService, CreateUserRequest},
    audit::{AuditLogger, RocksAuditSink},
    setup_token::SetupTokenManager,
};
use graphica::storage::kv_store::KvStore;

// ============================================================================
// Authentication Benchmarks
// ============================================================================

/// Benchmark password hashing (argon2id)
fn bench_password_hashing(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let kv_store = Arc::new(KvStore::new_in_memory().unwrap());
    let user_service = Arc::new(UserService::new(kv_store));

    c.bench_function("password_hash_argon2id", |b| {
        b.to_async(&rt).iter(|| async {
            let request = CreateUserRequest {
                username: format!("user_{}", rand::random::<u64>()),
                password: "SecurePassword123!".to_string(),
                role: Role::Viewer,
            };

            user_service.create_user(request).await.unwrap();
        });
    });
}

/// Benchmark password verification
fn bench_password_verification(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let kv_store = Arc::new(KvStore::new_in_memory().unwrap());
    let user_service = Arc::new(UserService::new(kv_store));

    // Create test user
    rt.block_on(async {
        let request = CreateUserRequest {
            username: "benchuser".to_string(),
            password: "SecurePassword123!".to_string(),
            role: Role::Operator,
        };
        user_service.create_user(request).await.unwrap();
    });

    c.bench_function("password_verify_argon2id", |b| {
        b.to_async(&rt).iter(|| async {
            user_service
                .validate_credentials("benchuser", "SecurePassword123!")
                .await
                .unwrap();
        });
    });
}

/// Benchmark JWT token generation
fn bench_jwt_generation(c: &mut Criterion) {
    let test_secret: [u8; 32] = [
        0x1a, 0x2b, 0x3c, 0x4d, 0x5e, 0x6f, 0x70, 0x81,
        0x92, 0xa3, 0xb4, 0xc5, 0xd6, 0xe7, 0xf8, 0x09,
        0xfe, 0xdc, 0xba, 0x98, 0x76, 0x54, 0x32, 0x10,
        0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88,
    ];
    let auth_config = Arc::new(AuthConfig::from_secret_bytes(&test_secret).unwrap());

    c.bench_function("jwt_generate", |b| {
        b.iter(|| {
            auth_config
                .generate_token(
                    black_box("user123"),
                    black_box(Role::Admin),
                )
                .unwrap();
        });
    });
}

/// Benchmark JWT token validation
fn bench_jwt_validation(c: &mut Criterion) {
    let test_secret: [u8; 32] = [
        0x1a, 0x2b, 0x3c, 0x4d, 0x5e, 0x6f, 0x70, 0x81,
        0x92, 0xa3, 0xb4, 0xc5, 0xd6, 0xe7, 0xf8, 0x09,
        0xfe, 0xdc, 0xba, 0x98, 0x76, 0x54, 0x32, 0x10,
        0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88,
    ];
    let auth_config = Arc::new(AuthConfig::from_secret_bytes(&test_secret).unwrap());

    // Generate test token
    let token = auth_config.generate_token("user123", Role::Admin).unwrap();

    c.bench_function("jwt_validate", |b| {
        b.iter(|| {
            auth_config.validate_token(black_box(&token)).unwrap();
        });
    });
}

/// Benchmark concurrent authentication (simulates realistic load)
fn bench_concurrent_auth(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let kv_store = Arc::new(KvStore::new_in_memory().unwrap());
    let user_service = Arc::new(UserService::new(kv_store));

    // Create test users
    rt.block_on(async {
        for i in 0..10 {
            let request = CreateUserRequest {
                username: format!("concurrent_user_{}", i),
                password: "SecurePassword123!".to_string(),
                role: Role::Operator,
            };
            user_service.create_user(request).await.unwrap();
        }
    });

    let test_secret: [u8; 32] = [
        0x1a, 0x2b, 0x3c, 0x4d, 0x5e, 0x6f, 0x70, 0x81,
        0x92, 0xa3, 0xb4, 0xc5, 0xd6, 0xe7, 0xf8, 0x09,
        0xfe, 0xdc, 0xba, 0x98, 0x76, 0x54, 0x32, 0x10,
        0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88,
    ];
    let auth_config = Arc::new(AuthConfig::from_secret_bytes(&test_secret).unwrap());

    let mut group = c.benchmark_group("concurrent_auth");

    for concurrency in [1, 5, 10, 20].iter() {
        group.bench_with_input(
            BenchmarkId::from_parameter(concurrency),
            concurrency,
            |b, &concurrency| {
                b.to_async(&rt).iter(|| {
                    let user_service = user_service.clone();
                    let auth_config = auth_config.clone();

                    async move {
                        let mut handles = vec![];

                        for i in 0..concurrency {
                            let user_service = user_service.clone();
                            let auth_config = auth_config.clone();

                            let handle = tokio::spawn(async move {
                                let user_idx = i % 10;
                                let username = format!("concurrent_user_{}", user_idx);

                                // Validate credentials
                                let user = user_service
                                    .validate_credentials(&username, "SecurePassword123!")
                                    .await
                                    .unwrap();

                                // Generate JWT
                                auth_config
                                    .generate_token(&user.id, user.role)
                                    .unwrap();
                            });

                            handles.push(handle);
                        }

                        for handle in handles {
                            handle.await.unwrap();
                        }
                    }
                });
            },
        );
    }

    group.finish();
}

// ============================================================================
// Audit Logging Benchmarks
// ============================================================================

/// Benchmark audit event writing
fn bench_audit_write(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let kv_store = Arc::new(KvStore::new_in_memory().unwrap());
    let audit_sink = Arc::new(RocksAuditSink::new(kv_store));
    let audit_logger = Arc::new(AuditLogger::new(audit_sink));

    c.bench_function("audit_log_login_success", |b| {
        b.to_async(&rt).iter(|| async {
            audit_logger
                .log_login_success(
                    black_box("user123"),
                    black_box("testuser"),
                    black_box(Role::Admin),
                    Some("192.168.1.1".parse().unwrap()),
                    Some("Mozilla/5.0".to_string()),
                )
                .await
                .unwrap();
        });
    });
}

/// Benchmark audit logging with authentication (realistic scenario)
fn bench_auth_with_audit(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let kv_store = Arc::new(KvStore::new_in_memory().unwrap());
    let user_service = Arc::new(UserService::new(kv_store.clone()));

    let audit_kv_store = Arc::new(KvStore::new_in_memory().unwrap());
    let audit_sink = Arc::new(RocksAuditSink::new(audit_kv_store));
    let audit_logger = Arc::new(AuditLogger::new(audit_sink));

    let test_secret: [u8; 32] = [
        0x1a, 0x2b, 0x3c, 0x4d, 0x5e, 0x6f, 0x70, 0x81,
        0x92, 0xa3, 0xb4, 0xc5, 0xd6, 0xe7, 0xf8, 0x09,
        0xfe, 0xdc, 0xba, 0x98, 0x76, 0x54, 0x32, 0x10,
        0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88,
    ];
    let auth_config = Arc::new(AuthConfig::from_secret_bytes(&test_secret).unwrap());

    // Create test user
    rt.block_on(async {
        let request = CreateUserRequest {
            username: "audituser".to_string(),
            password: "SecurePassword123!".to_string(),
            role: Role::Operator,
        };
        user_service.create_user(request).await.unwrap();
    });

    c.bench_function("auth_full_flow_with_audit", |b| {
        b.to_async(&rt).iter(|| {
            let user_service = user_service.clone();
            let auth_config = auth_config.clone();
            let audit_logger = audit_logger.clone();

            async move {
                // Validate credentials
                let user = user_service
                    .validate_credentials("audituser", "SecurePassword123!")
                    .await
                    .unwrap();

                // Generate JWT
                let _token = auth_config
                    .generate_token(&user.id, user.role.clone())
                    .unwrap();

                // Audit log success
                audit_logger
                    .log_login_success(
                        &user.id,
                        &user.username,
                        user.role,
                        Some("192.168.1.1".parse().unwrap()),
                        None,
                    )
                    .await
                    .unwrap();
            }
        });
    });
}

// ============================================================================
// User Database Benchmarks
// ============================================================================

/// Benchmark user lookup by username
fn bench_user_lookup(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let kv_store = Arc::new(KvStore::new_in_memory().unwrap());
    let user_service = Arc::new(UserService::new(kv_store));

    // Create test users
    rt.block_on(async {
        for i in 0..100 {
            let request = CreateUserRequest {
                username: format!("lookup_user_{}", i),
                password: "SecurePassword123!".to_string(),
                role: Role::Viewer,
            };
            user_service.create_user(request).await.unwrap();
        }
    });

    c.bench_function("user_lookup_by_username", |b| {
        b.to_async(&rt).iter(|| async {
            // Lookup user (internally uses RocksDB get)
            user_service
                .validate_credentials(
                    black_box("lookup_user_50"),
                    black_box("WrongPassword123!"),
                )
                .await
                .ok(); // Ignore error (wrong password expected)
        });
    });
}

/// Benchmark user creation
fn bench_user_creation(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let kv_store = Arc::new(KvStore::new_in_memory().unwrap());
    let user_service = Arc::new(UserService::new(kv_store));

    let mut counter = 0u64;

    c.bench_function("user_create", |b| {
        b.to_async(&rt).iter(|| {
            let user_service = user_service.clone();
            let username = format!("create_user_{}", counter);
            counter += 1;

            async move {
                let request = CreateUserRequest {
                    username,
                    password: "SecurePassword123!".to_string(),
                    role: Role::Viewer,
                };
                user_service.create_user(request).await.unwrap();
            }
        });
    });
}

// ============================================================================
// Setup Token Benchmarks
// ============================================================================

/// Benchmark setup token generation
fn bench_setup_token_generation(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    c.bench_function("setup_token_generate", |b| {
        b.to_async(&rt).iter(|| async {
            let manager = SetupTokenManager::new();
            manager.generate_token().await.unwrap();
        });
    });
}

/// Benchmark setup token validation
fn bench_setup_token_validation(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let manager = Arc::new(SetupTokenManager::new());

    let token = rt.block_on(async {
        manager.generate_token().await.unwrap().token
    });

    c.bench_function("setup_token_validate", |b| {
        b.to_async(&rt).iter(|| {
            let manager = manager.clone();
            let token = token.clone();

            async move {
                // Note: This will fail on second iteration (token consumed)
                // But still benchmarks the validation path
                let _ = manager.validate_and_consume(&token).await;
            }
        });
    });
}

// ============================================================================
// Criterion Configuration
// ============================================================================

criterion_group!(
    auth_benches,
    bench_password_hashing,
    bench_password_verification,
    bench_jwt_generation,
    bench_jwt_validation,
    bench_concurrent_auth,
);

criterion_group!(
    audit_benches,
    bench_audit_write,
    bench_auth_with_audit,
);

criterion_group!(
    user_benches,
    bench_user_lookup,
    bench_user_creation,
);

criterion_group!(
    setup_benches,
    bench_setup_token_generation,
    bench_setup_token_validation,
);

criterion_main!(
    auth_benches,
    audit_benches,
    user_benches,
    setup_benches,
);
