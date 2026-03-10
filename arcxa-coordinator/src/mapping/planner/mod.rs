//! Goal-driven SQL planner.
//!
//! Builds executable SQL from ontology goal properties + approved physical mappings.

mod dialect;

use anyhow::{anyhow, Context, Result};
use graphica_core::catalog::api_types::{SchemaDefinition, TableRelationshipDefinition};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap, HashSet};

pub use dialect::SqlDialect;

/// High-level goal that needs SQL data retrieval.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoalRequest {
    /// Ontology entity URI (for context/logging).
    pub entity_uri: String,
    /// Ontology property URIs required by this goal.
    pub required_properties: Vec<String>,
    /// Optional equality filters expressed by ontology property URI.
    #[serde(default)]
    pub filters: Vec<GoalFilter>,
    /// Optional LIMIT.
    pub limit: Option<usize>,
}

/// Simple filter expression.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoalFilter {
    pub ontology_uri: String,
    pub value: String,
}

/// Physical field binding selected from ontology mapping candidates.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhysicalFieldBinding {
    pub ontology_uri: String,
    pub table: String,
    pub column: String,
    pub confidence: f64,
}

/// Join information for observability/debugging.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlannedJoin {
    pub from_table: String,
    pub to_table: String,
    pub condition: String,
}

/// Parameter metadata for prepared-statement execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlannedParameter {
    pub index: usize,
    pub placeholder: String,
    pub ontology_uri: String,
    pub value: String,
    pub data_type: Option<String>,
}

/// Planned SQL output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoalSqlPlan {
    pub dialect: SqlDialect,
    pub sql: String,
    pub explain_sql: Option<String>,
    pub selected_tables: Vec<String>,
    pub covered_properties: Vec<String>,
    pub missing_properties: Vec<String>,
    pub joins: Vec<PlannedJoin>,
    pub parameters: Vec<PlannedParameter>,
}

pub struct GoalSqlPlanner {
    schema: SchemaDefinition,
}

impl GoalSqlPlanner {
    pub fn new(schema: SchemaDefinition) -> Self {
        Self { schema }
    }

    pub fn plan_goal(
        &self,
        goal: &GoalRequest,
        bindings: &[PhysicalFieldBinding],
    ) -> Result<GoalSqlPlan> {
        self.plan_goal_with_dialect(goal, bindings, SqlDialect::default())
    }

    pub fn plan_goal_with_dialect(
        &self,
        goal: &GoalRequest,
        bindings: &[PhysicalFieldBinding],
        dialect: SqlDialect,
    ) -> Result<GoalSqlPlan> {
        self.plan_goal_with_options(goal, bindings, dialect, false)
    }

