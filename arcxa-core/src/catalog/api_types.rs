//! Data Source Catalog API Types
//!
//! Request and response types for the data source catalog REST/gRPC APIs.

use super::types::{ConnectionDetails, DataSource};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use utoipa::ToSchema;

/// Request to create a new data source
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateDataSourceRequest {
    /// Human-readable title
    pub title: String,

    /// Optional description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Source type (PostgreSQL, Oracle, etc.)
    #[serde(rename = "sourceType")]
    pub source_type: String,

    /// Connection configuration (JSON object)
    pub connection: serde_json::Value,

    /// Optional schema reference (URN to schema in graph)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "schemaRef")]
    pub schema_ref: Option<String>,

    /// Tags for categorization
    #[serde(default)]
    pub tags: Vec<String>,

    /// Additional metadata
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

/// Response after creating/retrieving a data source
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DataSourceResponse {
    /// The data source
    #[serde(flatten)]
    pub source: DataSource,

    /// Status of the source (active, error, testing)
    pub status: DataSourceStatus,

    /// Last connection test result
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "lastTestResult")]
    pub last_test_result: Option<ConnectionTestResult>,

    /// Derived datasource capabilities used by UI and workflows
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<DataSourceCapabilities>,
}

/// Capabilities for a concrete datasource instance.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DataSourceCapabilities {
    #[serde(rename = "canTest")]
    pub can_test: bool,
    #[serde(rename = "canInferSchema")]
    pub can_infer_schema: bool,
    #[serde(rename = "canQuery")]
    pub can_query: bool,
    #[serde(rename = "canReadWorkflow")]
    pub can_read_workflow: bool,
    #[serde(rename = "canWriteWorkflow")]
    pub can_write_workflow: bool,
    #[serde(rename = "supportsParameters")]
    pub supports_parameters: bool,
    #[serde(rename = "supportsTls")]
    pub supports_tls: bool,
    #[serde(rename = "supportsIncremental")]
    pub supports_incremental: bool,
    #[serde(rename = "supportsCancellation")]
    pub supports_cancellation: bool,
}

/// Data source operational status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum DataSourceStatus {
    /// Active and ready for use
    Active,

    /// Connection test pending
    Testing,

    /// Connection test failed
    Error,

    /// Disabled by user
    Disabled,
}

/// Request to update an existing data source
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UpdateDataSourceRequest {
    /// Updated title
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,

    /// Updated description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Updated connection configuration
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connection: Option<serde_json::Value>,

    /// Updated source type
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "sourceType")]
    pub source_type: Option<String>,

    /// Updated schema reference
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "schemaRef")]
    pub schema_ref: Option<String>,

    /// Updated tags
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,

    /// Updated metadata
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, String>>,
}

/// Typed patch used internally to update datasource records atomically.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateDataSourcePatch {
    pub title: Option<String>,
    pub description: Option<String>,
    pub source_type: Option<String>,
    pub connection: Option<ConnectionDetails>,
    pub schema_ref: Option<String>,
    pub tags: Option<Vec<String>>,
    pub metadata: Option<HashMap<String, String>>,
}

impl UpdateDataSourcePatch {
    pub fn is_empty(&self) -> bool {
        self.title.is_none()
            && self.description.is_none()
            && self.source_type.is_none()
            && self.connection.is_none()
            && self.schema_ref.is_none()
            && self.tags.is_none()
            && self.metadata.is_none()
    }
}

/// Request to list data sources with filters
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ListDataSourcesRequest {
    /// Filter by source type
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "sourceType")]
    pub source_type: Option<String>,

    /// Filter by tags (any match)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,

    /// Filter by status
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<DataSourceStatus>,

    /// Pagination: page number (0-indexed)
    #[serde(default)]
    pub page: usize,

    /// Pagination: page size
    #[serde(default = "default_page_size")]
    #[serde(rename = "pageSize")]
    pub page_size: usize,
}

impl Default for ListDataSourcesRequest {
    fn default() -> Self {
        Self {
            source_type: None,
            tags: None,
            status: None,
            page: 0,
            page_size: default_page_size(),
        }
    }
}

fn default_page_size() -> usize {
    50
}

/// Response for list data sources request
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ListDataSourcesResponse {
    /// Data sources matching the filters
    pub sources: Vec<DataSourceResponse>,

    /// Total count of matching sources
    pub total: usize,

    /// Current page
    pub page: usize,

    /// Page size
    #[serde(rename = "pageSize")]
    pub page_size: usize,
}

/// Request to test a data source connection
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TestConnectionRequest {
    /// Data source ID (for existing source)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "sourceId")]
    pub source_id: Option<String>,

    /// Inline connection config (for testing before creation)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connection: Option<serde_json::Value>,

    /// Source type (required if connection provided)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "sourceType")]
    pub source_type: Option<String>,
}

