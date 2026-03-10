//! DB2 SQL Dialect
//!
//! SQL generation for IBM DB2.

use super::*;
use serde_json::Value as JsonValue;

/// DB2 SQL dialect
pub struct Db2Dialect;

impl SqlDialect for Db2Dialect {
    fn name(&self) -> &str {
        "DB2"
    }

    fn map_datatype(&self, xsd_uri: &str, max_length: Option<u32>) -> String {
        match xsd_uri {
            "http://www.w3.org/2001/XMLSchema#string" => {
                let len = max_length.unwrap_or(255);
                format!("VARCHAR({})", len)
            }
            "http://www.w3.org/2001/XMLSchema#integer" => "INTEGER".to_string(),
            "http://www.w3.org/2001/XMLSchema#long" => "BIGINT".to_string(),
            "http://www.w3.org/2001/XMLSchema#int" => "INTEGER".to_string(),
            "http://www.w3.org/2001/XMLSchema#short" => "SMALLINT".to_string(),
            "http://www.w3.org/2001/XMLSchema#decimal" => "DECIMAL(19,4)".to_string(),
            "http://www.w3.org/2001/XMLSchema#double" => "DOUBLE".to_string(),
            "http://www.w3.org/2001/XMLSchema#float" => "REAL".to_string(),
            "http://www.w3.org/2001/XMLSchema#boolean" => "BOOLEAN".to_string(),
            "http://www.w3.org/2001/XMLSchema#date" => "DATE".to_string(),
            "http://www.w3.org/2001/XMLSchema#dateTime" => "TIMESTAMP".to_string(),
            "http://www.w3.org/2001/XMLSchema#time" => "TIME".to_string(),
            "http://www.w3.org/2001/XMLSchema#hexBinary" => "BLOB".to_string(),
            "http://www.w3.org/2001/XMLSchema#base64Binary" => "BLOB".to_string(),
            "http://www.w3.org/1999/02/22-rdf-syntax-ns#JSON" => "CLOB".to_string(),
            _ => {
                // Fallback to VARCHAR
                let len = max_length.unwrap_or(255);
                format!("VARCHAR({})", len)
            }
        }
    }

    fn create_table(&self, table: &TableDefinition) -> String {
        // Validate table definition to prevent SQL injection
        if let Err(e) = table.validate() {
            panic!("Invalid table definition for DB2 DDL generation: {}", e);
        }

        let table_name = table.name.to_uppercase();
        let mut sql = format!("CREATE TABLE {} (\n", table_name);

        // Add columns
        let column_defs: Vec<String> = table
            .columns
            .iter()
            .map(|col| format!("  {}", self.create_column(col)))
            .collect();

        sql.push_str(&column_defs.join(",\n"));

        // Add primary key constraint
        if !table.primary_key.is_empty() {
            let pk_columns: Vec<String> = table
                .primary_key
                .iter()
                .map(|col| col.to_uppercase())
                .collect();
            sql.push_str(",\n  ");
            sql.push_str(&format!("PRIMARY KEY ({})", pk_columns.join(", ")));
        }

        sql.push_str("\n)");

        // Add table comment if present
        if let Some(comment) = &table.comment {
            sql.push_str(&format!(
                ";\nCOMMENT ON TABLE {} IS '{}'",
                table_name, comment
            ));
        }

        sql
    }

    fn create_column(&self, column: &ColumnDefinition) -> String {
        let column_name = column.name.to_uppercase();
        let mut parts = vec![column_name.clone(), column.sql_type.clone()];

        // NOT NULL constraint
        if !column.nullable {
            parts.push("NOT NULL".to_string());
        }

        // DEFAULT value
        if let Some(default) = &column.default_value {
            parts.push(format!("DEFAULT {}", default));
        }

        // CHECK constraint
        if let Some(check) = &column.check_constraint {
            let normalized_check = check.replace(&column.name, &column_name);
            parts.push(format!("CHECK ({})", normalized_check));
        }

        parts.join(" ")
    }

    fn create_primary_key(&self, table_name: &str, columns: &[String]) -> String {
        let normalized_table = table_name.to_uppercase();
        let normalized_columns: Vec<String> =
            columns.iter().map(|col| col.to_uppercase()).collect();
        format!(
            "ALTER TABLE {} ADD PRIMARY KEY ({})",
            normalized_table,
            normalized_columns.join(", ")
        )
    }

