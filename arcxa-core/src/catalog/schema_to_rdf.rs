//! Schema to RDF Conversion
//!
//! Converts inferred database schemas (with semantic types and statistics)
//! to RDF triples using the Graphica ontology vocabulary.

use anyhow::Result;
use chrono::Utc;

use crate::catalog::api_types::{
    ColumnDefinition, SchemaDefinition, TableDefinition, TableRelationshipDefinition,
};
use crate::catalog::ontology_extensions::semantic_type_to_uri;
use crate::inference::types::SemanticType;

/// RDF triple (subject, predicate, object)
#[derive(Debug, Clone, PartialEq)]
pub struct RdfTriple {
    pub subject: String,
    pub predicate: String,
    pub object: RdfNode,
}

/// RDF node - either URI, Literal, or Blank node
#[derive(Debug, Clone, PartialEq)]
pub enum RdfNode {
    Uri(String),
    Literal(String),
    TypedLiteral(String, String), // (value, datatype_uri)
    LangLiteral(String, String),  // (value, language_tag)
}

impl RdfNode {
    /// Create a string literal
    pub fn string(value: impl Into<String>) -> Self {
        RdfNode::Literal(value.into())
    }

    /// Create an integer literal
    pub fn integer(value: i64) -> Self {
        RdfNode::TypedLiteral(
            value.to_string(),
            "http://www.w3.org/2001/XMLSchema#integer".to_string(),
        )
    }

    /// Create a double literal
    pub fn double(value: f64) -> Self {
        RdfNode::TypedLiteral(
            value.to_string(),
            "http://www.w3.org/2001/XMLSchema#double".to_string(),
        )
    }

    /// Create a boolean literal
    pub fn boolean(value: bool) -> Self {
        RdfNode::TypedLiteral(
            value.to_string(),
            "http://www.w3.org/2001/XMLSchema#boolean".to_string(),
        )
    }

    /// Create a dateTime literal
    pub fn datetime(value: &chrono::DateTime<Utc>) -> Self {
        RdfNode::TypedLiteral(
            value.to_rfc3339(),
            "http://www.w3.org/2001/XMLSchema#dateTime".to_string(),
        )
    }

    /// Create a URI node
    pub fn uri(value: impl Into<String>) -> Self {
        RdfNode::Uri(value.into())
    }
}

/// Schema to RDF converter
pub struct SchemaRdfConverter {
    /// Namespace for schema resources
    base_namespace: String,

    /// Data source ID
    source_id: String,
}

impl SchemaRdfConverter {
    /// Create a new converter
    pub fn new(source_id: impl Into<String>) -> Self {
        Self {
            base_namespace: "http://graphica.io/catalog/".to_string(),
            source_id: source_id.into(),
        }
    }

    /// Convert a schema definition to RDF triples
    pub fn convert_schema(&self, schema: &SchemaDefinition) -> Result<Vec<RdfTriple>> {
        let mut triples = Vec::new();

        // Create schema URI
        let schema_uri = format!(
            "{}schema/{}/{}",
            self.base_namespace, self.source_id, schema.name
        );

        // Schema metadata
        triples.push(RdfTriple {
            subject: schema_uri.clone(),
            predicate: "http://www.w3.org/1999/02/22-rdf-syntax-ns#type".to_string(),
            object: RdfNode::uri("http://graphica.io/ontology#Schema"),
        });

        triples.push(RdfTriple {
            subject: schema_uri.clone(),
            predicate: "http://purl.org/dc/terms/title".to_string(),
            object: RdfNode::string(&schema.name),
        });

        triples.push(RdfTriple {
            subject: schema_uri.clone(),
            predicate: "http://graphica.io/ontology#inferredAt".to_string(),
            object: RdfNode::datetime(&schema.inferred_at),
        });

        // Convert tables
        for table in &schema.tables {
            let table_triples = self.convert_table(&schema_uri, table)?;
            triples.extend(table_triples);
        }

        // Convert relationships
        for relationship in &schema.relationships {
            let relationship_triples = self.convert_relationship(&schema_uri, relationship);
            triples.extend(relationship_triples);
        }

        Ok(triples)
    }