/// Result of a connection test
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ConnectionTestResult {
    /// Whether the connection succeeded
    pub success: bool,

    /// Duration in milliseconds
    #[serde(rename = "durationMs")]
    pub duration_ms: u64,

    /// Error message if failed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,

    /// Additional metadata from the test
    #[serde(default)]
    pub metadata: HashMap<String, String>,

    /// Timestamp of the test
    #[serde(rename = "testedAt")]
    pub tested_at: DateTime<Utc>,
}

/// Request to infer schema from a data source
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct InferSchemaRequest {
    /// Data source ID
    #[serde(rename = "sourceId")]
    pub source_id: String,

    /// Optional table/collection name to infer
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "tableName")]
    pub table_name: Option<String>,

    /// Sample size for inference
    #[serde(default = "default_sample_size")]
    #[serde(rename = "sampleSize")]
    pub sample_size: usize,
}

fn default_sample_size() -> usize {
    1000
}

/// Inferred schema from a data source
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SchemaDefinition {
    /// Schema name (database, collection, etc.)
    pub name: String,

    /// Tables/collections in the schema
    pub tables: Vec<TableDefinition>,

    /// Inter-table relationships (foreign keys, one-to-many, etc.)
    #[serde(default)]
    pub relationships: Vec<TableRelationshipDefinition>,

    /// Table indexes discovered during schema introspection
    #[serde(default)]
    pub indexes: Vec<TableIndexDefinition>,

    /// When the schema was inferred
    #[serde(rename = "inferredAt")]
    pub inferred_at: DateTime<Utc>,
}

/// Table relationship definition (typically a foreign key)
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TableRelationshipDefinition {
    /// Optional relationship/constraint name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    /// Source table name
    #[serde(rename = "sourceTable")]
    pub source_table: String,

    /// Source column names (supports composite keys)
    #[serde(rename = "sourceColumns")]
    pub source_columns: Vec<String>,

    /// Target/reference table name
    #[serde(rename = "targetTable")]
    pub target_table: String,

    /// Target/reference column names (supports composite keys)
    #[serde(rename = "targetColumns")]
    pub target_columns: Vec<String>,

    /// Relationship type
    #[serde(rename = "relationshipType")]
    pub relationship_type: RelationshipType,

    /// ON DELETE action when available
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "onDelete")]
    pub on_delete: Option<String>,

    /// ON UPDATE action when available
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "onUpdate")]
    pub on_update: Option<String>,
}

/// Relationship cardinality/type
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum RelationshipType {
    ForeignKey,
    OneToOne,
    OneToMany,
    ManyToMany,
}

/// Index definition discovered from source metadata
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TableIndexDefinition {
    /// Table that owns this index
    pub table: String,

    /// Index name
    pub name: String,

    /// Indexed columns (in order)
    pub columns: Vec<String>,

    /// Whether the index is unique
    pub unique: bool,

    /// Optional source-specific index type (btree, hash, bitmap, etc.)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "indexType")]
    pub index_type: Option<String>,
}

/// Table/collection definition
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TableDefinition {
    /// Table/collection name
    pub name: String,

    /// Columns/fields
    pub columns: Vec<ColumnDefinition>,

    /// Estimated row count
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "estimatedRows")]
    pub estimated_rows: Option<u64>,
}

/// Column/field definition
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ColumnDefinition {
    /// Column name
    pub name: String,

    /// Data type (database-specific)
    #[serde(rename = "dataType")]
    pub data_type: String,

    /// Whether nullable
    pub nullable: bool,

    /// Whether primary key
    #[serde(default)]
    #[serde(rename = "primaryKey")]
    pub primary_key: bool,

    /// Default value
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "defaultValue")]
    pub default_value: Option<String>,

    /// Inferred semantic type (e.g., Email, PhoneNumber, SSN)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "semanticType")]
    pub semantic_type: Option<crate::inference::types::SemanticType>,

    /// Column statistics (cardinality, distribution, etc.)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub statistics: Option<crate::inference::types::ColumnStatistics>,
}

/// Request to execute a query against a data source
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ExecuteQueryRequest {
    /// Data source ID
    #[serde(rename = "sourceId")]
    pub source_id: String,

    /// Query string (SQL, etc.)
    pub query: String,

    /// Query parameters
    #[serde(default)]
    pub parameters: HashMap<String, serde_json::Value>,

    /// Maximum number of rows to return
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,

    /// Query timeout in seconds
    #[serde(default = "default_query_timeout")]
    pub timeout: u64,
}

fn default_query_timeout() -> u64 {
    30
}

/// Result of query execution
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct QueryResult {
    /// Result rows (array of JSON objects)
    pub rows: Vec<serde_json::Value>,

    /// Number of rows returned
    #[serde(rename = "rowCount")]
    pub row_count: usize,

    /// Execution time in milliseconds
    #[serde(rename = "executionTimeMs")]
    pub execution_time_ms: u64,

    /// Whether results were truncated (due to limit)
    pub truncated: bool,

    /// Column metadata
    #[serde(skip_serializing_if = "Option::is_none")]
    pub columns: Option<Vec<ColumnDefinition>>,
}