    fn create_foreign_key(&self, table_name: &str, fk: &ForeignKeyDefinition) -> String {
        let normalized_table = table_name.to_uppercase();
        let normalized_columns: Vec<String> =
            fk.columns.iter().map(|col| col.to_uppercase()).collect();
        let normalized_ref_table = fk.ref_table.to_uppercase();
        let normalized_ref_columns: Vec<String> = fk
            .ref_columns
            .iter()
            .map(|col| col.to_uppercase())
            .collect();
        let mut sql = format!(
            "ALTER TABLE {} ADD CONSTRAINT {} FOREIGN KEY ({}) REFERENCES {} ({})",
            normalized_table,
            fk.name,
            normalized_columns.join(", "),
            normalized_ref_table,
            normalized_ref_columns.join(", ")
        );

        if let Some(on_delete) = fk.on_delete {
            sql.push_str(&format!(" ON DELETE {}", on_delete.to_sql()));
        }

        if let Some(on_update) = fk.on_update {
            sql.push_str(&format!(" ON UPDATE {}", on_update.to_sql()));
        }

        sql
    }

    fn create_index(&self, index: &IndexDefinition) -> String {
        let unique_keyword = if index.unique { "UNIQUE " } else { "" };
        let normalized_table = index.table.to_uppercase();
        let normalized_columns: Vec<String> =
            index.columns.iter().map(|col| col.to_uppercase()).collect();

        format!(
            "CREATE {}INDEX {} ON {} ({})",
            unique_keyword,
            index.name,
            normalized_table,
            normalized_columns.join(", ")
        )
    }

    fn alter_table_add_column(&self, table: &str, column: &ColumnDefinition) -> String {
        format!(
            "ALTER TABLE {} ADD COLUMN {}",
            table.to_uppercase(),
            self.create_column(column)
        )
    }

    fn alter_table_drop_column(&self, table: &str, column: &str) -> String {
        format!(
            "ALTER TABLE {} DROP COLUMN {}",
            table.to_uppercase(),
            column.to_uppercase()
        )
    }

    fn alter_table_modify_column(&self, table: &str, column: &ColumnDefinition) -> Result<String> {
        // DB2 uses ALTER COLUMN for modifications
        let mut sql = format!(
            "ALTER TABLE {} ALTER COLUMN {}",
            table.to_uppercase(),
            column.name.to_uppercase()
        );

        // DB2 requires separate statements for different modifications
        // This is a simplified version - full implementation would need multiple statements

        if !column.nullable {
            sql.push_str(" SET NOT NULL");
        } else {
            sql.push_str(" DROP NOT NULL");
        }

        Ok(sql)
    }

    fn check_table_exists(&self, table: &str) -> String {
        format!(
            "SELECT 1 FROM SYSCAT.TABLES WHERE TABNAME = '{}' AND TABSCHEMA = CURRENT SCHEMA",
            table.to_uppercase()
        )
    }

    fn check_column_exists(&self, table: &str, column: &str) -> String {
        format!(
            "SELECT 1 FROM SYSCAT.COLUMNS WHERE TABNAME = '{}' AND COLNAME = '{}' AND TABSCHEMA = CURRENT SCHEMA",
            table.to_uppercase(),
            column.to_uppercase()
        )
    }

    fn pattern_constraint(&self, column: &str, pattern: &str) -> String {
        // DB2 uses REGEXP_LIKE function
        format!("REGEXP_LIKE({}, '{}')", column, pattern.replace("'", "''"))
    }
}

// =============================================================================
// DB2-specific extensions (not part of SqlDialect trait)
// =============================================================================

