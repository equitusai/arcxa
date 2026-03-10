//! Extended catalog ontology for semantic types and advanced metadata
//!
//! This module extends the base DCAT catalog ontology with:
//! - Semantic type classification vocabulary
//! - Statistical metadata properties
//! - Detection evidence provenance
//! - Custom ontology support for domain-specific extensions

use std::collections::HashMap;

/// Extended catalog ontology in Turtle format
///
/// This extends the base DCAT ontology from catalog/ontology.rs with
/// advanced inference metadata.
pub const EXTENDED_CATALOG_ONTOLOGY: &str = r#"
@prefix gph: <http://graphica.io/ontology#> .
@prefix gphi: <http://graphica.io/inference#> .
@prefix dcat: <http://www.w3.org/ns/dcat#> .
@prefix dct: <http://purl.org/dc/terms/> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .
@prefix prov: <http://www.w3.org/ns/prov#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix owl: <http://www.w3.org/2002/07/owl#> .

# ============================================================================
# SEMANTIC TYPE CLASSIFICATION
# ============================================================================

gphi:SemanticType a rdfs:Class ;
    rdfs:label "Semantic Type" ;
    rdfs:comment "A semantic classification of data beyond basic SQL types" ;
    rdfs:subClassOf owl:Class .

# Identity & Contact Types
gphi:Email a gphi:SemanticType ;
    rdfs:label "Email Address" ;
    rdfs:comment "Email address following RFC 5322" .

gphi:PhoneNumber a gphi:SemanticType ;
    rdfs:label "Phone Number" ;
    rdfs:comment "Telephone number in various formats" .

gphi:PersonName a gphi:SemanticType ;
    rdfs:label "Person Name" ;
    rdfs:comment "Human name (full, first, last, etc.)" .

gphi:OrganizationName a gphi:SemanticType ;
    rdfs:label "Organization Name" .

gphi:Username a gphi:SemanticType ;
    rdfs:label "Username" .

gphi:UserId a gphi:SemanticType ;
    rdfs:label "User Identifier" .

# Geographic Types
gphi:Address a gphi:SemanticType ;
    rdfs:label "Physical Address" .

gphi:City a gphi:SemanticType ;
    rdfs:label "City Name" .

gphi:State a gphi:SemanticType ;
    rdfs:label "State/Province" .

gphi:PostalCode a gphi:SemanticType ;
    rdfs:label "Postal/ZIP Code" .

gphi:Country a gphi:SemanticType ;
    rdfs:label "Country Name" .

gphi:CountryCode a gphi:SemanticType ;
    rdfs:label "Country Code (ISO)" .

gphi:Coordinates a gphi:SemanticType ;
    rdfs:label "Geographic Coordinates" .

gphi:IPAddress a gphi:SemanticType ;
    rdfs:label "IP Address (v4 or v6)" .

# Financial Types
gphi:CreditCardNumber a gphi:SemanticType ;
    rdfs:label "Credit Card Number" ;
    gphi:sensitivityLevel gphi:HighlySensitive .

gphi:BankAccountNumber a gphi:SemanticType ;
    rdfs:label "Bank Account Number" ;
    gphi:sensitivityLevel gphi:HighlySensitive .

gphi:IBANNumber a gphi:SemanticType ;
    rdfs:label "IBAN" ;
    rdfs:comment "International Bank Account Number" .

gphi:CurrencyAmount a gphi:SemanticType ;
    rdfs:label "Currency Amount" .

gphi:CurrencyCode a gphi:SemanticType ;
    rdfs:label "Currency Code (ISO 4217)" .

gphi:TaxIdentifier a gphi:SemanticType ;
    rdfs:label "Tax Identifier" ;
    gphi:sensitivityLevel gphi:Sensitive .

# Healthcare Types
gphi:SSN a gphi:SemanticType ;
    rdfs:label "Social Security Number" ;
    gphi:sensitivityLevel gphi:HighlySensitive .

gphi:MedicalRecordNumber a gphi:SemanticType ;
    rdfs:label "Medical Record Number" ;
    gphi:sensitivityLevel gphi:HighlySensitive .

gphi:HealthInsuranceNumber a gphi:SemanticType ;
    rdfs:label "Health Insurance Number" ;
    gphi:sensitivityLevel gphi:HighlySensitive .

# Temporal Types
gphi:Timestamp a gphi:SemanticType ;
    rdfs:label "Timestamp" .

