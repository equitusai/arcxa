//! DDL Generation for Database Schema Creation
//!
//! Generates CREATE TABLE, DROP TABLE, and other DDL statements from table schemas.

use anyhow::{anyhow, Result};
use std::sync::Arc;
use tracing::{debug, info, warn};

use super::types::*;

/// Trait for generating DDL statements
pub trait DDLGenerator: Send + Sync {
    /// Generate CREATE TABLE statement
    fn generate_create_table(&self, schema: &TableSchema) -> Result<String>;

    /// Generate DROP TABLE statement
    fn generate_drop_table(&self, table_name: &str) -> Result<String>;

    /// Generate DROP TABLE IF EXISTS statement
    fn generate_drop_table_if_exists(&self, table_name: &str) -> Result<String>;

    /// Generate table existence check query
    fn generate_table_exists_query(&self, table_name: &str) -> Result<String>;

    /// Get the database dialect name
    fn dialect_name(&self) -> &str;
}

/// DB2-specific DDL generator
#[derive(Debug, Clone)]
pub struct DB2DDLGenerator {
    schema_name: String,
}

impl DB2DDLGenerator {
    /// Create a new DB2 DDL generator
    ///
    /// # Arguments
    /// * `schema_name` - The DB2 schema name (e.g., "DB2INST1")
    pub fn new(schema_name: String) -> Self {
        Self { schema_name }
    }

    /// Validate identifier (table/column name) to prevent SQL injection
    fn validate_identifier(&self, identifier: &str) -> Result<()> {
        if identifier.is_empty() {
            return Err(anyhow!("Identifier cannot be empty"));
        }

        if identifier.len() > 128 {
            return Err(anyhow!(
                "Identifier '{}' exceeds maximum length of 128 characters",
                identifier
            ));
        }

        let first_char = identifier.chars().next().unwrap();
        if !first_char.is_ascii_alphabetic() && first_char != '_' {
            return Err(anyhow!(
                "Identifier '{}' must start with letter or underscore",
                identifier
            ));
        }

        if !identifier
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            return Err(anyhow!(
                "Identifier '{}' can only contain alphanumeric characters and underscores",
                identifier
            ));
        }

        Ok(())
    }

    /// Quote a SQL identifier for DB2 compatibility
    ///
    /// DB2 requires quoting identifiers that contain underscores (especially leading underscores)
    /// to prevent them from being interpreted as conditional compilation directives.
    fn quote_identifier(&self, name: &str) -> String {
        // Escape any embedded double quotes by doubling them (SQL standard)
        let escaped = name.replace("\"", "\"\"");
        format!("\"{}\"", escaped)
    }

    /// Generate column definition for CREATE TABLE
    fn generate_column(&self, col: &ColumnDefinition) -> Result<String> {
        self.validate_identifier(&col.name)?;

        let mut parts = vec![self.quote_identifier(&col.name), col.sql_type.clone()];

        if !col.nullable {
            parts.push("NOT NULL".to_string());
        }

        Ok(parts.join(" "))
    }

    /// Generate primary key constraint
    fn generate_primary_key(&self, schema: &TableSchema) -> Option<String> {
        if schema.primary_key.is_empty() {
            return None;
        }

        let pk_cols = schema
            .primary_key
            .iter()
            .map(|col| self.quote_identifier(col))
            .collect::<Vec<_>>()
            .join(", ");
        Some(format!("PRIMARY KEY ({})", pk_cols))
    }

    /// Generate foreign key constraint
    fn generate_foreign_key(&self, fk: &ForeignKeyDefinition) -> Result<String> {
        self.validate_identifier(&fk.column)?;
        self.validate_identifier(&fk.referenced_table)?;
        self.validate_identifier(&fk.referenced_column)?;

        Ok(format!(
            "FOREIGN KEY ({}) REFERENCES {} ({})",
            self.quote_identifier(&fk.column),
            self.quote_identifier(&fk.referenced_table),
            self.quote_identifier(&fk.referenced_column)
        ))
    }
}