    pub fn plan_goal_with_options(
        &self,
        goal: &GoalRequest,
        bindings: &[PhysicalFieldBinding],
        dialect: SqlDialect,
        include_explain_plan: bool,
    ) -> Result<GoalSqlPlan> {
        let selected = self.select_best_bindings(goal, bindings);
        let missing_properties: Vec<String> = goal
            .required_properties
            .iter()
            .filter(|uri| !selected.contains_key(*uri))
            .cloned()
            .collect();

        if !missing_properties.is_empty() {
            return Err(anyhow!(
                "Missing physical bindings for required ontology properties: {:?}",
                missing_properties
            ));
        }

        let selected_bindings: Vec<&PhysicalFieldBinding> = goal
            .required_properties
            .iter()
            .filter_map(|uri| selected.get(uri))
            .copied()
            .collect();

        let mut table_usage: HashMap<String, usize> = HashMap::new();
        for binding in &selected_bindings {
            *table_usage.entry(binding.table.clone()).or_default() += 1;
        }

        let anchor_table = table_usage
            .into_iter()
            .max_by_key(|(_, count)| *count)
            .map(|(table, _)| table)
            .ok_or_else(|| anyhow!("No selected table bindings were found"))?;

        let mut alias_by_table = HashMap::new();
        let mut ordered_tables = vec![anchor_table.clone()];
        alias_by_table.insert(anchor_table.clone(), "t0".to_string());

        let mut join_clauses = Vec::new();
        let mut planned_joins = Vec::new();
        let mut joined_tables: HashSet<String> = HashSet::from([anchor_table.clone()]);

        let target_tables: BTreeSet<String> =
            selected_bindings.iter().map(|b| b.table.clone()).collect();

        for target_table in target_tables {
            if joined_tables.contains(&target_table) {
                continue;
            }

            let path = self
                .find_path(&anchor_table, &target_table)
                .with_context(|| {
                    format!(
                        "Unable to build join path from '{}' to '{}'",
                        anchor_table, target_table
                    )
                })?;

            for step in path {
                if joined_tables.contains(&step.to) {
                    continue;
                }

                let from_alias = alias_by_table
                    .get(&step.from)
                    .cloned()
                    .ok_or_else(|| anyhow!("Missing alias for table '{}'", step.from))?;

                let next_alias = format!("t{}", alias_by_table.len());
                alias_by_table.insert(step.to.clone(), next_alias.clone());
                ordered_tables.push(step.to.clone());

                let condition = step.join_condition(&from_alias, &next_alias, dialect);
                join_clauses.push(format!(
                    "JOIN {} {} ON {}",
                    dialect.quote_ident(&step.to),
                    next_alias,
                    condition
                ));
                planned_joins.push(PlannedJoin {
                    from_table: step.from.clone(),
                    to_table: step.to.clone(),
                    condition,
                });

                joined_tables.insert(step.to);
            }
        }

        let select_clauses: Vec<String> = goal
            .required_properties
            .iter()
            .filter_map(|uri| selected.get(uri).copied())
            .map(|binding| {
                let alias = alias_by_table
                    .get(&binding.table)
                    .cloned()
                    .unwrap_or_else(|| "t0".to_string());
                let select_alias = Self::property_alias(&binding.ontology_uri);
                format!(
                    "{}.{} AS {}",
                    alias,
                    dialect.quote_ident(&binding.column),
                    dialect.quote_ident(&select_alias)
                )
            })
            .collect();

        let mut where_clauses = Vec::new();
        let mut parameters = Vec::new();
        for filter in &goal.filters {
            if let Some(binding) = selected.get(&filter.ontology_uri) {
                if let Some(alias) = alias_by_table.get(&binding.table) {
                    let idx = parameters.len() + 1;
                    let placeholder = dialect.placeholder(idx);
                    where_clauses.push(format!(
                        "{}.{} = {}",
                        alias,
                        dialect.quote_ident(&binding.column),
                        placeholder
                    ));
                    parameters.push(PlannedParameter {
                        index: idx,
                        placeholder,
                        ontology_uri: filter.ontology_uri.clone(),
                        value: filter.value.clone(),
                        data_type: self.lookup_column_data_type(&binding.table, &binding.column),
                    });
                }
            }
        }

        let anchor_alias = alias_by_table
            .get(&anchor_table)
            .cloned()
            .unwrap_or_else(|| "t0".to_string());

        let mut sql = format!(
            "SELECT {}\nFROM {} {}",
            select_clauses.join(", "),
            dialect.quote_ident(&anchor_table),
            anchor_alias
        );

        if !join_clauses.is_empty() {
            sql.push('\n');
            sql.push_str(&join_clauses.join("\n"));
        }

        if !where_clauses.is_empty() {
            sql.push('\n');
            sql.push_str("WHERE ");
            sql.push_str(&where_clauses.join(" AND "));
        }

        if let Some(limit) = goal.limit {
            sql.push('\n');
            sql.push_str(&dialect.render_limit_clause(limit));
        }

        let explain_sql = if include_explain_plan {
            Some(dialect.render_explain_statement(&sql))
        } else {
            None
        };

        Ok(GoalSqlPlan {
            dialect,
            sql,
            explain_sql,
            selected_tables: ordered_tables,
            covered_properties: goal.required_properties.clone(),
            missing_properties: vec![],
            joins: planned_joins,
            parameters,
        })
    }

