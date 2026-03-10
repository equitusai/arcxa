//! Authentication Performance Tests
//!
//! Measure performance of security components under realistic load.
//! Run with: cargo test --test auth_performance_test --release -- --nocapture

use anyhow::Result;
use std::sync::Arc;
use std::time::Instant;

use graphica_coordinator::api::audit::{AuditLogger, RocksAuditSink};
use graphica_coordinator::api::auth::{AuthConfig, Role};
use graphica_coordinator::api::users::{CreateUserRequest, UserService};
use graphica_coordinator::storage::kv_store::KvStore;

#[tokio::test]
async fn perf_password_hashing() -> Result<()> {
    let kv_store = Arc::new(KvStore::new_in_memory()?);
    let user_service = Arc::new(UserService::new(kv_store));

    let iterations = 10;
    let start = Instant::now();

    for i in 0..iterations {
        let request = CreateUserRequest {
            username: format!("user_{}", i),
            password: "SecurePassword123!".to_string(),
            role: Role::Viewer,
        };
        user_service.create_user(request).await?;
    }

    let duration = start.elapsed();
    let avg_ms = duration.as_millis() / iterations;

    println!("\n╔══════════════════════════════════════════════════════════╗");
    println!("║         PASSWORD HASHING (Argon2id)                     ║");
    println!("╠══════════════════════════════════════════════════════════╣");
    println!(
        "║  Iterations:  {:>10}                                  ║",
        iterations
    );
    println!(
        "║  Total time:  {:>10.2?}                              ║",
        duration
    );
    println!(
        "║  Average:     {:>10} ms/op                           ║",
        avg_ms
    );
    println!(
        "║  Throughput:  {:>10.2} ops/sec                       ║",
        1000.0 / avg_ms as f64
    );
    println!("╚══════════════════════════════════════════════════════════╝\n");

    // Debug builds add significant overhead; keep stricter threshold for release runs.
    let max_avg_ms = if cfg!(debug_assertions) { 650 } else { 500 };

    // Performance assertions
    assert!(
        avg_ms < max_avg_ms,
        "Password hashing should be under {}ms (was {} ms)",
        max_avg_ms,
        avg_ms
    );

    Ok(())
}

#[tokio::test]
async fn perf_password_verification() -> Result<()> {
    let kv_store = Arc::new(KvStore::new_in_memory()?);
    let user_service = Arc::new(UserService::new(kv_store));

    // Create test user
    let request = CreateUserRequest {
        username: "perfuser".to_string(),
        password: "SecurePassword123!".to_string(),
        role: Role::Operator,
    };
    user_service.create_user(request).await?;

    let iterations = 10;
    let start = Instant::now();

    for _ in 0..iterations {
        user_service
            .validate_credentials("perfuser", "SecurePassword123!")
            .await?;
    }

    let duration = start.elapsed();
    let avg_ms = duration.as_millis() / iterations;

    println!("\n╔══════════════════════════════════════════════════════════╗");
    println!("║       PASSWORD VERIFICATION (Argon2id)                   ║");
    println!("╠══════════════════════════════════════════════════════════╣");
    println!(
        "║  Iterations:  {:>10}                                  ║",
        iterations
    );
    println!(
        "║  Total time:  {:>10.2?}                              ║",
        duration
    );
    println!(
        "║  Average:     {:>10} ms/op                           ║",
        avg_ms
    );
    println!(
        "║  Throughput:  {:>10.2} ops/sec                       ║",
        1000.0 / avg_ms as f64
    );
    println!("╚══════════════════════════════════════════════════════════╝\n");

    let max_avg_ms = if cfg!(debug_assertions) { 650 } else { 500 };

    assert!(
        avg_ms < max_avg_ms,
        "Password verification should be under {}ms (was {} ms)",
        max_avg_ms,
        avg_ms
    );

    Ok(())
}

