//! Data Source Catalog Ontology
//!
//! Defines the RDF ontology for data sources using W3C DCAT as the base
//! with Graphica-specific extensions for governance and lineage.

use std::fmt;

/// Namespace constants for RDF ontologies
pub mod namespaces {
    pub const DCAT: &str = "http://www.w3.org/ns/dcat#";
    pub const DCT: &str = "http://purl.org/dc/terms/";
    pub const GRAPHICA: &str = "http://graphica.io/ontology#";
    pub const PROV: &str = "http://www.w3.org/ns/prov#";
    pub const XSD: &str = "http://www.w3.org/2001/XMLSchema#";
    pub const RDF: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#";
    pub const RDFS: &str = "http://www.w3.org/2000/01/rdf-schema#";
}

/// Data Source Catalog Ontology in Turtle format
pub const CATALOG_ONTOLOGY: &str = r#"
@prefix gph: <http://graphica.io/ontology#> .
@prefix dcat: <http://www.w3.org/ns/dcat#> .
@prefix dct: <http://purl.org/dc/terms/> .
@prefix prov: <http://www.w3.org/ns/prov#> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix sh: <http://www.w3.org/ns/shacl#> .

# =============================================================================
# Core Classes
# =============================================================================

gph:DataSourceCatalog a rdfs:Class ;
    rdfs:subClassOf dcat:Catalog ;
    rdfs:label "Data Source Catalog" ;
    rdfs:comment "A curated collection of data source metadata" .

gph:DataSource a rdfs:Class ;
    rdfs:subClassOf dcat:Dataset , prov:Entity ;
    rdfs:label "Data Source" ;
    rdfs:comment "An external data source (database, file, API) registered in the catalog" .

gph:PostgreSQLDataSource a rdfs:Class ;
    rdfs:subClassOf gph:DataSource ;
    rdfs:label "PostgreSQL Data Source" .

gph:OracleDataSource a rdfs:Class ;
    rdfs:subClassOf gph:DataSource ;
    rdfs:label "Oracle Data Source" .

gph:DB2DataSource a rdfs:Class ;
    rdfs:subClassOf gph:DataSource ;
    rdfs:label "IBM DB2 Data Source" .

gph:SAPHANADataSource a rdfs:Class ;
    rdfs:subClassOf gph:DataSource ;
    rdfs:label "SAP HANA Data Source" .

gph:SnowflakeDataSource a rdfs:Class ;
    rdfs:subClassOf gph:DataSource ;
    rdfs:label "Snowflake Data Source" .

gph:DatabricksDataSource a rdfs:Class ;
    rdfs:subClassOf gph:DataSource ;
    rdfs:label "Databricks Data Source" .

gph:S3ParquetDataSource a rdfs:Class ;
    rdfs:subClassOf gph:DataSource ;
    rdfs:label "S3 Parquet Data Source" .

gph:CsvFileDataSource a rdfs:Class ;
    rdfs:subClassOf gph:DataSource ;
    rdfs:label "CSV File Data Source" .

gph:DataDistribution a rdfs:Class ;
    rdfs:subClassOf dcat:Distribution ;
    rdfs:label "Data Distribution" ;
    rdfs:comment "A specific table, view, or file within a data source" .

gph:DataSourceSchema a rdfs:Class ;
    rdfs:label "Data Source Schema" ;
    rdfs:comment "Schema definition for a data source (columns, types, constraints)" .

# =============================================================================
# Core Properties
# =============================================================================

gph:sourceType a rdf:Property ;
    rdfs:domain gph:DataSource ;
    rdfs:range xsd:string ;
    rdfs:label "source type" ;
    rdfs:comment "Type of data source (PostgreSQL, Oracle, DB2, etc.)" .

gph:connectionDetails a rdf:Property ;
    rdfs:domain gph:DataSource ;
    rdfs:label "connection details" ;
    rdfs:comment "Connection configuration (non-sensitive)" .

gph:credentialsRef a rdf:Property ;
    rdfs:domain gph:DataSource ;
    rdfs:range xsd:anyURI ;
    rdfs:label "credentials reference" ;
    rdfs:comment "URI reference to secret store (vault://..., aws://..., etc.)" .

gph:encryptionEnabled a rdf:Property ;
    rdfs:domain gph:DataSource ;
    rdfs:range xsd:boolean ;
    rdfs:label "encryption enabled" ;
    rdfs:comment "Whether TLS/SSL encryption is enabled for connections" .

gph:lastSyncedAt a rdf:Property ;
    rdfs:domain gph:DataSource ;
    rdfs:range xsd:dateTime ;
    rdfs:label "last synced at" ;
    rdfs:comment "Timestamp of last successful sync" .

