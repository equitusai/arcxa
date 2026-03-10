//! DataSource to Dataset RDF Bridge
//!
//! Converts datasource catalog entries into gph:Dataset RDF triples for governance tracking

use chrono::{DateTime, Utc};
use graphica_core::catalog::types::DataSource;
use std::collections::HashMap;

use crate::governance::ontology::{GRAPHICA_NS, XSD_NS};

/// Convert a DataSource to Dataset RDF triples (Turtle format)
pub fn datasource_to_dataset_triples(source: &DataSource) -> String {
    let dataset_id = format!("ds_{}", sanitize_id(&source.id));
    // Use slash URI style (no fragment) for dataset URIs
    let dataset_uri = format!("http://graphica.io/ontology/dataset/{}", dataset_id);

    let created_at = source.created_at.unwrap_or_else(|| Utc::now());
    let updated_at = source.updated_at.unwrap_or_else(|| Utc::now());

    // Generate Turtle triples
    format!(
        r#"@prefix gph: <{GRAPHICA_NS}> .
@prefix xsd: <{XSD_NS}> .

<{dataset_uri}> a gph:Dataset ;
    gph:datasetName "{name}" ;
    gph:datasetType "source" ;
    gph:recordCount 0 ;
    gph:schemaHash "pending" ;
    gph:sourceDataSource "{source_id}" ;
    gph:createdAt "{created_at}"^^xsd:dateTime ;
    gph:updatedAt "{updated_at}"^^xsd:dateTime .
"#,
        GRAPHICA_NS = GRAPHICA_NS,
        XSD_NS = XSD_NS,
        dataset_uri = dataset_uri,
        name = escape_turtle_string(&source.title),
        source_id = escape_turtle_string(&source.id),
        created_at = created_at.to_rfc3339(),
        updated_at = updated_at.to_rfc3339(),
    )
}

/// Convert schema definition to dataset column triples
pub fn schema_to_column_triples(
    dataset_id: &str,
    schema: &graphica_core::catalog::api_types::SchemaDefinition,
) -> String {
    // Use slash URI style (no fragment) for dataset URIs
    let dataset_uri = format!(
        "http://graphica.io/ontology/dataset/ds_{}",
        sanitize_id(dataset_id)
    );
    let mut triples = String::new();

    triples.push_str(&format!("@prefix gph: <{}> .\n", GRAPHICA_NS));
    triples.push_str(&format!("@prefix xsd: <{}> .\n\n", XSD_NS));

    // For each table in schema
    for table in &schema.tables {
        let table_token = sanitize_id(&table.name);
        for (idx, column) in table.columns.iter().enumerate() {
            let column_uri = format!("{}/column/{}_{}", dataset_uri, table_token, idx);

            triples.push_str(&format!(
                r#"<{column_uri}> a gph:DatasetColumn ;
    gph:columnName "{name}" ;
    gph:columnType "{data_type}" ;
    gph:nullable "{nullable}"^^xsd:boolean .

<{dataset_uri}> gph:hasColumn <{column_uri}> .

"#,
                column_uri = column_uri,
                name = escape_turtle_string(&column.name),
                data_type = escape_turtle_string(&column.data_type),
                nullable = column.nullable,
                dataset_uri = dataset_uri,
            ));
        }
    }

    triples
}

