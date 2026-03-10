//! SQL Dialect Module
//!
//! Abstraction for different SQL database dialects.

pub mod db2;
pub mod oracle;
pub mod postgresql;

use anyhow::Result;

/// SQL column definition
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ColumnDefinition {
    pub name: String,
    pub sql_type: String,
    pub nullable: bool,
    pub default_value: Option<String>,
    pub primary_key: bool,
    pub unique: bool,
    pub check_constraint: Option<String>,
    pub comment: Option<String>,
}

impl ColumnDefinition {
    /// Validate column definition to prevent SQL injection
    pub fn validate(&self) -> Result<()> {
        use graphica_core::security::{validate_identifier, validate_sql_type};

        // Validate column name
        validate_identifier(&self.name)
            .map_err(|e| anyhow::anyhow!("Invalid column name '{}': {}", self.name, e))?;

        // Validate SQL type
        validate_sql_type(&self.sql_type).map_err(|e| {
            anyhow::anyhow!(
                "Invalid SQL type '{}' for column '{}': {}",
                self.sql_type,
                self.name,
                e
            )
        })?;

        Ok(())
    }
}

/// SQL table definition
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TableDefinition {
    pub name: String,
    pub columns: Vec<ColumnDefinition>,
    pub primary_key: Vec<String>,
    pub foreign_keys: Vec<ForeignKeyDefinition>,
    pub indexes: Vec<IndexDefinition>,
    pub comment: Option<String>,
}

impl TableDefinition {
    /// Validate table definition to prevent SQL injection
    pub fn validate(&self) -> Result<()> {
        use graphica_core::security::validate_identifier;

        // Validate table name
        validate_identifier(&self.name)
            .map_err(|e| anyhow::anyhow!("Invalid table name '{}': {}", self.name, e))?;

        // Validate all columns
        for col in &self.columns {
            col.validate()
                .map_err(|e| anyhow::anyhow!("Invalid column in table '{}': {}", self.name, e))?;
        }

        // Validate all primary key column names
        for pk in &self.primary_key {
            validate_identifier(pk).map_err(|e| {
                anyhow::anyhow!(
                    "Invalid primary key column '{}' in table '{}': {}",
                    pk,
                    self.name,
                    e
                )
            })?;
        }

        // Validate all foreign keys
        for fk in &self.foreign_keys {
            fk.validate().map_err(|e| {
                anyhow::anyhow!("Invalid foreign key in table '{}': {}", self.name, e)
            })?;
        }

        // Validate all indexes
        for idx in &self.indexes {
            idx.validate()
                .map_err(|e| anyhow::anyhow!("Invalid index in table '{}': {}", self.name, e))?;
        }

        Ok(())
    }
}

/// Foreign key definition
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ForeignKeyDefinition {
    pub name: String,
    pub columns: Vec<String>,
    pub ref_table: String,
    pub ref_columns: Vec<String>,
    pub on_delete: Option<ReferentialAction>,
    pub on_update: Option<ReferentialAction>,
}

impl ForeignKeyDefinition {
    /// Validate foreign key definition to prevent SQL injection
    pub fn validate(&self) -> Result<()> {
        use graphica_core::security::validate_identifier;

        // Validate FK name
        validate_identifier(&self.name)
            .map_err(|e| anyhow::anyhow!("Invalid foreign key name '{}': {}", self.name, e))?;

        // Validate all column names
        for col in &self.columns {
            validate_identifier(col).map_err(|e| {
                anyhow::anyhow!(
                    "Invalid column name '{}' in foreign key '{}': {}",
                    col,
                    self.name,
                    e
                )
            })?;
        }

        // Validate referenced table name
        validate_identifier(&self.ref_table).map_err(|e| {
            anyhow::anyhow!(
                "Invalid referenced table '{}' in foreign key '{}': {}",
                self.ref_table,
                self.name,
                e
            )
        })?;

        // Validate all referenced column names
        for col in &self.ref_columns {
            validate_identifier(col).map_err(|e| {
                anyhow::anyhow!(
                    "Invalid referenced column '{}' in foreign key '{}': {}",
                    col,
                    self.name,
                    e
                )
            })?;
        }

        Ok(())
    }
}

/// Index definition
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct IndexDefinition {
    pub name: String,
    pub table: String,
    pub columns: Vec<String>,
    pub unique: bool,
}