    /// Convert a table definition to RDF triples
    fn convert_table(&self, schema_uri: &str, table: &TableDefinition) -> Result<Vec<RdfTriple>> {
        let mut triples = Vec::new();

        // Create table URI
        let table_uri = format!("{}/table/{}", schema_uri, table.name);

        // Table metadata
        triples.push(RdfTriple {
            subject: table_uri.clone(),
            predicate: "http://www.w3.org/1999/02/22-rdf-syntax-ns#type".to_string(),
            object: RdfNode::uri("http://graphica.io/ontology#Table"),
        });

        triples.push(RdfTriple {
            subject: table_uri.clone(),
            predicate: "http://purl.org/dc/terms/title".to_string(),
            object: RdfNode::string(&table.name),
        });

        // Link table to schema
        triples.push(RdfTriple {
            subject: schema_uri.to_string(),
            predicate: "http://graphica.io/ontology#hasTable".to_string(),
            object: RdfNode::uri(&table_uri),
        });

        // Estimated rows
        if let Some(rows) = table.estimated_rows {
            triples.push(RdfTriple {
                subject: table_uri.clone(),
                predicate: "http://graphica.io/ontology#estimatedRows".to_string(),
                object: RdfNode::integer(rows as i64),
            });
        }

        // Convert columns
        for column in &table.columns {
            let column_triples = self.convert_column(&table_uri, column)?;
            triples.extend(column_triples);
        }

        Ok(triples)
    }

    /// Convert a column definition to RDF triples
    fn convert_column(&self, table_uri: &str, column: &ColumnDefinition) -> Result<Vec<RdfTriple>> {
        let mut triples = Vec::new();

        // Create column URI
        let column_uri = format!("{}/column/{}", table_uri, column.name);

        // Column metadata
        triples.push(RdfTriple {
            subject: column_uri.clone(),
            predicate: "http://www.w3.org/1999/02/22-rdf-syntax-ns#type".to_string(),
            object: RdfNode::uri("http://graphica.io/ontology#Column"),
        });

        triples.push(RdfTriple {
            subject: column_uri.clone(),
            predicate: "http://purl.org/dc/terms/title".to_string(),
            object: RdfNode::string(&column.name),
        });

        // Link column to table
        triples.push(RdfTriple {
            subject: table_uri.to_string(),
            predicate: "http://graphica.io/ontology#hasColumn".to_string(),
            object: RdfNode::uri(&column_uri),
        });

        // Data type
        triples.push(RdfTriple {
            subject: column_uri.clone(),
            predicate: "http://graphica.io/ontology#dataType".to_string(),
            object: RdfNode::string(&column.data_type),
        });

        // Nullable
        triples.push(RdfTriple {
            subject: column_uri.clone(),
            predicate: "http://graphica.io/ontology#nullable".to_string(),
            object: RdfNode::boolean(column.nullable),
        });

        // Primary key
        if column.primary_key {
            triples.push(RdfTriple {
                subject: column_uri.clone(),
                predicate: "http://graphica.io/ontology#primaryKey".to_string(),
                object: RdfNode::boolean(true),
            });
        }

        // Default value
        if let Some(ref default_val) = column.default_value {
            triples.push(RdfTriple {
                subject: column_uri.clone(),
                predicate: "http://graphica.io/ontology#defaultValue".to_string(),
                object: RdfNode::string(default_val),
            });
        }

        // Semantic type (IMPORTANT!)
        if let Some(ref semantic_type) = column.semantic_type {
            let semantic_uri = semantic_type_to_uri(semantic_type);

            triples.push(RdfTriple {
                subject: column_uri.clone(),
                predicate: "http://graphica.io/inference#semanticType".to_string(),
                object: RdfNode::uri(&semantic_uri),
            });
        }

        // Statistics
        if let Some(ref stats) = column.statistics {
            // Distinct count
            if let Some(distinct) = stats.distinct_count {
                triples.push(RdfTriple {
                    subject: column_uri.clone(),
                    predicate: "http://graphica.io/inference#distinctCount".to_string(),
                    object: RdfNode::integer(distinct as i64),
                });
            }

            // Null count
            triples.push(RdfTriple {
                subject: column_uri.clone(),
                predicate: "http://graphica.io/inference#nullCount".to_string(),
                object: RdfNode::integer(stats.null_count as i64),
            });

            // Null percentage
            triples.push(RdfTriple {
                subject: column_uri.clone(),
                predicate: "http://graphica.io/inference#nullPercentage".to_string(),
                object: RdfNode::double(stats.null_percentage),
            });

            // Cardinality class
            if let Some(ref cardinality) = stats.cardinality {
                let cardinality_uri = format!("http://graphica.io/inference#{:?}", cardinality);
                triples.push(RdfTriple {
                    subject: column_uri.clone(),
                    predicate: "http://graphica.io/inference#cardinalityClass".to_string(),
                    object: RdfNode::uri(&cardinality_uri),
                });
            }

            // Correlation (PostgreSQL specific)
            if let Some(correlation) = stats.correlation {
                triples.push(RdfTriple {
                    subject: column_uri.clone(),
                    predicate: "http://graphica.io/inference#correlation".to_string(),
                    object: RdfNode::double(correlation),
                });
            }

            // Average width
            if let Some(avg_width) = stats.avg_width {
                triples.push(RdfTriple {
                    subject: column_uri.clone(),
                    predicate: "http://graphica.io/inference#avgWidth".to_string(),
                    object: RdfNode::integer(avg_width as i64),
                });
            }
        }

        Ok(triples)
    }