gph:cdcLogPosition a rdf:Property ;
    rdfs:domain gph:DataSource ;
    rdfs:range xsd:string ;
    rdfs:label "CDC log position" ;
    rdfs:comment "Change data capture position (LSN, SCN, offset, etc.)" .

gph:schema a rdf:Property ;
    rdfs:domain gph:DataSource ;
    rdfs:range gph:DataSourceSchema ;
    rdfs:label "schema" ;
    rdfs:comment "Link to schema definition" .

gph:expectedRowCount a rdf:Property ;
    rdfs:domain gph:DataDistribution ;
    rdfs:range xsd:long ;
    rdfs:label "expected row count" ;
    rdfs:comment "Expected number of rows (for validation)" .

gph:dataFreshness a rdf:Property ;
    rdfs:domain gph:DataSource ;
    rdfs:range xsd:duration ;
    rdfs:label "data freshness" ;
    rdfs:comment "Expected data freshness (ISO 8601 duration)" .

gph:tags a rdf:Property ;
    rdfs:domain gph:DataSource ;
    rdfs:range xsd:string ;
    rdfs:label "tags" ;
    rdfs:comment "Tags for categorization (production, staging, etc.)" .

# =============================================================================
# Database-Specific Properties
# =============================================================================

gph:host a rdf:Property ;
    rdfs:domain gph:DataSource ;
    rdfs:range xsd:string ;
    rdfs:label "host" ;
    rdfs:comment "Database host or endpoint" .

gph:port a rdf:Property ;
    rdfs:domain gph:DataSource ;
    rdfs:range xsd:int ;
    rdfs:label "port" ;
    rdfs:comment "Database port number" .

gph:database a rdf:Property ;
    rdfs:domain gph:DataSource ;
    rdfs:range xsd:string ;
    rdfs:label "database" ;
    rdfs:comment "Database or service name" .

gph:schemaName a rdf:Property ;
    rdfs:domain gph:DataDistribution ;
    rdfs:range xsd:string ;
    rdfs:label "schema name" ;
    rdfs:comment "Database schema name" .

gph:tableName a rdf:Property ;
    rdfs:domain gph:DataDistribution ;
    rdfs:range xsd:string ;
    rdfs:label "table name" ;
    rdfs:comment "Table or view name" .

gph:viewName a rdf:Property ;
    rdfs:domain gph:DataDistribution ;
    rdfs:range xsd:string ;
    rdfs:label "view name" ;
    rdfs:comment "View name (for materialized or regular views)" .

# Oracle-specific
gph:oracleServiceName a rdf:Property ;
    rdfs:domain gph:OracleDataSource ;
    rdfs:range xsd:string ;
    rdfs:label "Oracle service name" ;
    rdfs:comment "Oracle TNS service name" .

gph:oracleSID a rdf:Property ;
    rdfs:domain gph:OracleDataSource ;
    rdfs:range xsd:string ;
    rdfs:label "Oracle SID" ;
    rdfs:comment "Oracle system identifier" .

# Snowflake-specific
gph:snowflakeAccount a rdf:Property ;
    rdfs:domain gph:SnowflakeDataSource ;
    rdfs:range xsd:string ;
    rdfs:label "Snowflake account" ;
    rdfs:comment "Snowflake account identifier" .

gph:snowflakeWarehouse a rdf:Property ;
    rdfs:domain gph:SnowflakeDataSource ;
    rdfs:range xsd:string ;
    rdfs:label "Snowflake warehouse" ;
    rdfs:comment "Snowflake virtual warehouse name" .

gph:snowflakeRole a rdf:Property ;
    rdfs:domain gph:SnowflakeDataSource ;
    rdfs:range xsd:string ;
    rdfs:label "Snowflake role" ;
    rdfs:comment "Snowflake role for access control" .

# S3-specific
gph:s3Bucket a rdf:Property ;
    rdfs:domain gph:S3ParquetDataSource ;
    rdfs:range xsd:string ;
    rdfs:label "S3 bucket" ;
    rdfs:comment "S3 bucket name" .

gph:s3PathPrefix a rdf:Property ;
    rdfs:domain gph:S3ParquetDataSource ;
    rdfs:range xsd:string ;
    rdfs:label "S3 path prefix" ;
    rdfs:comment "S3 object key prefix" .

gph:s3Region a rdf:Property ;
    rdfs:domain gph:S3ParquetDataSource ;
    rdfs:range xsd:string ;
    rdfs:label "S3 region" ;
    rdfs:comment "AWS region for S3 bucket" .

