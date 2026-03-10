//! Oracle SQL Dialect
//!
//! SQL generation for Oracle Database.

use super::*;

/// Oracle SQL dialect
pub struct OracleDialect;

impl SqlDialect for OracleDialect {
    fn name(&self) -> &str {
        "Oracle"
    }

    fn map_datatype(&self, xsd_uri: &str, max_length: Option<u32>) -> String {
        match xsd_uri {
            "http://www.w3.org/2001/XMLSchema#string" => {
                let len = max_length.unwrap_or(255);
                format!("VARCHAR2({})", len)
            }
            "http://www.w3.org/2001/XMLSchema#integer" => "NUMBER(10)".to_string(),
            "http://www.w3.org/2001/XMLSchema#long" => "NUMBER(19)".to_string(),
            "http://www.w3.org/2001/XMLSchema#int" => "NUMBER(10)".to_string(),
            "http://www.w3.org/2001/XMLSchema#short" => "NUMBER(5)".to_string(),
            "http://www.w3.org/2001/XMLSchema#decimal" => "NUMBER(19,4)".to_string(),
            "http://www.w3.org/2001/XMLSchema#double" => "BINARY_DOUBLE".to_string(),
            "http://www.w3.org/2001/XMLSchema#float" => "BINARY_FLOAT".to_string(),
            "http://www.w3.org/2001/XMLSchema#boolean" => "NUMBER(1)".to_string(),
            "http://www.w3.org/2001/XMLSchema#date" => "DATE".to_string(),
            "http://www.w3.org/2001/XMLSchema#dateTime" => "TIMESTAMP".to_string(),
            "http://www.w3.org/2001/XMLSchema#time" => "TIMESTAMP".to_string(),
            "http://www.w3.org/2001/XMLSchema#hexBinary" => "BLOB".to_string(),
            "http://www.w3.org/2001/XMLSchema#base64Binary" => "BLOB".to_string(),
            "http://www.w3.org/1999/02/22-rdf-syntax-ns#JSON" => "CLOB".to_string(),
            _ => {
                // Fallback to VARCHAR2
                let len = max_length.unwrap_or(255);
                format!("VARCHAR2({})", len)
            }
        }
    }

    fn create_table(&self, table: &TableDefinition) -> String {
        // Validate table definition to prevent SQL injection
        if let Err(e) = table.validate() {
            panic!("Invalid table definition for Oracle DDL generation: {}", e);
        }

        let mut sql = format!("CREATE TABLE {} (\n", table.name);

        // Add columns
        let column_defs: Vec<String> = table
            .columns
            .iter()
            .map(|col| format!("  {}", self.create_column(col)))
            .collect();

        sql.push_str(&column_defs.join(",\n"));

        // Add primary key constraint
        if !table.primary_key.is_empty() {
            sql.push_str(",\n  ");
            sql.push_str(&format!(
                "CONSTRAINT {}_PK PRIMARY KEY ({})",
                table.name,
                table.primary_key.join(", ")
            ));
        }

        sql.push_str("\n)");

        // Add table comment if present
        if let Some(comment) = &table.comment {
            sql.push_str(&format!(
                ";\nCOMMENT ON TABLE {} IS '{}'",
                table.name, comment
            ));
        }

        sql
    }

    fn create_column(&self, column: &ColumnDefinition) -> String {
        let mut parts = vec![column.name.clone(), column.sql_type.clone()];

        // DEFAULT value (must come before NOT NULL in Oracle)
        if let Some(default) = &column.default_value {
            parts.push(format!("DEFAULT {}", default));
        }

        // NOT NULL constraint
        if !column.nullable {
            parts.push("NOT NULL".to_string());
        }

        // CHECK constraint
        if let Some(check) = &column.check_constraint {
            parts.push(format!("CHECK ({})", check));
        }

        parts.join(" ")
    }

    fn create_primary_key(&self, table_name: &str, columns: &[String]) -> String {
        format!(
            "ALTER TABLE {} ADD CONSTRAINT {}_PK PRIMARY KEY ({})",
            table_name,
            table_name,
            columns.join(", ")
        )
    }

    fn create_foreign_key(&self, table_name: &str, fk: &ForeignKeyDefinition) -> String {
        let mut sql = format!(
            "ALTER TABLE {} ADD CONSTRAINT {} FOREIGN KEY ({}) REFERENCES {} ({})",
            table_name,
            fk.name,
            fk.columns.join(", "),
            fk.ref_table,
            fk.ref_columns.join(", ")
        );

        if let Some(on_delete) = fk.on_delete {
            sql.push_str(&format!(" ON DELETE {}", on_delete.to_sql()));
        }

        // Oracle does not support ON UPDATE
        if fk.on_update.is_some() {
            // Silently ignore - Oracle doesn't support ON UPDATE CASCADE
        }

        sql
    }

    fn create_index(&self, index: &IndexDefinition) -> String {
        let unique_keyword = if index.unique { "UNIQUE " } else { "" };

        format!(
            "CREATE {}INDEX {} ON {} ({})",
            unique_keyword,
            index.name,
            index.table,
            index.columns.join(", ")
        )
    }

    fn alter_table_add_column(&self, table: &str, column: &ColumnDefinition) -> String {
        format!("ALTER TABLE {} ADD {}", table, self.create_column(column))
    }

    fn alter_table_drop_column(&self, table: &str, column: &str) -> String {
        format!("ALTER TABLE {} DROP COLUMN {}", table, column)
    }