    /// Convert a table relationship definition to RDF triples.
    fn convert_relationship(
        &self,
        schema_uri: &str,
        relationship: &TableRelationshipDefinition,
    ) -> Vec<RdfTriple> {
        let rel_name = relationship.name.clone().unwrap_or_else(|| {
            format!(
                "{}_{}_to_{}_{}",
                relationship.source_table,
                relationship.source_columns.join("_"),
                relationship.target_table,
                relationship.target_columns.join("_")
            )
        });

        let relationship_uri = format!("{}/relationship/{}", schema_uri, rel_name);

        let mut triples = vec![
            RdfTriple {
                subject: relationship_uri.clone(),
                predicate: "http://www.w3.org/1999/02/22-rdf-syntax-ns#type".to_string(),
                object: RdfNode::uri("http://graphica.io/ontology#TableRelationship"),
            },
            RdfTriple {
                subject: schema_uri.to_string(),
                predicate: "http://graphica.io/ontology#hasRelationship".to_string(),
                object: RdfNode::uri(&relationship_uri),
            },
            RdfTriple {
                subject: relationship_uri.clone(),
                predicate: "http://graphica.io/ontology#sourceTable".to_string(),
                object: RdfNode::string(&relationship.source_table),
            },
            RdfTriple {
                subject: relationship_uri.clone(),
                predicate: "http://graphica.io/ontology#targetTable".to_string(),
                object: RdfNode::string(&relationship.target_table),
            },
            RdfTriple {
                subject: relationship_uri.clone(),
                predicate: "http://graphica.io/ontology#sourceColumns".to_string(),
                object: RdfNode::string(relationship.source_columns.join(",")),
            },
            RdfTriple {
                subject: relationship_uri.clone(),
                predicate: "http://graphica.io/ontology#targetColumns".to_string(),
                object: RdfNode::string(relationship.target_columns.join(",")),
            },
            RdfTriple {
                subject: relationship_uri.clone(),
                predicate: "http://graphica.io/ontology#relationshipType".to_string(),
                object: RdfNode::string(format!("{:?}", relationship.relationship_type)),
            },
        ];

        if let Some(ref on_delete) = relationship.on_delete {
            triples.push(RdfTriple {
                subject: relationship_uri.clone(),
                predicate: "http://graphica.io/ontology#onDelete".to_string(),
                object: RdfNode::string(on_delete),
            });
        }

        if let Some(ref on_update) = relationship.on_update {
            triples.push(RdfTriple {
                subject: relationship_uri.clone(),
                predicate: "http://graphica.io/ontology#onUpdate".to_string(),
                object: RdfNode::string(on_update),
            });
        }

        triples
    }

