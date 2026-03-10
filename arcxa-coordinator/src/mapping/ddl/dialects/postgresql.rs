//! PostgreSQL SQL Dialect
//!
//! SQL generation for PostgreSQL.

use super::*;

/// PostgreSQL SQL dialect
pub struct PostgreSqlDialect;

impl SqlDialect for PostgreSqlDialect {
    fn name(&self) -> &str {
        "PostgreSQL"
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
            "http://www.w3.org/2001/XMLSchema#decimal" => "NUMERIC(19,4)".to_string(),
            "http://www.w3.org/2001/XMLSchema#double" => "DOUBLE PRECISION".to_string(),
            "http://www.w3.org/2001/XMLSchema#float" => "REAL".to_string(),
            "http://www.w3.org/2001/XMLSchema#boolean" => "BOOLEAN".to_string(),
            "http://www.w3.org/2001/XMLSchema#date" => "DATE".to_string(),
            "http://www.w3.org/2001/XMLSchema#dateTime" => "TIMESTAMP".to_string(),
            "http://www.w3.org/2001/XMLSchema#time" => "TIME".to_string(),
            "http://www.w3.org/2001/XMLSchema#hexBinary" => "BYTEA".to_string(),
            "http://www.w3.org/2001/XMLSchema#base64Binary" => "BYTEA".to_string(),
            "http://www.w3.org/1999/02/22-rdf-syntax-ns#JSON" => "JSONB".to_string(),
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
            panic!(
                "Invalid table definition for PostgreSQL DDL generation: {}",
                e
            );
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
            sql.push_str(&format!("PRIMARY KEY ({})", table.primary_key.join(", ")));
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
            parts.push(format!("CHECK ({})", check));
        }

        parts.join(" ")
    }

    fn create_primary_key(&self, table_name: &str, columns: &[String]) -> String {
        format!(
            "ALTER TABLE {} ADD PRIMARY KEY ({})",
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

        if let Some(on_update) = fk.on_update {
            sql.push_str(&format!(" ON UPDATE {}", on_update.to_sql()));
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
        format!(
            "ALTER TABLE {} ADD COLUMN {}",
            table,
            self.create_column(column)
        )
    }

    fn alter_table_drop_column(&self, table: &str, column: &str) -> String {
        format!("ALTER TABLE {} DROP COLUMN {}", table, column)
    }

    fn alter_table_modify_column(&self, table: &str, column: &ColumnDefinition) -> Result<String> {
        // PostgreSQL uses ALTER COLUMN for modifications
        let mut sql = format!("ALTER TABLE {} ALTER COLUMN {}", table, column.name);

        // PostgreSQL requires separate statements for different modifications
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
            "SELECT 1 FROM information_schema.tables WHERE table_name = '{}'",
            table.to_lowercase()
        )
    }

    fn check_column_exists(&self, table: &str, column: &str) -> String {
        format!(
            "SELECT 1 FROM information_schema.columns WHERE table_name = '{}' AND column_name = '{}'",
            table.to_lowercase(),
            column.to_lowercase()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_map_datatype_string() {
        let dialect = PostgreSqlDialect;
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
        let dialect = PostgreSqlDialect;
        assert_eq!(
            dialect.map_datatype("http://www.w3.org/2001/XMLSchema#integer", None),
            "INTEGER"
        );
        assert_eq!(
            dialect.map_datatype("http://www.w3.org/2001/XMLSchema#decimal", None),
            "NUMERIC(19,4)"
        );
        assert_eq!(
            dialect.map_datatype("http://www.w3.org/2001/XMLSchema#double", None),
            "DOUBLE PRECISION"
        );
    }

    #[test]
    fn test_map_datatype_json() {
        let dialect = PostgreSqlDialect;
        assert_eq!(
            dialect.map_datatype("http://www.w3.org/1999/02/22-rdf-syntax-ns#JSON", None),
            "JSONB"
        );
    }

    #[test]
    fn test_create_column() {
        let dialect = PostgreSqlDialect;
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
        assert_eq!(sql, "email VARCHAR(255) NOT NULL");
    }

    #[test]
    fn test_create_column_with_default() {
        let dialect = PostgreSqlDialect;
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
        let dialect = PostgreSqlDialect;
        let table = TableDefinition {
            name: "customers".to_string(),
            columns: vec![
                ColumnDefinition {
                    name: "id".to_string(),
                    sql_type: "INTEGER".to_string(),
                    nullable: false,
                    default_value: None,
                    primary_key: true,
                    unique: false,
                    check_constraint: None,
                    comment: None,
                },
                ColumnDefinition {
                    name: "email".to_string(),
                    sql_type: "VARCHAR(255)".to_string(),
                    nullable: false,
                    default_value: None,
                    primary_key: false,
                    unique: true,
                    check_constraint: None,
                    comment: None,
                },
            ],
            primary_key: vec!["id".to_string()],
            foreign_keys: vec![],
            indexes: vec![],
            comment: None,
        };

        let sql = dialect.create_table(&table);
        assert!(sql.contains("CREATE TABLE customers"));
        assert!(sql.contains("id INTEGER NOT NULL"));
        assert!(sql.contains("email VARCHAR(255) NOT NULL"));
        assert!(sql.contains("PRIMARY KEY (id)"));
    }

    #[test]
    fn test_create_index() {
        let dialect = PostgreSqlDialect;
        let index = IndexDefinition {
            name: "idx_email".to_string(),
            table: "customers".to_string(),
            columns: vec!["email".to_string()],
            unique: true,
        };

        let sql = dialect.create_index(&index);
        assert_eq!(sql, "CREATE UNIQUE INDEX idx_email ON customers (email)");
    }

    #[test]
    fn test_check_table_exists() {
        let dialect = PostgreSqlDialect;
        let sql = dialect.check_table_exists("customers");
        assert!(sql.contains("information_schema.tables"));
        assert!(sql.contains("customers"));
    }

    #[test]
    fn test_check_column_exists() {
        let dialect = PostgreSqlDialect;
        let sql = dialect.check_column_exists("customers", "email");
        assert!(sql.contains("information_schema.columns"));
        assert!(sql.contains("customers"));
        assert!(sql.contains("email"));
    }
}
