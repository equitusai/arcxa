//! Type Conversion Rules Engine
//!
//! Provides comprehensive type conversion rules for cross-source data mapping:
//! - Safe vs lossy vs invalid conversion detection
//! - Multi-dialect SQL conversion functions
//! - Conversion validation and warnings

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::schema::UniversalDataType;

/// SQL dialect for conversion functions
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SqlDialect {
    PostgreSQL,
    MySQL,
    Oracle,
    DB2,
    SQLServer,
    Snowflake,
    Generic,
}

/// Conversion safety classification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConversionSafety {
    /// Conversion is safe with no data loss
    Safe,
    /// Conversion may lose precision or information
    Lossy,
    /// Conversion is not supported/recommended
    Invalid,
}

/// Detailed conversion rule
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversionRule {
    /// Source data type
    pub source: UniversalDataType,

    /// Target data type
    pub target: UniversalDataType,

    /// Conversion safety
    pub safety: ConversionSafety,

    /// SQL conversion functions by dialect
    pub sql_functions: HashMap<SqlDialect, String>,

    /// Warning message for lossy conversions
    pub warning: Option<String>,

    /// Additional notes
    pub notes: Option<String>,
}

impl ConversionRule {
    /// Create a new safe conversion rule
    pub fn safe(source: UniversalDataType, target: UniversalDataType) -> Self {
        Self {
            source,
            target,
            safety: ConversionSafety::Safe,
            sql_functions: HashMap::new(),
            warning: None,
            notes: None,
        }
    }

    /// Create a lossy conversion rule
    pub fn lossy(
        source: UniversalDataType,
        target: UniversalDataType,
        warning: impl Into<String>,
    ) -> Self {
        Self {
            source,
            target,
            safety: ConversionSafety::Lossy,
            sql_functions: HashMap::new(),
            warning: Some(warning.into()),
            notes: None,
        }
    }

    /// Create an invalid conversion rule
    pub fn invalid(source: UniversalDataType, target: UniversalDataType) -> Self {
        Self {
            source,
            target,
            safety: ConversionSafety::Invalid,
            sql_functions: HashMap::new(),
            warning: Some("Conversion not recommended".to_string()),
            notes: None,
        }
    }

    /// Add SQL conversion function for a dialect
    pub fn with_sql(mut self, dialect: SqlDialect, function: impl Into<String>) -> Self {
        self.sql_functions.insert(dialect, function.into());
        self
    }

    /// Add note
    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.notes = Some(note.into());
        self
    }
}

/// Type conversion rules engine
pub struct ConversionRulesEngine {
    /// Registered conversion rules
    rules: HashMap<(String, String), ConversionRule>,
}

impl ConversionRulesEngine {
    /// Create a new conversion rules engine with default rules
    pub fn new() -> Self {
        let mut engine = Self {
            rules: HashMap::new(),
        };

        engine.register_default_rules();
        engine
    }

    /// Register a conversion rule
    pub fn register_rule(&mut self, rule: ConversionRule) {
        let key = (rule.source.to_string(), rule.target.to_string());
        self.rules.insert(key, rule);
    }

    /// Get conversion rule for source → target
    pub fn get_rule(
        &self,
        source: &UniversalDataType,
        target: &UniversalDataType,
    ) -> Option<&ConversionRule> {
        let key = (source.to_string(), target.to_string());
        self.rules.get(&key)
    }

    /// Check if conversion is safe
    pub fn is_safe_conversion(
        &self,
        source: &UniversalDataType,
        target: &UniversalDataType,
    ) -> bool {
        if source == target {
            return true;
        }

        self.get_rule(source, target)
            .map(|r| r.safety == ConversionSafety::Safe)
            .unwrap_or(false)
    }

    /// Check if conversion is lossy
    pub fn is_lossy_conversion(
        &self,
        source: &UniversalDataType,
        target: &UniversalDataType,
    ) -> bool {
        self.get_rule(source, target)
            .map(|r| r.safety == ConversionSafety::Lossy)
            .unwrap_or(false)
    }