gph:partitionColumns a rdf:Property ;
    rdfs:domain gph:S3ParquetDataSource ;
    rdfs:range xsd:string ;
    rdfs:label "partition columns" ;
    rdfs:comment "Columns used for partitioning (comma-separated)" .

# CSV-specific
gph:filePath a rdf:Property ;
    rdfs:domain gph:CsvFileDataSource ;
    rdfs:range xsd:string ;
    rdfs:label "file path" ;
    rdfs:comment "File system path or URL" .

gph:delimiter a rdf:Property ;
    rdfs:domain gph:CsvFileDataSource ;
    rdfs:range xsd:string ;
    rdfs:label "delimiter" ;
    rdfs:comment "Field delimiter character" .

gph:hasHeader a rdf:Property ;
    rdfs:domain gph:CsvFileDataSource ;
    rdfs:range xsd:boolean ;
    rdfs:label "has header" ;
    rdfs:comment "Whether the CSV file has a header row" .

# =============================================================================
# Lineage Integration
# =============================================================================

gph:sourceSystem a rdf:Property ;
    rdfs:domain gph:DataRef ;
    rdfs:range gph:DataSource ;
    rdfs:label "source system" ;
    rdfs:comment "Link from lineage DataRef to catalog DataSource" .

# =============================================================================
# SHACL Shapes for Validation
# =============================================================================

gph:DataSourceShape a sh:NodeShape ;
    sh:targetClass gph:DataSource ;
    sh:property [
        sh:path dct:title ;
        sh:minCount 1 ;
        sh:datatype xsd:string ;
        sh:message "Data source must have a title"
    ] ;
    sh:property [
        sh:path gph:sourceType ;
        sh:minCount 1 ;
        sh:datatype xsd:string ;
        sh:message "Data source must have a source type"
    ] ;
    sh:property [
        sh:path gph:credentialsRef ;
        sh:minCount 1 ;
        sh:nodeKind sh:IRI ;
        sh:message "Data source must have a credentials reference"
    ] ;
    sh:property [
        sh:path gph:encryptionEnabled ;
        sh:minCount 1 ;
        sh:datatype xsd:boolean ;
        sh:hasValue true ;
        sh:message "Encryption must be enabled for all data sources"
    ] .

gph:PostgreSQLShape a sh:NodeShape ;
    sh:targetClass gph:PostgreSQLDataSource ;
    sh:property [
        sh:path gph:host ;
        sh:minCount 1 ;
        sh:message "PostgreSQL source must have a host"
    ] ;
    sh:property [
        sh:path gph:database ;
        sh:minCount 1 ;
        sh:message "PostgreSQL source must have a database"
    ] .

gph:OracleShape a sh:NodeShape ;
    sh:targetClass gph:OracleDataSource ;
    sh:property [
        sh:path gph:host ;
        sh:minCount 1 ;
        sh:message "Oracle source must have a host"
    ] .

gph:SnowflakeShape a sh:NodeShape ;
    sh:targetClass gph:SnowflakeDataSource ;
    sh:property [
        sh:path gph:snowflakeAccount ;
        sh:minCount 1 ;
        sh:message "Snowflake source must have an account identifier"
    ] ;
    sh:property [
        sh:path gph:snowflakeWarehouse ;
        sh:minCount 1 ;
        sh:message "Snowflake source must have a warehouse"
    ] .
"#;

/// Data source types supported by Graphica
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum DataSourceType {
    PostgreSQL,
    Oracle,
    DB2,
    SAPHANA,
    Snowflake,
    Databricks,
    S3Parquet,
    CsvFile,
}

impl DataSourceType {
    /// Get RDF class URI for this source type
    pub fn rdf_class(&self) -> String {
        match self {
            Self::PostgreSQL => format!("{}PostgreSQLDataSource", namespaces::GRAPHICA),
            Self::Oracle => format!("{}OracleDataSource", namespaces::GRAPHICA),
            Self::DB2 => format!("{}DB2DataSource", namespaces::GRAPHICA),
            Self::SAPHANA => format!("{}SAPHANADataSource", namespaces::GRAPHICA),
            Self::Snowflake => format!("{}SnowflakeDataSource", namespaces::GRAPHICA),
            Self::Databricks => format!("{}DatabricksDataSource", namespaces::GRAPHICA),
            Self::S3Parquet => format!("{}S3ParquetDataSource", namespaces::GRAPHICA),
            Self::CsvFile => format!("{}CsvFileDataSource", namespaces::GRAPHICA),
        }
    }

