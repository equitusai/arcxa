//! Schema Diff Engine
//!
//! Compare two SHACL schemas and generate a diff.

use crate::mapping::ddl::dialects::{ColumnDefinition, TableDefinition};
use crate::mapping::ddl::shacl::NodeShape;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// Schema difference
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaDiff {
    /// Tables to create
    pub tables_to_create: Vec<TableDefinition>,

    /// Tables to drop
    pub tables_to_drop: Vec<String>,

    /// Columns to add (table_name, column)
    pub columns_to_add: Vec<(String, ColumnDefinition)>,

    /// Columns to drop (table_name, column_name)
    pub columns_to_drop: Vec<(String, String)>,

    /// Columns to modify (table_name, new column definition)
    pub columns_to_modify: Vec<(String, ColumnDefinition)>,
}

impl SchemaDiff {
    /// Check if the diff contains any changes
    pub fn has_changes(&self) -> bool {
        !self.tables_to_create.is_empty()
            || !self.tables_to_drop.is_empty()
            || !self.columns_to_add.is_empty()
            || !self.columns_to_drop.is_empty()
            || !self.columns_to_modify.is_empty()
    }

    /// Check if the diff contains only safe operations
    pub fn is_safe(&self) -> bool {
        // Safe operations: CREATE TABLE, ADD COLUMN (nullable), CREATE INDEX
        // Unsafe: DROP TABLE, DROP COLUMN, MODIFY COLUMN (type change)
        self.tables_to_drop.is_empty()
            && self.columns_to_drop.is_empty()
            && self.columns_to_modify.is_empty()
    }

    /// Get warnings for unsafe operations
    pub fn get_warnings(&self) -> Vec<String> {
        let mut warnings = Vec::new();

        for table in &self.tables_to_drop {
            warnings.push(format!("DROP TABLE {} (data loss risk)", table));
        }

        for (table, column) in &self.columns_to_drop {
            warnings.push(format!("DROP COLUMN {}.{} (data loss risk)", table, column));
        }

        for (table, column) in &self.columns_to_modify {
            warnings.push(format!(
                "MODIFY COLUMN {}.{} (potential incompatibility)",
                table, column.name
            ));
        }

        warnings
    }
}

/// Schema diff engine
pub struct SchemaDiffEngine;

impl SchemaDiffEngine {
    /// Create a new diff engine
    pub fn new() -> Self {
        Self
    }

    /// Compute schema difference between current and desired schemas
    ///
    /// # Arguments
    ///
    /// * `current_tables` - Current table definitions (empty if new schema)
    /// * `desired_tables` - Desired table definitions from SHACL
    ///
    /// # Returns
    ///
    /// A `SchemaDiff` describing the changes needed
    pub fn compute_diff(
        &self,
        current_tables: &[TableDefinition],
        desired_tables: &[TableDefinition],
    ) -> SchemaDiff {
        let mut diff = SchemaDiff {
            tables_to_create: Vec::new(),
            tables_to_drop: Vec::new(),
            columns_to_add: Vec::new(),
            columns_to_drop: Vec::new(),
            columns_to_modify: Vec::new(),
        };

        // Build lookup maps
        let current_map: HashMap<_, _> =
            current_tables.iter().map(|t| (t.name.clone(), t)).collect();

        let desired_map: HashMap<_, _> =
            desired_tables.iter().map(|t| (t.name.clone(), t)).collect();

        let current_names: HashSet<_> = current_map.keys().cloned().collect();
        let desired_names: HashSet<_> = desired_map.keys().cloned().collect();

        // Find tables to create
        for table_name in desired_names.difference(&current_names) {
            if let Some(table) = desired_map.get(table_name) {
                diff.tables_to_create.push((*table).clone());
            }
        }

        // Find tables to drop
        for table_name in current_names.difference(&desired_names) {
            diff.tables_to_drop.push(table_name.clone());
        }

        // Compare columns in existing tables
        for table_name in current_names.intersection(&desired_names) {
            let current_table = current_map.get(table_name).unwrap();
            let desired_table = desired_map.get(table_name).unwrap();

            let current_cols: HashMap<_, _> = current_table
                .columns
                .iter()
                .map(|c| (c.name.clone(), c))
                .collect();

            let desired_cols: HashMap<_, _> = desired_table
                .columns
                .iter()
                .map(|c| (c.name.clone(), c))
                .collect();

            let current_col_names: HashSet<_> = current_cols.keys().cloned().collect();
            let desired_col_names: HashSet<_> = desired_cols.keys().cloned().collect();

            // Columns to add
            for col_name in desired_col_names.difference(&current_col_names) {
                if let Some(col) = desired_cols.get(col_name) {
                    diff.columns_to_add
                        .push((table_name.clone(), (*col).clone()));
                }
            }

            // Columns to drop
            for col_name in current_col_names.difference(&desired_col_names) {
                diff.columns_to_drop
                    .push((table_name.clone(), col_name.clone()));
            }

            // Columns to modify (type or constraints changed)
            for col_name in current_col_names.intersection(&desired_col_names) {
                let current_col = current_cols.get(col_name).unwrap();
                let desired_col = desired_cols.get(col_name).unwrap();

                if Self::column_changed(current_col, desired_col) {
                    diff.columns_to_modify
                        .push((table_name.clone(), (*desired_col).clone()));
                }
            }
        }

        diff
    }