    /// Check if conversion is invalid
    pub fn is_invalid_conversion(
        &self,
        source: &UniversalDataType,
        target: &UniversalDataType,
    ) -> bool {
        self.get_rule(source, target)
            .map(|r| r.safety == ConversionSafety::Invalid)
            .unwrap_or(true) // Unknown conversions are invalid by default
    }

    /// Get SQL conversion function for dialect
    pub fn get_conversion_sql(
        &self,
        source: &UniversalDataType,
        target: &UniversalDataType,
        dialect: SqlDialect,
    ) -> Result<String> {
        if source == target {
            return Ok("$1".to_string()); // No conversion needed
        }

        let rule = self
            .get_rule(source, target)
            .ok_or_else(|| anyhow!("No conversion rule found for {} → {}", source, target))?;

        // Try specific dialect first, fall back to generic
        rule.sql_functions
            .get(&dialect)
            .or_else(|| rule.sql_functions.get(&SqlDialect::Generic))
            .cloned()
            .ok_or_else(|| anyhow!("No SQL function for dialect {:?}", dialect))
    }

    /// Validate conversion and return warnings
    pub fn validate_conversion(
        &self,
        source: &UniversalDataType,
        target: &UniversalDataType,
    ) -> Result<Vec<String>> {
        if source == target {
            return Ok(Vec::new());
        }

        let rule = self
            .get_rule(source, target)
            .ok_or_else(|| anyhow!("Unsupported conversion: {} → {}", source, target))?;

        let mut warnings = Vec::new();

        match rule.safety {
            ConversionSafety::Safe => {
                // No warnings for safe conversions
            }
            ConversionSafety::Lossy => {
                if let Some(ref warning) = rule.warning {
                    warnings.push(warning.clone());
                }
            }
            ConversionSafety::Invalid => {
                return Err(anyhow!("Invalid conversion: {} → {}", source, target));
            }
        }

        Ok(warnings)
    }

    /// Register all default conversion rules
    fn register_default_rules(&mut self) {
        // Integer conversions
        self.register_integer_rules();

        // Float conversions
        self.register_float_rules();

        // Decimal conversions
        self.register_decimal_rules();

        // String conversions
        self.register_string_rules();

        // Temporal conversions
        self.register_temporal_rules();

        // Boolean conversions
        self.register_boolean_rules();

        // Special type conversions
        self.register_special_rules();
    }

    fn register_integer_rules(&mut self) {
        use UniversalDataType::*;

        // Integer → String (safe)
        self.register_rule(
            ConversionRule::safe(Integer { bits: Some(32) }, String { max_length: None })
                .with_sql(SqlDialect::PostgreSQL, "CAST($1 AS VARCHAR)")
                .with_sql(SqlDialect::MySQL, "CAST($1 AS CHAR)")
                .with_sql(SqlDialect::Oracle, "TO_CHAR($1)")
                .with_sql(SqlDialect::DB2, "CHAR($1)")
                .with_sql(SqlDialect::Generic, "CAST($1 AS VARCHAR)"),
        );

        // Integer → Float (safe)
        self.register_rule(
            ConversionRule::safe(Integer { bits: Some(32) }, Float { bits: Some(64) })
                .with_sql(SqlDialect::PostgreSQL, "CAST($1 AS DOUBLE PRECISION)")
                .with_sql(SqlDialect::MySQL, "CAST($1 AS DOUBLE)")
                .with_sql(SqlDialect::Generic, "CAST($1 AS DOUBLE)"),
        );

        // Integer → Decimal (safe)
        self.register_rule(
            ConversionRule::safe(
                Integer { bits: Some(32) },
                Decimal {
                    precision: 18,
                    scale: 2,
                },
            )
            .with_sql(SqlDialect::PostgreSQL, "CAST($1 AS NUMERIC(18,2))")
            .with_sql(SqlDialect::MySQL, "CAST($1 AS DECIMAL(18,2))")
            .with_sql(SqlDialect::Generic, "CAST($1 AS DECIMAL(18,2))"),
        );

        // Integer → Boolean (safe for 0/1)
        self.register_rule(
            ConversionRule::safe(Integer { bits: Some(32) }, Boolean)
                .with_sql(SqlDialect::PostgreSQL, "($1::INTEGER = 1)")
                .with_sql(SqlDialect::MySQL, "($1 = 1)")
                .with_sql(
                    SqlDialect::Generic,
                    "CASE WHEN $1 = 1 THEN TRUE ELSE FALSE END",
                )
                .with_note("Assumes 0=false, 1=true, other values may behave unexpectedly"),
        );
    }

