//! Common Database Utilities
//!
//! Shared functionality across all database loaders.

use anyhow::Result;
use std::collections::HashMap;

fn validate_postgres_identifier(identifier: &str, identifier_type: &str) -> Result<()> {
    use graphica_core::security::validate_identifier;

    validate_identifier(identifier).map_err(|e| {
        anyhow::anyhow!("Invalid PostgreSQL {} '{}': {}", identifier_type, identifier, e)
    })?;
    Ok(())
}

fn quote_postgres_identifier(identifier: &str) -> String {
    format!("\"{}\"", identifier.replace('"', "\"\""))
}

pub fn quote_postgres_table_name(table_name: &str) -> Result<String> {
    let parts: Vec<&str> = table_name.split('.').collect();
    if parts.is_empty() || parts.iter().any(|part| part.is_empty()) {
        anyhow::bail!("Invalid PostgreSQL table name '{}'", table_name);
    }

    let mut quoted_parts = Vec::with_capacity(parts.len());
    for part in parts {
        validate_postgres_identifier(part, "table name")?;
        quoted_parts.push(quote_postgres_identifier(part));
    }

    Ok(quoted_parts.join("."))
}

pub fn quote_postgres_columns(columns: &[String]) -> Result<Vec<String>> {
    if columns.is_empty() {
        anyhow::bail!("No columns specified");
    }

    columns
        .iter()
        .map(|column| {
            validate_postgres_identifier(column, "column name")?;
            Ok(quote_postgres_identifier(column))
        })
        .collect()
}

/// Generate INSERT SQL for batch loading
///
/// Creates parameterized INSERT statement for efficient bulk loading.
///
/// # Example
/// ```ignore
/// let sql = generate_insert_sql("users", &["id", "name"], 2)?;
/// // INSERT INTO users (id, name) VALUES ($1, $2), ($3, $4)
/// ```
pub fn generate_insert_sql(
    table_name: &str,
    columns: &[String],
    row_count: usize,
) -> Result<String> {
    if columns.is_empty() {
        anyhow::bail!("No columns specified for INSERT");
    }

    let quoted_table = quote_postgres_table_name(table_name)?;
    let quoted_columns = quote_postgres_columns(columns)?;

    let mut sql = format!("INSERT INTO {} (", quoted_table);
    sql.push_str(&quoted_columns.join(", "));
    sql.push_str(") VALUES\n");

    let mut value_clauses = Vec::new();
    for i in 0..row_count {
        let mut placeholders = Vec::new();
        for j in 0..columns.len() {
            let param_num = i * columns.len() + j + 1;
            placeholders.push(format!("${}", param_num));
        }
        value_clauses.push(format!("    ({})", placeholders.join(", ")));
    }

    sql.push_str(&value_clauses.join(",\n"));

    Ok(sql)
}

/// Generate UPSERT SQL (INSERT ... ON CONFLICT UPDATE)
///
/// PostgreSQL-specific UPSERT with ON CONFLICT clause.
///
/// # Example
/// ```ignore
/// let sql = generate_upsert_sql("users", &["id", "name", "email"], &["id"], 1)?;
/// // INSERT INTO users (id, name, email) VALUES ($1, $2, $3)
/// // ON CONFLICT (id) DO UPDATE SET name = EXCLUDED.name, email = EXCLUDED.email
/// ```
pub fn generate_upsert_sql(
    table_name: &str,
    columns: &[String],
    key_fields: &[String],
    row_count: usize,
) -> Result<String> {
    if columns.is_empty() {
        anyhow::bail!("No columns specified for UPSERT");
    }

    if key_fields.is_empty() {
        anyhow::bail!("No key fields specified for UPSERT");
    }

    let quoted_table = quote_postgres_table_name(table_name)?;
    let quoted_columns = quote_postgres_columns(columns)?;
    let quoted_key_fields = quote_postgres_columns(key_fields)?;

    let mut sql = format!("INSERT INTO {} (", quoted_table);
    sql.push_str(&quoted_columns.join(", "));
    sql.push_str(") VALUES\n");

    let mut value_clauses = Vec::new();
    for i in 0..row_count {
        let mut placeholders = Vec::new();
        for j in 0..columns.len() {
            let param_num = i * columns.len() + j + 1;
            placeholders.push(format!("${}", param_num));
        }
        value_clauses.push(format!("    ({})", placeholders.join(", ")));
    }

    sql.push_str(&value_clauses.join(",\n"));

    // Add ON CONFLICT clause
    sql.push_str(&format!(
        "\nON CONFLICT ({}) DO UPDATE SET\n",
        quoted_key_fields.join(", ")
    ));

    // Generate UPDATE SET clause for non-key columns
    let update_columns: Vec<String> = columns
        .iter()
        .filter(|col| !key_fields.contains(col))
        .map(|col| {
            let quoted = quote_postgres_identifier(col);
            format!("    {} = EXCLUDED.{}", quoted, quoted)
        })
        .collect();

    if update_columns.is_empty() {
        anyhow::bail!("No columns to update in UPSERT (all columns are key fields)");
    }

    sql.push_str(&update_columns.join(",\n"));

    Ok(sql)
}