    /// Compute diff from SHACL shapes
    ///
    /// # Arguments
    ///
    /// * `current_shapes` - Current SHACL shapes (empty if new)
    /// * `desired_shapes` - Desired SHACL shapes
    /// * `convert_fn` - Function to convert NodeShape to TableDefinition
    ///
    /// # Returns
    ///
    /// A `SchemaDiff` describing the changes needed
    pub fn compute_diff_from_shapes<F>(
        &self,
        current_shapes: &[NodeShape],
        desired_shapes: &[NodeShape],
        convert_fn: F,
    ) -> SchemaDiff
    where
        F: Fn(&NodeShape) -> TableDefinition,
    {
        let current_tables: Vec<_> = current_shapes.iter().map(&convert_fn).collect();
        let desired_tables: Vec<_> = desired_shapes.iter().map(&convert_fn).collect();

        self.compute_diff(&current_tables, &desired_tables)
    }

    /// Check if a column has changed
    fn column_changed(current: &ColumnDefinition, desired: &ColumnDefinition) -> bool {
        current.sql_type != desired.sql_type
            || current.nullable != desired.nullable
            || current.default_value != desired.default_value
            || current.check_constraint != desired.check_constraint
    }
}

impl Default for SchemaDiffEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_table(name: &str, columns: Vec<(&str, &str, bool)>) -> TableDefinition {
        TableDefinition {
            name: name.to_string(),
            columns: columns
                .into_iter()
                .map(|(col_name, sql_type, nullable)| ColumnDefinition {
                    name: col_name.to_string(),
                    sql_type: sql_type.to_string(),
                    nullable,
                    default_value: None,
                    primary_key: false,
                    unique: false,
                    check_constraint: None,
                    comment: None,
                })
                .collect(),
            primary_key: vec![],
            foreign_keys: vec![],
            indexes: vec![],
            comment: None,
        }
    }

    #[test]
    fn test_empty_diff() {
        let engine = SchemaDiffEngine::new();
        let current = vec![create_test_table(
            "customers",
            vec![("id", "INTEGER", false), ("name", "VARCHAR(100)", false)],
        )];
        let desired = current.clone();

        let diff = engine.compute_diff(&current, &desired);

        assert!(!diff.has_changes());
        assert!(diff.is_safe());
    }

    #[test]
    fn test_create_table() {
        let engine = SchemaDiffEngine::new();
        let current = vec![];
        let desired = vec![create_test_table(
            "customers",
            vec![("id", "INTEGER", false), ("name", "VARCHAR(100)", false)],
        )];

        let diff = engine.compute_diff(&current, &desired);

        assert!(diff.has_changes());
        assert!(diff.is_safe());
        assert_eq!(diff.tables_to_create.len(), 1);
        assert_eq!(diff.tables_to_create[0].name, "customers");
    }

    #[test]
    fn test_drop_table() {
        let engine = SchemaDiffEngine::new();
        let current = vec![create_test_table(
            "old_table",
            vec![("id", "INTEGER", false)],
        )];
        let desired = vec![];

        let diff = engine.compute_diff(&current, &desired);

        assert!(diff.has_changes());
        assert!(!diff.is_safe()); // DROP TABLE is unsafe
        assert_eq!(diff.tables_to_drop.len(), 1);
        assert_eq!(diff.tables_to_drop[0], "old_table");

        let warnings = diff.get_warnings();
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("DROP TABLE"));
    }

    #[test]
    fn test_add_column() {
        let engine = SchemaDiffEngine::new();
        let current = vec![create_test_table(
            "customers",
            vec![("id", "INTEGER", false)],
        )];
        let desired = vec![create_test_table(
            "customers",
            vec![("id", "INTEGER", false), ("email", "VARCHAR(255)", true)],
        )];

        let diff = engine.compute_diff(&current, &desired);

        assert!(diff.has_changes());
        assert!(diff.is_safe()); // ADD COLUMN is safe if nullable
        assert_eq!(diff.columns_to_add.len(), 1);
        assert_eq!(diff.columns_to_add[0].0, "customers");
        assert_eq!(diff.columns_to_add[0].1.name, "email");
    }

    #[test]
    fn test_drop_column() {
        let engine = SchemaDiffEngine::new();
        let current = vec![create_test_table(
            "customers",
            vec![
                ("id", "INTEGER", false),
                ("old_field", "VARCHAR(100)", true),
            ],
        )];
        let desired = vec![create_test_table(
            "customers",
            vec![("id", "INTEGER", false)],
        )];

        let diff = engine.compute_diff(&current, &desired);

        assert!(diff.has_changes());
        assert!(!diff.is_safe()); // DROP COLUMN is unsafe
        assert_eq!(diff.columns_to_drop.len(), 1);
        assert_eq!(diff.columns_to_drop[0].0, "customers");
        assert_eq!(diff.columns_to_drop[0].1, "old_field");

        let warnings = diff.get_warnings();
        assert!(warnings.iter().any(|w| w.contains("DROP COLUMN")));
    }

    #[test]
    fn test_modify_column() {
        let engine = SchemaDiffEngine::new();
        let current = vec![create_test_table(
            "customers",
            vec![("email", "VARCHAR(100)", true)],
        )];
        let desired = vec![create_test_table(
            "customers",
            vec![("email", "VARCHAR(255)", false)],
        )];

        let diff = engine.compute_diff(&current, &desired);

        assert!(diff.has_changes());
        assert!(!diff.is_safe()); // MODIFY COLUMN is unsafe
        assert_eq!(diff.columns_to_modify.len(), 1);
        assert_eq!(diff.columns_to_modify[0].0, "customers");
        assert_eq!(diff.columns_to_modify[0].1.sql_type, "VARCHAR(255)");
        assert!(!diff.columns_to_modify[0].1.nullable);
    }
}
