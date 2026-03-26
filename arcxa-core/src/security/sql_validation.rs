//! SQL Injection Prevention
//!
//! Validates SQL identifiers to prevent injection attacks.

use anyhow::{anyhow, Result};
use lazy_static::lazy_static;
use regex::Regex;

lazy_static! {
    /// Valid SQL identifier pattern
    static ref IDENTIFIER_REGEX: Regex =
        Regex::new(r"^[a-zA-Z_][a-zA-Z0-9_]{0,127}$").unwrap();
}

/// Database types for identifier quoting
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatabaseType {
    PostgreSQL,
    MySQL,
    DB2,
    Snowflake,
    Oracle,
}

/// Validate SQL identifier
///
/// # Security
///
/// Prevents SQL injection by:
/// - Allowing only [a-zA-Z0-9_]
/// - Rejecting reserved keywords
/// - Limiting length to 128 chars
///
/// # Examples
///
/// ```
/// use graphica_core::security::validate_identifier;
///
/// assert!(validate_identifier("customers").is_ok());
/// assert!(validate_identifier("users; DROP TABLE").is_err());
/// ```
pub fn validate_identifier(identifier: &str) -> Result<&str> {
    // Empty check
    if identifier.is_empty() {
        return Err(anyhow!("SQL identifier cannot be empty"));
    }

    // Length check
    if identifier.len() > 128 {
        return Err(anyhow!(
            "SQL identifier too long (max 128 chars): {}",
            identifier
        ));
    }

    // Pattern check
    if !IDENTIFIER_REGEX.is_match(identifier) {
        return Err(anyhow!(
            "Invalid SQL identifier '{}': must contain only alphanumeric and underscore",
            identifier
        ));
    }

    // Reserved keyword check
    let upper = identifier.to_uppercase();
    if is_reserved_keyword(&upper) {
        return Err(anyhow!(
            "SQL identifier '{}' is a reserved keyword",
            identifier
        ));
    }

    Ok(identifier)
}

/// Check if keyword is SQL reserved
fn is_reserved_keyword(word: &str) -> bool {
    const RESERVED: &[&str] = &[
        "SELECT",
        "INSERT",
        "UPDATE",
        "DELETE",
        "DROP",
        "CREATE",
        "ALTER",
        "TABLE",
        "DATABASE",
        "INDEX",
        "VIEW",
        "PROCEDURE",
        "FUNCTION",
        "TRIGGER",
        "FROM",
        "WHERE",
        "JOIN",
        "UNION",
        "AND",
        "OR",
        "NOT",
        "NULL",
        "TRUE",
        "FALSE",
    ];

    RESERVED.contains(&word)
}

/// Validate qualified identifier (schema.table)
pub fn validate_qualified_identifier(identifier: &str) -> Result<String> {
    let parts: Vec<&str> = identifier.split('.').collect();

    if parts.is_empty() {
        return Err(anyhow!("Qualified identifier cannot be empty"));
    }

    if parts.len() > 3 {
        return Err(anyhow!(
            "Too many parts in qualified identifier (max 3): {}",
            identifier
        ));
    }

    for part in &parts {
        validate_identifier(part)?;
    }

    Ok(identifier.to_string())
}

/// Quote identifier for SQL
pub fn quote_identifier(identifier: &str, db_type: DatabaseType) -> Result<String> {
    let validated = validate_identifier(identifier)?;

    match db_type {
        DatabaseType::PostgreSQL | DatabaseType::DB2 => Ok(format!("\"{}\"", validated)),
        DatabaseType::MySQL => Ok(format!("`{}`", validated)),
        DatabaseType::Snowflake => Ok(validated.to_uppercase()),
        DatabaseType::Oracle => Ok(format!("\"{}\"", validated)),
    }
}