    /// Get human-readable label
    pub fn label(&self) -> &'static str {
        match self {
            Self::PostgreSQL => "PostgreSQL",
            Self::Oracle => "Oracle Database",
            Self::DB2 => "IBM DB2",
            Self::SAPHANA => "SAP HANA",
            Self::Snowflake => "Snowflake",
            Self::Databricks => "Databricks",
            Self::S3Parquet => "S3 Parquet",
            Self::CsvFile => "CSV File",
        }
    }
}

impl fmt::Display for DataSourceType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.label())
    }
}

impl std::str::FromStr for DataSourceType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "postgresql" | "postgres" => Ok(Self::PostgreSQL),
            "oracle" => Ok(Self::Oracle),
            "db2" => Ok(Self::DB2),
            "saphana" | "sap_hana" | "hana" => Ok(Self::SAPHANA),
            "snowflake" => Ok(Self::Snowflake),
            "databricks" => Ok(Self::Databricks),
            "s3parquet" | "s3_parquet" | "parquet" => Ok(Self::S3Parquet),
            "csv" | "csvfile" | "csv_file" => Ok(Self::CsvFile),
            _ => Err(format!("Unknown data source type: {}", s)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_data_source_type_from_str() {
        assert_eq!(
            "postgresql".parse::<DataSourceType>().unwrap(),
            DataSourceType::PostgreSQL
        );
        assert_eq!(
            "Oracle".parse::<DataSourceType>().unwrap(),
            DataSourceType::Oracle
        );
        assert_eq!(
            "DB2".parse::<DataSourceType>().unwrap(),
            DataSourceType::DB2
        );
        assert_eq!(
            "saphana".parse::<DataSourceType>().unwrap(),
            DataSourceType::SAPHANA
        );
        assert_eq!(
            "Snowflake".parse::<DataSourceType>().unwrap(),
            DataSourceType::Snowflake
        );
        assert_eq!(
            "databricks".parse::<DataSourceType>().unwrap(),
            DataSourceType::Databricks
        );
        assert_eq!(
            "s3parquet".parse::<DataSourceType>().unwrap(),
            DataSourceType::S3Parquet
        );
        assert_eq!(
            "csv".parse::<DataSourceType>().unwrap(),
            DataSourceType::CsvFile
        );
    }

    #[test]
    fn test_data_source_type_rdf_class() {
        assert_eq!(
            DataSourceType::PostgreSQL.rdf_class(),
            "http://graphica.io/ontology#PostgreSQLDataSource"
        );
        assert_eq!(
            DataSourceType::Snowflake.rdf_class(),
            "http://graphica.io/ontology#SnowflakeDataSource"
        );
        assert_eq!(
            DataSourceType::Databricks.rdf_class(),
            "http://graphica.io/ontology#DatabricksDataSource"
        );
    }

    #[test]
    fn test_data_source_type_display() {
        assert_eq!(DataSourceType::PostgreSQL.to_string(), "PostgreSQL");
        assert_eq!(DataSourceType::SAPHANA.to_string(), "SAP HANA");
        assert_eq!(DataSourceType::Databricks.to_string(), "Databricks");
    }

    #[test]
    fn test_ontology_contains_all_types() {
        // Verify ontology defines all source types
        assert!(CATALOG_ONTOLOGY.contains("gph:PostgreSQLDataSource"));
        assert!(CATALOG_ONTOLOGY.contains("gph:OracleDataSource"));
        assert!(CATALOG_ONTOLOGY.contains("gph:DB2DataSource"));
        assert!(CATALOG_ONTOLOGY.contains("gph:SAPHANADataSource"));
        assert!(CATALOG_ONTOLOGY.contains("gph:SnowflakeDataSource"));
        assert!(CATALOG_ONTOLOGY.contains("gph:S3ParquetDataSource"));
        assert!(CATALOG_ONTOLOGY.contains("gph:CsvFileDataSource"));
    }

    #[test]
    fn test_ontology_has_shacl_shapes() {
        // Verify SHACL validation shapes are defined
        assert!(CATALOG_ONTOLOGY.contains("gph:DataSourceShape"));
        assert!(CATALOG_ONTOLOGY.contains("gph:PostgreSQLShape"));
        assert!(CATALOG_ONTOLOGY.contains("gph:OracleShape"));
        assert!(CATALOG_ONTOLOGY.contains("gph:SnowflakeShape"));
        assert!(CATALOG_ONTOLOGY.contains("sh:targetClass"));
    }

    #[test]
    fn test_ontology_has_lineage_integration() {
        // Verify lineage integration properties
        assert!(CATALOG_ONTOLOGY.contains("gph:sourceSystem"));
        assert!(CATALOG_ONTOLOGY.contains("prov:Entity"));
    }
}