/// Generate CSV data for COPY FROM STDIN
///
/// PostgreSQL COPY format with proper escaping.
///
/// # Arguments
/// * `columns` - Column names in order
/// * `rows` - Data rows
/// * `delimiter` - CSV delimiter (default: ',')
/// * `quote` - Quote character (default: '"')
/// * `null_string` - NULL representation (default: empty string)
pub fn generate_csv_for_copy(
    columns: &[String],
    rows: &[HashMap<String, Option<String>>],
    delimiter: char,
    quote: char,
    null_string: &str,
) -> Result<String> {
    let mut csv_data = String::new();

    for row in rows {
        let mut values = Vec::new();

        for column in columns {
            let value = row.get(column).and_then(|v| v.as_ref());

            match value {
                Some(v) => {
                    // Escape quotes and add quotes if needed
                    let escaped = v.replace(&quote.to_string(), &format!("{}{}", quote, quote));

                    // Quote if contains delimiter, quote, or newline
                    if escaped.contains(delimiter)
                        || escaped.contains(quote)
                        || escaped.contains('\n')
                        || escaped.contains('\r')
                    {
                        values.push(format!("{}{}{}", quote, escaped, quote));
                    } else {
                        values.push(escaped);
                    }
                }
                None => {
                    values.push(null_string.to_string());
                }
            }
        }

        csv_data.push_str(&values.join(&delimiter.to_string()));
        csv_data.push('\n');
    }

    Ok(csv_data)
}

/// Flatten row values into parameter list for parameterized queries
///
/// Converts row data into a flat vector suitable for tokio-postgres execute.
pub fn flatten_rows_to_params(
    columns: &[String],
    rows: &[HashMap<String, Option<String>>],
) -> Vec<Option<String>> {
    rows.iter()
        .flat_map(|row| {
            columns
                .iter()
                .map(move |col| row.get(col).and_then(|v| v.clone()))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_insert_sql() {
        let columns = vec!["id".to_string(), "name".to_string()];
        let sql = generate_insert_sql("users", &columns, 2).unwrap();

        assert!(sql.contains("INSERT INTO \"users\""));
        assert!(sql.contains("(\"id\", \"name\")"));
        assert!(sql.contains("($1, $2)"));
        assert!(sql.contains("($3, $4)"));
    }

    #[test]
    fn test_generate_upsert_sql() {
        let columns = vec!["id".to_string(), "name".to_string(), "email".to_string()];
        let key_fields = vec!["id".to_string()];
        let sql = generate_upsert_sql("users", &columns, &key_fields, 1).unwrap();

        assert!(sql.contains("INSERT INTO \"users\""));
        assert!(sql.contains("ON CONFLICT (\"id\") DO UPDATE SET"));
        assert!(sql.contains("\"name\" = EXCLUDED.\"name\""));
        assert!(sql.contains("\"email\" = EXCLUDED.\"email\""));
        assert!(!sql.contains("\"id\" = EXCLUDED.\"id\"")); // Key field shouldn't be in UPDATE
    }

    #[test]
    fn test_quote_postgres_table_name_supports_schema_qualified_names() {
        let quoted = quote_postgres_table_name("ml_demo.customer_training_features").unwrap();
        assert_eq!(quoted, "\"ml_demo\".\"customer_training_features\"");
    }

    #[test]
    fn test_generate_csv_for_copy() {
        let columns = vec!["id".to_string(), "name".to_string()];
        let mut row1 = HashMap::new();
        row1.insert("id".to_string(), Some("1".to_string()));
        row1.insert("name".to_string(), Some("Alice".to_string()));

        let mut row2 = HashMap::new();
        row2.insert("id".to_string(), Some("2".to_string()));
        row2.insert("name".to_string(), None);

        let rows = vec![row1, row2];

        let csv = generate_csv_for_copy(&columns, &rows, ',', '"', "").unwrap();

        assert!(csv.contains("1,Alice"));
        assert!(csv.contains("2,\n")); // NULL represented as empty
    }

    #[test]
    fn test_csv_escaping() {
        let columns = vec!["name".to_string()];
        let mut row = HashMap::new();
        row.insert(
            "name".to_string(),
            Some("John \"The Boss\" Smith".to_string()),
        );

        let rows = vec![row];
        let csv = generate_csv_for_copy(&columns, &rows, ',', '"', "").unwrap();

        // Quotes should be escaped and value quoted
        assert!(csv.contains("\"John \"\"The Boss\"\" Smith\""));
    }
}