impl Db2Dialect {
    /// Infer DB2 SQL type from JSON value
    ///
    /// This is the canonical type inference method for JSON-to-DB2 mapping.
    /// Used by ETL workflows and data loaders to automatically determine
    /// appropriate DB2 column types from JSON data.
    ///
    /// # Type Mapping Rules
    ///
    /// - `null` → `VARCHAR(255)` (default fallback)
    /// - `boolean` → `SMALLINT` (DB2 doesn't have native BOOLEAN pre-v11.1)
    /// - `integer` → `BIGINT` (safe for large integers)
    /// - `float` → `DECIMAL(19,4)` (preserves precision)
    /// - `string` → `VARCHAR(n)` or `CLOB` (based on length)
    /// - `array/object` → `CLOB` (JSON stored as text)
    ///
    /// # Examples
    ///
    /// ```
    /// use graphica_coordinator::mapping::ddl::dialects::db2::Db2Dialect;
    /// use serde_json::json;
    ///
    /// let dialect = Db2Dialect;
    /// assert_eq!(dialect.infer_sql_type_from_json(&json!(42)), "BIGINT");
    /// assert_eq!(dialect.infer_sql_type_from_json(&json!("test")), "VARCHAR(255)");
    /// assert_eq!(dialect.infer_sql_type_from_json(&json!(true)), "SMALLINT");
    /// ```
    pub fn infer_sql_type_from_json(&self, value: &JsonValue) -> String {
        match value {
            JsonValue::Null => "VARCHAR(255)".to_string(),
            JsonValue::Bool(_) => "SMALLINT".to_string(), // DB2 uses SMALLINT for boolean
            JsonValue::Number(n) => {
                if n.is_i64() {
                    "BIGINT".to_string()
                } else {
                    "DECIMAL(19,4)".to_string()
                }
            }
            JsonValue::String(s) => {
                // Infer based on string length
                let len = s.len();
                if len <= 255 {
                    "VARCHAR(255)".to_string()
                } else if len <= 4000 {
                    // Round up to nearest 100 for efficiency
                    format!("VARCHAR({})", ((len / 100) + 1) * 100)
                } else {
                    "CLOB".to_string()
                }
            }
            // Complex types stored as JSON text
            JsonValue::Array(_) => "CLOB".to_string(),
            JsonValue::Object(_) => "CLOB".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_map_datatype_string() {
        let dialect = Db2Dialect;
        assert_eq!(
            dialect.map_datatype("http://www.w3.org/2001/XMLSchema#string", None),
            "VARCHAR(255)"
        );
        assert_eq!(
            dialect.map_datatype("http://www.w3.org/2001/XMLSchema#string", Some(100)),
            "VARCHAR(100)"
        );
    }

    #[test]
    fn test_map_datatype_numeric() {
        let dialect = Db2Dialect;
        assert_eq!(
            dialect.map_datatype("http://www.w3.org/2001/XMLSchema#integer", None),
            "INTEGER"
        );
        assert_eq!(
            dialect.map_datatype("http://www.w3.org/2001/XMLSchema#decimal", None),
            "DECIMAL(19,4)"
        );
    }

    #[test]
    fn test_create_column() {
        let dialect = Db2Dialect;
        let column = ColumnDefinition {
            name: "email".to_string(),
            sql_type: "VARCHAR(255)".to_string(),
            nullable: false,
            default_value: None,
            primary_key: false,
            unique: false,
            check_constraint: None,
            comment: None,
        };

        let sql = dialect.create_column(&column);
        assert_eq!(sql, "EMAIL VARCHAR(255) NOT NULL");
    }

    #[test]
    fn test_create_column_with_default() {
        let dialect = Db2Dialect;
        let column = ColumnDefinition {
            name: "status".to_string(),
            sql_type: "VARCHAR(20)".to_string(),
            nullable: false,
            default_value: Some("'ACTIVE'".to_string()),
            primary_key: false,
            unique: false,
            check_constraint: None,
            comment: None,
        };

        let sql = dialect.create_column(&column);
        assert!(sql.contains("DEFAULT 'ACTIVE'"));
    }

    #[test]
    fn test_create_table() {
        let dialect = Db2Dialect;
        let table = TableDefinition {
            name: "CUSTOMERS".to_string(),
            columns: vec![
                ColumnDefinition {
                    name: "ID".to_string(),
                    sql_type: "INTEGER".to_string(),
                    nullable: false,
                    default_value: None,
                    primary_key: true,
                    unique: false,
                    check_constraint: None,
                    comment: None,
                },
                ColumnDefinition {
                    name: "EMAIL".to_string(),
                    sql_type: "VARCHAR(255)".to_string(),
                    nullable: false,
                    default_value: None,
                    primary_key: false,
                    unique: true,
                    check_constraint: None,
                    comment: None,
                },
            ],
            primary_key: vec!["ID".to_string()],
            foreign_keys: vec![],
            indexes: vec![],
            comment: None,
        };

        let sql = dialect.create_table(&table);
        assert!(sql.contains("CREATE TABLE CUSTOMERS"));
        assert!(sql.contains("ID INTEGER NOT NULL"));
        assert!(sql.contains("EMAIL VARCHAR(255) NOT NULL"));
        assert!(sql.contains("PRIMARY KEY (ID)"));
    }

    #[test]
    fn test_create_index() {
        let dialect = Db2Dialect;
        let index = IndexDefinition {
            name: "IDX_EMAIL".to_string(),
            table: "CUSTOMERS".to_string(),
            columns: vec!["EMAIL".to_string()],
            unique: true,
        };

        let sql = dialect.create_index(&index);
        assert_eq!(sql, "CREATE UNIQUE INDEX IDX_EMAIL ON CUSTOMERS (EMAIL)");
    }
}