impl DDLGenerator for DB2DDLGenerator {
    fn generate_create_table(&self, schema: &TableSchema) -> Result<String> {
        self.validate_identifier(&schema.table_name)?;

        if schema.columns.is_empty() {
            return Err(anyhow!(
                "Cannot create table '{}': no columns defined",
                schema.table_name
            ));
        }

        let mut lines = Vec::new();

        // Header - quote schema and table names
        lines.push(format!(
            "CREATE TABLE {}.{} (",
            self.quote_identifier(&self.schema_name),
            self.quote_identifier(&schema.table_name)
        ));

        // Columns
        let mut column_defs = Vec::new();
        for col in &schema.columns {
            column_defs.push(format!("    {}", self.generate_column(col)?));
        }

        // Primary key
        if let Some(pk) = self.generate_primary_key(schema) {
            column_defs.push(format!("    {}", pk));
        }

        // Foreign keys
        for fk in &schema.foreign_keys {
            column_defs.push(format!("    {}", self.generate_foreign_key(fk)?));
        }

        lines.push(column_defs.join(",\n"));
        lines.push(")".to_string());

        let ddl = lines.join("\n");

        info!(
            "Generated DB2 DDL for table {}.{}: {} characters",
            self.schema_name,
            schema.table_name,
            ddl.len()
        );
        debug!("DDL:\n{}", ddl);

        Ok(ddl)
    }

    fn generate_drop_table(&self, table_name: &str) -> Result<String> {
        self.validate_identifier(table_name)?;
        Ok(format!(
            "DROP TABLE {}.{}",
            self.quote_identifier(&self.schema_name),
            self.quote_identifier(table_name)
        ))
    }

    fn generate_drop_table_if_exists(&self, table_name: &str) -> Result<String> {
        self.validate_identifier(table_name)?;
        // DB2 doesn't have native "IF EXISTS" in older versions, so we use this approach
        Ok(format!(
            "DROP TABLE {}.{} IF EXISTS",
            self.quote_identifier(&self.schema_name),
            self.quote_identifier(table_name)
        ))
    }

    fn generate_table_exists_query(&self, table_name: &str) -> Result<String> {
        self.validate_identifier(table_name)?;
        Ok(format!(
            r#"SELECT COUNT(*) FROM SYSCAT.TABLES WHERE TABSCHEMA = '{}' AND TABNAME = '{}'"#,
            self.schema_name.to_uppercase(),
            table_name.to_uppercase()
        ))
    }

    fn dialect_name(&self) -> &str {
        "DB2"
    }
}

/// PostgreSQL-specific DDL generator
#[derive(Debug, Clone)]
pub struct PostgresDDLGenerator {
    schema_name: String,
}

impl PostgresDDLGenerator {
    /// Create a new PostgreSQL DDL generator
    ///
    /// # Arguments
    /// * `schema_name` - The PostgreSQL schema name (e.g., "public")
    pub fn new(schema_name: String) -> Self {
        Self { schema_name }
    }

    /// Validate identifier (table/column name)
    fn validate_identifier(&self, identifier: &str) -> Result<()> {
        if identifier.is_empty() {
            return Err(anyhow!("Identifier cannot be empty"));
        }

        if identifier.len() > 63 {
            return Err(anyhow!(
                "Identifier '{}' exceeds PostgreSQL maximum length of 63 characters",
                identifier
            ));
        }

        let first_char = identifier.chars().next().unwrap();
        if !first_char.is_ascii_alphabetic() && first_char != '_' {
            return Err(anyhow!(
                "Identifier '{}' must start with letter or underscore",
                identifier
            ));
        }

        if !identifier
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            return Err(anyhow!(
                "Identifier '{}' can only contain alphanumeric characters and underscores",
                identifier
            ));
        }

