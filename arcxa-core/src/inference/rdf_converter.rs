// graphica-core/src/inference/rdf_converter.rs
//! Convert schema metadata to RDF triples using DCAT, PROV, and Graphica ontology.

use crate::inference::types::*;
use crate::ingestion::FieldSemanticMetadata;
use anyhow::Result;
use chrono::Utc;
use std::collections::HashMap;

/// RDF namespaces
pub mod ns {
    pub const DCAT: &str = "http://www.w3.org/ns/dcat#";
    pub const DCTERMS: &str = "http://purl.org/dc/terms/";
    pub const PROV: &str = "http://www.w3.org/ns/prov#";
    pub const GPH: &str = "http://graphica.io/ontology#";
    pub const SCHEMA: &str = "http://graphica.io/schema#";
    pub const XSD: &str = "http://www.w3.org/2001/XMLSchema#";
    pub const RDF: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#";
    pub const RDFS: &str = "http://www.w3.org/2000/01/rdf-schema#";
}

/// RDF triple representation
#[derive(Debug, Clone)]
pub struct Triple {
    pub subject: String,
    pub predicate: String,
    pub object: RdfValue,
}

#[derive(Debug, Clone)]
pub enum RdfValue {
    Uri(String),
    Literal(String),
    TypedLiteral(String, String), // (value, datatype)
}

impl RdfValue {
    pub fn string(s: impl Into<String>) -> Self {
        RdfValue::TypedLiteral(s.into(), format!("{}string", ns::XSD))
    }

    pub fn integer(i: i64) -> Self {
        RdfValue::TypedLiteral(i.to_string(), format!("{}integer", ns::XSD))
    }

    pub fn double(d: f64) -> Self {
        RdfValue::TypedLiteral(d.to_string(), format!("{}double", ns::XSD))
    }

    pub fn boolean(b: bool) -> Self {
        RdfValue::TypedLiteral(b.to_string(), format!("{}boolean", ns::XSD))
    }

    pub fn datetime(dt: &chrono::DateTime<chrono::Utc>) -> Self {
        RdfValue::TypedLiteral(dt.to_rfc3339(), format!("{}dateTime", ns::XSD))
    }
}

/// Convert schema metadata to RDF triples
pub struct RdfConverter {
    source_uri_prefix: String,
}

impl RdfConverter {
    pub fn new(source_id: &str) -> Self {
        Self {
            source_uri_prefix: format!("urn:graphica:source:{}", source_id),
        }
    }

    /// Convert complete schema metadata to RDF
    pub fn convert_schema_metadata(&self, metadata: &SchemaMetadata) -> Result<Vec<Triple>> {
        let mut triples = Vec::new();

        // Create DCAT Dataset for the schema
        let schema_uri = format!("{}/schema/{}", self.source_uri_prefix, metadata.schema_name);

        triples.push(Triple {
            subject: schema_uri.clone(),
            predicate: format!("{}type", ns::RDF),
            object: RdfValue::Uri(format!("{}Dataset", ns::DCAT)),
        });

        triples.push(Triple {
            subject: schema_uri.clone(),
            predicate: format!("{}title", ns::DCTERMS),
            object: RdfValue::string(&metadata.schema_name),
        });

        triples.push(Triple {
            subject: schema_uri.clone(),
            predicate: format!("{}issued", ns::DCTERMS),
            object: RdfValue::datetime(&metadata.inferred_at),
        });

        // Link to inference activity (PROV)
        let activity_uri = format!("urn:graphica:activity:{}", metadata.lineage_id);
        triples.push(Triple {
            subject: schema_uri.clone(),
            predicate: format!("{}wasGeneratedBy", ns::PROV),
            object: RdfValue::Uri(activity_uri.clone()),
        });

        triples.push(Triple {
            subject: activity_uri.clone(),
            predicate: format!("{}type", ns::RDF),
            object: RdfValue::Uri(format!("{}Activity", ns::PROV)),
        });

        triples.push(Triple {
            subject: activity_uri.clone(),
            predicate: format!("{}activityType", ns::GPH),
            object: RdfValue::string("schema_inference"),
        });

        triples.push(Triple {
            subject: activity_uri,
            predicate: format!("{}inferenceTier", ns::GPH),
            object: RdfValue::integer(metadata.tier_completed as i64),
        });

        // Convert each table
        for table in &metadata.tables {
            triples.extend(self.convert_table(
                &schema_uri,
                &metadata.schema_name,
                table,
                metadata.tier_completed,
            )?);
        }

        Ok(triples)
    }