    fn select_best_bindings<'a>(
        &self,
        goal: &GoalRequest,
        bindings: &'a [PhysicalFieldBinding],
    ) -> HashMap<String, &'a PhysicalFieldBinding> {
        let mut selected: HashMap<String, &'a PhysicalFieldBinding> = HashMap::new();
        let required: HashSet<&str> = goal
            .required_properties
            .iter()
            .map(|s| s.as_str())
            .collect();

        for binding in bindings {
            if !required.contains(binding.ontology_uri.as_str()) {
                continue;
            }

            let replace = selected
                .get(&binding.ontology_uri)
                .map(|current| binding.confidence > current.confidence)
                .unwrap_or(true);

            if replace {
                selected.insert(binding.ontology_uri.clone(), binding);
            }
        }

        selected
    }

    fn find_path(&self, from_table: &str, to_table: &str) -> Result<Vec<RelationshipStep>> {
        if from_table == to_table {
            return Ok(vec![]);
        }

        // Dijkstra-style scoring:
        // - Prefer explicit FK/1:1 edges over fan-out edges.
        // - Prefer paths with indexed join columns.
        // - Prefer smaller intermediate tables when row estimates exist.
        let mut distances: HashMap<String, f64> = HashMap::new();
        let mut unvisited: HashSet<String> = self
            .schema
            .tables
            .iter()
            .map(|t| t.name.clone())
            .collect::<HashSet<_>>();
        for rel in &self.schema.relationships {
            unvisited.insert(rel.source_table.clone());
            unvisited.insert(rel.target_table.clone());
        }
        let mut parent: HashMap<String, RelationshipStep> = HashMap::new();

        distances.insert(from_table.to_string(), 0.0);
        unvisited.insert(from_table.to_string());
        unvisited.insert(to_table.to_string());

        while let Some(current) = Self::next_lowest_distance(&unvisited, &distances) {
            if current == to_table {
                return Self::reconstruct_path(from_table, to_table, &parent);
            }
            unvisited.remove(&current);

            let current_cost = distances.get(&current).copied().unwrap_or(f64::INFINITY);
            if !current_cost.is_finite() {
                break;
            }

            for step in self.neighbor_steps(&current) {
                if !unvisited.contains(&step.to) {
                    continue;
                }

                let step_cost = self.step_cost(&step);
                let candidate_cost = current_cost + step_cost;
                let known_cost = distances.get(&step.to).copied().unwrap_or(f64::INFINITY);

                if candidate_cost < known_cost {
                    distances.insert(step.to.clone(), candidate_cost);
                    parent.insert(step.to.clone(), step);
                }
            }
        }

        Err(anyhow!(
            "No relationship path between '{}' and '{}'",
            from_table,
            to_table
        ))
    }

    fn reconstruct_path(
        from_table: &str,
        to_table: &str,
        parent: &HashMap<String, RelationshipStep>,
    ) -> Result<Vec<RelationshipStep>> {
        let mut steps = Vec::new();
        let mut cursor = to_table.to_string();

        while cursor != from_table {
            let step = parent
                .get(&cursor)
                .cloned()
                .ok_or_else(|| anyhow!("Failed to reconstruct join path"))?;
            cursor = step.from.clone();
            steps.push(step);
        }

        steps.reverse();
        Ok(steps)
    }

    fn neighbor_steps(&self, table: &str) -> Vec<RelationshipStep> {
        let mut neighbors = Vec::new();
        for rel in &self.schema.relationships {
            if rel.source_table == table {
                neighbors.push(RelationshipStep::forward(rel));
            } else if rel.target_table == table {
                neighbors.push(RelationshipStep::reverse(rel));
            }
        }
        neighbors
    }

    fn next_lowest_distance(
        unvisited: &HashSet<String>,
        distances: &HashMap<String, f64>,
    ) -> Option<String> {
        unvisited
            .iter()
            .filter_map(|table| {
                distances
                    .get(table)
                    .copied()
                    .map(|cost| (table.clone(), cost))
            })
            .min_by(|(_, lhs), (_, rhs)| lhs.total_cmp(rhs))
            .map(|(table, _)| table)
    }

    fn step_cost(&self, step: &RelationshipStep) -> f64 {
        let relationship_cost = match step.rel.relationship_type {
            graphica_core::catalog::api_types::RelationshipType::OneToOne => 1.0,
            graphica_core::catalog::api_types::RelationshipType::ForeignKey => 1.2,
            graphica_core::catalog::api_types::RelationshipType::OneToMany => 1.8,
            graphica_core::catalog::api_types::RelationshipType::ManyToMany => 3.5,
        };

        let source_size = self.table_row_estimate(&step.rel.source_table);
        let target_size = self.table_row_estimate(&step.rel.target_table);
        let size_penalty = if source_size > 0.0 && target_size > 0.0 {
            let ratio = (source_size.max(target_size)) / (source_size.min(target_size));
            ratio.log10().clamp(0.0, 1.2) * 0.3
        } else {
            0.0
        };

        let source_index_bonus =
            if self.has_index_for_columns(&step.rel.source_table, &step.rel.source_columns) {
                0.25
            } else {
                0.0
            };
        let target_index_bonus =
            if self.has_index_for_columns(&step.rel.target_table, &step.rel.target_columns) {
                0.25
            } else {
                0.0
            };

        (relationship_cost + size_penalty - source_index_bonus - target_index_bonus).max(0.4)
    }

    fn table_row_estimate(&self, table_name: &str) -> f64 {
        self.schema
            .tables
            .iter()
            .find(|table| table.name == table_name)
            .and_then(|table| table.estimated_rows)
            .map(|rows| rows as f64)
            .unwrap_or(0.0)
    }

    fn has_index_for_columns(&self, table_name: &str, columns: &[String]) -> bool {
        if columns.is_empty() {
            return false;
        }
        self.schema.indexes.iter().any(|index| {
            index.table == table_name
                && columns
                    .iter()
                    .all(|column| index.columns.iter().any(|idx_col| idx_col == column))
        })
    }

    fn lookup_column_data_type(&self, table_name: &str, column_name: &str) -> Option<String> {
        self.schema
            .tables
            .iter()
            .find(|table| table.name.eq_ignore_ascii_case(table_name))
            .and_then(|table| {
                table
                    .columns
                    .iter()
                    .find(|column| column.name.eq_ignore_ascii_case(column_name))
            })
            .map(|column| column.data_type.clone())
    }

    fn property_alias(uri: &str) -> String {
        let raw = uri.rsplit(['#', '/']).next().unwrap_or("field").trim();
        let cleaned: String = raw
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        if cleaned.is_empty() {
            "field".to_string()
        } else {
            cleaned
        }
    }
}