/// Convert discovered schema tables into per-table Dataset triples.
///
/// This creates one `gph:Dataset` per physical table so API consumers can
/// discover source tables directly as pipeline-usable datasets.
pub fn schema_to_table_dataset_triples(
    source: &DataSource,
    schema: &graphica_core::catalog::api_types::SchemaDefinition,
) -> String {
    let mut triples = String::new();
    triples.push_str(&format!("@prefix gph: <{}> .\n", GRAPHICA_NS));
    triples.push_str(&format!("@prefix xsd: <{}> .\n\n", XSD_NS));

    let created_at = source.created_at.unwrap_or_else(Utc::now).to_rfc3339();
    let updated_at = source.updated_at.unwrap_or_else(Utc::now).to_rfc3339();
    let source_token = sanitize_id(&source.id);

    for table in &schema.tables {
        let table_token = sanitize_id(&table.name);
        let table_dataset_id = format!("ds_{}_{}", source_token, table_token);
        let table_dataset_uri = format!("http://graphica.io/ontology/dataset/{}", table_dataset_id);
        let dataset_name = format!("{}.{}", source.title, table.name);
        let record_count = table.estimated_rows.unwrap_or(0);

        triples.push_str(&format!(
            r#"<{dataset_uri}> a gph:Dataset ;
    gph:datasetName "{dataset_name}" ;
    gph:datasetType "source" ;
    gph:recordCount "{record_count}"^^xsd:integer ;
    gph:schemaHash "pending" ;
    gph:sourceDataSource "{source_id}" ;
    gph:sourceTable "{table_name}" ;
    gph:createdAt "{created_at}"^^xsd:dateTime ;
    gph:updatedAt "{updated_at}"^^xsd:dateTime .
"#,
            dataset_uri = table_dataset_uri,
            dataset_name = escape_turtle_string(&dataset_name),
            record_count = record_count,
            source_id = escape_turtle_string(&source.id),
            table_name = escape_turtle_string(&table.name),
            created_at = created_at,
            updated_at = updated_at,
        ));

        for (idx, column) in table.columns.iter().enumerate() {
            let column_uri = format!(
                "{}/column/{}_{}",
                table_dataset_uri,
                sanitize_id(&column.name),
                idx
            );

            triples.push_str(&format!(
                r#"<{column_uri}> a gph:DatasetColumn ;
    gph:columnName "{column_name}" ;
    gph:columnType "{column_type}" ;
    gph:nullable "{nullable}"^^xsd:boolean .

<{dataset_uri}> gph:hasColumn <{column_uri}> .

"#,
                column_uri = column_uri,
                column_name = escape_turtle_string(&column.name),
                column_type = escape_turtle_string(&column.data_type),
                nullable = column.nullable,
                dataset_uri = table_dataset_uri,
            ));
        }
    }

    triples
}

/// Update dataset record count
pub fn update_dataset_record_count(dataset_id: &str, record_count: i64) -> String {
    // Use slash URI style (no fragment) for dataset URIs
    let dataset_uri = format!(
        "http://graphica.io/ontology/dataset/ds_{}",
        sanitize_id(dataset_id)
    );

    format!(
        r#"@prefix gph: <{GRAPHICA_NS}> .
@prefix xsd: <{XSD_NS}> .

<{dataset_uri}> gph:recordCount "{record_count}"^^xsd:integer .
"#,
        GRAPHICA_NS = GRAPHICA_NS,
        XSD_NS = XSD_NS,
        dataset_uri = dataset_uri,
        record_count = record_count,
    )
}

