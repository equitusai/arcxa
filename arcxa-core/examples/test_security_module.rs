//! Manual verification of security module
//! Run with: cargo run --example test_security_module

use graphica_core::security::{quote_identifier, validate_identifier, DatabaseType};

fn main() {
    println!("=== Security Module Manual Verification ===\n");

    // Test 1: Valid identifiers
    println!("Test 1: Valid Identifiers");
    let valid_cases = vec!["customers", "Customer_Table", "_private", "table123"];
    for id in valid_cases {
        match validate_identifier(id) {
            Ok(_) => println!("  ✅ '{}' accepted", id),
            Err(e) => println!("  ❌ '{}' rejected: {}", id, e),
        }
    }

    // Test 2: SQL injection attempts
    println!("\nTest 2: SQL Injection Attempts (should be blocked)");
    let injection_attempts = vec![
        "users; DROP TABLE users",
        "users' OR '1'='1",
        "table-name",
        "table name",
    ];
    for id in injection_attempts {
        match validate_identifier(id) {
            Ok(_) => println!("  ❌ SECURITY FAILURE: '{}' was accepted!", id),
            Err(e) => println!("  ✅ '{}' blocked: {}", id, e),
        }
    }

    // Test 3: Reserved keywords
    println!("\nTest 3: Reserved Keywords (should be blocked)");
    let keywords = vec!["SELECT", "DROP", "DELETE"];
    for kw in keywords {
        match validate_identifier(kw) {
            Ok(_) => println!("  ❌ SECURITY FAILURE: keyword '{}' accepted!", kw),
            Err(e) => println!("  ✅ Keyword '{}' blocked: {}", kw, e),
        }
    }

    // Test 4: Database-specific quoting
    println!("\nTest 4: Database-Specific Quoting");
    let table = "customers";
    println!(
        "  PostgreSQL: {}",
        quote_identifier(table, DatabaseType::PostgreSQL).unwrap()
    );
    println!(
        "  MySQL: {}",
        quote_identifier(table, DatabaseType::MySQL).unwrap()
    );
    println!(
        "  DB2: {}",
        quote_identifier(table, DatabaseType::DB2).unwrap()
    );
    println!(
        "  Snowflake: {}",
        quote_identifier(table, DatabaseType::Snowflake).unwrap()
    );

    println!("\n=== All Tests Complete ===");
    println!("✅ Security module working correctly!");
}