    fn alter_table_modify_column(&self, table: &str, column: &ColumnDefinition) -> Result<String> {
        // Oracle uses MODIFY for column alterations
        Ok(format!(
            "ALTER TABLE {} MODIFY {}",
            table,
            self.create_column(column)
        ))
    }

    fn check_table_exists(&self, table: &str) -> String {
        format!(
            "SELECT 1 FROM user_tables WHERE table_name = '{}'",
            table.to_uppercase()
        )
    }

    fn check_column_exists(&self, table: &str, column: &str) -> String {
        format!(
            "SELECT 1 FROM user_tab_columns WHERE table_name = '{}' AND column_name = '{}'",
            table.to_uppercase(),
            column.to_uppercase()
        )
    }

    fn pattern_constraint(&self, column: &str, pattern: &str) -> String {
        // Oracle uses REGEXP_LIKE function (similar to DB2)
        format!("REGEXP_LIKE({}, '{}')", column, pattern.replace("'", "''"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_map_datatype_string() {
        let dialect = OracleDialect;
        assert_eq!(
            dialect.map_datatype("http://www.w3.org/2001/XMLSchema#string", None),
            "VARCHAR2(255)"
        );
        assert_eq!(
            dialect.map_datatype("http://www.w3.org/2001/XMLSchema#string", Some(100)),
            "VARCHAR2(100)"
        );
    }

    #[test]
    fn test_map_datatype_numeric() {
        let dialect = OracleDialect;
        assert_eq!(
            dialect.map_datatype("http://www.w3.org/2001/XMLSchema#integer", None),
            "NUMBER(10)"
        );
        assert_eq!(
            dialect.map_datatype("http://www.w3.org/2001/XMLSchema#decimal", None),
            "NUMBER(19,4)"
        );
        assert_eq!(
            dialect.map_datatype("http://www.w3.org/2001/XMLSchema#double", None),
            "BINARY_DOUBLE"
        );
    }

    #[test]
    fn test_map_datatype_boolean() {
        let dialect = OracleDialect;
        assert_eq!(
            dialect.map_datatype("http://www.w3.org/2001/XMLSchema#boolean", None),
            "NUMBER(1)"
        );
    }

    #[test]
    fn test_create_column() {
        let dialect = OracleDialect;
        let column = ColumnDefinition {
            name: "email".to_string(),
            sql_type: "VARCHAR2(255)".to_string(),
            nullable: false,
            default_value: None,
            primary_key: false,
            unique: false,
            check_constraint: None,
            comment: None,
        };

        let sql = dialect.create_column(&column);
        assert_eq!(sql, "email VARCHAR2(255) NOT NULL");
    }

    #[test]
    fn test_create_column_with_default() {
        let dialect = OracleDialect;
        let column = ColumnDefinition {
            name: "status".to_string(),
            sql_type: "VARCHAR2(20)".to_string(),
            nullable: false,
            default_value: Some("'ACTIVE'".to_string()),
            primary_key: false,
            unique: false,
            check_constraint: None,
            comment: None,
        };

        let sql = dialect.create_column(&column);
        // In Oracle, DEFAULT comes before NOT NULL
        assert!(sql.contains("DEFAULT 'ACTIVE'"));
        assert!(sql.contains("NOT NULL"));
        assert!(sql.find("DEFAULT").unwrap() < sql.find("NOT NULL").unwrap());
    }

    #[test]
    fn test_create_table() {
        let dialect = OracleDialect;
        let table = TableDefinition {
            name: "CUSTOMERS".to_string(),
            columns: vec![
                ColumnDefinition {
                    name: "ID".to_string(),
                    sql_type: "NUMBER(10)".to_string(),
                    nullable: false,
                    default_value: None,
                    primary_key: true,
                    unique: false,
                    check_constraint: None,
                    comment: None,
                },
                ColumnDefinition {
                    name: "EMAIL".to_string(),
                    sql_type: "VARCHAR2(255)".to_string(),
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
        assert!(sql.contains("ID NUMBER(10) NOT NULL"));
        assert!(sql.contains("EMAIL VARCHAR2(255) NOT NULL"));
        assert!(sql.contains("CONSTRAINT CUSTOMERS_PK PRIMARY KEY (ID)"));
    }

    #[test]
    fn test_create_index() {
        let dialect = OracleDialect;
        let index = IndexDefinition {
            name: "IDX_EMAIL".to_string(),
            table: "CUSTOMERS".to_string(),
            columns: vec!["EMAIL".to_string()],
            unique: true,
        };

        let sql = dialect.create_index(&index);
        assert_eq!(sql, "CREATE UNIQUE INDEX IDX_EMAIL ON CUSTOMERS (EMAIL)");
    }

    #[test]
    fn test_create_primary_key() {
        let dialect = OracleDialect;
        let sql = dialect.create_primary_key("CUSTOMERS", &["ID".to_string()]);
        assert!(sql.contains("CONSTRAINT CUSTOMERS_PK"));
        assert!(sql.contains("PRIMARY KEY (ID)"));
    }

    #[test]
    fn test_check_table_exists() {
        let dialect = OracleDialect;
        let sql = dialect.check_table_exists("customers");
        assert!(sql.contains("user_tables"));
        assert!(sql.contains("CUSTOMERS")); // Should be uppercase
    }

    #[test]
    fn test_check_column_exists() {
        let dialect = OracleDialect;
        let sql = dialect.check_column_exists("customers", "email");
        assert!(sql.contains("user_tab_columns"));
        assert!(sql.contains("CUSTOMERS")); // Should be uppercase
        assert!(sql.contains("EMAIL")); // Should be uppercase
    }
}
