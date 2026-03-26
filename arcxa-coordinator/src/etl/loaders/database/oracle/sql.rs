//! Oracle SQL generation helpers for workflow and ETL loading.

use anyhow::{anyhow, Result};

const MAX_IDENTIFIER_LENGTH: usize = 128;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OracleCreateTableColumn {
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
}

pub fn validate_table_name(name: &str) -> Result<()> {
    let parts: Vec<&str> = name.split('.').collect();
    if parts.is_empty() {
        return Err(anyhow!("Oracle table name cannot be empty"));
    }

    for part in parts {
        validate_identifier_segment(part, "table name")?;
    }

    Ok(())
}

pub fn validate_column_names(columns: &[String]) -> Result<()> {
    if columns.is_empty() {
        return Err(anyhow!("Oracle load requires at least one column"));
    }

    for column in columns {
        validate_identifier_segment(column, "column name")?;
    }

    Ok(())
}

pub fn resolve_owner_and_table(
    table_name: &str,
    default_schema: Option<&str>,
    default_owner: Option<&str>,
) -> Result<(String, String)> {
    validate_table_name(table_name)?;

    let parts: Vec<&str> = table_name.split('.').collect();
    match parts.as_slice() {
        [schema, table] => Ok((schema.to_ascii_uppercase(), table.to_ascii_uppercase())),
        [table] => {
            let owner = default_schema
                .or(default_owner)
                .ok_or_else(|| anyhow!("Oracle schema resolution requires a default owner"))?;
            Ok((owner.to_ascii_uppercase(), table.to_ascii_uppercase()))
        }
        _ => Err(anyhow!(
            "Oracle table name '{}' must be TABLE or SCHEMA.TABLE",
            table_name
        )),
    }
}

pub fn generate_insert_all_sql(
    table_name: &str,
    columns: &[String],
    row_count: usize,
) -> Result<String> {
    validate_table_name(table_name)?;
    validate_column_names(columns)?;

    if row_count == 0 {
        return Err(anyhow!("Oracle INSERT ALL requires at least one row"));
    }

    let column_list = columns.join(", ");
    let placeholders = columns.iter().map(|_| "?").collect::<Vec<_>>().join(", ");

    let mut sql = String::from("INSERT ALL\n");
    for _ in 0..row_count {
        sql.push_str(&format!(
            "  INTO {} ({}) VALUES ({})\n",
            table_name, column_list, placeholders
        ));
    }
    sql.push_str("SELECT 1 FROM DUAL");

    Ok(sql)
}

pub fn generate_merge_sql(
    table_name: &str,
    columns: &[String],
    key_fields: &[String],
    row_count: usize,
) -> Result<String> {
    validate_table_name(table_name)?;
    validate_column_names(columns)?;

    if key_fields.is_empty() {
        return Err(anyhow!("Oracle MERGE requires one or more key fields"));
    }

    if row_count == 0 {
        return Err(anyhow!("Oracle MERGE requires at least one row"));
    }

    for key_field in key_fields {
        validate_identifier_segment(key_field, "key field")?;
        if !columns.contains(key_field) {
            return Err(anyhow!(
                "Oracle MERGE key field '{}' not present in load columns",
                key_field
            ));
        }
    }

    let mut source_rows = Vec::with_capacity(row_count);
    source_rows.push(format!(
        "SELECT {} FROM DUAL",
        columns
            .iter()
            .map(|column| format!("? {}", column))
            .collect::<Vec<_>>()
            .join(", ")
    ));

    let plain_select = format!(
        "SELECT {} FROM DUAL",
        columns.iter().map(|_| "?").collect::<Vec<_>>().join(", ")
    );
    for _ in 1..row_count {
        source_rows.push(plain_select.clone());
    }

    let on_clause = key_fields
        .iter()
        .map(|field| format!("target.{field} = source.{field}"))
        .collect::<Vec<_>>()
        .join(" AND ");

    let update_assignments = columns
        .iter()
        .filter(|column| !key_fields.contains(*column))
        .map(|column| format!("target.{column} = source.{column}"))
        .collect::<Vec<_>>();

    let mut sql = format!(
        "MERGE INTO {} target USING (\n  {}\n) source ON ({})",
        table_name,
        source_rows.join("\n  UNION ALL\n  "),
        on_clause
    );

    if !update_assignments.is_empty() {
        sql.push_str(&format!(
            "\nWHEN MATCHED THEN UPDATE SET {}",
            update_assignments.join(", ")
        ));
    }

    sql.push_str(&format!(
        "\nWHEN NOT MATCHED THEN INSERT ({}) VALUES ({})",
        columns.join(", "),
        columns
            .iter()
            .map(|column| format!("source.{column}"))
            .collect::<Vec<_>>()
            .join(", ")
    ));

    Ok(sql)
}

pub fn generate_replace_sql(table_name: &str) -> Result<String> {
    validate_table_name(table_name)?;
    Ok(format!("DELETE FROM {}", table_name))
}

