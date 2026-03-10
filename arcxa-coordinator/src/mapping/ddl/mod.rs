//! DDL Generator Module
//!
//! Generate SQL DDL (Data Definition Language) from SHACL constraints.
//!
//! # Architecture
//!
//! This module provides a complete pipeline for generating database schemas from
//! RDF constraints:
//!
//! 1. **SHACL Parsing**: Parse SHACL shapes from RDF triple store
//! 2. **DDL Generation**: Map SHACL constraints to SQL DDL statements
//! 3. **Dialect Support**: Support multiple SQL databases (DB2, PostgreSQL, Oracle)
//! 4. **Schema Evolution**: Generate idempotent migrations for schema changes
//!
//! # Example
//!
//! ```ignore
//! use graphica_coordinator::mapping::ddl::{NodeShape, PropertyShape, get_dialect};
//!
//! // Create a SHACL node shape
//! let mut shape = NodeShape::new(
//!     "http://example.com/shape/Customer".to_string(),
//!     "http://example.com/Customer".to_string(),
//! );
//!
//! // Add a property (column)
//! let mut email_prop = PropertyShape::new("http://schema.org/email".to_string());
//! email_prop.name = Some("email".to_string());
//! email_prop.datatype = Some("http://www.w3.org/2001/XMLSchema#string".to_string());
//! email_prop.min_count = Some(1); // NOT NULL
//! email_prop.max_length = Some(255);
//! shape.add_property(email_prop);
//!
//! // Generate DDL for PostgreSQL
//! let dialect = get_dialect("postgresql").unwrap();
//! let table_def = convert_shape_to_table(&shape, &*dialect);
//! let ddl = dialect.create_table(&table_def);
//! ```

pub mod dialects;
pub mod evolution;
pub mod parser;
pub mod shacl;

// Re-export commonly used types
pub use dialects::{
    get_dialect, ColumnDefinition, ForeignKeyDefinition, IndexDefinition, ReferentialAction,
    SqlDialect, TableDefinition,
};
pub use evolution::{
    MigrationGenerator, MigrationPlan, MigrationStep, SchemaDiff, SchemaDiffEngine,
};
pub use parser::{DbDialect, DdlParser, DdlStatement};
pub use shacl::{NodeKind, NodeShape, PropertyShape, SeverityLevel, ShaclParser};

use anyhow::Result;

/// Convert a SHACL NodeShape to a SQL TableDefinition
///
/// This is the core mapping function that translates SHACL constraints into
/// SQL DDL structures.
///
/// # Mapping Rules
///
/// - `sh:targetClass` → Table name (extracted from URI)
/// - `sh:property` → Column definitions
/// - `sh:minCount >= 1` → NOT NULL
/// - `sh:maxCount = 1` → UNIQUE (if also required)
/// - `sh:datatype` → SQL type via dialect mapping
/// - `sh:maxLength` → VARCHAR size
/// - `sh:pattern` → CHECK constraint
/// - `sh:class` → Foreign key relationship
///
/// # Arguments
///
/// * `shape` - The SHACL NodeShape to convert
/// * `dialect` - The SQL dialect for type mapping
///
/// # Returns
///
/// A `TableDefinition` ready for SQL generation
pub fn convert_shape_to_table(shape: &NodeShape, dialect: &dyn SqlDialect) -> TableDefinition {
    let table_name = shape.get_table_name();

    // Convert each property shape to a column definition
    let columns: Vec<ColumnDefinition> = shape
        .properties
        .iter()
        .map(|prop| convert_property_to_column(prop, dialect))
        .collect();

    // Determine primary key from properties
    let primary_key: Vec<String> = shape
        .get_primary_key_properties()
        .iter()
        .map(|p| p.get_column_name())
        .collect();

    // Generate foreign keys from sh:class constraints
    let foreign_keys: Vec<ForeignKeyDefinition> = shape
        .get_foreign_key_properties()
        .iter()
        .enumerate()
        .map(|(idx, prop)| {
            let column_name = prop.get_column_name();
            let ref_table = prop
                .class
                .as_ref()
                .and_then(|uri| uri.split(['/', '#']).last())
                .unwrap_or("UNKNOWN")
                .to_uppercase();

            ForeignKeyDefinition {
                name: format!("FK_{}_{}", table_name, idx),
                columns: vec![column_name],
                ref_table,
                ref_columns: vec!["ID".to_string()], // Assume ID convention
                on_delete: Some(ReferentialAction::Restrict),
                on_update: None,
            }
        })
        .collect();

    // Generate indexes for unique constraints
    let indexes: Vec<IndexDefinition> = shape
        .properties
        .iter()
        .filter(|p| p.is_unique())
        .enumerate()
        .map(|(idx, prop)| {
            let column_name = prop.get_column_name();
            IndexDefinition {
                name: format!("UQ_{}_{}", table_name, idx),
                table: table_name.clone(),
                columns: vec![column_name],
                unique: true,
            }
        })
        .collect();

    TableDefinition {
        name: table_name,
        columns,
        primary_key,
        foreign_keys,
        indexes,
        comment: shape.label.clone(),
    }
}

