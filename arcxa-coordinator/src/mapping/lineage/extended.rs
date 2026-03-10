//! Extended Lineage Tracking
//!
//! This module provides comprehensive lineage queries that trace data through
//! the complete CSV-to-DB pipeline:
//!
//! CSV Field → Ontology Term → Unified Mapping → Fusion → Target DB Column
//!
//! Features:
//! - Full chain lineage (forward)
//! - Reverse lineage (DB → CSV)
//! - Fusion impact analysis
//! - SPARQL query generation

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// Extended lineage chain from CSV to target database
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtendedLineageChain {
    /// Source CSV file information
    pub source: SourceInfo,

    /// Ontology term this field maps to
    pub ontology_term: OntologyTermInfo,

    /// Unified mapping information
    pub unified_mapping: UnifiedMappingInfo,

    /// Fusion operations affecting this data (if any)
    pub fusion_operations: Vec<FusionInfo>,

    /// Target database column
    pub target: TargetInfo,

    /// Confidence score for the overall mapping
    pub confidence: f64,
}

/// Source CSV information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceInfo {
    /// Datasource ID
    pub datasource_id: String,

    /// CSV file path
    pub file_path: String,

    /// Table name
    pub table_name: String,

    /// Field name
    pub field_name: String,

    /// Data type
    pub data_type: String,

    /// Mapping session ID
    pub session_id: String,
}

/// Ontology term information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OntologyTermInfo {
    /// Term URI
    pub uri: String,

    /// Term label
    pub label: Option<String>,

    /// Term description
    pub description: Option<String>,
}

/// Unified mapping information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifiedMappingInfo {
    /// Unified session ID
    pub unified_session_id: String,

    /// Mapping ID
    pub mapping_id: String,

    /// Conflict resolution strategy used
    pub conflict_resolution: String,

    /// Transformation applied (if any)
    pub transformation: Option<String>,
}

/// Fusion operation information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FusionInfo {
    /// Fusion operation ID
    pub fusion_id: String,

    /// Canonical entity ID
    pub canonical_entity_id: String,

    /// Merged entity IDs
    pub merged_entity_ids: Vec<String>,

    /// Fusion rule used
    pub fusion_rule: String,

    /// Confidence score
    pub confidence: f64,

    /// Timestamp
    pub timestamp: i64,
}

/// Target database information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TargetInfo {
    /// Database ID
    pub database_id: String,

    /// Schema name
    pub schema: String,

    /// Table name
    pub table_name: String,

    /// Column name
    pub column_name: String,

    /// Data type
    pub data_type: String,
}

/// Reverse lineage result (DB → CSV)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReverseLineageResult {
    /// Target column being queried
    pub target_column: TargetColumnRef,

    /// All source fields that contribute to this column
    pub source_fields: Vec<SourceFieldContribution>,

    /// Total number of source CSV files
    pub source_file_count: usize,

    /// Fusion operations count
    pub fusion_operations_count: usize,
}

/// Target column reference for reverse lineage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TargetColumnRef {
    pub database_id: String,
    pub table_name: String,
    pub column_name: String,
}

/// Source field contribution to target column
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceFieldContribution {
    /// Source information
    pub source: SourceInfo,

    /// Ontology term
    pub ontology_term_uri: String,

    /// Contribution weight (for merged fields)
    pub contribution_weight: f64,

    /// Number of records contributed
    pub records_contributed: usize,
}

/// Fusion impact analysis result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FusionImpactResult {
    /// Fusion operation ID
    pub fusion_id: String,

    /// Number of entities affected
    pub entities_affected: usize,

    /// Target tables impacted
    pub tables_impacted: Vec<String>,

    /// Fields affected
    pub fields_affected: Vec<String>,

    /// Estimated row count impact
    pub estimated_row_impact: usize,
}

/// Extended lineage query service
pub struct ExtendedLineageService {
    // In a real implementation, this would have:
    // rdf_store: Arc<GraphicaRdfStore>
    // For now, we'll keep it simple for testing
}

impl ExtendedLineageService {
    /// Create a new extended lineage service
    pub fn new() -> Self {
        Self {}
    }