#[tokio::test]
async fn perf_jwt_generation() -> Result<()> {
    let test_secret: [u8; 32] = [
        0x1a, 0x2b, 0x3c, 0x4d, 0x5e, 0x6f, 0x70, 0x81, 0x92, 0xa3, 0xb4, 0xc5, 0xd6, 0xe7, 0xf8,
        0x09, 0xfe, 0xdc, 0xba, 0x98, 0x76, 0x54, 0x32, 0x10, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66,
        0x77, 0x88,
    ];
    let auth_config = Arc::new(AuthConfig::from_secret_bytes(&test_secret)?);

    let iterations = 10000;
    let start = Instant::now();

    for _ in 0..iterations {
        auth_config.generate_token("user123", Role::Admin)?;
    }

    let duration = start.elapsed();
    let avg_us = duration.as_micros() / iterations;

    println!("\n╔══════════════════════════════════════════════════════════╗");
    println!("║              JWT TOKEN GENERATION                        ║");
    println!("╠══════════════════════════════════════════════════════════╣");
    println!(
        "║  Iterations:  {:>10}                                  ║",
        iterations
    );
    println!(
        "║  Total time:  {:>10.2?}                              ║",
        duration
    );
    println!(
        "║  Average:     {:>10} μs/op                           ║",
        avg_us
    );
    println!(
        "║  Throughput:  {:>10.0} ops/sec                       ║",
        1_000_000.0 / avg_us as f64
    );
    println!("╚══════════════════════════════════════════════════════════╝\n");

    assert!(
        avg_us < 1000,
        "JWT generation should be under 1ms (was {} μs)",
        avg_us
    );

    Ok(())
}

#[tokio::test]
async fn perf_jwt_validation() -> Result<()> {
    let test_secret: [u8; 32] = [
        0x1a, 0x2b, 0x3c, 0x4d, 0x5e, 0x6f, 0x70, 0x81, 0x92, 0xa3, 0xb4, 0xc5, 0xd6, 0xe7, 0xf8,
        0x09, 0xfe, 0xdc, 0xba, 0x98, 0x76, 0x54, 0x32, 0x10, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66,
        0x77, 0x88,
    ];
    let auth_config = Arc::new(AuthConfig::from_secret_bytes(&test_secret)?);

    let token = auth_config.generate_token("user123", Role::Admin)?;

    let iterations = 10000;
    let start = Instant::now();

    for _ in 0..iterations {
        auth_config.validate_token(&token)?;
    }

    let duration = start.elapsed();
    let avg_us = duration.as_micros() / iterations;

    println!("\n╔══════════════════════════════════════════════════════════╗");
    println!("║              JWT TOKEN VALIDATION                        ║");
    println!("╠══════════════════════════════════════════════════════════╣");
    println!(
        "║  Iterations:  {:>10}                                  ║",
        iterations
    );
    println!(
        "║  Total time:  {:>10.2?}                              ║",
        duration
    );
    println!(
        "║  Average:     {:>10} μs/op                           ║",
        avg_us
    );
    println!(
        "║  Throughput:  {:>10.0} ops/sec                       ║",
        1_000_000.0 / avg_us as f64
    );
    println!("╚══════════════════════════════════════════════════════════╝\n");

    assert!(
        avg_us < 500,
        "JWT validation should be under 500μs (was {} μs)",
        avg_us
    );

    Ok(())
}

#[tokio::test]
async fn perf_audit_logging() -> Result<()> {
    let kv_store = Arc::new(KvStore::new_in_memory()?);
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
            .await?;
    }

    let duration = start.elapsed();
    let avg_us = duration.as_micros() / iterations;

    println!("\n╔══════════════════════════════════════════════════════════╗");
    println!("║            AUDIT LOG WRITE (RocksDB)                     ║");
    println!("╠══════════════════════════════════════════════════════════╣");
    println!(
        "║  Iterations:  {:>10}                                  ║",
        iterations
    );
    println!(
        "║  Total time:  {:>10.2?}                              ║",
        duration
    );
    println!(
        "║  Average:     {:>10} μs/op                           ║",
        avg_us
    );
    println!(
        "║  Throughput:  {:>10.0} ops/sec                       ║",
        1_000_000.0 / avg_us as f64
    );
    println!("╚══════════════════════════════════════════════════════════╝\n");

    assert!(
        avg_us < 10000,
        "Audit logging should be under 10ms (was {} μs)",
        avg_us
    );

    Ok(())
}