pub fn generate_create_table_sql(
    table_name: &str,
    columns: &[OracleCreateTableColumn],
    primary_keys: &[String],
) -> Result<String> {
    validate_table_name(table_name)?;

    if columns.is_empty() {
        return Err(anyhow!("Oracle CREATE TABLE requires at least one column"));
    }

    let mut definitions = Vec::with_capacity(columns.len() + 1);
    for column in columns {
        validate_identifier_segment(&column.name, "column name")?;
        let oracle_type = normalize_oracle_type(&column.data_type)?;
        let nullable = if column.nullable { "" } else { " NOT NULL" };
        definitions.push(format!("{} {}{}", column.name, oracle_type, nullable));
    }

    if !primary_keys.is_empty() {
        for key in primary_keys {
            validate_identifier_segment(key, "primary key")?;
        }
        definitions.push(format!("PRIMARY KEY ({})", primary_keys.join(", ")));
    }

    Ok(format!(
        "CREATE TABLE {} ({})",
        table_name,
        definitions.join(", ")
    ))
}

fn normalize_oracle_type(data_type: &str) -> Result<String> {
    let normalized = data_type.trim().to_ascii_uppercase();
    if normalized.is_empty() {
        return Err(anyhow!("Oracle column type cannot be empty"));
    }

    let rewritten = match normalized.as_str() {
        "TEXT" | "STRING" | "JSON" | "JSONB" => "CLOB".to_string(),
        "VARCHAR" => "VARCHAR2(255)".to_string(),
        "BOOLEAN" | "BOOL" => "NUMBER(1)".to_string(),
        "DOUBLE PRECISION" => "BINARY_DOUBLE".to_string(),
        "TIMESTAMP WITH TIME ZONE" => "TIMESTAMP WITH TIME ZONE".to_string(),
        _ if normalized.starts_with("VARCHAR(") => normalized.replacen("VARCHAR(", "VARCHAR2(", 1),
        _ if normalized.starts_with("CHAR(") => normalized,
        _ if normalized.starts_with("DECIMAL(") || normalized.starts_with("NUMERIC(") => normalized,
        _ if matches!(
            normalized.as_str(),
            "INTEGER"
                | "BIGINT"
                | "REAL"
                | "DATE"
                | "TIMESTAMP"
                | "CLOB"
                | "BLOB"
                | "NUMBER"
                | "FLOAT"
        ) =>
        {
            normalized
        }
        _ => normalized,
    };

    Ok(rewritten)
}

fn validate_identifier_segment(identifier: &str, identifier_type: &str) -> Result<()> {
    if identifier.is_empty() {
        return Err(anyhow!("Oracle {} cannot be empty", identifier_type));
    }

    if identifier.len() > MAX_IDENTIFIER_LENGTH {
        return Err(anyhow!(
            "Oracle {} '{}' exceeds {} characters",
            identifier_type,
            identifier,
            MAX_IDENTIFIER_LENGTH
        ));
    }

    let first = identifier
        .chars()
        .next()
        .ok_or_else(|| anyhow!("Oracle {} cannot be empty", identifier_type))?;

    if !first.is_ascii_alphabetic() && first != '_' {
        return Err(anyhow!(
            "Oracle {} '{}' must start with a letter or underscore",
            identifier_type,
            identifier
        ));
    }

    if !identifier
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$' || c == '#')
    {
        return Err(anyhow!(
            "Invalid Oracle {} '{}': only [A-Za-z0-9_$#] allowed",
            identifier_type,
            identifier
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_insert_all_sql() {
        let columns = vec!["CUSTOMER_ID".to_string(), "NAME".to_string()];
        let sql = generate_insert_all_sql("CRM.CUSTOMERS", &columns, 2).unwrap();

        assert!(sql.contains("INSERT ALL"));
        assert!(sql.contains("INTO CRM.CUSTOMERS (CUSTOMER_ID, NAME) VALUES (?, ?)"));
        assert!(sql.contains("SELECT 1 FROM DUAL"));
    }

    #[test]
    fn generates_merge_sql() {
        let columns = vec!["CUSTOMER_ID".to_string(), "NAME".to_string()];
        let keys = vec!["CUSTOMER_ID".to_string()];
        let sql = generate_merge_sql("CRM.CUSTOMERS", &columns, &keys, 2).unwrap();

        assert!(sql.contains("MERGE INTO CRM.CUSTOMERS target"));
        assert!(sql.contains("target.CUSTOMER_ID = source.CUSTOMER_ID"));
        assert!(sql.contains("WHEN MATCHED THEN UPDATE SET target.NAME = source.NAME"));
        assert!(sql.contains("WHEN NOT MATCHED THEN INSERT (CUSTOMER_ID, NAME) VALUES (source.CUSTOMER_ID, source.NAME)"));
    }

    #[test]
    fn rejects_invalid_identifier() {
        let columns = vec!["bad-name".to_string()];
        assert!(generate_insert_all_sql("CRM.CUSTOMERS", &columns, 1).is_err());
    }

    #[test]
    fn generates_create_table_sql() {
        let columns = vec![
            OracleCreateTableColumn {
                name: "CUSTOMER_ID".to_string(),
                data_type: "INTEGER".to_string(),
                nullable: false,
            },
            OracleCreateTableColumn {
                name: "PROFILE_JSON".to_string(),
                data_type: "JSON".to_string(),
                nullable: true,
            },
        ];

        let sql =
            generate_create_table_sql("CRM.CUSTOMERS", &columns, &["CUSTOMER_ID".to_string()])
                .unwrap();

        assert!(sql.contains("CREATE TABLE CRM.CUSTOMERS"));
        assert!(sql.contains("CUSTOMER_ID INTEGER NOT NULL"));
        assert!(sql.contains("PROFILE_JSON CLOB"));
        assert!(sql.contains("PRIMARY KEY (CUSTOMER_ID)"));
    }
}