    /// Convert table metadata to RDF
    fn convert_table(
        &self,
        schema_uri: &str,
        schema_name: &str,
        table: &TableMetadata,
        tier: InferenceTier,
    ) -> Result<Vec<Triple>> {
        let mut triples = Vec::new();

        // Create DCAT Distribution for the table
        let table_uri = format!(
            "{}/schema/{}/table/{}",
            self.source_uri_prefix, schema_name, table.name
        );

        triples.push(Triple {
            subject: table_uri.clone(),
            predicate: format!("{}type", ns::RDF),
            object: RdfValue::Uri(format!("{}Distribution", ns::DCAT)),
        });

        triples.push(Triple {
            subject: table_uri.clone(),
            predicate: format!("{}title", ns::DCTERMS),
            object: RdfValue::string(&table.name),
        });

        // Link to parent dataset
        triples.push(Triple {
            subject: schema_uri.to_string(),
            predicate: format!("{}distribution", ns::DCAT),
            object: RdfValue::Uri(table_uri.clone()),
        });

        // Table type
        triples.push(Triple {
            subject: table_uri.clone(),
            predicate: format!("{}tableType", ns::GPH),
            object: RdfValue::string(match table.table_type {
                TableType::BaseTable => "base_table",
                TableType::View => "view",
                TableType::MaterializedView => "materialized_view",
                TableType::ExternalTable => "external_table",
                TableType::TemporaryTable => "temporary_table",
            }),
        });

        // Row count
        if let Some(rows) = table.estimated_rows {
            triples.push(Triple {
                subject: table_uri.clone(),
                predicate: format!("{}byteSize", ns::DCAT), // Reuse DCAT property
                object: RdfValue::integer(rows as i64),
            });
        }

        // Columns
        for col in &table.columns {
            triples.extend(self.convert_column(&table_uri, &table.name, col)?);
        }

        // Tier 1: Relationships
        if tier >= InferenceTier::Relationships {
            if let Some(ref rels) = table.relationships {
                for fk in &rels.foreign_keys {
                    triples.extend(self.convert_foreign_key(&table_uri, fk)?);
                }
            }

            for idx in &table.indexes {
                triples.extend(self.convert_index(&table_uri, idx)?);
            }
        }

        // Tier 2: Statistics
        if tier >= InferenceTier::Statistics {
            if let Some(ref stats) = table.statistics {
                triples.extend(self.convert_table_statistics(&table_uri, stats)?);
            }
        }

        // Tier 3: Governance
        if tier >= InferenceTier::Governance {
            if let Some(ref gov) = table.governance {
                triples.extend(self.convert_governance(&table_uri, gov)?);
            }
        }

        Ok(triples)
    }