#[tokio::test]
async fn perf_full_auth_flow() -> Result<()> {
    let kv_store = Arc::new(KvStore::new_in_memory()?);
    let user_service = Arc::new(UserService::new(kv_store.clone()));

    let audit_kv_store = Arc::new(KvStore::new_in_memory()?);
    let audit_sink = Arc::new(RocksAuditSink::new(audit_kv_store));
    let audit_logger = Arc::new(AuditLogger::new(audit_sink));

    let test_secret: [u8; 32] = [
        0x1a, 0x2b, 0x3c, 0x4d, 0x5e, 0x6f, 0x70, 0x81, 0x92, 0xa3, 0xb4, 0xc5, 0xd6, 0xe7, 0xf8,
        0x09, 0xfe, 0xdc, 0xba, 0x98, 0x76, 0x54, 0x32, 0x10, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66,
        0x77, 0x88,
    ];
    let auth_config = Arc::new(AuthConfig::from_secret_bytes(&test_secret)?);

    // Create test user
    let request = CreateUserRequest {
        username: "flowuser".to_string(),
        password: "SecurePassword123!".to_string(),
        role: Role::Operator,
    };
    user_service.create_user(request).await?;

    let iterations = 10;
    let start = Instant::now();

    for _ in 0..iterations {
        // Full authentication flow: verify + JWT + audit
        let user = user_service
            .validate_credentials("flowuser", "SecurePassword123!")
            .await?;

        let _token = auth_config.generate_token(&user.id, user.role.clone())?;

        audit_logger
            .log_login_success(
                &user.id,
                &user.username,
                user.role,
                Some("192.168.1.1".parse().unwrap()),
                None,
            )
            .await?;
    }

    let duration = start.elapsed();
    let avg_ms = duration.as_millis() / iterations;

    println!("\n╔══════════════════════════════════════════════════════════╗");
    println!("║     FULL AUTH FLOW (Verify + JWT + Audit)               ║");
    println!("╠══════════════════════════════════════════════════════════╣");
    println!(
        "║  Iterations:  {:>10}                                  ║",
        iterations
    );
    println!(
        "║  Total time:  {:>10.2?}                              ║",
        duration
    );
    println!(
        "║  Average:     {:>10} ms/op                           ║",
        avg_ms
    );
    println!(
        "║  Throughput:  {:>10.2} ops/sec                       ║",
        1000.0 / avg_ms as f64
    );
    println!("╚══════════════════════════════════════════════════════════╝\n");

    assert!(
        avg_ms < 1000,
        "Full auth flow should be under 1s (was {} ms)",
        avg_ms
    );

    Ok(())
}

#[tokio::test]
async fn perf_concurrent_auth() -> Result<()> {
    let kv_store = Arc::new(KvStore::new_in_memory()?);
    let user_service = Arc::new(UserService::new(kv_store));

    // Create test users
    for i in 0..10 {
        let request = CreateUserRequest {
            username: format!("concurrent_{}", i),
            password: "SecurePassword123!".to_string(),
            role: Role::Operator,
        };
        user_service.create_user(request).await?;
    }

    let test_secret: [u8; 32] = [
        0x1a, 0x2b, 0x3c, 0x4d, 0x5e, 0x6f, 0x70, 0x81, 0x92, 0xa3, 0xb4, 0xc5, 0xd6, 0xe7, 0xf8,
        0x09, 0xfe, 0xdc, 0xba, 0x98, 0x76, 0x54, 0x32, 0x10, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66,
        0x77, 0x88,
    ];
    let auth_config = Arc::new(AuthConfig::from_secret_bytes(&test_secret)?);

    println!("\n╔══════════════════════════════════════════════════════════╗");
    println!("║           CONCURRENT AUTHENTICATION LOAD                 ║");
    println!("╠══════════════════════════════════════════════════════════╣");

    // Test different concurrency levels
    for concurrency in [1, 5, 10, 20] {
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
        let ops_per_sec = concurrency as f64 / duration.as_secs_f64();

        println!(
            "║  Concurrency: {:>3}                                       ║",
            concurrency
        );
        println!(
            "║    Total time:  {:>8.2?}                               ║",
            duration
        );
        println!(
            "║    Throughput:  {:>8.0} ops/sec                       ║",
            ops_per_sec
        );
        println!("║                                                          ║");
    }

    println!("╚══════════════════════════════════════════════════════════╝\n");

    Ok(())
}