/// Sanitize an ID for use in URIs (replace invalid characters)
fn sanitize_id(id: &str) -> String {
    id.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Escape special characters for Turtle strings
fn escape_turtle_string(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphica_core::catalog::types::{ConnectionDetails, SourceConfig};

    #[test]
    fn test_datasource_to_dataset_triples() {
        let source = DataSource::new(
            "Test Database".to_string(),
            "PostgreSQL".to_string(),
            ConnectionDetails {
                secret_ref: "vault://test".to_string(),
                config: SourceConfig::PostgreSQL(graphica_core::catalog::types::PostgreSQLConfig {
                    host: "localhost".to_string(),
                    port: 5432,
                    database: "testdb".to_string(),
                    schema: None,
                    ssl_mode: None,
                }),
                encryption_enabled: true,
                credentials: Default::default(),
            },
        );

        let triples = datasource_to_dataset_triples(&source);

        assert!(triples.contains("gph:Dataset"));
        assert!(triples.contains("Test Database"));
        assert!(triples.contains("datasetType \"source\""));
        assert!(triples.contains("sourceDataSource"));
    }

    #[test]
    fn test_sanitize_id() {
        assert_eq!(sanitize_id("abc-123_def"), "abc-123_def");
        assert_eq!(
            sanitize_id("datasource:postgres:001"),
            "datasource_postgres_001"
        );
        assert_eq!(sanitize_id("test/path"), "test_path");
    }

    #[test]
    fn test_escape_turtle_string() {
        assert_eq!(escape_turtle_string("simple"), "simple");
        assert_eq!(escape_turtle_string("with\"quotes"), "with\\\"quotes");
        assert_eq!(escape_turtle_string("line\nbreak"), "line\\nbreak");
    }

    #[test]
    fn test_schema_to_column_triples_valid_uris() {
        use graphica_core::catalog::api_types::{
            ColumnDefinition, SchemaDefinition, TableDefinition,
        };

        let schema = SchemaDefinition {
            name: "public".to_string(),
            tables: vec![TableDefinition {
                name: "users".to_string(),
                columns: vec![
                    ColumnDefinition {
                        name: "id".to_string(),
                        data_type: "INTEGER".to_string(),
                        nullable: false,
                        primary_key: true,
                        default_value: None,
                        semantic_type: None,
                        statistics: None,
                    },
                    ColumnDefinition {
                        name: "email".to_string(),
                        data_type: "VARCHAR".to_string(),
                        nullable: true,
                        primary_key: false,
                        default_value: None,
                        semantic_type: None,
                        statistics: None,
                    },
                ],
                estimated_rows: Some(0),
            }],
            relationships: vec![],
            indexes: vec![],
            inferred_at: chrono::Utc::now(),
        };

        let triples = schema_to_column_triples("urn:graphica:datasource:test-123", &schema);

        // Verify URIs don't have fragment identifiers in the middle
        assert!(
            !triples.contains("#dataset/"),
            "URIs should not have path after fragment"
        );

        // Verify proper slash URI style
        assert!(
            triples.contains(
                "http://graphica.io/ontology/dataset/ds_urn_graphica_datasource_test-123"
            ),
            "Should use slash URI style"
        );

        // Verify column URIs are properly formed and table-aware
        assert!(
            triples.contains(
                "http://graphica.io/ontology/dataset/ds_urn_graphica_datasource_test-123/column/users_0"
            ),
            "Column URIs should include table token"
        );

        println!("Generated triples:\n{}", triples);
    }

    #[test]
    fn test_schema_to_table_dataset_triples_generates_per_table_datasets() {
        use graphica_core::catalog::api_types::{
            ColumnDefinition, SchemaDefinition, TableDefinition,
        };
        use graphica_core::catalog::types::{ConnectionDetails, SourceConfig};

        let source = DataSource::new(
            "Oracle ERP".to_string(),
            "Oracle".to_string(),
            ConnectionDetails {
                secret_ref: "vault://oracle".to_string(),
                config: SourceConfig::Oracle(graphica_core::catalog::types::OracleConfig {
                    host: "oracle.example.com".to_string(),
                    port: 1521,
                    service_name: Some("ORCLPDB1".to_string()),
                    sid: None,
                    schema: Some("APPS".to_string()),
                }),
                encryption_enabled: true,
                credentials: Default::default(),
            },
        );

        let schema = SchemaDefinition {
            name: "APPS".to_string(),
            tables: vec![TableDefinition {
                name: "GL_JE_HEADERS".to_string(),
                columns: vec![ColumnDefinition {
                    name: "JE_HEADER_ID".to_string(),
                    data_type: "NUMBER".to_string(),
                    nullable: false,
                    primary_key: true,
                    default_value: None,
                    semantic_type: None,
                    statistics: None,
                }],
                estimated_rows: Some(42),
            }],
            relationships: vec![],
            indexes: vec![],
            inferred_at: chrono::Utc::now(),
        };

        let turtle = schema_to_table_dataset_triples(&source, &schema);

        assert!(turtle.contains("gph:sourceTable \"GL_JE_HEADERS\""));
        assert!(turtle.contains("gph:datasetType \"source\""));
        assert!(turtle.contains("ds_"));
    }
}