    /// Convert column metadata to RDF
    fn convert_column(
        &self,
        table_uri: &str,
        table_name: &str,
        column: &ColumnMetadata,
    ) -> Result<Vec<Triple>> {
        let mut triples = Vec::new();

        let column_uri = format!("{}/column/{}", table_uri, column.name);

        triples.push(Triple {
            subject: column_uri.clone(),
            predicate: format!("{}type", ns::RDF),
            object: RdfValue::Uri(format!("{}Column", ns::SCHEMA)),
        });

        triples.push(Triple {
            subject: table_uri.to_string(),
            predicate: format!("{}hasColumn", ns::SCHEMA),
            object: RdfValue::Uri(column_uri.clone()),
        });

        triples.push(Triple {
            subject: column_uri.clone(),
            predicate: format!("{}name", ns::SCHEMA),
            object: RdfValue::string(&column.name),
        });

        triples.push(Triple {
            subject: column_uri.clone(),
            predicate: format!("{}dataType", ns::SCHEMA),
            object: RdfValue::string(&column.data_type),
        });

        triples.push(Triple {
            subject: column_uri.clone(),
            predicate: format!("{}nativeType", ns::GPH),
            object: RdfValue::string(&column.native_type),
        });

        triples.push(Triple {
            subject: column_uri.clone(),
            predicate: format!("{}nullable", ns::SCHEMA),
            object: RdfValue::boolean(column.nullable),
        });

        triples.push(Triple {
            subject: column_uri.clone(),
            predicate: format!("{}isPrimaryKey", ns::GPH),
            object: RdfValue::boolean(column.is_primary_key),
        });

        triples.push(Triple {
            subject: column_uri.clone(),
            predicate: format!("{}ordinalPosition", ns::GPH),
            object: RdfValue::integer(column.ordinal_position as i64),
        });

        if let Some(ref comment) = column.comment {
            triples.push(Triple {
                subject: column_uri.clone(),
                predicate: format!("{}description", ns::DCTERMS),
                object: RdfValue::string(comment),
            });
        }

        // PII detection
        if let Some(ref pii) = column.pii_detected {
            let pii_uri = format!("{}/pii", column_uri);

            triples.push(Triple {
                subject: pii_uri.clone(),
                predicate: format!("{}type", ns::RDF),
                object: RdfValue::Uri(format!("{}PiiDetection", ns::GPH)),
            });

            triples.push(Triple {
                subject: column_uri.clone(),
                predicate: format!("{}hasPiiDetection", ns::GPH),
                object: RdfValue::Uri(pii_uri.clone()),
            });

            triples.push(Triple {
                subject: pii_uri.clone(),
                predicate: format!("{}piiType", ns::GPH),
                object: RdfValue::string(format!("{:?}", pii.pii_type)),
            });

            triples.push(Triple {
                subject: pii_uri.clone(),
                predicate: format!("{}confidence", ns::GPH),
                object: RdfValue::double(pii.confidence),
            });
        }

        // Column statistics
        if let Some(ref stats) = column.statistics {
            if let Some(distinct) = stats.distinct_count {
                triples.push(Triple {
                    subject: column_uri.clone(),
                    predicate: format!("{}distinctCount", ns::GPH),
                    object: RdfValue::integer(distinct as i64),
                });
            }

            triples.push(Triple {
                subject: column_uri.clone(),
                predicate: format!("{}nullPercentage", ns::GPH),
                object: RdfValue::double(stats.null_percentage),
            });
        }

        Ok(triples)
    }

    /// Convert foreign key to RDF
    fn convert_foreign_key(&self, table_uri: &str, fk: &ForeignKeyMetadata) -> Result<Vec<Triple>> {
        let mut triples = Vec::new();

        let fk_uri = format!("{}/fk/{}", table_uri, fk.constraint_name);

        triples.push(Triple {
            subject: fk_uri.clone(),
            predicate: format!("{}type", ns::RDF),
            object: RdfValue::Uri(format!("{}ForeignKey", ns::SCHEMA)),
        });

        triples.push(Triple {
            subject: table_uri.to_string(),
            predicate: format!("{}hasForeignKey", ns::SCHEMA),
            object: RdfValue::Uri(fk_uri.clone()),
        });

        triples.push(Triple {
            subject: fk_uri.clone(),
            predicate: format!("{}name", ns::SCHEMA),
            object: RdfValue::string(&fk.constraint_name),
        });

        let ref_table_uri = format!(
            "{}/schema/{}/table/{}",
            self.source_uri_prefix, fk.referenced_schema, fk.referenced_table
        );

        triples.push(Triple {
            subject: fk_uri.clone(),
            predicate: format!("{}referencesTable", ns::SCHEMA),
            object: RdfValue::Uri(ref_table_uri),
        });

        for col in &fk.columns {
            triples.push(Triple {
                subject: fk_uri.clone(),
                predicate: format!("{}sourceColumn", ns::SCHEMA),
                object: RdfValue::string(col),
            });
        }

        for col in &fk.referenced_columns {
            triples.push(Triple {
                subject: fk_uri.clone(),
                predicate: format!("{}targetColumn", ns::SCHEMA),
                object: RdfValue::string(col),
            });
        }

        Ok(triples)
    }

    /// Convert index to RDF
    fn convert_index(&self, table_uri: &str, idx: &IndexMetadata) -> Result<Vec<Triple>> {
        let mut triples = Vec::new();

        let idx_uri = format!("{}/index/{}", table_uri, idx.name);

        triples.push(Triple {
            subject: idx_uri.clone(),
            predicate: format!("{}type", ns::RDF),
            object: RdfValue::Uri(format!("{}Index", ns::SCHEMA)),
        });

        triples.push(Triple {
            subject: table_uri.to_string(),
            predicate: format!("{}hasIndex", ns::SCHEMA),
            object: RdfValue::Uri(idx_uri.clone()),
        });

        triples.push(Triple {
            subject: idx_uri.clone(),
            predicate: format!("{}indexType", ns::GPH),
            object: RdfValue::string(format!("{:?}", idx.index_type)),
        });

        triples.push(Triple {
            subject: idx_uri.clone(),
            predicate: format!("{}isUnique", ns::GPH),
            object: RdfValue::boolean(idx.is_unique),
        });

        if let Some(size) = idx.size_bytes {
            triples.push(Triple {
                subject: idx_uri.clone(),
                predicate: format!("{}byteSize", ns::DCAT),
                object: RdfValue::integer(size as i64),
            });
        }

        Ok(triples)
    }