    fn register_float_rules(&mut self) {
        use UniversalDataType::*;

        // Float → Integer (lossy - precision loss)
        self.register_rule(
            ConversionRule::lossy(
                Float { bits: Some(64) },
                Integer { bits: Some(32) },
                "Decimal values will be truncated",
            )
            .with_sql(SqlDialect::PostgreSQL, "CAST($1 AS INTEGER)")
            .with_sql(SqlDialect::MySQL, "CAST($1 AS SIGNED)")
            .with_sql(SqlDialect::Oracle, "TRUNC($1)")
            .with_sql(SqlDialect::Generic, "CAST($1 AS INTEGER)"),
        );

        // Float → String (safe)
        self.register_rule(
            ConversionRule::safe(Float { bits: Some(64) }, String { max_length: None })
                .with_sql(SqlDialect::PostgreSQL, "CAST($1 AS VARCHAR)")
                .with_sql(SqlDialect::MySQL, "CAST($1 AS CHAR)")
                .with_sql(SqlDialect::Oracle, "TO_CHAR($1)")
                .with_sql(SqlDialect::Generic, "CAST($1 AS VARCHAR)"),
        );

        // Float → Decimal (safe if decimal has sufficient precision)
        self.register_rule(
            ConversionRule::safe(
                Float { bits: Some(64) },
                Decimal {
                    precision: 18,
                    scale: 6,
                },
            )
            .with_sql(SqlDialect::PostgreSQL, "CAST($1 AS NUMERIC(18,6))")
            .with_sql(SqlDialect::MySQL, "CAST($1 AS DECIMAL(18,6))")
            .with_sql(SqlDialect::Generic, "CAST($1 AS DECIMAL(18,6))"),
        );
    }

    fn register_decimal_rules(&mut self) {
        use UniversalDataType::*;

        // Decimal → Integer (lossy)
        self.register_rule(
            ConversionRule::lossy(
                Decimal {
                    precision: 18,
                    scale: 2,
                },
                Integer { bits: Some(32) },
                "Decimal places will be truncated",
            )
            .with_sql(SqlDialect::PostgreSQL, "CAST($1 AS INTEGER)")
            .with_sql(SqlDialect::MySQL, "CAST($1 AS SIGNED)")
            .with_sql(SqlDialect::Oracle, "TRUNC($1)")
            .with_sql(SqlDialect::Generic, "CAST($1 AS INTEGER)"),
        );

        // Decimal → Float (safe)
        self.register_rule(
            ConversionRule::safe(
                Decimal {
                    precision: 18,
                    scale: 2,
                },
                Float { bits: Some(64) },
            )
            .with_sql(SqlDialect::PostgreSQL, "CAST($1 AS DOUBLE PRECISION)")
            .with_sql(SqlDialect::MySQL, "CAST($1 AS DOUBLE)")
            .with_sql(SqlDialect::Generic, "CAST($1 AS DOUBLE)"),
        );

        // Decimal → String (safe)
        self.register_rule(
            ConversionRule::safe(
                Decimal {
                    precision: 18,
                    scale: 2,
                },
                String { max_length: None },
            )
            .with_sql(SqlDialect::PostgreSQL, "CAST($1 AS VARCHAR)")
            .with_sql(SqlDialect::MySQL, "CAST($1 AS CHAR)")
            .with_sql(SqlDialect::Oracle, "TO_CHAR($1)")
            .with_sql(SqlDialect::Generic, "CAST($1 AS VARCHAR)"),
        );
    }