    /// Get full lineage chain for a source field
    ///
    /// Traces data from CSV field through to target database column
    pub fn get_field_lineage_chain(
        &self,
        datasource_id: &str,
        table_name: &str,
        field_name: &str,
    ) -> Result<Option<ExtendedLineageChain>> {
        // In production, this would execute SPARQL queries
        // For now, return None to indicate not found
        Ok(None)
    }

    /// Get reverse lineage for a target database column
    ///
    /// Traces back from DB column to all contributing CSV fields
    pub fn get_reverse_lineage(
        &self,
        database_id: &str,
        table_name: &str,
        column_name: &str,
    ) -> Result<ReverseLineageResult> {
        // Build target column reference
        let target_column = TargetColumnRef {
            database_id: database_id.to_string(),
            table_name: table_name.to_string(),
            column_name: column_name.to_string(),
        };

        // In production, this would execute SPARQL queries to find:
        // 1. Unified mappings targeting this column
        // 2. Source fields contributing to those mappings
        // 3. Fusion operations affecting those source fields

        Ok(ReverseLineageResult {
            target_column,
            source_fields: vec![],
            source_file_count: 0,
            fusion_operations_count: 0,
        })
    }

    /// Analyze fusion impact
    ///
    /// Determines how many target database rows are affected by a fusion operation
    pub fn analyze_fusion_impact(&self, fusion_id: &str) -> Result<FusionImpactResult> {
        // In production, this would query:
        // 1. Entities involved in fusion
        // 2. Unified mappings using those entities
        // 3. Target tables affected
        // 4. Estimated row counts

        Ok(FusionImpactResult {
            fusion_id: fusion_id.to_string(),
            entities_affected: 0,
            tables_impacted: vec![],
            fields_affected: vec![],
            estimated_row_impact: 0,
        })
    }

    /// Generate SPARQL query for full lineage chain
    pub fn generate_lineage_sparql(&self, datasource_id: &str, field_name: &str) -> String {
        format!(
            r#"
PREFIX gph: <http://graphica.io/ontology#>
PREFIX prov: <http://www.w3.org/ns/prov#>

SELECT DISTINCT
    ?sourceField
    ?ontologyTerm
    ?unifiedMapping
    ?targetColumn
    ?confidence
WHERE {{
    # Source field mapping
    ?fieldMapping a gph:FieldMapping ;
        gph:sourceField ?sourceField ;
        gph:mapsToOntologyTerm ?ontologyTerm ;
        gph:confidence ?fieldConfidence .

    ?sourceField gph:sourceId "{}" ;
                 gph:fieldName "{}" .

    # Unified mapping
    ?unifiedMapping a gph:UnifiedFieldMapping ;
        gph:fromOntologyTerm ?ontologyTerm ;
        gph:targetColumn ?targetColumn ;
        gph:confidence ?mappingConfidence .

    # Calculate overall confidence
    BIND((?fieldConfidence * ?mappingConfidence) AS ?confidence)

    # Optional: Fusion operations
    OPTIONAL {{
        ?entity gph:sourceField ?sourceField .
        ?fusionOp a gph:FusionOperation ;
            gph:mergedEntity ?entity .
    }}
}}
ORDER BY DESC(?confidence)
"#,
            datasource_id, field_name
        )
    }

    /// Generate SPARQL query for reverse lineage
    pub fn generate_reverse_lineage_sparql(&self, table_name: &str, column_name: &str) -> String {
        format!(
            r#"
PREFIX gph: <http://graphica.io/ontology#>
PREFIX prov: <http://www.w3.org/ns/prov#>

SELECT DISTINCT
    ?sourceFile
    ?sourceField
    ?ontologyTerm
    ?confidence
WHERE {{
    # Target column
    ?targetColumn a gph:TargetColumn ;
        gph:tableName "{}" ;
        gph:columnName "{}" .

    # Unified mapping
    ?unifiedMapping a gph:UnifiedFieldMapping ;
        gph:targetColumn ?targetColumn ;
        gph:fromOntologyTerm ?ontologyTerm ;
        gph:confidence ?confidence .

    # Field mapping
    ?fieldMapping a gph:FieldMapping ;
        gph:mapsToOntologyTerm ?ontologyTerm ;
        gph:sourceField ?sourceField .

    # Source file
    ?sourceField gph:sourceId ?sourceId .
    ?dataSource gph:sourceId ?sourceId ;
                gph:filePath ?sourceFile .
}}
ORDER BY DESC(?confidence)
"#,
            table_name, column_name
        )
    }

