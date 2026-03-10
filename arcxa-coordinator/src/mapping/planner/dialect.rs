//! Dialect-aware SQL rendering helpers for goal planner.

use serde::{Deserialize, Serialize};

/// SQL dialect used for rendering the final query text.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SqlDialect {
    PostgreSql,
    Edb,
    Oracle,
    SapHana,
    Db2,
    Databricks,
}

impl Default for SqlDialect {
    fn default() -> Self {
        Self::PostgreSql
    }
}

impl SqlDialect {
    pub fn quote_ident(&self, ident: &str) -> String {
        match self {
            Self::Databricks => format!("`{}`", ident),
            _ => format!("\"{}\"", ident),
        }
    }

    pub fn placeholder(&self, index: usize) -> String {
        match self {
            Self::PostgreSql | Self::Edb => format!("${}", index),
            Self::Oracle => format!(":p{}", index),
            Self::SapHana | Self::Db2 | Self::Databricks => "?".to_string(),
        }
    }

    pub fn render_limit_clause(&self, limit: usize) -> String {
        match self {
            Self::PostgreSql | Self::Edb | Self::SapHana | Self::Databricks => {
                format!("LIMIT {}", limit)
            }
            Self::Oracle | Self::Db2 => format!("FETCH FIRST {} ROWS ONLY", limit),
        }
    }

    /// Render an explain-plan statement for the dialect.
    pub fn render_explain_statement(&self, sql: &str) -> String {
        match self {
            Self::PostgreSql | Self::Edb => format!("EXPLAIN (FORMAT JSON) {}", sql),
            Self::Oracle => format!("EXPLAIN PLAN FOR {}", sql),
            Self::SapHana => format!(
                "EXPLAIN PLAN SET STATEMENT_NAME = 'GRAPHICA_PLAN' FOR {}",
                sql
            ),
            Self::Db2 => format!("EXPLAIN PLAN FOR {}", sql),
            Self::Databricks => format!("EXPLAIN FORMAT=JSON {}", sql),
        }
    }
}