    fn register_string_rules(&mut self) {
        use UniversalDataType::*;

        // String → Integer (lossy - may fail for non-numeric strings)
        self.register_rule(
            ConversionRule::lossy(
                String { max_length: None },
                Integer { bits: Some(32) },
                "Conversion will fail for non-numeric strings",
            )
            .with_sql(SqlDialect::PostgreSQL, "CAST($1 AS INTEGER)")
            .with_sql(SqlDialect::MySQL, "CAST($1 AS SIGNED)")
            .with_sql(SqlDialect::Oracle, "TO_NUMBER($1)")
            .with_sql(SqlDialect::Generic, "CAST($1 AS INTEGER)"),
        );

        // String → Float (lossy - may fail)
        self.register_rule(
            ConversionRule::lossy(
                String { max_length: None },
                Float { bits: Some(64) },
                "Conversion will fail for non-numeric strings",
            )
            .with_sql(SqlDialect::PostgreSQL, "CAST($1 AS DOUBLE PRECISION)")
            .with_sql(SqlDialect::MySQL, "CAST($1 AS DOUBLE)")
            .with_sql(SqlDialect::Oracle, "TO_NUMBER($1)")
            .with_sql(SqlDialect::Generic, "CAST($1 AS DOUBLE)"),
        );

        // String → Date (lossy - format dependent)
        self.register_rule(
            ConversionRule::lossy(
                String { max_length: None },
                Date,
                "Conversion depends on string format, may fail",
            )
            .with_sql(SqlDialect::PostgreSQL, "CAST($1 AS DATE)")
            .with_sql(SqlDialect::MySQL, "STR_TO_DATE($1, '%Y-%m-%d')")
            .with_sql(SqlDialect::Oracle, "TO_DATE($1, 'YYYY-MM-DD')")
            .with_sql(SqlDialect::Generic, "CAST($1 AS DATE)"),
        );

        // String → DateTime (lossy - format dependent)
        self.register_rule(
            ConversionRule::lossy(
                String { max_length: None },
                DateTime {
                    with_timezone: false,
                },
                "Conversion depends on string format, may fail",
            )
            .with_sql(SqlDialect::PostgreSQL, "CAST($1 AS TIMESTAMP)")
            .with_sql(SqlDialect::MySQL, "STR_TO_DATE($1, '%Y-%m-%d %H:%i:%s')")
            .with_sql(
                SqlDialect::Oracle,
                "TO_TIMESTAMP($1, 'YYYY-MM-DD HH24:MI:SS')",
            )
            .with_sql(SqlDialect::Generic, "CAST($1 AS TIMESTAMP)"),
        );

        // String → Boolean (lossy - format dependent)
        self.register_rule(
            ConversionRule::lossy(
                String { max_length: None },
                Boolean,
                "Conversion depends on string value (true/false, 1/0, etc.)",
            )
            .with_sql(SqlDialect::PostgreSQL, "CAST($1 AS BOOLEAN)")
            .with_sql(SqlDialect::MySQL, "($1 IN ('true', '1', 'yes'))")
            .with_sql(
                SqlDialect::Generic,
                "CASE WHEN LOWER($1) IN ('true', '1', 'yes') THEN TRUE ELSE FALSE END",
            ),
        );

        // String → Text (safe)
        self.register_rule(
            ConversionRule::safe(
                String {
                    max_length: Some(255),
                },
                Text,
            )
            .with_sql(SqlDialect::PostgreSQL, "CAST($1 AS TEXT)")
            .with_sql(SqlDialect::MySQL, "CAST($1 AS TEXT)")
            .with_sql(SqlDialect::Generic, "CAST($1 AS TEXT)"),
        );
    }