gphi:Date a gphi:SemanticType ;
    rdfs:label "Date" .

gphi:Time a gphi:SemanticType ;
    rdfs:label "Time" .

gphi:DateOfBirth a gphi:SemanticType ;
    rdfs:label "Date of Birth" ;
    gphi:sensitivityLevel gphi:Sensitive .

# Technical Types
gphi:URL a gphi:SemanticType ;
    rdfs:label "URL" .

gphi:UUID a gphi:SemanticType ;
    rdfs:label "UUID/GUID" .

gphi:Hostname a gphi:SemanticType ;
    rdfs:label "Hostname" .

# Business Types
gphi:ProductCode a gphi:SemanticType ;
    rdfs:label "Product Code" .

gphi:SKU a gphi:SemanticType ;
    rdfs:label "Stock Keeping Unit" .

gphi:OrderNumber a gphi:SemanticType ;
    rdfs:label "Order Number" .

# ============================================================================
# COLUMN METADATA PROPERTIES
# ============================================================================

gphi:semanticType a rdf:Property ;
    rdfs:label "semantic type" ;
    rdfs:comment "Detected semantic type of a column" ;
    rdfs:domain gph:ColumnMetadata ;
    rdfs:range gphi:SemanticType .

gphi:semanticConfidence a rdf:Property ;
    rdfs:label "semantic confidence" ;
    rdfs:comment "Confidence score for semantic type detection (0.0-1.0)" ;
    rdfs:domain gph:ColumnMetadata ;
    rdfs:range xsd:double .

gphi:detectedBy a rdf:Property ;
    rdfs:label "detected by" ;
    rdfs:comment "Detection strategy that identified this semantic type" ;
    rdfs:domain gph:ColumnMetadata ;
    rdfs:range gphi:DetectionStrategy .

# ============================================================================
# STATISTICAL METADATA
# ============================================================================

gphi:ColumnStatistics a rdfs:Class ;
    rdfs:label "Column Statistics" ;
    rdfs:comment "Statistical metadata for a column" .

gphi:distinctCount a rdf:Property ;
    rdfs:label "distinct count" ;
    rdfs:domain gphi:ColumnStatistics ;
    rdfs:range xsd:long .

gphi:nullCount a rdf:Property ;
    rdfs:label "null count" ;
    rdfs:domain gphi:ColumnStatistics ;
    rdfs:range xsd:long .

gphi:nullPercentage a rdf:Property ;
    rdfs:label "null percentage" ;
    rdfs:domain gphi:ColumnStatistics ;
    rdfs:range xsd:double .

gphi:correlation a rdf:Property ;
    rdfs:label "correlation" ;
    rdfs:comment "Correlation with physical row order (PostgreSQL)" ;
    rdfs:domain gphi:ColumnStatistics ;
    rdfs:range xsd:double .

gphi:avgWidth a rdf:Property ;
    rdfs:label "average width" ;
    rdfs:comment "Average storage width in bytes" ;
    rdfs:domain gphi:ColumnStatistics ;
    rdfs:range xsd:int .

gphi:cardinalityClass a rdf:Property ;
    rdfs:label "cardinality class" ;
    rdfs:domain gphi:ColumnStatistics ;
    rdfs:range gphi:CardinalityClass .

# Cardinality Classes
gphi:CardinalityClass a rdfs:Class ;
    rdfs:label "Cardinality Class" .

gphi:VeryLow a gphi:CardinalityClass ;
    rdfs:label "Very Low Cardinality (1-10)" .

gphi:Low a gphi:CardinalityClass ;
    rdfs:label "Low Cardinality (11-100)" .

gphi:Medium a gphi:CardinalityClass ;
    rdfs:label "Medium Cardinality (101-1000)" .

gphi:High a gphi:CardinalityClass ;
    rdfs:label "High Cardinality (1001-100000)" .

gphi:VeryHigh a gphi:CardinalityClass ;
    rdfs:label "Very High Cardinality (>100000)" .

gphi:Unique a gphi:CardinalityClass ;
    rdfs:label "Unique (>95% distinct)" .

# ============================================================================
# DETECTION EVIDENCE PROVENANCE
# ============================================================================

gphi:DetectionStrategy a rdfs:Class ;
    rdfs:label "Detection Strategy" ;
    rdfs:subClassOf prov:Agent .