    /// Convert triples to Turtle format
    pub fn triples_to_turtle(triples: &[RdfTriple]) -> String {
        let mut turtle = String::new();

        // Prefixes
        turtle.push_str("@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n");
        turtle.push_str("@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n");
        turtle.push_str("@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .\n");
        turtle.push_str("@prefix dcterms: <http://purl.org/dc/terms/> .\n");
        turtle.push_str("@prefix gph: <http://graphica.io/ontology#> .\n");
        turtle.push_str("@prefix gphi: <http://graphica.io/inference#> .\n");
        turtle.push_str("\n");

        // Group triples by subject
        use std::collections::HashMap;
        let mut by_subject: HashMap<String, Vec<&RdfTriple>> = HashMap::new();
        for triple in triples {
            by_subject
                .entry(triple.subject.clone())
                .or_insert_with(Vec::new)
                .push(triple);
        }

        // Write triples
        for (subject, subject_triples) in by_subject {
            turtle.push_str(&format!("<{}>\n", subject));

            for (i, triple) in subject_triples.iter().enumerate() {
                let predicate = format!("<{}>", triple.predicate);
                let object = match &triple.object {
                    RdfNode::Uri(uri) => format!("<{}>", uri),
                    RdfNode::Literal(lit) => format!("\"{}\"", lit.replace('\"', "\\\"")),
                    RdfNode::TypedLiteral(val, dtype) => {
                        format!("\"{}\"^^<{}>", val.replace('\"', "\\\""), dtype)
                    }
                    RdfNode::LangLiteral(val, lang) => {
                        format!("\"{}\"@{}", val.replace('\"', "\\\""), lang)
                    }
                };

                let separator = if i == subject_triples.len() - 1 {
                    " ."
                } else {
                    " ;"
                };
                turtle.push_str(&format!("    {} {}{}\n", predicate, object, separator));
            }
            turtle.push('\n');
        }

        turtle
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_schema_to_rdf_conversion() {
        let schema = SchemaDefinition {
            name: "public".to_string(),
            tables: vec![TableDefinition {
                name: "users".to_string(),
                columns: vec![ColumnDefinition {
                    name: "email".to_string(),
                    data_type: "varchar".to_string(),
                    nullable: false,
                    primary_key: false,
                    default_value: None,
                    semantic_type: Some(SemanticType::Email),
                    statistics: None,
                }],
                estimated_rows: Some(1000),
            }],
            relationships: vec![],
            indexes: vec![],
            inferred_at: Utc::now(),
        };

        let converter = SchemaRdfConverter::new("test_source");
        let triples = converter.convert_schema(&schema).unwrap();

        // Should have triples for schema, table, and column
        assert!(triples.len() > 5);

        // Check for semantic type triple
        let has_semantic_type = triples.iter().any(|t| t.predicate.contains("semanticType"));
        assert!(has_semantic_type, "Should include semantic type triple");
    }

    #[test]
    fn test_turtle_output() {
        let triples = vec![
            RdfTriple {
                subject: "http://example.com/subject".to_string(),
                predicate: "http://www.w3.org/1999/02/22-rdf-syntax-ns#type".to_string(),
                object: RdfNode::uri("http://example.com/Type"),
            },
            RdfTriple {
                subject: "http://example.com/subject".to_string(),
                predicate: "http://purl.org/dc/terms/title".to_string(),
                object: RdfNode::string("Test"),
            },
        ];

        let turtle = SchemaRdfConverter::triples_to_turtle(&triples);

        assert!(turtle.contains("@prefix"));
        assert!(turtle.contains("<http://example.com/subject>"));
        assert!(turtle.contains("\"Test\""));
    }
}