    /// Convert table statistics to RDF
    fn convert_table_statistics(
        &self,
        table_uri: &str,
        stats: &TableStatistics,
    ) -> Result<Vec<Triple>> {
        let mut triples = Vec::new();

        let stats_uri = format!("{}/statistics", table_uri);

        triples.push(Triple {
            subject: stats_uri.clone(),
            predicate: format!("{}type", ns::RDF),
            object: RdfValue::Uri(format!("{}TableStatistics", ns::GPH)),
        });

        triples.push(Triple {
            subject: table_uri.to_string(),
            predicate: format!("{}hasStatistics", ns::GPH),
            object: RdfValue::Uri(stats_uri.clone()),
        });

        triples.push(Triple {
            subject: stats_uri.clone(),
            predicate: format!("{}rowCount", ns::GPH),
            object: RdfValue::integer(stats.actual_row_count as i64),
        });

        triples.push(Triple {
            subject: stats_uri.clone(),
            predicate: format!("{}sizeBytes", ns::GPH),
            object: RdfValue::integer(stats.size_bytes as i64),
        });

        if let Some(ref modified) = stats.last_modified {
            triples.push(Triple {
                subject: stats_uri.clone(),
                predicate: format!("{}modified", ns::DCTERMS),
                object: RdfValue::datetime(modified),
            });
        }

        Ok(triples)
    }

    /// Convert governance metadata to RDF
    fn convert_governance(&self, table_uri: &str, gov: &GovernanceMetadata) -> Result<Vec<Triple>> {
        let mut triples = Vec::new();

        let gov_uri = format!("{}/governance", table_uri);

        triples.push(Triple {
            subject: gov_uri.clone(),
            predicate: format!("{}type", ns::RDF),
            object: RdfValue::Uri(format!("{}GovernanceMetadata", ns::GPH)),
        });

        triples.push(Triple {
            subject: table_uri.to_string(),
            predicate: format!("{}hasGovernance", ns::GPH),
            object: RdfValue::Uri(gov_uri.clone()),
        });

        triples.push(Triple {
            subject: gov_uri.clone(),
            predicate: format!("{}classification", ns::GPH),
            object: RdfValue::string(format!("{:?}", gov.data_classification)),
        });

        // Quality metrics
        triples.push(Triple {
            subject: gov_uri.clone(),
            predicate: format!("{}completeness", ns::GPH),
            object: RdfValue::double(gov.quality_metrics.completeness),
        });

        triples.push(Triple {
            subject: gov_uri.clone(),
            predicate: format!("{}validity", ns::GPH),
            object: RdfValue::double(gov.quality_metrics.validity),
        });

        triples.push(Triple {
            subject: gov_uri.clone(),
            predicate: format!("{}timeliness", ns::GPH),
            object: RdfValue::double(gov.quality_metrics.timeliness),
        });

        Ok(triples)
    }

    /// Convert record with semantic metadata to RDF (Phase 2.1)
    ///
    /// Creates triples for:
    /// - Record entity
    /// - Field entities with semantic type annotations
    /// - Detection provenance
    pub fn convert_record_with_semantics(
        &self,
        record_id: &str,
        dataset: &str,
        semantic_metadata: &HashMap<String, FieldSemanticMetadata>,
        timestamp: i64,
    ) -> Result<Vec<Triple>> {
        let mut triples = Vec::new();

        // Create record entity
        let record_uri = format!("urn:graphica:record:{}/{}", dataset, record_id);

        triples.push(Triple {
            subject: record_uri.clone(),
            predicate: format!("{}type", ns::RDF),
            object: RdfValue::Uri(format!("{}Record", ns::GPH)),
        });

        triples.push(Triple {
            subject: record_uri.clone(),
            predicate: format!("{}recordId", ns::GPH),
            object: RdfValue::string(record_id),
        });

        triples.push(Triple {
            subject: record_uri.clone(),
            predicate: format!("{}dataset", ns::GPH),
            object: RdfValue::string(dataset),
        });

        let timestamp_dt =
            chrono::DateTime::from_timestamp(timestamp / 1000, 0).unwrap_or_else(|| Utc::now());

        triples.push(Triple {
            subject: record_uri.clone(),
            predicate: format!("{}ingestedAt", ns::GPH),
            object: RdfValue::datetime(&timestamp_dt),
        });

        // Convert each field with semantic metadata
        for (field_name, metadata) in semantic_metadata {
            triples.extend(self.convert_field_semantic_metadata(
                &record_uri,
                field_name,
                metadata,
            )?);
        }

        Ok(triples)
    }