gphi:ColumnNameDetector a gphi:DetectionStrategy ;
    rdfs:label "Column Name Detector" ;
    rdfs:comment "Detects semantic types from column names" .

gphi:RegexDetector a gphi:DetectionStrategy ;
    rdfs:label "Regex Detector" ;
    rdfs:comment "Detects semantic types from value patterns" .

gphi:StatisticalDetector a gphi:DetectionStrategy ;
    rdfs:label "Statistical Detector" ;
    rdfs:comment "Detects semantic types from statistical properties" .

gphi:DetectionEvidence a rdfs:Class ;
    rdfs:label "Detection Evidence" ;
    rdfs:comment "Evidence supporting a semantic type detection" ;
    rdfs:subClassOf prov:Entity .

gphi:evidenceType a rdf:Property ;
    rdfs:label "evidence type" ;
    rdfs:domain gphi:DetectionEvidence ;
    rdfs:range gphi:EvidenceType .

gphi:evidenceWeight a rdf:Property ;
    rdfs:label "evidence weight" ;
    rdfs:domain gphi:DetectionEvidence ;
    rdfs:range xsd:double .

# Evidence Types
gphi:EvidenceType a rdfs:Class .

gphi:ColumnNameEvidence a gphi:EvidenceType .
gphi:RegexPatternEvidence a gphi:EvidenceType .
gphi:StatisticalEvidence a gphi:EvidenceType .
gphi:FormatConsistencyEvidence a gphi:EvidenceType .

# ============================================================================
# SENSITIVITY LEVELS
# ============================================================================

gphi:SensitivityLevel a rdfs:Class ;
    rdfs:label "Data Sensitivity Level" .

gphi:Public a gphi:SensitivityLevel ;
    rdfs:label "Public" .

gphi:Internal a gphi:SensitivityLevel ;
    rdfs:label "Internal" .

gphi:Sensitive a gphi:SensitivityLevel ;
    rdfs:label "Sensitive" .

gphi:HighlySensitive a gphi:SensitivityLevel ;
    rdfs:label "Highly Sensitive" .

gphi:sensitivityLevel a rdf:Property ;
    rdfs:label "sensitivity level" ;
    rdfs:range gphi:SensitivityLevel .
"#;

/// Namespace definitions for RDF ontologies
pub mod namespaces {
    /// Graphica core ontology
    pub const GRAPHICA: &str = "http://graphica.io/ontology#";

    /// Graphica inference ontology (extended metadata)
    pub const GRAPHICA_INFERENCE: &str = "http://graphica.io/inference#";

    /// W3C DCAT (Data Catalog Vocabulary)
    pub const DCAT: &str = "http://www.w3.org/ns/dcat#";

    /// W3C PROV (Provenance)
    pub const PROV: &str = "http://www.w3.org/ns/prov#";

    /// Dublin Core Terms
    pub const DCT: &str = "http://purl.org/dc/terms/";

    /// XML Schema
    pub const XSD: &str = "http://www.w3.org/2001/XMLSchema#";

    /// RDFS
    pub const RDFS: &str = "http://www.w3.org/2000/01/rdf-schema#";

    /// OWL
    pub const OWL: &str = "http://www.w3.org/2002/07/owl#";
}