        Ok(())
    }

    /// Generate column definition for CREATE TABLE
    fn generate_column(&self, col: &ColumnDefinition) -> Result<String> {
        self.validate_identifier(&col.name)?;

        let mut parts = vec![col.name.clone(), col.sql_type.clone()];

        if !col.nullable {
            parts.push("NOT NULL".to_string());
        }

        Ok(parts.join(" "))
    }

    /// Generate primary key constraint
    fn generate_primary_key(&self, schema: &TableSchema) -> Option<String> {
        if schema.primary_key.is_empty() {
            return None;
        }

        let pk_cols = schema.primary_key.join(", ");
        Some(format!("PRIMARY KEY ({})", pk_cols))
    }

    /// Generate foreign key constraint
    fn generate_foreign_key(&self, fk: &ForeignKeyDefinition) -> Result<String> {
        self.validate_identifier(&fk.column)?;
        self.validate_identifier(&fk.referenced_table)?;
        self.validate_identifier(&fk.referenced_column)?;

        Ok(format!(
            "FOREIGN KEY ({}) REFERENCES {} ({})",
            fk.column, fk.referenced_table, fk.referenced_column
        ))
    }
}

impl DDLGenerator for PostgresDDLGenerator {
    fn generate_create_table(&self, schema: &TableSchema) -> Result<String> {
        self.validate_identifier(&schema.table_name)?;

        if schema.columns.is_empty() {
            return Err(anyhow!(
                "Cannot create table '{}': no columns defined",
                schema.table_name
            ));
        }

        let mut lines = Vec::new();

        // Header
        lines.push(format!(
            "CREATE TABLE {}.{} (",
            self.schema_name, schema.table_name
        ));

        // Columns
        let mut column_defs = Vec::new();
        for col in &schema.columns {
            column_defs.push(format!("    {}", self.generate_column(col)?));
        }

        // Primary key
        if let Some(pk) = self.generate_primary_key(schema) {
            column_defs.push(format!("    {}", pk));
        }

        // Foreign keys
        for fk in &schema.foreign_keys {
            column_defs.push(format!("    {}", self.generate_foreign_key(fk)?));
        }

        lines.push(column_defs.join(",\n"));
        lines.push(")".to_string());

        let ddl = lines.join("\n");

        info!(
            "Generated PostgreSQL DDL for table {}.{}: {} characters",
            self.schema_name,
            schema.table_name,
            ddl.len()
        );
        debug!("DDL:\n{}", ddl);

        Ok(ddl)
    }

    fn generate_drop_table(&self, table_name: &str) -> Result<String> {
        self.validate_identifier(table_name)?;
        Ok(format!("DROP TABLE {}.{}", self.schema_name, table_name))
    }

    fn generate_drop_table_if_exists(&self, table_name: &str) -> Result<String> {
        self.validate_identifier(table_name)?;
        Ok(format!(
            "DROP TABLE IF EXISTS {}.{}",
            self.schema_name, table_name
        ))
    }

    fn generate_table_exists_query(&self, table_name: &str) -> Result<String> {
        self.validate_identifier(table_name)?;
        Ok(format!(
            r#"SELECT COUNT(*) FROM information_schema.tables WHERE table_schema = '{}' AND table_name = '{}'"#,
            self.schema_name,
            table_name.to_lowercase()
        ))
    }

    fn dialect_name(&self) -> &str {
        "PostgreSQL"
    }
}

