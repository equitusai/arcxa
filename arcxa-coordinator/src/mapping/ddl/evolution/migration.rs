//! Migration Generator
//!
//! Generate SQL migration statements from schema diffs.

use super::diff::SchemaDiff;
use crate::mapping::ddl::dialects::SqlDialect;
use anyhow::Result;
use serde::{Deserialize, Serialize};

/// Migration step
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationStep {
    /// SQL statement to execute
    pub sql: String,

    /// Description of what this step does
    pub description: String,

    /// Whether this step is reversible
    pub reversible: bool,

    /// Whether this step is safe (no data loss)
    pub safe: bool,
}

/// Migration plan
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationPlan {
    /// Migration steps in execution order
    pub steps: Vec<MigrationStep>,

    /// Whether the entire migration is safe
    pub safe: bool,

    /// Warnings about unsafe operations
    pub warnings: Vec<String>,
}

impl MigrationPlan {
    /// Generate SQL script from migration plan
    pub fn to_sql(&self) -> String {
        let mut sql = String::new();

        sql.push_str("-- Generated Migration Script\n");
        sql.push_str("-- WARNING: Review carefully before executing\n\n");

        for (idx, step) in self.steps.iter().enumerate() {
            sql.push_str(&format!("-- Step {}: {}\n", idx + 1, step.description));

            if !step.safe {
                sql.push_str("-- ⚠️ UNSAFE: This operation may cause data loss\n");
            }

            if !step.reversible {
                sql.push_str("-- ⚠️ NOT REVERSIBLE: Cannot be automatically rolled back\n");
            }

            sql.push_str(&step.sql);
            sql.push_str(";\n\n");
        }

        sql
    }
}

/// Migration generator
pub struct MigrationGenerator;

impl MigrationGenerator {
    /// Create a new migration generator
    pub fn new() -> Self {
        Self
    }

    /// Generate migration plan from schema diff
    ///
    /// # Arguments
    ///
    /// * `diff` - Schema difference
    /// * `dialect` - SQL dialect for statement generation
    ///
    /// # Returns
    ///
    /// A `MigrationPlan` with all steps in execution order
    pub fn generate_migration(
        &self,
        diff: &SchemaDiff,
        dialect: &dyn SqlDialect,
    ) -> Result<MigrationPlan> {
        let mut steps = Vec::new();

        // Step 1: Create new tables
        for table in &diff.tables_to_create {
            let sql = dialect.create_table(table);
            steps.push(MigrationStep {
                sql,
                description: format!("Create table {}", table.name),
                reversible: true,
                safe: true,
            });

            // Create indexes for the new table
            for index in &table.indexes {
                let sql = dialect.create_index(index);
                steps.push(MigrationStep {
                    sql,
                    description: format!("Create index {} on {}", index.name, index.table),
                    reversible: true,
                    safe: true,
                });
            }

            // Create foreign keys for the new table
            for fk in &table.foreign_keys {
                let sql = dialect.create_foreign_key(&table.name, fk);
                steps.push(MigrationStep {
                    sql,
                    description: format!("Create foreign key {} on {}", fk.name, table.name),
                    reversible: true,
                    safe: true,
                });
            }
        }

        // Step 2: Add columns to existing tables
        for (table_name, column) in &diff.columns_to_add {
            let sql = dialect.alter_table_add_column(table_name, column);
            let safe = column.nullable; // Only safe if nullable
            steps.push(MigrationStep {
                sql,
                description: format!("Add column {}.{}", table_name, column.name),
                reversible: true,
                safe,
            });
        }

        // Step 3: Modify columns (UNSAFE - requires data migration)
        for (table_name, column) in &diff.columns_to_modify {
            let sql = dialect.alter_table_modify_column(table_name, column)?;
            steps.push(MigrationStep {
                sql,
                description: format!("Modify column {}.{}", table_name, column.name),
                reversible: false,
                safe: false,
            });
        }

        // Step 4: Drop columns (UNSAFE - data loss)
        for (table_name, column_name) in &diff.columns_to_drop {
            let sql = dialect.alter_table_drop_column(table_name, column_name);
            steps.push(MigrationStep {
                sql,
                description: format!("Drop column {}.{}", table_name, column_name),
                reversible: false,
                safe: false,
            });
        }

        // Step 5: Drop tables (UNSAFE - data loss)
        for table_name in &diff.tables_to_drop {
            steps.push(MigrationStep {
                sql: format!("DROP TABLE {}", table_name),
                description: format!("Drop table {}", table_name),
                reversible: false,
                safe: false,
            });
        }

        let safe = diff.is_safe();
        let warnings = diff.get_warnings();

        Ok(MigrationPlan {
            steps,
            safe,
            warnings,
        })
    }