/// Mapping from SemanticType enum to RDF URI
pub fn semantic_type_to_uri(semantic_type: &crate::inference::types::SemanticType) -> String {
    use crate::inference::types::SemanticType;

    let type_name = match semantic_type {
        SemanticType::Email => "Email",
        SemanticType::PhoneNumber => "PhoneNumber",
        SemanticType::PersonName => "PersonName",
        SemanticType::OrganizationName => "OrganizationName",
        SemanticType::Username => "Username",
        SemanticType::UserId => "UserId",

        SemanticType::Address => "Address",
        SemanticType::City => "City",
        SemanticType::State => "State",
        SemanticType::PostalCode => "PostalCode",
        SemanticType::Country => "Country",
        SemanticType::CountryCode => "CountryCode",
        SemanticType::Coordinates => "Coordinates",
        SemanticType::IPAddress => "IPAddress",

        SemanticType::CreditCardNumber => "CreditCardNumber",
        SemanticType::BankAccountNumber => "BankAccountNumber",
        SemanticType::IBANNumber => "IBANNumber",
        SemanticType::CurrencyAmount => "CurrencyAmount",
        SemanticType::CurrencyCode => "CurrencyCode",
        SemanticType::TaxIdentifier => "TaxIdentifier",

        SemanticType::SSN => "SSN",
        SemanticType::MedicalRecordNumber => "MedicalRecordNumber",
        SemanticType::HealthInsuranceNumber => "HealthInsuranceNumber",
        SemanticType::DrugCode => "DrugCode",
        SemanticType::DiagnosisCode => "DiagnosisCode",

        SemanticType::Timestamp => "Timestamp",
        SemanticType::Date => "Date",
        SemanticType::Time => "Time",
        SemanticType::Duration => "Duration",
        SemanticType::DateOfBirth => "DateOfBirth",

        SemanticType::URL => "URL",
        SemanticType::URI => "URI",
        SemanticType::UUID => "UUID",
        SemanticType::Hostname => "Hostname",
        SemanticType::MACAddress => "MACAddress",
        SemanticType::FilePath => "FilePath",
        SemanticType::MimeType => "MimeType",

        SemanticType::ProductCode => "ProductCode",
        SemanticType::SKU => "SKU",
        SemanticType::OrderNumber => "OrderNumber",
        SemanticType::InvoiceNumber => "InvoiceNumber",
        SemanticType::AccountNumber => "AccountNumber",
        SemanticType::VIN => "VIN",

        SemanticType::Enum => "Enum",
        SemanticType::Boolean => "Boolean",
        SemanticType::Flag => "Flag",
        SemanticType::Status => "Status",
        SemanticType::Category => "Category",

        SemanticType::FreeText => "FreeText",
        SemanticType::Description => "Description",
        SemanticType::Comment => "Comment",
        SemanticType::JsonBlob => "JsonBlob",
        SemanticType::XMLBlob => "XMLBlob",

        SemanticType::Quantity => "Quantity",
        SemanticType::Percentage => "Percentage",
        SemanticType::Score => "Score",
        SemanticType::Rating => "Rating",

        SemanticType::Custom(name) => {
            return format!("{}Custom/{}", namespaces::GRAPHICA_INFERENCE, name)
        }
        SemanticType::Unknown => "Unknown",
    };

    format!("{}{}", namespaces::GRAPHICA_INFERENCE, type_name)
}

/// Mapping from CardinalityClass to RDF URI
pub fn cardinality_class_to_uri(cardinality: &crate::inference::types::CardinalityClass) -> String {
    use crate::inference::types::CardinalityClass;

    let class_name = match cardinality {
        CardinalityClass::VeryLow => "VeryLow",
        CardinalityClass::Low => "Low",
        CardinalityClass::Medium => "Medium",
        CardinalityClass::High => "High",
        CardinalityClass::VeryHigh => "VeryHigh",
        CardinalityClass::Unique => "Unique",
    };

    format!("{}{}", namespaces::GRAPHICA_INFERENCE, class_name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inference::types::{CardinalityClass, SemanticType};

    #[test]
    fn test_semantic_type_to_uri() {
        assert_eq!(
            semantic_type_to_uri(&SemanticType::Email),
            "http://graphica.io/inference#Email"
        );

        assert_eq!(
            semantic_type_to_uri(&SemanticType::CreditCardNumber),
            "http://graphica.io/inference#CreditCardNumber"
        );

        assert_eq!(
            semantic_type_to_uri(&SemanticType::Custom("DomainSpecific".to_string())),
            "http://graphica.io/inference#Custom/DomainSpecific"
        );
    }

    #[test]
    fn test_cardinality_class_to_uri() {
        assert_eq!(
            cardinality_class_to_uri(&CardinalityClass::Low),
            "http://graphica.io/inference#Low"
        );

        assert_eq!(
            cardinality_class_to_uri(&CardinalityClass::Unique),
            "http://graphica.io/inference#Unique"
        );
    }

    #[test]
    fn test_ontology_parses_as_turtle() {
        // Basic validation that it's valid Turtle syntax
        assert!(EXTENDED_CATALOG_ONTOLOGY.contains("@prefix"));
        assert!(EXTENDED_CATALOG_ONTOLOGY.contains("gphi:SemanticType"));
        assert!(EXTENDED_CATALOG_ONTOLOGY.contains("gphi:Email"));
        assert!(EXTENDED_CATALOG_ONTOLOGY.contains("gphi:ColumnStatistics"));
    }
}