    fn register_temporal_rules(&mut self) {
        use UniversalDataType::*;

        // Date → DateTime (safe)
        self.register_rule(
            ConversionRule::safe(
                Date,
                DateTime {
                    with_timezone: false,
                },
            )
            .with_sql(SqlDialect::PostgreSQL, "CAST($1 AS TIMESTAMP)")
            .with_sql(SqlDialect::MySQL, "CAST($1 AS DATETIME)")
            .with_sql(SqlDialect::Oracle, "CAST($1 AS TIMESTAMP)")
            .with_sql(SqlDialect::Generic, "CAST($1 AS TIMESTAMP)"),
        );

        // DateTime → Date (lossy - time component lost)
        self.register_rule(
            ConversionRule::lossy(
                DateTime {
                    with_timezone: false,
                },
                Date,
                "Time component will be discarded",
            )
            .with_sql(SqlDialect::PostgreSQL, "CAST($1 AS DATE)")
            .with_sql(SqlDialect::MySQL, "DATE($1)")
            .with_sql(SqlDialect::Oracle, "TRUNC($1)")
            .with_sql(SqlDialect::Generic, "CAST($1 AS DATE)"),
        );

        // DateTime → Time (lossy - date component lost)
        self.register_rule(
            ConversionRule::lossy(
                DateTime {
                    with_timezone: false,
                },
                Time {
                    with_timezone: false,
                },
                "Date component will be discarded",
            )
            .with_sql(SqlDialect::PostgreSQL, "CAST($1 AS TIME)")
            .with_sql(SqlDialect::MySQL, "TIME($1)")
            .with_sql(SqlDialect::Oracle, "TO_CHAR($1, 'HH24:MI:SS')")
            .with_sql(SqlDialect::Generic, "CAST($1 AS TIME)"),
        );

        // Date → String (safe)
        self.register_rule(
            ConversionRule::safe(Date, String { max_length: None })
                .with_sql(SqlDialect::PostgreSQL, "TO_CHAR($1, 'YYYY-MM-DD')")
                .with_sql(SqlDialect::MySQL, "DATE_FORMAT($1, '%Y-%m-%d')")
                .with_sql(SqlDialect::Oracle, "TO_CHAR($1, 'YYYY-MM-DD')")
                .with_sql(SqlDialect::Generic, "CAST($1 AS VARCHAR)"),
        );

        // DateTime → String (safe)
        self.register_rule(
            ConversionRule::safe(
                DateTime {
                    with_timezone: false,
                },
                String { max_length: None },
            )
            .with_sql(
                SqlDialect::PostgreSQL,
                "TO_CHAR($1, 'YYYY-MM-DD HH24:MI:SS')",
            )
            .with_sql(SqlDialect::MySQL, "DATE_FORMAT($1, '%Y-%m-%d %H:%i:%s')")
            .with_sql(SqlDialect::Oracle, "TO_CHAR($1, 'YYYY-MM-DD HH24:MI:SS')")
            .with_sql(SqlDialect::Generic, "CAST($1 AS VARCHAR)"),
        );
    }

    fn register_boolean_rules(&mut self) {
        use UniversalDataType::*;

        // Boolean → Integer (safe)
        self.register_rule(
            ConversionRule::safe(Boolean, Integer { bits: Some(32) })
                .with_sql(SqlDialect::PostgreSQL, "CASE WHEN $1 THEN 1 ELSE 0 END")
                .with_sql(SqlDialect::MySQL, "CAST($1 AS SIGNED)")
                .with_sql(SqlDialect::Generic, "CASE WHEN $1 THEN 1 ELSE 0 END"),
        );

        // Boolean → String (safe)
        self.register_rule(
            ConversionRule::safe(Boolean, String { max_length: None })
                .with_sql(SqlDialect::PostgreSQL, "CAST($1 AS VARCHAR)")
                .with_sql(SqlDialect::MySQL, "CAST($1 AS CHAR)")
                .with_sql(
                    SqlDialect::Generic,
                    "CASE WHEN $1 THEN 'true' ELSE 'false' END",
                ),
        );
    }