    /// Generate idempotent migration (with existence checks)
    ///
    /// # Arguments
    ///
    /// * `diff` - Schema difference
    /// * `dialect` - SQL dialect for statement generation
    ///
    /// # Returns
    ///
    /// A `MigrationPlan` with idempotent statements (IF NOT EXISTS, etc.)
    pub fn generate_idempotent_migration(
        &self,
        diff: &SchemaDiff,
        dialect: &dyn SqlDialect,
    ) -> Result<MigrationPlan> {
        let mut steps = Vec::new();

        // Idempotent create tables
        for table in &diff.tables_to_create {
            // Check if table exists
            let check_sql = dialect.check_table_exists(&table.name);
            let create_sql = dialect.create_table(table);

            let combined_sql = format!(
                "DO $$\nBEGIN\n  IF NOT EXISTS ({}) THEN\n    {};\n  END IF;\nEND $$",
                check_sql, create_sql
            );

            steps.push(MigrationStep {
                sql: combined_sql,
                description: format!("Create table {} (if not exists)", table.name),
                reversible: true,
                safe: true,
            });
        }

        // Idempotent add columns
        for (table_name, column) in &diff.columns_to_add {
            let check_sql = dialect.check_column_exists(table_name, &column.name);
            let add_sql = dialect.alter_table_add_column(table_name, column);

            let combined_sql = format!(
                "DO $$\nBEGIN\n  IF NOT EXISTS ({}) THEN\n    {};\n  END IF;\nEND $$",
                check_sql, add_sql
            );

            let safe = column.nullable;
            steps.push(MigrationStep {
                sql: combined_sql,
                description: format!("Add column {}.{} (if not exists)", table_name, column.name),
                reversible: true,
                safe,
            });
        }

        // Note: MODIFY and DROP operations are intentionally not idempotent
        // They require explicit confirmation

        let safe = diff.is_safe();
        let warnings = diff.get_warnings();

        Ok(MigrationPlan {
            steps,
            safe,
            warnings,
        })
    }
}

impl Default for MigrationGenerator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mapping::ddl::dialects::{ColumnDefinition, TableDefinition};
    use crate::mapping::ddl::evolution::diff::SchemaDiffEngine;

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
    fn test_generate_create_table_migration() {
        use crate::mapping::ddl::dialects::postgresql::PostgreSqlDialect;

        let diff_engine = SchemaDiffEngine::new();
        let migration_gen = MigrationGenerator::new();
        let dialect = PostgreSqlDialect;

        let current = vec![];
        let desired = vec![create_test_table(
            "customers",
            vec![("id", "INTEGER", false), ("name", "VARCHAR(100)", false)],
        )];

        let diff = diff_engine.compute_diff(&current, &desired);
        let plan = migration_gen.generate_migration(&diff, &dialect).unwrap();

        assert!(plan.safe);
        assert_eq!(plan.steps.len(), 1);
        assert!(plan.steps[0].sql.contains("CREATE TABLE customers"));
        assert!(plan.steps[0].safe);
        assert!(plan.steps[0].reversible);
    }

    #[test]
    fn test_generate_add_column_migration() {
        use crate::mapping::ddl::dialects::postgresql::PostgreSqlDialect;

        let diff_engine = SchemaDiffEngine::new();
        let migration_gen = MigrationGenerator::new();
        let dialect = PostgreSqlDialect;

        let current = vec![create_test_table(
            "customers",
            vec![("id", "INTEGER", false)],
        )];
        let desired = vec![create_test_table(
            "customers",
            vec![("id", "INTEGER", false), ("email", "VARCHAR(255)", true)],
        )];

        let diff = diff_engine.compute_diff(&current, &desired);
        let plan = migration_gen.generate_migration(&diff, &dialect).unwrap();

        assert!(plan.safe);
        assert_eq!(plan.steps.len(), 1);
        assert!(plan.steps[0]
            .sql
            .contains("ALTER TABLE customers ADD COLUMN email"));
        assert!(plan.steps[0].safe); // Nullable column is safe
    }

    #[test]
    fn test_unsafe_migration() {
        use crate::mapping::ddl::dialects::postgresql::PostgreSqlDialect;

        let diff_engine = SchemaDiffEngine::new();
        let migration_gen = MigrationGenerator::new();
        let dialect = PostgreSqlDialect;

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

        let diff = diff_engine.compute_diff(&current, &desired);
        let plan = migration_gen.generate_migration(&diff, &dialect).unwrap();

        assert!(!plan.safe);
        assert!(plan.warnings.len() > 0);
        assert_eq!(plan.steps.len(), 1);
        assert!(!plan.steps[0].safe);
        assert!(!plan.steps[0].reversible);
    }

    #[test]
    fn test_to_sql() {
        use crate::mapping::ddl::dialects::postgresql::PostgreSqlDialect;

        let diff_engine = SchemaDiffEngine::new();
        let migration_gen = MigrationGenerator::new();
        let dialect = PostgreSqlDialect;

        let current = vec![];
        let desired = vec![create_test_table(
            "customers",
            vec![("id", "INTEGER", false)],
        )];

        let diff = diff_engine.compute_diff(&current, &desired);
        let plan = migration_gen.generate_migration(&diff, &dialect).unwrap();

        let sql = plan.to_sql();
        assert!(sql.contains("Generated Migration Script"));
        assert!(sql.contains("Step 1:"));
        assert!(sql.contains("CREATE TABLE customers"));
    }
}