#[derive(Debug, Clone)]
struct RelationshipStep {
    from: String,
    to: String,
    rel: TableRelationshipDefinition,
    direction: StepDirection,
}

#[derive(Debug, Clone, Copy)]
enum StepDirection {
    SourceToTarget,
    TargetToSource,
}

impl RelationshipStep {
    fn forward(rel: &TableRelationshipDefinition) -> Self {
        Self {
            from: rel.source_table.clone(),
            to: rel.target_table.clone(),
            rel: rel.clone(),
            direction: StepDirection::SourceToTarget,
        }
    }

    fn reverse(rel: &TableRelationshipDefinition) -> Self {
        Self {
            from: rel.target_table.clone(),
            to: rel.source_table.clone(),
            rel: rel.clone(),
            direction: StepDirection::TargetToSource,
        }
    }

    fn join_condition(&self, from_alias: &str, to_alias: &str, dialect: SqlDialect) -> String {
        let pairs: Vec<(String, String)> = self
            .rel
            .source_columns
            .iter()
            .zip(self.rel.target_columns.iter())
            .map(|(source_col, target_col)| match self.direction {
                StepDirection::SourceToTarget => (
                    format!("{}.{}", from_alias, dialect.quote_ident(source_col)),
                    format!("{}.{}", to_alias, dialect.quote_ident(target_col)),
                ),
                StepDirection::TargetToSource => (
                    format!("{}.{}", to_alias, dialect.quote_ident(source_col)),
                    format!("{}.{}", from_alias, dialect.quote_ident(target_col)),
                ),
            })
            .collect();

        pairs
            .into_iter()
            .map(|(left, right)| format!("{} = {}", left, right))
            .collect::<Vec<_>>()
            .join(" AND ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use graphica_core::catalog::api_types::{
        RelationshipType, TableDefinition, TableIndexDefinition, TableRelationshipDefinition,
    };

    fn test_schema() -> SchemaDefinition {
        SchemaDefinition {
            name: "public".to_string(),
            tables: vec![
                TableDefinition {
                    name: "users".to_string(),
                    columns: vec![],
                    estimated_rows: Some(100),
                },
                TableDefinition {
                    name: "orders".to_string(),
                    columns: vec![],
                    estimated_rows: Some(1000),
                },
            ],
            relationships: vec![TableRelationshipDefinition {
                name: Some("fk_orders_user_id".to_string()),
                source_table: "orders".to_string(),
                source_columns: vec!["user_id".to_string()],
                target_table: "users".to_string(),
                target_columns: vec!["id".to_string()],
                relationship_type: RelationshipType::ForeignKey,
                on_delete: None,
                on_update: None,
            }],
            indexes: vec![TableIndexDefinition {
                table: "orders".to_string(),
                name: "idx_orders_user_id".to_string(),
                columns: vec!["user_id".to_string()],
                unique: false,
                index_type: Some("btree".to_string()),
            }],
            inferred_at: Utc::now(),
        }
    }

    #[test]
    fn plans_join_sql_for_multi_table_goal() {
        let planner = GoalSqlPlanner::new(test_schema());
        let goal = GoalRequest {
            entity_uri: "http://example.org/Order".to_string(),
            required_properties: vec![
                "http://example.org/userEmail".to_string(),
                "http://example.org/orderTotal".to_string(),
            ],
            filters: vec![],
            limit: Some(50),
        };

        let bindings = vec![
            PhysicalFieldBinding {
                ontology_uri: "http://example.org/userEmail".to_string(),
                table: "users".to_string(),
                column: "email".to_string(),
                confidence: 0.92,
            },
            PhysicalFieldBinding {
                ontology_uri: "http://example.org/orderTotal".to_string(),
                table: "orders".to_string(),
                column: "total".to_string(),
                confidence: 0.9,
            },
        ];

        let plan = planner.plan_goal(&goal, &bindings).unwrap();
        assert!(plan.sql.contains("JOIN \"orders\"") || plan.sql.contains("JOIN \"users\""));
        assert!(plan.sql.contains("LIMIT 50"));
        assert_eq!(plan.covered_properties.len(), 2);
    }

    #[test]
    fn plans_oracle_sql_with_fetch_clause() {
        let planner = GoalSqlPlanner::new(test_schema());
        let goal = GoalRequest {
            entity_uri: "http://example.org/Order".to_string(),
            required_properties: vec!["http://example.org/orderTotal".to_string()],
            filters: vec![GoalFilter {
                ontology_uri: "http://example.org/orderTotal".to_string(),
                value: "100".to_string(),
            }],
            limit: Some(10),
        };

        let bindings = vec![PhysicalFieldBinding {
            ontology_uri: "http://example.org/orderTotal".to_string(),
            table: "orders".to_string(),
            column: "total".to_string(),
            confidence: 0.99,
        }];

        let plan = planner
            .plan_goal_with_dialect(&goal, &bindings, SqlDialect::Oracle)
            .unwrap();
        assert!(plan.sql.contains("FETCH FIRST 10 ROWS ONLY"));
        assert!(plan.sql.contains(":p1"));
        assert_eq!(plan.parameters.len(), 1);
        assert_eq!(plan.parameters[0].value, "100");
    }

    #[test]
    fn prefers_fk_join_path_over_many_to_many_when_hops_tie() {
        let schema = SchemaDefinition {
            name: "public".to_string(),
            tables: vec![
                TableDefinition {
                    name: "a".to_string(),
                    columns: vec![],
                    estimated_rows: Some(1_000),
                },
                TableDefinition {
                    name: "b".to_string(),
                    columns: vec![],
                    estimated_rows: Some(10_000),
                },
                TableDefinition {
                    name: "c".to_string(),
                    columns: vec![],
                    estimated_rows: Some(50_000),
                },
                TableDefinition {
                    name: "d".to_string(),
                    columns: vec![],
                    estimated_rows: Some(500_000),
                },
            ],
            relationships: vec![
                TableRelationshipDefinition {
                    name: Some("fk_b_a".to_string()),
                    source_table: "b".to_string(),
                    source_columns: vec!["a_id".to_string()],
                    target_table: "a".to_string(),
                    target_columns: vec!["id".to_string()],
                    relationship_type: RelationshipType::ForeignKey,
                    on_delete: None,
                    on_update: None,
                },
                TableRelationshipDefinition {
                    name: Some("fk_c_b".to_string()),
                    source_table: "c".to_string(),
                    source_columns: vec!["b_id".to_string()],
                    target_table: "b".to_string(),
                    target_columns: vec!["id".to_string()],
                    relationship_type: RelationshipType::ForeignKey,
                    on_delete: None,
                    on_update: None,
                },
                TableRelationshipDefinition {
                    name: Some("m2m_a_d".to_string()),
                    source_table: "a".to_string(),
                    source_columns: vec!["id".to_string()],
                    target_table: "d".to_string(),
                    target_columns: vec!["a_id".to_string()],
                    relationship_type: RelationshipType::ManyToMany,
                    on_delete: None,
                    on_update: None,
                },
                TableRelationshipDefinition {
                    name: Some("m2m_d_c".to_string()),
                    source_table: "d".to_string(),
                    source_columns: vec!["c_id".to_string()],
                    target_table: "c".to_string(),
                    target_columns: vec!["id".to_string()],
                    relationship_type: RelationshipType::ManyToMany,
                    on_delete: None,
                    on_update: None,
                },
            ],
            indexes: vec![
                TableIndexDefinition {
                    table: "b".to_string(),
                    name: "idx_b_a_id".to_string(),
                    columns: vec!["a_id".to_string()],
                    unique: false,
                    index_type: Some("btree".to_string()),
                },
                TableIndexDefinition {
                    table: "c".to_string(),
                    name: "idx_c_b_id".to_string(),
                    columns: vec!["b_id".to_string()],
                    unique: false,
                    index_type: Some("btree".to_string()),
                },
            ],
            inferred_at: Utc::now(),
        };

        let planner = GoalSqlPlanner::new(schema);
        let goal = GoalRequest {
            entity_uri: "http://example.org/C".to_string(),
            required_properties: vec![
                "http://example.org/a_id".to_string(),
                "http://example.org/c_value".to_string(),
            ],
            filters: vec![],
            limit: None,
        };
        let bindings = vec![
            PhysicalFieldBinding {
                ontology_uri: "http://example.org/a_id".to_string(),
                table: "a".to_string(),
                column: "id".to_string(),
                confidence: 0.99,
            },
            PhysicalFieldBinding {
                ontology_uri: "http://example.org/c_value".to_string(),
                table: "c".to_string(),
                column: "id".to_string(),
                confidence: 0.99,
            },
        ];

        let plan = planner.plan_goal(&goal, &bindings).expect("plan");
        assert!(plan.selected_tables.contains(&"b".to_string()));
        assert!(!plan.selected_tables.contains(&"d".to_string()));
    }

    #[test]
    fn includes_explain_sql_when_requested() {
        let planner = GoalSqlPlanner::new(test_schema());
        let goal = GoalRequest {
            entity_uri: "http://example.org/Order".to_string(),
            required_properties: vec!["http://example.org/orderTotal".to_string()],
            filters: vec![],
            limit: Some(5),
        };
        let bindings = vec![PhysicalFieldBinding {
            ontology_uri: "http://example.org/orderTotal".to_string(),
            table: "orders".to_string(),
            column: "total".to_string(),
            confidence: 0.99,
        }];

        let plan = planner
            .plan_goal_with_options(&goal, &bindings, SqlDialect::PostgreSql, true)
            .expect("plan");
        assert!(plan.explain_sql.is_some());
        let explain = plan.explain_sql.unwrap_or_default();
        assert!(explain.starts_with("EXPLAIN"));
        assert!(explain.contains("SELECT"));
    }

    #[test]
    fn fails_when_required_binding_missing() {
        let planner = GoalSqlPlanner::new(test_schema());
        let goal = GoalRequest {
            entity_uri: "http://example.org/Order".to_string(),
            required_properties: vec!["http://example.org/orderTotal".to_string()],
            filters: vec![],
            limit: None,
        };

        let bindings = vec![];
        let result = planner.plan_goal(&goal, &bindings);
        assert!(result.is_err());
    }
}