/// Error response for catalog operations
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CatalogErrorResponse {
    /// Error code
    pub code: String,

    /// Human-readable error message
    pub message: String,

    /// Detailed error information
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<Vec<String>>,

    /// Request ID for debugging
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "requestId")]
    pub request_id: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_data_source_request_serialization() {
        let request = CreateDataSourceRequest {
            title: "Test PostgreSQL".to_string(),
            description: Some("Test database".to_string()),
            source_type: "PostgreSQL".to_string(),
            connection: serde_json::json!({
                "type": "PostgreSQL",
                "host": "localhost",
                "port": 5432,
                "database": "test"
            }),
            schema_ref: None,
            tags: vec!["test".to_string()],
            metadata: HashMap::new(),
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"sourceType\":\"PostgreSQL\""));
        assert!(json.contains("\"title\":\"Test PostgreSQL\""));

        // Test deserialization
        let deserialized: CreateDataSourceRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.title, "Test PostgreSQL");
        assert_eq!(deserialized.source_type, "PostgreSQL");
    }

    #[test]
    fn test_data_source_status_serialization() {
        let status = DataSourceStatus::Active;
        let json = serde_json::to_string(&status).unwrap();
        assert_eq!(json, "\"active\"");

        let status = DataSourceStatus::Error;
        let json = serde_json::to_string(&status).unwrap();
        assert_eq!(json, "\"error\"");
    }

    #[test]
    fn test_list_request_defaults() {
        let request = ListDataSourcesRequest::default();
        assert_eq!(request.page, 0);
        assert_eq!(request.page_size, 50);
        assert!(request.source_type.is_none());
        assert!(request.tags.is_none());
    }

    #[test]
    fn test_connection_test_result() {
        let result = ConnectionTestResult {
            success: true,
            duration_ms: 125,
            error: None,
            metadata: HashMap::new(),
            tested_at: Utc::now(),
        };

        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("\"success\":true"));
        assert!(json.contains("\"durationMs\":125"));
    }

    #[test]
    fn test_query_result_truncation() {
        let result = QueryResult {
            rows: vec![serde_json::json!({"id": 1})],
            row_count: 1,
            execution_time_ms: 50,
            truncated: true,
            columns: None,
        };

        assert!(result.truncated);
        assert_eq!(result.row_count, 1);
    }

    #[test]
    fn test_schema_definition_structure() {
        let schema = SchemaDefinition {
            name: "public".to_string(),
            tables: vec![TableDefinition {
                name: "users".to_string(),
                columns: vec![
                    ColumnDefinition {
                        name: "id".to_string(),
                        data_type: "integer".to_string(),
                        nullable: false,
                        primary_key: true,
                        default_value: None,
                        semantic_type: None,
                        statistics: None,
                    },
                    ColumnDefinition {
                        name: "email".to_string(),
                        data_type: "varchar".to_string(),
                        nullable: false,
                        primary_key: false,
                        default_value: None,
                        semantic_type: None,
                        statistics: None,
                    },
                ],
                estimated_rows: Some(10000),
            }],
            relationships: vec![],
            indexes: vec![],
            inferred_at: Utc::now(),
        };

        assert_eq!(schema.tables.len(), 1);
        assert_eq!(schema.tables[0].columns.len(), 2);
        assert!(schema.tables[0].columns[0].primary_key);
    }

    #[test]
    fn test_schema_relationships_and_indexes() {
        let schema = SchemaDefinition {
            name: "public".to_string(),
            tables: vec![],
            relationships: vec![TableRelationshipDefinition {
                name: Some("fk_orders_customer_id".to_string()),
                source_table: "orders".to_string(),
                source_columns: vec!["customer_id".to_string()],
                target_table: "customers".to_string(),
                target_columns: vec!["id".to_string()],
                relationship_type: RelationshipType::ForeignKey,
                on_delete: Some("CASCADE".to_string()),
                on_update: Some("NO ACTION".to_string()),
            }],
            indexes: vec![TableIndexDefinition {
                table: "orders".to_string(),
                name: "idx_orders_customer_id".to_string(),
                columns: vec!["customer_id".to_string()],
                unique: false,
                index_type: Some("btree".to_string()),
            }],
            inferred_at: Utc::now(),
        };

        assert_eq!(schema.relationships.len(), 1);
        assert_eq!(schema.indexes.len(), 1);
        assert_eq!(schema.relationships[0].source_table, "orders");
        assert_eq!(schema.indexes[0].table, "orders");
    }

    #[test]
    fn test_catalog_error_response() {
        let error = CatalogErrorResponse {
            code: "INVALID_CONFIG".to_string(),
            message: "Configuration validation failed".to_string(),
            details: Some(vec!["Host cannot be empty".to_string()]),
            request_id: Some("req_123".to_string()),
        };

        let json = serde_json::to_string(&error).unwrap();
        assert!(json.contains("\"code\":\"INVALID_CONFIG\""));
        assert!(json.contains("\"requestId\":\"req_123\""));
    }
}