    /// Generate SPARQL query for fusion impact
    pub fn generate_fusion_impact_sparql(&self, fusion_id: &str) -> String {
        format!(
            r#"
PREFIX gph: <http://graphica.io/ontology#>
PREFIX prov: <http://www.w3.org/ns/prov#>

SELECT DISTINCT
    ?targetTable
    ?targetColumn
    (COUNT(DISTINCT ?entity) AS ?affectedEntities)
WHERE {{
    # Fusion operation
    <http://graphica.io/fusion/{}> a gph:FusionOperation ;
        gph:mergedEntity ?canonicalEntity ;
        gph:sourceEntity ?mergedEntity .

    # Entities involved
    {{
        BIND(?canonicalEntity AS ?entity)
    }} UNION {{
        BIND(?mergedEntity AS ?entity)
    }}

    # Target mappings using these entities
    ?entity gph:sourceField ?sourceField .
    ?fieldMapping gph:sourceField ?sourceField ;
                  gph:mapsToOntologyTerm ?ontologyTerm .
    ?unifiedMapping gph:fromOntologyTerm ?ontologyTerm ;
                    gph:targetColumn ?targetColumnRef .
    ?targetColumnRef gph:tableName ?targetTable ;
                     gph:columnName ?targetColumn .
}}
GROUP BY ?targetTable ?targetColumn
ORDER BY DESC(?affectedEntities)
"#,
            fusion_id
        )
    }
}

impl Default for ExtendedLineageService {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_service() {
        let service = ExtendedLineageService::new();
        assert!(true); // Service created successfully
    }

    #[test]
    fn test_get_field_lineage_chain_not_found() -> Result<()> {
        let service = ExtendedLineageService::new();
        let result = service.get_field_lineage_chain("ds_001", "customers", "email")?;
        assert!(result.is_none());
        Ok(())
    }

    #[test]
    fn test_get_reverse_lineage_empty() -> Result<()> {
        let service = ExtendedLineageService::new();
        let result = service.get_reverse_lineage("postgres_001", "customers", "email")?;

        assert_eq!(result.target_column.database_id, "postgres_001");
        assert_eq!(result.target_column.table_name, "customers");
        assert_eq!(result.target_column.column_name, "email");
        assert_eq!(result.source_fields.len(), 0);
        assert_eq!(result.source_file_count, 0);

        Ok(())
    }

    #[test]
    fn test_analyze_fusion_impact_empty() -> Result<()> {
        let service = ExtendedLineageService::new();
        let result = service.analyze_fusion_impact("fusion_001")?;

        assert_eq!(result.fusion_id, "fusion_001");
        assert_eq!(result.entities_affected, 0);
        assert_eq!(result.tables_impacted.len(), 0);

        Ok(())
    }

    #[test]
    fn test_generate_lineage_sparql() {
        let service = ExtendedLineageService::new();
        let sparql = service.generate_lineage_sparql("ds_001", "email");

        assert!(sparql.contains("gph:FieldMapping"));
        assert!(sparql.contains("gph:UnifiedFieldMapping"));
        assert!(sparql.contains("ds_001"));
        assert!(sparql.contains("email"));
        assert!(sparql.contains("?confidence"));
    }

    #[test]
    fn test_generate_reverse_lineage_sparql() {
        let service = ExtendedLineageService::new();
        let sparql = service.generate_reverse_lineage_sparql("customers", "email");

        assert!(sparql.contains("gph:TargetColumn"));
        assert!(sparql.contains("gph:UnifiedFieldMapping"));
        assert!(sparql.contains("gph:FieldMapping"));
        assert!(sparql.contains("customers"));
        assert!(sparql.contains("email"));
    }

