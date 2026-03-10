//! Quick performance tests for security components
//!
//! Run with: cargo test --test performance_test --release -- --nocapture

use graphica_coordinator::api::{
    audit::{AuditLogger, RocksAuditSink},
    auth::{AuthConfig, Role},
    users::{CreateUserRequest, UserService},
};
use graphica_coordinator::storage::kv_store::KvStore;
use std::sync::Arc;
use std::time::Instant;

#[tokio::test]
async fn perf_password_hashing() {
    let kv_store = Arc::new(KvStore::new_in_memory().unwrap());
    let user_service = Arc::new(UserService::new(kv_store));

    let iterations = 10;
    let start = Instant::now();

    for i in 0..iterations {
        let request = CreateUserRequest {
            username: format!("user_{}", i),
            password: "SecurePassword123!".to_string(),
            role: Role::Viewer,
        };
        user_service.create_user(request).await.unwrap();
    }

    let duration = start.elapsed();
    let avg_ms = duration.as_millis() / iterations;

    println!("\n=== Password Hashing (Argon2id) ===");
    println!("Iterations: {}", iterations);
    println!("Total time: {:?}", duration);
    println!("Average: {} ms/op", avg_ms);
    println!("Throughput: {:.2} ops/sec", 1000.0 / avg_ms as f64);
}

#[tokio::test]
async fn perf_password_verification() {
    let kv_store = Arc::new(KvStore::new_in_memory().unwrap());
    let user_service = Arc::new(UserService::new(kv_store));

    // Create test user
    let request = CreateUserRequest {
        username: "perfuser".to_string(),
        password: "SecurePassword123!".to_string(),
        role: Role::Operator,
    };
    user_service.create_user(request).await.unwrap();

    let iterations = 10;
    let start = Instant::now();

    for _ in 0..iterations {
        user_service
            .validate_credentials("perfuser", "SecurePassword123!")
            .await
            .unwrap();
    }

    let duration = start.elapsed();
    let avg_ms = duration.as_millis() / iterations;

    println!("\n=== Password Verification (Argon2id) ===");
    println!("Iterations: {}", iterations);
    println!("Total time: {:?}", duration);
    println!("Average: {} ms/op", avg_ms);
    println!("Throughput: {:.2} ops/sec", 1000.0 / avg_ms as f64);
}

#[test]
fn perf_jwt_generation() {
    let test_secret: [u8; 32] = [
        0x1a, 0x2b, 0x3c, 0x4d, 0x5e, 0x6f, 0x70, 0x81, 0x92, 0xa3, 0xb4, 0xc5, 0xd6, 0xe7, 0xf8,
        0x09, 0xfe, 0xdc, 0xba, 0x98, 0x76, 0x54, 0x32, 0x10, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66,
        0x77, 0x88,
    ];
    let auth_config = Arc::new(AuthConfig::from_secret_bytes(&test_secret).unwrap());

    let iterations = 10000;
    let start = Instant::now();

    for _ in 0..iterations {
        auth_config.generate_token("user123", Role::Admin).unwrap();
    }

    let duration = start.elapsed();
    let avg_us = duration.as_micros() / iterations;

    println!("\n=== JWT Generation ===");
    println!("Iterations: {}", iterations);
    println!("Total time: {:?}", duration);
    println!("Average: {} μs/op", avg_us);
    println!("Throughput: {:.0} ops/sec", 1_000_000.0 / avg_us as f64);
}

#[test]
fn perf_jwt_validation() {
    let test_secret: [u8; 32] = [
        0x1a, 0x2b, 0x3c, 0x4d, 0x5e, 0x6f, 0x70, 0x81, 0x92, 0xa3, 0xb4, 0xc5, 0xd6, 0xe7, 0xf8,
        0x09, 0xfe, 0xdc, 0xba, 0x98, 0x76, 0x54, 0x32, 0x10, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66,
        0x77, 0x88,
    ];
    let auth_config = Arc::new(AuthConfig::from_secret_bytes(&test_secret).unwrap());

    let token = auth_config.generate_token("user123", Role::Admin).unwrap();

    let iterations = 10000;
    let start = Instant::now();

    for _ in 0..iterations {
        auth_config.validate_token(&token).unwrap();
    }

    let duration = start.elapsed();
    let avg_us = duration.as_micros() / iterations;

    println!("\n=== JWT Validation ===");
    println!("Iterations: {}", iterations);
    println!("Total time: {:?}", duration);
    println!("Average: {} μs/op", avg_us);
    println!("Throughput: {:.0} ops/sec", 1_000_000.0 / avg_us as f64);
}