    /// Convert field semantic metadata to RDF
    fn convert_field_semantic_metadata(
        &self,
        record_uri: &str,
        field_name: &str,
        metadata: &FieldSemanticMetadata,
    ) -> Result<Vec<Triple>> {
        let mut triples = Vec::new();

        // Create field entity
        let field_uri = format!("{}/field/{}", record_uri, field_name);

        triples.push(Triple {
            subject: field_uri.clone(),
            predicate: format!("{}type", ns::RDF),
            object: RdfValue::Uri(format!("{}Field", ns::GPH)),
        });

        triples.push(Triple {
            subject: record_uri.to_string(),
            predicate: format!("{}hasField", ns::GPH),
            object: RdfValue::Uri(field_uri.clone()),
        });

        triples.push(Triple {
            subject: field_uri.clone(),
            predicate: format!("{}fieldName", ns::GPH),
            object: RdfValue::string(field_name),
        });

        // Semantic type annotation
        let semantic_type_uri = format!("{}SemanticType/{:?}", ns::GPH, metadata.semantic_type);

        triples.push(Triple {
            subject: field_uri.clone(),
            predicate: format!("{}semanticType", ns::GPH),
            object: RdfValue::Uri(semantic_type_uri.clone()),
        });

        // Detection metadata
        let detection_uri = format!("{}/detection", field_uri);

        triples.push(Triple {
            subject: detection_uri.clone(),
            predicate: format!("{}type", ns::RDF),
            object: RdfValue::Uri(format!("{}SemanticTypeDetection", ns::GPH)),
        });

        triples.push(Triple {
            subject: field_uri.clone(),
            predicate: format!("{}hasDetection", ns::GPH),
            object: RdfValue::Uri(detection_uri.clone()),
        });

        triples.push(Triple {
            subject: detection_uri.clone(),
            predicate: format!("{}detectionConfidence", ns::GPH),
            object: RdfValue::double(metadata.confidence),
        });

        triples.push(Triple {
            subject: detection_uri.clone(),
            predicate: format!("{}detectionMethod", ns::GPH),
            object: RdfValue::string(&metadata.detection_method),
        });

        triples.push(Triple {
            subject: detection_uri.clone(),
            predicate: format!("{}detectedAt", ns::PROV),
            object: RdfValue::datetime(&Utc::now()),
        });

        // Link to detection activity (PROV)
        triples.push(Triple {
            subject: detection_uri.clone(),
            predicate: format!("{}wasGeneratedBy", ns::PROV),
            object: RdfValue::Uri("urn:graphica:detector:column_name_default".to_string()),
        });

        Ok(triples)
    }