impl IndexDefinition {
    /// Validate index definition to prevent SQL injection
    pub fn validate(&self) -> Result<()> {
        use graphica_core::security::validate_identifier;

        // Validate index name
        validate_identifier(&self.name)
            .map_err(|e| anyhow::anyhow!("Invalid index name '{}': {}", self.name, e))?;

        // Validate table name
        validate_identifier(&self.table).map_err(|e| {
            anyhow::anyhow!(
                "Invalid table name '{}' in index '{}': {}",
                self.table,
                self.name,
                e
            )
        })?;

        // Validate all column names
        for col in &self.columns {
            validate_identifier(col).map_err(|e| {
                anyhow::anyhow!(
                    "Invalid column name '{}' in index '{}': {}",
                    col,
                    self.name,
                    e
                )
            })?;
        }

        Ok(())
    }
}

/// Referential action
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ReferentialAction {
    Cascade,
    SetNull,
    SetDefault,
    Restrict,
    NoAction,
}

impl ReferentialAction {
    pub fn to_sql(&self) -> &str {
        match self {
            ReferentialAction::Cascade => "CASCADE",
            ReferentialAction::SetNull => "SET NULL",
            ReferentialAction::SetDefault => "SET DEFAULT",
            ReferentialAction::Restrict => "RESTRICT",
            ReferentialAction::NoAction => "NO ACTION",
        }
    }
}

/// SQL Dialect trait
///
/// Defines the interface for generating SQL DDL for different database systems.
pub trait SqlDialect: Send + Sync {
    /// Get dialect name
    fn name(&self) -> &str;

    /// Map XSD datatype to SQL type
    ///
    /// # Arguments
    /// * `xsd_uri` - XSD datatype URI (e.g., "http://www.w3.org/2001/XMLSchema#string")
    /// * `max_length` - Optional maximum length for string types
    ///
    /// # Returns
    /// SQL type string (e.g., "VARCHAR(255)")
    fn map_datatype(&self, xsd_uri: &str, max_length: Option<u32>) -> String;

    /// Generate CREATE TABLE statement
    fn create_table(&self, table: &TableDefinition) -> String;

    /// Generate column definition
    fn create_column(&self, column: &ColumnDefinition) -> String;

    /// Generate PRIMARY KEY constraint
    fn create_primary_key(&self, table_name: &str, columns: &[String]) -> String;

    /// Generate FOREIGN KEY constraint
    fn create_foreign_key(&self, table_name: &str, fk: &ForeignKeyDefinition) -> String;

    /// Generate CREATE INDEX statement
    fn create_index(&self, index: &IndexDefinition) -> String;

    /// Generate ALTER TABLE ADD COLUMN statement
    fn alter_table_add_column(&self, table: &str, column: &ColumnDefinition) -> String;

    /// Generate ALTER TABLE DROP COLUMN statement
    fn alter_table_drop_column(&self, table: &str, column: &str) -> String;

    /// Generate ALTER TABLE MODIFY COLUMN statement (if supported)
    fn alter_table_modify_column(&self, table: &str, column: &ColumnDefinition) -> Result<String>;

    /// Check if table exists SQL
    fn check_table_exists(&self, table: &str) -> String;

    /// Check if column exists SQL
    fn check_column_exists(&self, table: &str, column: &str) -> String;

    /// Generate pattern matching constraint (dialect-specific regex)
    ///
    /// # Arguments
    /// * `column` - Column name
    /// * `pattern` - Regex pattern
    ///
    /// # Returns
    /// SQL expression for pattern matching (e.g., "column ~ 'pattern'" for PostgreSQL)
    fn pattern_constraint(&self, column: &str, pattern: &str) -> String {
        // Default implementation uses PostgreSQL syntax
        // Subclasses should override for their dialect
        format!("{} ~ '{}'", column, pattern.replace("'", "''"))
    }
}

/// Get dialect by name
pub fn get_dialect(name: &str) -> Result<Box<dyn SqlDialect>> {
    match name.to_lowercase().as_str() {
        "db2" => Ok(Box::new(db2::Db2Dialect)),
        "postgresql" | "postgres" | "pg" => Ok(Box::new(postgresql::PostgreSqlDialect)),
        "oracle" => Ok(Box::new(oracle::OracleDialect)),
        _ => anyhow::bail!("Unsupported SQL dialect: {}", name),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_dialect_db2() {
        let dialect = get_dialect("db2").unwrap();
        assert_eq!(dialect.name(), "DB2");
    }

    #[test]
    fn test_get_dialect_postgresql() {
        let dialect = get_dialect("postgresql").unwrap();
        assert_eq!(dialect.name(), "PostgreSQL");
    }

    #[test]
    fn test_get_dialect_oracle() {
        let dialect = get_dialect("oracle").unwrap();
        assert_eq!(dialect.name(), "Oracle");
    }

    #[test]
    fn test_get_dialect_invalid() {
        let result = get_dialect("mysql");
        assert!(result.is_err());
    }
}