/// Convert a SHACL PropertyShape to a SQL ColumnDefinition
///
/// # Arguments
///
/// * `property` - The SHACL PropertyShape to convert
/// * `dialect` - The SQL dialect for type mapping
///
/// # Returns
///
/// A `ColumnDefinition` for this property
pub fn convert_property_to_column(
    property: &PropertyShape,
    dialect: &dyn SqlDialect,
) -> ColumnDefinition {
    let column_name = property.get_column_name();

    // Map XSD datatype to SQL type
    let sql_type = if let Some(datatype) = &property.datatype {
        // For string types without max_length, use TEXT/CLOB instead of VARCHAR(255)
        if datatype == "http://www.w3.org/2001/XMLSchema#string" && property.max_length.is_none() {
            // Use dialect-appropriate unbounded text type
            match dialect.name() {
                "PostgreSQL" => "TEXT".to_string(),
                "DB2" => "CLOB".to_string(),
                "Oracle" => "CLOB".to_string(),
                _ => "TEXT".to_string(),
            }
        } else {
            dialect.map_datatype(datatype, property.max_length)
        }
    } else {
        // Default to VARCHAR if no datatype specified
        // If no max_length, default to 255 for safety
        dialect.map_datatype(
            "http://www.w3.org/2001/XMLSchema#string",
            property.max_length.or(Some(255)),
        )
    };

    // Determine nullable constraint
    let nullable = !property.is_required();

    // Build comprehensive CHECK constraint from multiple SHACL constraints
    let check_constraint = build_check_constraint(property, &column_name, dialect);

    ColumnDefinition {
        name: column_name,
        sql_type,
        nullable,
        default_value: property.default_value.clone(),
        primary_key: false, // Handled separately in TableDefinition
        unique: property.is_unique(),
        check_constraint,
        comment: property.description.clone(),
    }
}