    fn register_special_rules(&mut self) {
        use UniversalDataType::*;

        // Uuid → String (safe)
        self.register_rule(
            ConversionRule::safe(
                Uuid,
                String {
                    max_length: Some(36),
                },
            )
            .with_sql(SqlDialect::PostgreSQL, "CAST($1 AS VARCHAR)")
            .with_sql(SqlDialect::MySQL, "CAST($1 AS CHAR(36))")
            .with_sql(SqlDialect::Generic, "CAST($1 AS VARCHAR)"),
        );

        // String → Uuid (lossy - format must be valid UUID)
        self.register_rule(
            ConversionRule::lossy(
                String {
                    max_length: Some(36),
                },
                Uuid,
                "String must be a valid UUID format",
            )
            .with_sql(SqlDialect::PostgreSQL, "CAST($1 AS UUID)")
            .with_sql(SqlDialect::Generic, "CAST($1 AS UUID)"),
        );

        // Json → String (safe)
        self.register_rule(
            ConversionRule::safe(Json, String { max_length: None })
                .with_sql(SqlDialect::PostgreSQL, "CAST($1 AS TEXT)")
                .with_sql(SqlDialect::MySQL, "CAST($1 AS CHAR)")
                .with_sql(SqlDialect::Generic, "CAST($1 AS VARCHAR)"),
        );

        // String → Json (lossy - must be valid JSON)
        self.register_rule(
            ConversionRule::lossy(
                String { max_length: None },
                Json,
                "String must be valid JSON",
            )
            .with_sql(SqlDialect::PostgreSQL, "CAST($1 AS JSONB)")
            .with_sql(SqlDialect::MySQL, "CAST($1 AS JSON)")
            .with_sql(SqlDialect::Generic, "CAST($1 AS JSON)"),
        );
    }
}