/// Validate SQL data type
///
/// # Security
///
/// Prevents SQL injection via data type specifications by:
/// - Allowing standard SQL types (INTEGER, VARCHAR, DECIMAL, etc.)
/// - Allowing type parameters (e.g., VARCHAR(255), DECIMAL(10,2))
/// - Rejecting semicolons, comments, and SQL keywords outside type context
///
/// # Examples
///
/// ```
/// use graphica_core::security::validate_sql_type;
///
/// assert!(validate_sql_type("INTEGER").is_ok());
/// assert!(validate_sql_type("VARCHAR(255)").is_ok());
/// assert!(validate_sql_type("DECIMAL(10,2)").is_ok());
/// assert!(validate_sql_type("INTEGER; DROP TABLE").is_err());
/// ```
pub fn validate_sql_type(sql_type: &str) -> Result<&str> {
    // Empty check
    if sql_type.is_empty() {
        return Err(anyhow!("SQL type cannot be empty"));
    }

    // Length check (reasonable max for type definitions)
    if sql_type.len() > 100 {
        return Err(anyhow!("SQL type too long (max 100 chars): {}", sql_type));
    }

    // Check for injection characters
    if sql_type.contains(';') {
        return Err(anyhow!("SQL type contains semicolon: {}", sql_type));
    }

    if sql_type.contains("--") || sql_type.contains("/*") || sql_type.contains("*/") {
        return Err(anyhow!("SQL type contains comment syntax: {}", sql_type));
    }

    // Pattern validation for SQL types
    // Allowed: BASE_TYPE or BASE_TYPE(params)
    // Examples: INTEGER, VARCHAR(255), DECIMAL(10,2), TIMESTAMP WITH TIME ZONE
    let type_pattern = regex::Regex::new(r"^[A-Z][A-Z0-9 ]*(\([0-9]+(,[0-9]+)?\))?$").unwrap();

    if !type_pattern.is_match(sql_type) {
        return Err(anyhow!(
            "Invalid SQL type format '{}': must be uppercase type with optional numeric parameters",
            sql_type
        ));
    }

    // Check for dangerous SQL keywords that shouldn't appear in types
    let upper = sql_type.to_uppercase();
    let dangerous_keywords = &[
        "DROP", "DELETE", "INSERT", "UPDATE", "UNION", "SELECT", "FROM", "WHERE", "EXEC",
        "EXECUTE", "ALTER",
    ];

    for keyword in dangerous_keywords {
        if upper.contains(keyword) {
            return Err(anyhow!(
                "SQL type contains dangerous keyword '{}': {}",
                keyword,
                sql_type
            ));
        }
    }

    Ok(sql_type)
}