#[tokio::test]
async fn perf_audit_logging() {
    let kv_store = Arc::new(KvStore::new_in_memory().unwrap());
    let audit_sink = Arc::new(RocksAuditSink::new(kv_store));
    let audit_logger = Arc::new(AuditLogger::new(audit_sink));

    let iterations = 1000;
    let start = Instant::now();

    for _ in 0..iterations {
        audit_logger
            .log_login_success(
                "user123",
                "testuser",
                Role::Admin,
                Some("192.168.1.1".parse().unwrap()),
                Some("Mozilla/5.0".to_string()),
            )
            .await
            .unwrap();
    }

    let duration = start.elapsed();
    let avg_us = duration.as_micros() / iterations;

    println!("\n=== Audit Log Write ===");
    println!("Iterations: {}", iterations);
    println!("Total time: {:?}", duration);
    println!("Average: {} μs/op", avg_us);
    println!("Throughput: {:.0} ops/sec", 1_000_000.0 / avg_us as f64);
}

#[tokio::test]
async fn perf_full_auth_flow_with_audit() {
    let kv_store = Arc::new(KvStore::new_in_memory().unwrap());
    let user_service = Arc::new(UserService::new(kv_store.clone()));

    let audit_kv_store = Arc::new(KvStore::new_in_memory().unwrap());
    let audit_sink = Arc::new(RocksAuditSink::new(audit_kv_store));
    let audit_logger = Arc::new(AuditLogger::new(audit_sink));

    let test_secret: [u8; 32] = [
        0x1a, 0x2b, 0x3c, 0x4d, 0x5e, 0x6f, 0x70, 0x81, 0x92, 0xa3, 0xb4, 0xc5, 0xd6, 0xe7, 0xf8,
        0x09, 0xfe, 0xdc, 0xba, 0x98, 0x76, 0x54, 0x32, 0x10, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66,
        0x77, 0x88,
    ];
    let auth_config = Arc::new(AuthConfig::from_secret_bytes(&test_secret).unwrap());

    // Create test user
    let request = CreateUserRequest {
        username: "flowuser".to_string(),
        password: "SecurePassword123!".to_string(),
        role: Role::Operator,
    };
    user_service.create_user(request).await.unwrap();

    let iterations = 10;
    let start = Instant::now();

    for _ in 0..iterations {
        // Full authentication flow
        let user = user_service
            .validate_credentials("flowuser", "SecurePassword123!")
            .await
            .unwrap();

        let _token = auth_config
            .generate_token(&user.id, user.role.clone())
            .unwrap();

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

    let duration = start.elapsed();
    let avg_ms = duration.as_millis() / iterations;

    println!("\n=== Full Auth Flow (Verify + JWT + Audit) ===");
    println!("Iterations: {}", iterations);
    println!("Total time: {:?}", duration);
    println!("Average: {} ms/op", avg_ms);
    println!("Throughput: {:.2} ops/sec", 1000.0 / avg_ms as f64);
}

#[tokio::test]
async fn perf_concurrent_auth() {
    let kv_store = Arc::new(KvStore::new_in_memory().unwrap());
    let user_service = Arc::new(UserService::new(kv_store));

    // Create test users
    for i in 0..10 {
        let request = CreateUserRequest {
            username: format!("concurrent_{}", i),
            password: "SecurePassword123!".to_string(),
            role: Role::Operator,
        };
        user_service.create_user(request).await.unwrap();
    }

    let test_secret: [u8; 32] = [
        0x1a, 0x2b, 0x3c, 0x4d, 0x5e, 0x6f, 0x70, 0x81, 0x92, 0xa3, 0xb4, 0xc5, 0xd6, 0xe7, 0xf8,
        0x09, 0xfe, 0xdc, 0xba, 0x98, 0x76, 0x54, 0x32, 0x10, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66,
        0x77, 0x88,
    ];
    let auth_config = Arc::new(AuthConfig::from_secret_bytes(&test_secret).unwrap());

    // Test different concurrency levels
    for concurrency in [1, 5, 10, 20] {
        let iterations = concurrency * 2; // Each task does 2 operations
        let start = Instant::now();

        let mut handles = vec![];
        for i in 0..concurrency {
            let user_service = user_service.clone();
            let auth_config = auth_config.clone();

            let handle = tokio::spawn(async move {
                let user_idx = i % 10;
                let username = format!("concurrent_{}", user_idx);

                let user = user_service
                    .validate_credentials(&username, "SecurePassword123!")
                    .await
                    .unwrap();

                auth_config.generate_token(&user.id, user.role).unwrap();
            });

            handles.push(handle);
        }

        for handle in handles {
            handle.await.unwrap();
        }

        let duration = start.elapsed();
        let avg_ms = duration.as_millis() / iterations as u128;

        println!("\n=== Concurrent Auth (Concurrency: {}) ===", concurrency);
        println!("Total operations: {}", iterations);
        println!("Total time: {:?}", duration);
        println!("Average: {} ms/op", avg_ms);
        println!(
            "Throughput: {:.2} ops/sec",
            1000.0 * iterations as f64 / duration.as_millis() as f64
        );
    }
}