/// Build comprehensive CHECK constraint from multiple SHACL constraints
///
/// Combines multiple constraint types into a single CHECK clause:
/// - sh:in → CHECK (column IN ('val1', 'val2'))
/// - sh:minInclusive/maxInclusive → CHECK (column >= min AND column <= max)
/// - sh:pattern → CHECK (dialect-specific regex)
/// - sh:hasValue → CHECK (column = 'value')
///
/// # Arguments
///
/// * `property` - The SHACL PropertyShape with constraints
/// * `column_name` - The SQL column name
/// * `dialect` - The SQL dialect for pattern syntax
///
/// # Returns
///
/// Optional CHECK constraint combining all applicable constraints with AND
fn build_check_constraint(
    property: &PropertyShape,
    column_name: &str,
    dialect: &dyn SqlDialect,
) -> Option<String> {
    let mut constraints = Vec::new();

    // 1. Enumeration constraint (sh:in)
    if let Some(ref in_values) = property.in_values {
        if !in_values.is_empty() {
            let values: Vec<String> = in_values
                .iter()
                .map(|v| format!("'{}'", v.replace("'", "''")))
                .collect();
            constraints.push(format!("{} IN ({})", column_name, values.join(", ")));
        }
    }

    // 2. Fixed value constraint (sh:hasValue)
    if let Some(ref has_value) = property.has_value {
        constraints.push(format!(
            "{} = '{}'",
            column_name,
            has_value.replace("'", "''")
        ));
    }

    // 3. Numeric range constraints (sh:minInclusive, sh:maxInclusive)
    let mut range_parts = Vec::new();

    if let Some(min) = property.min_inclusive {
        range_parts.push(format!("{} >= {}", column_name, min));
    }

    if let Some(max) = property.max_inclusive {
        range_parts.push(format!("{} <= {}", column_name, max));
    }

    // 4. Exclusive numeric range constraints (sh:minExclusive, sh:maxExclusive)
    if let Some(min) = property.min_exclusive {
        range_parts.push(format!("{} > {}", column_name, min));
    }

    if let Some(max) = property.max_exclusive {
        range_parts.push(format!("{} < {}", column_name, max));
    }

    if !range_parts.is_empty() {
        constraints.push(range_parts.join(" AND "));
    }

    // 5. String length constraints (sh:minLength)
    if let Some(min_len) = property.min_length {
        constraints.push(format!("LENGTH({}) >= {}", column_name, min_len));
    }

    // 6. Pattern constraint (sh:pattern) - uses dialect-specific syntax
    if let Some(ref pattern) = property.pattern {
        constraints.push(dialect.pattern_constraint(column_name, pattern));
    }

    // Combine all constraints with AND
    if constraints.is_empty() {
        None
    } else if constraints.len() == 1 {
        Some(constraints[0].clone())
    } else {
        Some(format!("({})", constraints.join(" AND ")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_convert_simple_shape_to_table() {
        let mut shape = NodeShape::new(
            "http://example.com/shape/Customer".to_string(),
            "http://example.com/Customer".to_string(),
        );

        let mut id_prop = PropertyShape::new("http://schema.org/identifier".to_string());
        id_prop.name = Some("id".to_string());
        id_prop.datatype = Some("http://www.w3.org/2001/XMLSchema#integer".to_string());
        id_prop.min_count = Some(1);
        id_prop.max_count = Some(1);
        shape.add_property(id_prop);

        let mut email_prop = PropertyShape::new("http://schema.org/email".to_string());
        email_prop.name = Some("email".to_string());
        email_prop.datatype = Some("http://www.w3.org/2001/XMLSchema#string".to_string());
        email_prop.min_count = Some(1);
        email_prop.max_length = Some(255);
        shape.add_property(email_prop);

        let dialect = dialects::postgresql::PostgreSqlDialect;
        let table = convert_shape_to_table(&shape, &dialect);

        assert_eq!(table.name, "CUSTOMER");
        assert_eq!(table.columns.len(), 2);
        assert_eq!(table.columns[0].name, "id");
        assert_eq!(table.columns[0].sql_type, "INTEGER");
        assert!(!table.columns[0].nullable);
        assert_eq!(table.columns[1].name, "email");
        assert_eq!(table.columns[1].sql_type, "VARCHAR(255)");
    }

    #[test]
    fn test_convert_property_with_pattern() {
        let mut prop = PropertyShape::new("http://schema.org/email".to_string());
        prop.name = Some("email".to_string());
        prop.datatype = Some("http://www.w3.org/2001/XMLSchema#string".to_string());
        prop.pattern = Some(r"^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}$".to_string());

        let dialect = dialects::postgresql::PostgreSqlDialect;
        let column = convert_property_to_column(&prop, &dialect);

        assert!(column.check_constraint.is_some());
        assert!(column.check_constraint.unwrap().contains("email ~"));
    }

    #[test]
    fn test_convert_shape_with_foreign_key() {
        let mut shape = NodeShape::new(
            "http://example.com/shape/Order".to_string(),
            "http://example.com/Order".to_string(),
        );

        let mut customer_prop = PropertyShape::new("http://schema.org/customer".to_string());
        customer_prop.name = Some("customer_id".to_string());
        customer_prop.class = Some("http://example.com/Customer".to_string());
        customer_prop.datatype = Some("http://www.w3.org/2001/XMLSchema#integer".to_string());
        shape.add_property(customer_prop);

        let dialect = dialects::postgresql::PostgreSqlDialect;
        let table = convert_shape_to_table(&shape, &dialect);

        assert_eq!(table.foreign_keys.len(), 1);
        assert_eq!(table.foreign_keys[0].ref_table, "CUSTOMER");
        assert_eq!(table.foreign_keys[0].columns[0], "customer_id");
    }

    #[test]
    fn test_multiple_dialects() {
        let mut prop = PropertyShape::new("http://schema.org/name".to_string());
        prop.datatype = Some("http://www.w3.org/2001/XMLSchema#string".to_string());
        prop.max_length = Some(100);

        let pg_dialect = dialects::postgresql::PostgreSqlDialect;
        let pg_col = convert_property_to_column(&prop, &pg_dialect);
        assert_eq!(pg_col.sql_type, "VARCHAR(100)");

        let db2_dialect = dialects::db2::Db2Dialect;
        let db2_col = convert_property_to_column(&prop, &db2_dialect);
        assert_eq!(db2_col.sql_type, "VARCHAR(100)");

        let oracle_dialect = dialects::oracle::OracleDialect;
        let oracle_col = convert_property_to_column(&prop, &oracle_dialect);
        assert_eq!(oracle_col.sql_type, "VARCHAR2(100)");
    }

    #[test]
    fn test_unbounded_string_uses_text_clob() {
        // Unbounded string should use TEXT/CLOB, not VARCHAR(255)
        let mut prop = PropertyShape::new("http://schema.org/description".to_string());
        prop.datatype = Some("http://www.w3.org/2001/XMLSchema#string".to_string());
        // No max_length specified

        let pg_dialect = dialects::postgresql::PostgreSqlDialect;
        let pg_col = convert_property_to_column(&prop, &pg_dialect);
        assert_eq!(pg_col.sql_type, "TEXT");

        let db2_dialect = dialects::db2::Db2Dialect;
        let db2_col = convert_property_to_column(&prop, &db2_dialect);
        assert_eq!(db2_col.sql_type, "CLOB");

        let oracle_dialect = dialects::oracle::OracleDialect;
        let oracle_col = convert_property_to_column(&prop, &oracle_dialect);
        assert_eq!(oracle_col.sql_type, "CLOB");
    }

    #[test]
    fn test_enumeration_constraint() {
        let mut prop = PropertyShape::new("http://schema.org/status".to_string());
        prop.name = Some("status".to_string());
        prop.datatype = Some("http://www.w3.org/2001/XMLSchema#string".to_string());
        prop.max_length = Some(20);
        prop.in_values = Some(vec![
            "active".to_string(),
            "inactive".to_string(),
            "pending".to_string(),
        ]);

        let dialect = dialects::postgresql::PostgreSqlDialect;
        let column = convert_property_to_column(&prop, &dialect);

        assert!(column.check_constraint.is_some());
        let check = column.check_constraint.unwrap();
        assert!(check.contains("status IN"));
        assert!(check.contains("'active'"));
        assert!(check.contains("'inactive'"));
        assert!(check.contains("'pending'"));
    }

    #[test]
    fn test_numeric_range_constraints() {
        let mut prop = PropertyShape::new("http://schema.org/age".to_string());
        prop.name = Some("age".to_string());
        prop.datatype = Some("http://www.w3.org/2001/XMLSchema#integer".to_string());
        prop.min_inclusive = Some(0.0);
        prop.max_inclusive = Some(150.0);

        let dialect = dialects::postgresql::PostgreSqlDialect;
        let column = convert_property_to_column(&prop, &dialect);

        assert!(column.check_constraint.is_some());
        let check = column.check_constraint.unwrap();
        assert!(check.contains("age >= 0"));
        assert!(check.contains("age <= 150"));
    }

    #[test]
    fn test_pattern_constraint_postgresql() {
        let mut prop = PropertyShape::new("http://schema.org/email".to_string());
        prop.name = Some("email".to_string());
        prop.datatype = Some("http://www.w3.org/2001/XMLSchema#string".to_string());
        prop.pattern = Some(r"^[a-z0-9._%+-]+@[a-z0-9.-]+\.[a-z]{2,}$".to_string());

        let dialect = dialects::postgresql::PostgreSqlDialect;
        let column = convert_property_to_column(&prop, &dialect);

        assert!(column.check_constraint.is_some());
        let check = column.check_constraint.unwrap();
        assert!(check.contains("email ~"));
    }

    #[test]
    fn test_pattern_constraint_db2() {
        let mut prop = PropertyShape::new("http://schema.org/email".to_string());
        prop.name = Some("email".to_string());
        prop.datatype = Some("http://www.w3.org/2001/XMLSchema#string".to_string());
        prop.pattern = Some(r"^[a-z0-9._%+-]+@[a-z0-9.-]+\.[a-z]{2,}$".to_string());

        let dialect = dialects::db2::Db2Dialect;
        let column = convert_property_to_column(&prop, &dialect);

        assert!(column.check_constraint.is_some());
        let check = column.check_constraint.unwrap();
        assert!(check.contains("REGEXP_LIKE(email,"));
    }

    #[test]
    fn test_pattern_constraint_oracle() {
        let mut prop = PropertyShape::new("http://schema.org/email".to_string());
        prop.name = Some("email".to_string());
        prop.datatype = Some("http://www.w3.org/2001/XMLSchema#string".to_string());
        prop.pattern = Some(r"^[a-z0-9._%+-]+@[a-z0-9.-]+\.[a-z]{2,}$".to_string());

        let dialect = dialects::oracle::OracleDialect;
        let column = convert_property_to_column(&prop, &dialect);

        assert!(column.check_constraint.is_some());
        let check = column.check_constraint.unwrap();
        assert!(check.contains("REGEXP_LIKE(email,"));
    }

    #[test]
    fn test_combined_constraints() {
        // Multiple constraints should be combined with AND
        let mut prop = PropertyShape::new("http://schema.org/score".to_string());
        prop.name = Some("score".to_string());
        prop.datatype = Some("http://www.w3.org/2001/XMLSchema#integer".to_string());
        prop.min_inclusive = Some(0.0);
        prop.max_inclusive = Some(100.0);
        prop.in_values = Some(vec![
            "0".to_string(),
            "25".to_string(),
            "50".to_string(),
            "75".to_string(),
            "100".to_string(),
        ]);

        let dialect = dialects::postgresql::PostgreSqlDialect;
        let column = convert_property_to_column(&prop, &dialect);

        assert!(column.check_constraint.is_some());
        let check = column.check_constraint.unwrap();
        // Should have both IN constraint and range constraint combined
        assert!(check.contains("IN"));
        assert!(check.contains("AND"));
        assert!(check.contains("score >= 0"));
        assert!(check.contains("score <= 100"));
    }

    #[test]
    fn test_has_value_constraint() {
        let mut prop = PropertyShape::new("http://schema.org/type".to_string());
        prop.name = Some("entity_type".to_string());
        prop.datatype = Some("http://www.w3.org/2001/XMLSchema#string".to_string());
        prop.has_value = Some("Customer".to_string());

        let dialect = dialects::postgresql::PostgreSqlDialect;
        let column = convert_property_to_column(&prop, &dialect);

        assert!(column.check_constraint.is_some());
        let check = column.check_constraint.unwrap();
        assert!(check.contains("entity_type = 'Customer'"));
    }

    #[test]
    fn test_min_length_constraint() {
        let mut prop = PropertyShape::new("http://schema.org/password".to_string());
        prop.name = Some("password".to_string());
        prop.datatype = Some("http://www.w3.org/2001/XMLSchema#string".to_string());
        prop.min_length = Some(8);

        let dialect = dialects::postgresql::PostgreSqlDialect;
        let column = convert_property_to_column(&prop, &dialect);

        assert!(column.check_constraint.is_some());
        let check = column.check_constraint.unwrap();
        assert!(check.contains("LENGTH(password) >= 8"));
    }

    #[test]
    fn test_exclusive_range_constraints() {
        let mut prop = PropertyShape::new("http://schema.org/temperature".to_string());
        prop.name = Some("temp".to_string());
        prop.datatype = Some("http://www.w3.org/2001/XMLSchema#decimal".to_string());
        prop.min_exclusive = Some(-273.15); // Absolute zero
        prop.max_exclusive = Some(1000.0);

        let dialect = dialects::postgresql::PostgreSqlDialect;
        let column = convert_property_to_column(&prop, &dialect);

        assert!(column.check_constraint.is_some());
        let check = column.check_constraint.unwrap();
        assert!(check.contains("temp > -273.15"));
        assert!(check.contains("temp < 1000"));
    }
}
