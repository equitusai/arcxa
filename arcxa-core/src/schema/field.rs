//! Unified Field Definition
//!
//! Represents a field/column that can come from any datasource.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::profile::FieldProfile;
use super::types::UniversalDataType;

/// Unified field/column definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifiedField {
    /// Field name
    pub name: String,

    /// Data type
    pub data_type: UniversalDataType,

    /// Whether this field can contain null values
    pub nullable: bool,

    /// Position in the schema (0-indexed)
    pub position: usize,

    /// Field constraints
    pub constraints: FieldConstraints,

    /// Profile information (if available)
    pub profile: Option<FieldProfile>,

    /// Semantic information
    pub semantic: SemanticInfo,

    /// Reference to the source of this field
    pub source_ref: String,

    /// Additional metadata
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Field constraints
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FieldConstraints {
    /// Is this a primary key?
    pub primary_key: bool,

    /// Is this field unique?
    pub unique: bool,

    /// Foreign key reference
    pub foreign_key: Option<ForeignKeyRef>,

    /// Default value expression
    pub default_value: Option<String>,

    /// Check constraint expression
    pub check_constraint: Option<String>,

    /// Not null constraint (redundant with nullable but explicit)
    pub not_null: bool,
}

/// Foreign key reference
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForeignKeyRef {
    /// Referenced schema/database
    pub schema: String,

    /// Referenced table
    pub table: String,

    /// Referenced column
    pub column: String,

    /// ON DELETE action
    pub on_delete: Option<ReferentialAction>,

    /// ON UPDATE action
    pub on_update: Option<ReferentialAction>,
}

/// Referential actions for foreign keys
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ReferentialAction {
    Cascade,
    SetNull,
    SetDefault,
    Restrict,
    NoAction,
}

/// Semantic information about a field
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SemanticInfo {
    /// Semantic type (e.g., Email, Phone, SSN)
    pub semantic_type: Option<SemanticType>,

    /// Data sensitivity level
    pub sensitivity: Option<SensitivityLevel>,

    /// Business glossary term
    pub business_term: Option<String>,

    /// Data classification tags
    pub tags: Vec<String>,

    /// Last time semantic info was updated
    pub last_classified: Option<DateTime<Utc>>,
}

/// Semantic types for fields
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum SemanticType {
    // Personal Information
    Email,
    PhoneNumber,
    SocialSecurityNumber,
    CreditCardNumber,
    BankAccountNumber,
    DriversLicense,
    PassportNumber,

    // Name Components
    FirstName,
    LastName,
    FullName,
    MiddleName,

    // Address Components
    StreetAddress,
    City,
    State,
    PostalCode,
    Country,
    FullAddress,

    // Geographic
    Latitude,
    Longitude,
    GeoPoint,

    // Temporal
    BirthDate,
    Age,
    Year,
    Month,
    DayOfWeek,

    // Identifiers
    UUID,
    SKU,
    ProductCode,
    OrderNumber,
    InvoiceNumber,
    CustomerId,

    // Financial
    Currency,
    Price,
    Amount,
    Percentage,
    TaxRate,

    // Network
    IPAddress,
    MACAddress,
    URL,
    Domain,

    // Other
    Gender,
    Title,
    CompanyName,
    JobTitle,
    Department,
    Custom(String),
}

/// Data sensitivity levels
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, PartialOrd)]
pub enum SensitivityLevel {
    Public,
    Internal,
    Confidential,
    Restricted,
    TopSecret,
}

impl UnifiedField {
    /// Create a new unified field
    pub fn new(name: String, data_type: UniversalDataType) -> Self {
        Self {
            name,
            data_type,
            nullable: true,
            position: 0,
            constraints: FieldConstraints::default(),
            profile: None,
            semantic: SemanticInfo::default(),
            source_ref: String::new(),
            metadata: HashMap::new(),
        }
    }

    /// Builder method to set nullable
    pub fn with_nullable(mut self, nullable: bool) -> Self {
        self.nullable = nullable;
        self.constraints.not_null = !nullable;
        self
    }

    /// Builder method to set position
    pub fn with_position(mut self, position: usize) -> Self {
        self.position = position;
        self
    }