/// Factory function to create a DDL generator by database type
pub fn create_ddl_generator(database: &str, schema_name: String) -> Result<Arc<dyn DDLGenerator>> {
    match database.to_lowercase().as_str() {
        "db2" => Ok(Arc::new(DB2DDLGenerator::new(schema_name))),
        "postgresql" | "postgres" | "pg" => Ok(Arc::new(PostgresDDLGenerator::new(schema_name))),
        _ => Err(anyhow!("Unsupported database type: {}", database)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // =============================================================================
    // DB2 DDL Generator Tests
    // =============================================================================

    fn create_test_schema() -> TableSchema {
        let mut schema = TableSchema::new("PATIENTS".to_string());

        schema.add_column(
            ColumnDefinition::new("id".to_string(), "INTEGER".to_string(), false).as_primary_key(),
        );

        schema.add_column(ColumnDefinition::new(
            "name".to_string(),
            "VARCHAR(255)".to_string(),
            true,
        ));

        schema.add_column(ColumnDefinition::new(
            "email".to_string(),
            "VARCHAR(255)".to_string(),
            false,
        ));

        schema.add_primary_key("id".to_string());

        schema
    }

    #[test]
    fn test_db2_create_table_simple() {
        let generator = DB2DDLGenerator::new("TESTSCHEMA".to_string());
        let schema = create_test_schema();

        let ddl = generator.generate_create_table(&schema).unwrap();

        assert!(ddl.contains("CREATE TABLE \"TESTSCHEMA\".\"PATIENTS\""));
        assert!(ddl.contains("\"id\" INTEGER NOT NULL"));
        assert!(ddl.contains("\"name\" VARCHAR(255)"));
        assert!(ddl.contains("\"email\" VARCHAR(255) NOT NULL"));
        assert!(ddl.contains("PRIMARY KEY (\"id\")"));
    }

    #[test]
    fn test_db2_create_table_with_foreign_key() {
        let generator = DB2DDLGenerator::new("TESTSCHEMA".to_string());
        let mut schema = TableSchema::new("APPOINTMENTS".to_string());

        schema.add_column(
            ColumnDefinition::new("id".to_string(), "INTEGER".to_string(), false).as_primary_key(),
        );

        schema.add_column(ColumnDefinition::new(
            "patient_id".to_string(),
            "INTEGER".to_string(),
            false,
        ));

        schema.add_primary_key("id".to_string());

        schema.add_foreign_key(ForeignKeyDefinition::new(
            "patient_id".to_string(),
            "PATIENTS".to_string(),
            "id".to_string(),
        ));

        let ddl = generator.generate_create_table(&schema).unwrap();

        assert!(ddl.contains("CREATE TABLE \"TESTSCHEMA\".\"APPOINTMENTS\""));
        assert!(ddl.contains("FOREIGN KEY (\"patient_id\") REFERENCES \"PATIENTS\" (\"id\")"));
    }

    #[test]
    fn test_db2_create_table_empty_columns() {
        let generator = DB2DDLGenerator::new("TESTSCHEMA".to_string());
        let schema = TableSchema::new("EMPTY".to_string());

        let result = generator.generate_create_table(&schema);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("no columns defined"));
    }

    #[test]
    fn test_db2_validate_identifier_invalid() {
        let generator = DB2DDLGenerator::new("TESTSCHEMA".to_string());

        // Empty identifier
        assert!(generator.validate_identifier("").is_err());

        // Starts with number
        assert!(generator.validate_identifier("123table").is_err());

        // Contains special characters
        assert!(generator.validate_identifier("table-name").is_err());
        assert!(generator.validate_identifier("table.name").is_err());
        assert!(generator.validate_identifier("table name").is_err());

        // Too long (> 128 chars)
        let long_name = "a".repeat(129);
        assert!(generator.validate_identifier(&long_name).is_err());
    }

    #[test]
    fn test_db2_validate_identifier_valid() {
        let generator = DB2DDLGenerator::new("TESTSCHEMA".to_string());

        assert!(generator.validate_identifier("table_name").is_ok());
        assert!(generator.validate_identifier("TableName").is_ok());
        assert!(generator.validate_identifier("_table").is_ok());
        assert!(generator.validate_identifier("table123").is_ok());
    }

    #[test]
    fn test_db2_drop_table() {
        let generator = DB2DDLGenerator::new("DB2INST1".to_string());
        let sql = generator.generate_drop_table("PATIENTS").unwrap();
        assert_eq!(sql, "DROP TABLE \"DB2INST1\".\"PATIENTS\"");
    }

    #[test]
    fn test_db2_drop_table_if_exists() {
        let generator = DB2DDLGenerator::new("DB2INST1".to_string());
        let sql = generator.generate_drop_table_if_exists("PATIENTS").unwrap();
        assert!(sql.contains("DROP TABLE \"DB2INST1\".\"PATIENTS\""));
        assert!(sql.contains("IF EXISTS"));
    }

    #[test]
    fn test_db2_table_exists_query() {
        let generator = DB2DDLGenerator::new("DB2INST1".to_string());
        let query = generator.generate_table_exists_query("PATIENTS").unwrap();

        assert!(query.contains("SYSCAT.TABLES"));
        assert!(query.contains("DB2INST1"));
        assert!(query.contains("PATIENTS"));
    }

    #[test]
    fn test_db2_dialect_name() {
        let generator = DB2DDLGenerator::new("TESTSCHEMA".to_string());
        assert_eq!(generator.dialect_name(), "DB2");
    }

    // =============================================================================
    // PostgreSQL DDL Generator Tests
    // =============================================================================

    #[test]
    fn test_postgres_create_table_simple() {
        let generator = PostgresDDLGenerator::new("public".to_string());
        let schema = create_test_schema();

        let ddl = generator.generate_create_table(&schema).unwrap();

        assert!(ddl.contains("CREATE TABLE public.PATIENTS"));
        assert!(ddl.contains("id INTEGER NOT NULL"));
        assert!(ddl.contains("name VARCHAR(255)"));
        assert!(ddl.contains("email VARCHAR(255) NOT NULL"));
        assert!(ddl.contains("PRIMARY KEY (id)"));
    }

    #[test]
    fn test_postgres_create_table_with_foreign_key() {
        let generator = PostgresDDLGenerator::new("public".to_string());
        let mut schema = TableSchema::new("appointments".to_string());

        schema.add_column(
            ColumnDefinition::new("id".to_string(), "INTEGER".to_string(), false).as_primary_key(),
        );

        schema.add_column(ColumnDefinition::new(
            "patient_id".to_string(),
            "INTEGER".to_string(),
            false,
        ));

        schema.add_primary_key("id".to_string());

        schema.add_foreign_key(ForeignKeyDefinition::new(
            "patient_id".to_string(),
            "patients".to_string(),
            "id".to_string(),
        ));

        let ddl = generator.generate_create_table(&schema).unwrap();

        assert!(ddl.contains("CREATE TABLE public.appointments"));
        assert!(ddl.contains("FOREIGN KEY (patient_id) REFERENCES patients (id)"));
    }

    #[test]
    fn test_postgres_validate_identifier_too_long() {
        let generator = PostgresDDLGenerator::new("public".to_string());

        // PostgreSQL has a 63-character limit
        let long_name = "a".repeat(64);
        assert!(generator.validate_identifier(&long_name).is_err());

        let ok_name = "a".repeat(63);
        assert!(generator.validate_identifier(&ok_name).is_ok());
    }

    #[test]
    fn test_postgres_drop_table() {
        let generator = PostgresDDLGenerator::new("public".to_string());
        let sql = generator.generate_drop_table("patients").unwrap();
        assert_eq!(sql, "DROP TABLE public.patients");
    }

    #[test]
    fn test_postgres_drop_table_if_exists() {
        let generator = PostgresDDLGenerator::new("public".to_string());
        let sql = generator.generate_drop_table_if_exists("patients").unwrap();
        assert_eq!(sql, "DROP TABLE IF EXISTS public.patients");
    }

    #[test]
    fn test_postgres_table_exists_query() {
        let generator = PostgresDDLGenerator::new("public".to_string());
        let query = generator.generate_table_exists_query("patients").unwrap();

        assert!(query.contains("information_schema.tables"));
        assert!(query.contains("public"));
        assert!(query.contains("patients"));
    }

    #[test]
    fn test_postgres_dialect_name() {
        let generator = PostgresDDLGenerator::new("public".to_string());
        assert_eq!(generator.dialect_name(), "PostgreSQL");
    }

    // =============================================================================
    // Factory Function Tests
    // =============================================================================

    #[test]
    fn test_create_ddl_generator_db2() {
        let generator = create_ddl_generator("db2", "DB2INST1".to_string()).unwrap();
        assert_eq!(generator.dialect_name(), "DB2");
    }

    #[test]
    fn test_create_ddl_generator_postgres() {
        let generator = create_ddl_generator("postgresql", "public".to_string()).unwrap();
        assert_eq!(generator.dialect_name(), "PostgreSQL");

        let generator = create_ddl_generator("postgres", "public".to_string()).unwrap();
        assert_eq!(generator.dialect_name(), "PostgreSQL");

        let generator = create_ddl_generator("pg", "public".to_string()).unwrap();
        assert_eq!(generator.dialect_name(), "PostgreSQL");
    }

    #[test]
    fn test_create_ddl_generator_invalid() {
        let result = create_ddl_generator("mysql", "test".to_string());
        assert!(result.is_err());
        let err = result.err().expect("expected create_ddl_generator to fail");
        assert!(err.to_string().contains("Unsupported database type"));
    }

    // =============================================================================
    // Integration Tests
    // =============================================================================

    #[test]
    fn test_full_workflow_db2() {
        let generator = DB2DDLGenerator::new("HEALTHCARE".to_string());

        // Create a complete schema
        let mut schema = TableSchema::new("DOCTORS".to_string());

        schema.add_column(
            ColumnDefinition::new("doctor_id".to_string(), "INTEGER".to_string(), false)
                .as_primary_key(),
        );

        schema.add_column(ColumnDefinition::new(
            "first_name".to_string(),
            "VARCHAR(100)".to_string(),
            false,
        ));

        schema.add_column(ColumnDefinition::new(
            "last_name".to_string(),
            "VARCHAR(100)".to_string(),
            false,
        ));

        schema.add_column(ColumnDefinition::new(
            "specialty".to_string(),
            "VARCHAR(255)".to_string(),
            true,
        ));

        schema.add_column(ColumnDefinition::new(
            "hire_date".to_string(),
            "DATE".to_string(),
            false,
        ));

        schema.add_primary_key("doctor_id".to_string());

        let ddl = generator.generate_create_table(&schema).unwrap();

        // Verify all elements are present
        assert!(ddl.contains("CREATE TABLE \"HEALTHCARE\".\"DOCTORS\""));
        assert!(ddl.contains("\"doctor_id\" INTEGER NOT NULL"));
        assert!(ddl.contains("\"first_name\" VARCHAR(100) NOT NULL"));
        assert!(ddl.contains("\"last_name\" VARCHAR(100) NOT NULL"));
        assert!(ddl.contains("\"specialty\" VARCHAR(255)"));
        assert!(ddl.contains("\"hire_date\" DATE NOT NULL"));
        assert!(ddl.contains("PRIMARY KEY (\"doctor_id\")"));
    }

    #[test]
    fn test_full_workflow_postgres() {
        let generator = PostgresDDLGenerator::new("healthcare".to_string());

        // Create a complete schema
        let mut schema = TableSchema::new("doctors".to_string());

        schema.add_column(
            ColumnDefinition::new("doctor_id".to_string(), "INTEGER".to_string(), false)
                .as_primary_key(),
        );

        schema.add_column(ColumnDefinition::new(
            "first_name".to_string(),
            "VARCHAR(100)".to_string(),
            false,
        ));

        schema.add_column(ColumnDefinition::new(
            "last_name".to_string(),
            "VARCHAR(100)".to_string(),
            false,
        ));

        schema.add_column(ColumnDefinition::new(
            "specialty".to_string(),
            "VARCHAR(255)".to_string(),
            true,
        ));

        schema.add_column(ColumnDefinition::new(
            "hire_date".to_string(),
            "DATE".to_string(),
            false,
        ));

        schema.add_primary_key("doctor_id".to_string());

        let ddl = generator.generate_create_table(&schema).unwrap();

        // Verify all elements are present
        assert!(ddl.contains("CREATE TABLE healthcare.doctors"));
        assert!(ddl.contains("doctor_id INTEGER NOT NULL"));
        assert!(ddl.contains("first_name VARCHAR(100) NOT NULL"));
        assert!(ddl.contains("last_name VARCHAR(100) NOT NULL"));
        assert!(ddl.contains("specialty VARCHAR(255)"));
        assert!(ddl.contains("hire_date DATE NOT NULL"));
        assert!(ddl.contains("PRIMARY KEY (doctor_id)"));
    }
}