impl Default for ConversionRulesEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_engine_creation() {
        let engine = ConversionRulesEngine::new();
        assert!(!engine.rules.is_empty());
    }

    #[test]
    fn test_safe_integer_to_string() {
        let engine = ConversionRulesEngine::new();

        let source = UniversalDataType::Integer { bits: Some(32) };
        let target = UniversalDataType::String { max_length: None };

        assert!(engine.is_safe_conversion(&source, &target));
        assert!(!engine.is_lossy_conversion(&source, &target));

        let sql = engine
            .get_conversion_sql(&source, &target, SqlDialect::PostgreSQL)
            .unwrap();
        assert!(sql.contains("CAST"));
        assert!(sql.contains("VARCHAR"));
    }

    #[test]
    fn test_lossy_float_to_integer() {
        let engine = ConversionRulesEngine::new();

        let source = UniversalDataType::Float { bits: Some(64) };
        let target = UniversalDataType::Integer { bits: Some(32) };

        assert!(!engine.is_safe_conversion(&source, &target));
        assert!(engine.is_lossy_conversion(&source, &target));

        let warnings = engine.validate_conversion(&source, &target).unwrap();
        assert!(!warnings.is_empty());
        assert!(warnings[0].contains("truncated"));
    }

    #[test]
    fn test_lossy_datetime_to_date() {
        let engine = ConversionRulesEngine::new();

        let source = UniversalDataType::DateTime {
            with_timezone: false,
        };
        let target = UniversalDataType::Date;

        assert!(engine.is_lossy_conversion(&source, &target));

        let sql = engine
            .get_conversion_sql(&source, &target, SqlDialect::PostgreSQL)
            .unwrap();
        assert!(sql.contains("DATE"));

        let warnings = engine.validate_conversion(&source, &target).unwrap();
        assert!(warnings[0].contains("Time component"));
    }

    #[test]
    fn test_safe_date_to_datetime() {
        let engine = ConversionRulesEngine::new();

        let source = UniversalDataType::Date;
        let target = UniversalDataType::DateTime {
            with_timezone: false,
        };

        assert!(engine.is_safe_conversion(&source, &target));
        assert!(!engine.is_lossy_conversion(&source, &target));
    }

    #[test]
    fn test_multi_dialect_support() {
        let engine = ConversionRulesEngine::new();

        let source = UniversalDataType::Integer { bits: Some(32) };
        let target = UniversalDataType::String { max_length: None };

        // PostgreSQL
        let pg_sql = engine
            .get_conversion_sql(&source, &target, SqlDialect::PostgreSQL)
            .unwrap();
        assert!(pg_sql.contains("VARCHAR"));

        // Oracle
        let oracle_sql = engine
            .get_conversion_sql(&source, &target, SqlDialect::Oracle)
            .unwrap();
        assert!(oracle_sql.contains("TO_CHAR"));

        // DB2
        let db2_sql = engine
            .get_conversion_sql(&source, &target, SqlDialect::DB2)
            .unwrap();
        assert!(db2_sql.contains("CHAR"));
    }

    #[test]
    fn test_same_type_no_conversion() {
        let engine = ConversionRulesEngine::new();

        let source = UniversalDataType::Integer { bits: Some(32) };
        let target = UniversalDataType::Integer { bits: Some(32) };

        assert!(engine.is_safe_conversion(&source, &target));

        let sql = engine
            .get_conversion_sql(&source, &target, SqlDialect::PostgreSQL)
            .unwrap();
        assert_eq!(sql, "$1");

        let warnings = engine.validate_conversion(&source, &target).unwrap();
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_string_to_integer_lossy() {
        let engine = ConversionRulesEngine::new();

        let source = UniversalDataType::String { max_length: None };
        let target = UniversalDataType::Integer { bits: Some(32) };

        assert!(engine.is_lossy_conversion(&source, &target));

        let warnings = engine.validate_conversion(&source, &target).unwrap();
        assert!(warnings[0].contains("non-numeric"));
    }

    #[test]
    fn test_boolean_conversions() {
        let engine = ConversionRulesEngine::new();

        // Boolean → Integer (safe)
        let bool_to_int = engine.is_safe_conversion(
            &UniversalDataType::Boolean,
            &UniversalDataType::Integer { bits: Some(32) },
        );
        assert!(bool_to_int);

        // Boolean → String (safe)
        let bool_to_string = engine.is_safe_conversion(
            &UniversalDataType::Boolean,
            &UniversalDataType::String { max_length: None },
        );
        assert!(bool_to_string);
    }

    #[test]
    fn test_uuid_conversions() {
        let engine = ConversionRulesEngine::new();

        // UUID → String (safe)
        let uuid_to_string = engine.is_safe_conversion(
            &UniversalDataType::Uuid,
            &UniversalDataType::String {
                max_length: Some(36),
            },
        );
        assert!(uuid_to_string);

        // String → UUID (lossy)
        let string_to_uuid = engine.is_lossy_conversion(
            &UniversalDataType::String {
                max_length: Some(36),
            },
            &UniversalDataType::Uuid,
        );
        assert!(string_to_uuid);
    }

    #[test]
    fn test_decimal_conversions() {
        let engine = ConversionRulesEngine::new();

        // Decimal → Integer (lossy)
        let dec_to_int = engine.is_lossy_conversion(
            &UniversalDataType::Decimal {
                precision: 18,
                scale: 2,
            },
            &UniversalDataType::Integer { bits: Some(32) },
        );
        assert!(dec_to_int);

        // Decimal → Float (safe)
        let dec_to_float = engine.is_safe_conversion(
            &UniversalDataType::Decimal {
                precision: 18,
                scale: 2,
            },
            &UniversalDataType::Float { bits: Some(64) },
        );
        assert!(dec_to_float);
    }
}