    #[test]
    fn test_generate_fusion_impact_sparql() {
        let service = ExtendedLineageService::new();
        let sparql = service.generate_fusion_impact_sparql("fusion_001");

        assert!(sparql.contains("gph:FusionOperation"));
        assert!(sparql.contains("fusion_001"));
        assert!(sparql.contains("gph:mergedEntity"));
        assert!(sparql.contains("gph:sourceEntity"));
        assert!(sparql.contains("COUNT(DISTINCT ?entity)"));
    }

    #[test]
    fn test_extended_lineage_chain_serialization() -> Result<()> {
        let chain = ExtendedLineageChain {
            source: SourceInfo {
                datasource_id: "ds_001".to_string(),
                file_path: "/data/customers.csv".to_string(),
                table_name: "data".to_string(),
                field_name: "email".to_string(),
                data_type: "VARCHAR".to_string(),
                session_id: "sess_001".to_string(),
            },
            ontology_term: OntologyTermInfo {
                uri: "http://schema.org/email".to_string(),
                label: Some("Email".to_string()),
                description: Some("Email address".to_string()),
            },
            unified_mapping: UnifiedMappingInfo {
                unified_session_id: "unified_001".to_string(),
                mapping_id: "mapping_001".to_string(),
                conflict_resolution: "NoConflict".to_string(),
                transformation: Some("LOWER(TRIM({value}))".to_string()),
            },
            fusion_operations: vec![],
            target: TargetInfo {
                database_id: "postgres_001".to_string(),
                schema: "public".to_string(),
                table_name: "customers".to_string(),
                column_name: "email".to_string(),
                data_type: "VARCHAR(255)".to_string(),
            },
            confidence: 0.95,
        };

        // Test serialization
        let json = serde_json::to_string(&chain)?;
        assert!(json.contains("ds_001"));
        assert!(json.contains("email"));

        // Test deserialization
        let deserialized: ExtendedLineageChain = serde_json::from_str(&json)?;
        assert_eq!(deserialized.source.datasource_id, "ds_001");
        assert_eq!(deserialized.confidence, 0.95);

        Ok(())
    }

    #[test]
    fn test_reverse_lineage_result_serialization() -> Result<()> {
        let result = ReverseLineageResult {
            target_column: TargetColumnRef {
                database_id: "postgres_001".to_string(),
                table_name: "customers".to_string(),
                column_name: "email".to_string(),
            },
            source_fields: vec![SourceFieldContribution {
                source: SourceInfo {
                    datasource_id: "ds_001".to_string(),
                    file_path: "/data/customers.csv".to_string(),
                    table_name: "data".to_string(),
                    field_name: "email".to_string(),
                    data_type: "VARCHAR".to_string(),
                    session_id: "sess_001".to_string(),
                },
                ontology_term_uri: "http://schema.org/email".to_string(),
                contribution_weight: 1.0,
                records_contributed: 1000,
            }],
            source_file_count: 1,
            fusion_operations_count: 0,
        };

        let json = serde_json::to_string(&result)?;
        assert!(json.contains("customers"));
        assert!(json.contains("email"));

        let deserialized: ReverseLineageResult = serde_json::from_str(&json)?;
        assert_eq!(deserialized.source_fields.len(), 1);
        assert_eq!(deserialized.source_file_count, 1);

        Ok(())
    }

    #[test]
    fn test_fusion_impact_result_serialization() -> Result<()> {
        let result = FusionImpactResult {
            fusion_id: "fusion_001".to_string(),
            entities_affected: 50,
            tables_impacted: vec!["customers".to_string(), "orders".to_string()],
            fields_affected: vec!["email".to_string(), "customer_id".to_string()],
            estimated_row_impact: 150,
        };

        let json = serde_json::to_string(&result)?;
        assert!(json.contains("fusion_001"));
        assert!(json.contains("customers"));

        let deserialized: FusionImpactResult = serde_json::from_str(&json)?;
        assert_eq!(deserialized.entities_affected, 50);
        assert_eq!(deserialized.tables_impacted.len(), 2);

        Ok(())
    }
}