    /// Convert to Turtle format (for persistence)
    pub fn triples_to_turtle(&self, triples: &[Triple]) -> String {
        let mut output = String::new();

        // Prefixes
        output.push_str(&format!("@prefix dcat: <{}> .\n", ns::DCAT));
        output.push_str(&format!("@prefix dcterms: <{}> .\n", ns::DCTERMS));
        output.push_str(&format!("@prefix prov: <{}> .\n", ns::PROV));
        output.push_str(&format!("@prefix gph: <{}> .\n", ns::GPH));
        output.push_str(&format!("@prefix schema: <{}> .\n", ns::SCHEMA));
        output.push_str(&format!("@prefix xsd: <{}> .\n", ns::XSD));
        output.push_str(&format!("@prefix rdf: <{}> .\n", ns::RDF));
        output.push_str("\n");

        // Triples
        for triple in triples {
            let obj_str = match &triple.object {
                RdfValue::Uri(uri) => format!("<{}>", uri),
                RdfValue::Literal(lit) => format!("\"{}\"", lit),
                RdfValue::TypedLiteral(val, dt) => format!("\"{}\"^^<{}>", val, dt),
            };

            output.push_str(&format!(
                "<{}> <{}> {} .\n",
                triple.subject, triple.predicate, obj_str
            ));
        }

        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inference::types::SemanticType;

    #[test]
    fn test_rdf_conversion() {
        let converter = RdfConverter::new("test_source");

        let metadata = SchemaMetadata {
            source_id: "test_source".to_string(),
            schema_name: "public".to_string(),
            inferred_at: chrono::Utc::now(),
            tier_completed: InferenceTier::Basic,
            tables: vec![],
            lineage_id: "test_lineage".to_string(),
        };

        let triples = converter.convert_schema_metadata(&metadata).unwrap();
        assert!(!triples.is_empty());

        let turtle = converter.triples_to_turtle(&triples);
        assert!(turtle.contains("@prefix dcat"));
        assert!(turtle.contains("Dataset"));
    }

    #[test]
    fn test_semantic_metadata_rdf_conversion() {
        let converter = RdfConverter::new("test_source");

        let mut semantic_metadata = HashMap::new();
        semantic_metadata.insert(
            "email".to_string(),
            FieldSemanticMetadata {
                field_name: "email".to_string(),
                semantic_type: SemanticType::Email,
                confidence: 0.9,
                detection_method: "Exact match: 'email'".to_string(),
            },
        );
        semantic_metadata.insert(
            "phone_number".to_string(),
            FieldSemanticMetadata {
                field_name: "phone_number".to_string(),
                semantic_type: SemanticType::PhoneNumber,
                confidence: 0.85,
                detection_method: "Contains: 'phone'".to_string(),
            },
        );

        let triples = converter
            .convert_record_with_semantics(
                "user-123",
                "customers",
                &semantic_metadata,
                1234567890000,
            )
            .unwrap();

        assert!(!triples.is_empty());

        // Verify record entity
        let record_triples: Vec<_> = triples
            .iter()
            .filter(|t| t.subject.contains("urn:graphica:record:customers/user-123"))
            .collect();
        assert!(!record_triples.is_empty());

        // Verify field entities
        let field_triples: Vec<_> = triples
            .iter()
            .filter(|t| t.subject.contains("/field/email"))
            .collect();
        assert!(!field_triples.is_empty());

        // Verify semantic type annotation
        let semantic_type_triples: Vec<_> = triples
            .iter()
            .filter(|t| t.predicate.contains("semanticType"))
            .collect();
        assert_eq!(semantic_type_triples.len(), 2); // email and phone_number

        // Verify detection metadata
        let detection_triples: Vec<_> = triples
            .iter()
            .filter(|t| t.predicate.contains("detectionConfidence"))
            .collect();
        assert_eq!(detection_triples.len(), 2);

        // Generate Turtle and verify structure
        let turtle = converter.triples_to_turtle(&triples);
        assert!(turtle.contains("@prefix gph"));
        assert!(turtle.contains("@prefix prov"));
        assert!(turtle.contains("Record"));
        assert!(turtle.contains("Field"));
        assert!(turtle.contains("SemanticTypeDetection"));
        assert!(turtle.contains("0.9")); // confidence for email
        assert!(turtle.contains("0.85")); // confidence for phone_number
    }

    #[test]
    fn test_turtle_serialization_with_semantics() {
        let converter = RdfConverter::new("test_source");

        let mut semantic_metadata = HashMap::new();
        semantic_metadata.insert(
            "ssn".to_string(),
            FieldSemanticMetadata {
                field_name: "ssn".to_string(),
                semantic_type: SemanticType::SSN,
                confidence: 0.95,
                detection_method: "Pattern match: SSN regex".to_string(),
            },
        );

        let triples = converter
            .convert_record_with_semantics(
                "record-001",
                "employees",
                &semantic_metadata,
                1000000000000,
            )
            .unwrap();

        let turtle = converter.triples_to_turtle(&triples);

        // Verify proper Turtle syntax
        assert!(turtle.starts_with("@prefix"));
        assert!(turtle.contains("http://graphica.io/ontology#Field"));
        assert!(turtle.contains("http://graphica.io/ontology#semanticType"));
        assert!(turtle.contains("http://graphica.io/ontology#SemanticType/SSN"));
        assert!(turtle.contains("\"0.95\"^^<http://www.w3.org/2001/XMLSchema#double>"));
    }
}