/// Validate foreign key action
///
/// # Security
///
/// Prevents SQL injection via foreign key ON DELETE/ON UPDATE actions by:
/// - Allowing only standard FK actions (CASCADE, SET NULL, etc.)
/// - Rejecting arbitrary SQL
///
/// # Examples
///
/// ```
/// use graphica_core::security::validate_fk_action;
///
/// assert!(validate_fk_action("CASCADE").is_ok());
/// assert!(validate_fk_action("SET NULL").is_ok());
/// assert!(validate_fk_action("DROP TABLE").is_err());
/// ```
pub fn validate_fk_action(action: &str) -> Result<&str> {
    // Valid foreign key actions per SQL standard
    const VALID_ACTIONS: &[&str] = &[
        "CASCADE",
        "SET NULL",
        "SET DEFAULT",
        "RESTRICT",
        "NO ACTION",
    ];

    // Trim and uppercase for comparison
    let trimmed = action.trim();
    let upper = trimmed.to_uppercase();

    // Normalize internal whitespace (collapse multiple spaces)
    let normalized = upper.split_whitespace().collect::<Vec<&str>>().join(" ");

    if VALID_ACTIONS.contains(&normalized.as_str()) {
        Ok(action)
    } else {
        Err(anyhow!(
            "Invalid foreign key action '{}': must be one of {:?}",
            action,
            VALID_ACTIONS
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_identifiers() {
        assert!(validate_identifier("customers").is_ok());
        assert!(validate_identifier("Customer_Table").is_ok());
        assert!(validate_identifier("_private").is_ok());
        assert!(validate_identifier("table123").is_ok());
    }

    #[test]
    fn test_invalid_identifiers() {
        // SQL injection attempt
        assert!(validate_identifier("users; DROP TABLE users").is_err());

        // Invalid characters
        assert!(validate_identifier("table-name").is_err());
        assert!(validate_identifier("table name").is_err());
        assert!(validate_identifier("table@name").is_err());

        // Reserved keywords
        assert!(validate_identifier("SELECT").is_err());
        assert!(validate_identifier("DROP").is_err());

        // Empty
        assert!(validate_identifier("").is_err());

        // Too long
        let long_name = "a".repeat(129);
        assert!(validate_identifier(&long_name).is_err());
    }

    #[test]
    fn test_qualified_identifiers() {
        assert!(validate_qualified_identifier("sales.customers").is_ok());
        assert!(validate_qualified_identifier("warehouse.sales.customers").is_ok());

        // Too many parts
        assert!(validate_qualified_identifier("a.b.c.d").is_err());

        // Invalid part
        assert!(validate_qualified_identifier("schema.DROP").is_err());
    }

    #[test]
    fn test_quote_identifier() {
        let id = "customers";

        assert_eq!(
            quote_identifier(id, DatabaseType::PostgreSQL).unwrap(),
            "\"customers\""
        );

        assert_eq!(
            quote_identifier(id, DatabaseType::MySQL).unwrap(),
            "`customers`"
        );

        assert_eq!(
            quote_identifier(id, DatabaseType::Snowflake).unwrap(),
            "CUSTOMERS"
        );
    }

    /// Regression tests for SQL injection vulnerabilities found in Day 5 audit
    /// These tests verify that the specific attack vectors from the audit are blocked
    #[test]
    fn test_sql_injection_table_name_attacks() {
        // Semicolon-based stacked query injection
        assert!(validate_identifier("users; DROP TABLE users; --").is_err());
        assert!(validate_identifier("admin; DELETE FROM users; --").is_err());

        // Comment-based injection
        assert!(validate_identifier("users--").is_err());
        assert!(validate_identifier("users/*comment*/").is_err());

        // Quote-based injection
        assert!(validate_identifier("users' OR '1'='1").is_err());
        assert!(validate_identifier("users\" OR \"1\"=\"1").is_err());

        // UNION-based injection
        assert!(validate_identifier("users UNION SELECT").is_err());

        // Whitespace injection
        assert!(validate_identifier("users WHERE 1=1").is_err());
    }

    #[test]
    fn test_sql_injection_column_name_attacks() {
        // Column wildcard expansion injection
        assert!(validate_identifier("*, password FROM admin_users WHERE '1'='1").is_err());
        assert!(validate_identifier("id, * FROM admin_users WHERE").is_err());

        // Subquery injection via column
        assert!(validate_identifier("(SELECT password FROM admin)").is_err());

        // Function call injection
        assert!(validate_identifier("COUNT(*)").is_err()); // Contains parentheses
        assert!(validate_identifier("user.password").is_err()); // Contains dot (use validate_qualified_identifier instead)
    }

    #[test]
    fn test_sql_injection_reserved_keywords() {
        // SQL keywords that should be rejected
        assert!(validate_identifier("SELECT").is_err());
        assert!(validate_identifier("DROP").is_err());
        assert!(validate_identifier("DELETE").is_err());
        assert!(validate_identifier("INSERT").is_err());
        assert!(validate_identifier("UPDATE").is_err());
        assert!(validate_identifier("UNION").is_err());
        assert!(validate_identifier("WHERE").is_err());
        assert!(validate_identifier("FROM").is_err());

        // Case variations should also be rejected
        assert!(validate_identifier("Select").is_err());
        assert!(validate_identifier("drop").is_err());
    }

    #[test]
    fn test_sql_injection_edge_cases() {
        // Empty string
        assert!(validate_identifier("").is_err());

        // Only special characters
        assert!(validate_identifier(";;;").is_err());
        assert!(validate_identifier("---").is_err());
        assert!(validate_identifier("/**/").is_err());

        // Mixed valid/invalid
        assert!(validate_identifier("valid_name; DROP TABLE").is_err());

        // Unicode/unusual characters
        assert!(validate_identifier("table\0name").is_err()); // Null byte
        assert!(validate_identifier("table\nname").is_err()); // Newline
    }

    #[test]
    fn test_valid_edge_cases() {
        // These should all be valid
        assert!(validate_identifier("_").is_ok()); // Single underscore
        assert!(validate_identifier("_table").is_ok()); // Leading underscore
        assert!(validate_identifier("TABLE123").is_ok()); // Numbers OK if not leading
        assert!(validate_identifier("a_b_c_d").is_ok()); // Multiple underscores
        assert!(validate_identifier("CamelCase123").is_ok()); // Mixed case with numbers

        // Max length should be accepted
        let max_length_name = "a".repeat(128);
        assert!(validate_identifier(&max_length_name).is_ok());
    }

    /// Tests for SQL type validation (Sprint 2)
    #[test]
    fn test_validate_sql_type_valid() {
        // Basic types
        assert!(validate_sql_type("INTEGER").is_ok());
        assert!(validate_sql_type("BIGINT").is_ok());
        assert!(validate_sql_type("TEXT").is_ok());
        assert!(validate_sql_type("BOOLEAN").is_ok());
        assert!(validate_sql_type("TIMESTAMP").is_ok());

        // Types with parameters
        assert!(validate_sql_type("VARCHAR(255)").is_ok());
        assert!(validate_sql_type("CHAR(10)").is_ok());
        assert!(validate_sql_type("DECIMAL(10,2)").is_ok());
        assert!(validate_sql_type("NUMERIC(18,4)").is_ok());

        // Types with spaces (allowed in type names like TIMESTAMP WITH TIME ZONE)
        assert!(validate_sql_type("TIMESTAMP WITH TIME ZONE").is_ok());
        assert!(validate_sql_type("DOUBLE PRECISION").is_ok());
    }

    #[test]
    fn test_validate_sql_type_invalid() {
        // SQL injection attempts
        assert!(validate_sql_type("INTEGER; DROP TABLE users").is_err());
        assert!(validate_sql_type("VARCHAR(255); DELETE FROM").is_err());
        assert!(validate_sql_type("TEXT--comment").is_err());
        assert!(validate_sql_type("INT/*comment*/EGER").is_err());

        // Dangerous keywords
        assert!(validate_sql_type("DROP TABLE").is_err());
        assert!(validate_sql_type("DELETE FROM").is_err());
        assert!(validate_sql_type("UNION SELECT").is_err());
        assert!(validate_sql_type("INSERT INTO").is_err());

        // Invalid format
        assert!(validate_sql_type("lowercase").is_err()); // Must be uppercase
        assert!(validate_sql_type("123INVALID").is_err()); // Can't start with number
        assert!(validate_sql_type("TYPE@NAME").is_err()); // Invalid characters
        assert!(validate_sql_type("VARCHAR(abc)").is_err()); // Non-numeric parameter

        // Empty/too long
        assert!(validate_sql_type("").is_err());
        let too_long = "A".repeat(101);
        assert!(validate_sql_type(&too_long).is_err());
    }

    #[test]
    fn test_validate_sql_type_edge_cases() {
        // Semicolon injection
        assert!(validate_sql_type("INT;").is_err());
        assert!(validate_sql_type(";INTEGER").is_err());

        // Comment injection
        assert!(validate_sql_type("VARCHAR--").is_err());
        assert!(validate_sql_type("/*TEXT*/").is_err());

        // Mixed case (should fail - requires uppercase)
        assert!(validate_sql_type("Integer").is_err());
        assert!(validate_sql_type("VarChar(255)").is_err());
    }

    /// Tests for foreign key action validation (Sprint 2)
    #[test]
    fn test_validate_fk_action_valid() {
        // All valid FK actions
        assert!(validate_fk_action("CASCADE").is_ok());
        assert!(validate_fk_action("SET NULL").is_ok());
        assert!(validate_fk_action("SET DEFAULT").is_ok());
        assert!(validate_fk_action("RESTRICT").is_ok());
        assert!(validate_fk_action("NO ACTION").is_ok());

        // Case variations (should be accepted)
        assert!(validate_fk_action("cascade").is_ok());
        assert!(validate_fk_action("Cascade").is_ok());
        assert!(validate_fk_action("set null").is_ok());
        assert!(validate_fk_action("Set Null").is_ok());
    }

    #[test]
    fn test_validate_fk_action_invalid() {
        // SQL injection attempts
        assert!(validate_fk_action("CASCADE; DROP TABLE").is_err());
        assert!(validate_fk_action("SET NULL; DELETE FROM").is_err());
        assert!(validate_fk_action("RESTRICT--comment").is_err());

        // Invalid actions
        assert!(validate_fk_action("DROP TABLE").is_err());
        assert!(validate_fk_action("DELETE").is_err());
        assert!(validate_fk_action("UPDATE").is_err());
        assert!(validate_fk_action("INVALID").is_err());

        // Empty
        assert!(validate_fk_action("").is_err());

        // Partial matches (must be exact)
        assert!(validate_fk_action("SET").is_err());
        assert!(validate_fk_action("NULL").is_err());
    }

    #[test]
    fn test_validate_fk_action_edge_cases() {
        // Extra whitespace should be handled gracefully (trimmed)
        assert!(validate_fk_action("CASCADE ").is_ok()); // Trailing space trimmed
        assert!(validate_fk_action(" CASCADE").is_ok()); // Leading space trimmed
        assert!(validate_fk_action("SET  NULL").is_ok()); // Double space normalized

        // Semicolon injection still blocked
        assert!(validate_fk_action("CASCADE;").is_err());
        assert!(validate_fk_action(";CASCADE").is_err());
        assert!(validate_fk_action("CASCADE; DROP TABLE").is_err());
    }
}