    /// Builder method to mark as primary key
    pub fn with_primary_key(mut self, is_pk: bool) -> Self {
        self.constraints.primary_key = is_pk;
        if is_pk {
            self.nullable = false;
            self.constraints.not_null = true;
            self.constraints.unique = true;
        }
        self
    }

    /// Builder method to set foreign key
    pub fn with_foreign_key(mut self, fk: ForeignKeyRef) -> Self {
        self.constraints.foreign_key = Some(fk);
        self
    }

    /// Builder method to set semantic type
    pub fn with_semantic_type(mut self, semantic_type: SemanticType) -> Self {
        self.semantic.semantic_type = Some(semantic_type);
        self
    }

    /// Builder method to set sensitivity
    pub fn with_sensitivity(mut self, sensitivity: SensitivityLevel) -> Self {
        self.semantic.sensitivity = Some(sensitivity);
        self
    }

    /// Check if this field is personally identifiable information
    pub fn is_pii(&self) -> bool {
        matches!(
            self.semantic.semantic_type,
            Some(SemanticType::Email)
                | Some(SemanticType::PhoneNumber)
                | Some(SemanticType::SocialSecurityNumber)
                | Some(SemanticType::CreditCardNumber)
                | Some(SemanticType::BankAccountNumber)
                | Some(SemanticType::DriversLicense)
                | Some(SemanticType::PassportNumber)
        ) || matches!(
            self.semantic.sensitivity,
            Some(SensitivityLevel::Restricted) | Some(SensitivityLevel::TopSecret)
        )
    }

    /// Check if this field is an identifier
    pub fn is_identifier(&self) -> bool {
        self.constraints.primary_key
            || matches!(
                self.semantic.semantic_type,
                Some(SemanticType::UUID)
                    | Some(SemanticType::CustomerId)
                    | Some(SemanticType::OrderNumber)
                    | Some(SemanticType::InvoiceNumber)
            )
    }

    /// Get a display name for the field
    pub fn display_name(&self) -> String {
        if let Some(term) = &self.semantic.business_term {
            term.clone()
        } else {
            // Convert snake_case to Title Case
            self.name
                .split('_')
                .map(|word| {
                    let mut chars = word.chars();
                    match chars.next() {
                        None => String::new(),
                        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                    }
                })
                .collect::<Vec<_>>()
                .join(" ")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_field_creation() {
        let field = UnifiedField::new(
            "customer_id".to_string(),
            UniversalDataType::Integer { bits: Some(64) },
        )
        .with_primary_key(true)
        .with_semantic_type(SemanticType::CustomerId);

        assert_eq!(field.name, "customer_id");
        assert!(!field.nullable);
        assert!(field.constraints.primary_key);
        assert!(field.constraints.unique);
        assert!(field.is_identifier());
        assert_eq!(field.display_name(), "Customer Id");
    }

    #[test]
    fn test_pii_detection() {
        let email_field = UnifiedField::new(
            "email".to_string(),
            UniversalDataType::String {
                max_length: Some(255),
            },
        )
        .with_semantic_type(SemanticType::Email);

        assert!(email_field.is_pii());

        let regular_field = UnifiedField::new(
            "product_name".to_string(),
            UniversalDataType::String { max_length: None },
        );

        assert!(!regular_field.is_pii());

        let sensitive_field = UnifiedField::new(
            "secret_data".to_string(),
            UniversalDataType::Binary { max_length: None },
        )
        .with_sensitivity(SensitivityLevel::TopSecret);

        assert!(sensitive_field.is_pii());
    }

    #[test]
    fn test_foreign_key() {
        let field = UnifiedField::new(
            "order_id".to_string(),
            UniversalDataType::Integer { bits: Some(64) },
        )
        .with_foreign_key(ForeignKeyRef {
            schema: "public".to_string(),
            table: "orders".to_string(),
            column: "id".to_string(),
            on_delete: Some(ReferentialAction::Cascade),
            on_update: Some(ReferentialAction::Restrict),
        });

        assert!(field.constraints.foreign_key.is_some());
        let fk = field.constraints.foreign_key.unwrap();
        assert_eq!(fk.table, "orders");
        assert_eq!(fk.column, "id");
    }
}
