// graphica-core/src/inference/types.rs
//! Rich metadata types for multi-tier schema inference.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use utoipa::ToSchema;

/// Inference depth tier
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum InferenceTier {
    /// Basic structure only (fast)
    Basic = 0,
    /// Include relationships
    Relationships = 1,
    /// Include statistics
    Statistics = 2,
    /// Include governance metadata
    Governance = 3,
    /// Deep value-level profiling
    Profiling = 4,
}

impl InferenceTier {
    pub fn all_up_to(self) -> Vec<Self> {
        use InferenceTier::*;
        match self {
            Basic => vec![Basic],
            Relationships => vec![Basic, Relationships],
            Statistics => vec![Basic, Relationships, Statistics],
            Governance => vec![Basic, Relationships, Statistics, Governance],
            Profiling => vec![Basic, Relationships, Statistics, Governance, Profiling],
        }
    }
}

/// Complete schema metadata envelope
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaMetadata {
    pub source_id: String,
    pub schema_name: String,
    pub inferred_at: DateTime<Utc>,
    pub tier_completed: InferenceTier,
    pub tables: Vec<TableMetadata>,
    pub lineage_id: String, // PROV activity URI
}

/// Tier 0: Basic table structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableMetadata {
    pub name: String,
    pub schema: String,
    pub table_type: TableType,
    pub columns: Vec<ColumnMetadata>,
    pub estimated_rows: Option<u64>,

    // Tier 1+
    pub relationships: Option<RelationshipMetadata>,
    pub indexes: Vec<IndexMetadata>,
    pub constraints: Vec<ConstraintMetadata>,

    // Tier 2+
    pub statistics: Option<TableStatistics>,
    pub partitioning: Option<PartitioningMetadata>,

    // Tier 3+
    pub governance: Option<GovernanceMetadata>,

    // Tier 4+
    pub profiling: Option<ProfilingMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TableType {
    BaseTable,
    View,
    MaterializedView,
    ExternalTable,
    TemporaryTable,
}

/// Tier 0: Basic column structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnMetadata {
    pub name: String,
    pub data_type: String,
    pub native_type: String, // DB-specific type
    pub nullable: bool,
    pub is_primary_key: bool,
    pub ordinal_position: i32,
    pub default_value: Option<String>,
    pub comment: Option<String>,

    // Tier 2+
    pub statistics: Option<ColumnStatistics>,

    // Tier 2.5: Semantic type detection (NEW for Phase 1)
    pub semantic_type: Option<SemanticType>,
    pub semantic_confidence: Option<f64>,

    // Tier 3+
    pub classification: Option<DataClassification>,
    pub pii_detected: Option<PiiDetection>,

    // Tier 4+
    pub value_profile: Option<ValueProfile>,
}

/// Tier 1: Foreign key relationships
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelationshipMetadata {
    pub foreign_keys: Vec<ForeignKeyMetadata>,
    pub referenced_by: Vec<ForeignKeyMetadata>, // Reverse relationships
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForeignKeyMetadata {
    pub constraint_name: String,
    pub columns: Vec<String>,
    pub referenced_schema: String,
    pub referenced_table: String,
    pub referenced_columns: Vec<String>,
    pub update_rule: ReferentialAction,
    pub delete_rule: ReferentialAction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ReferentialAction {
    NoAction,
    Restrict,
    Cascade,
    SetNull,
    SetDefault,
}

/// Tier 1: Index metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexMetadata {
    pub name: String,
    pub index_type: IndexType,
    pub columns: Vec<IndexColumn>,
    pub is_unique: bool,
    pub is_primary: bool,
    pub filter_condition: Option<String>,
    pub size_bytes: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IndexType {
    BTree,
    Hash,
    GiST,
    GIN,
    BRIN,
    Bitmap,
    Clustered,
    ColumnStore,
    Other(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexColumn {
    pub name: String,
    pub ordinal: i32,
    pub is_descending: bool,
}

/// Tier 1: Constraints
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConstraintMetadata {
    pub name: String,
    pub constraint_type: ConstraintType,
    pub columns: Vec<String>,
    pub definition: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConstraintType {
    PrimaryKey,
    ForeignKey,
    Unique,
    Check,
    NotNull,
    Default,
}

/// Tier 2: Table-level statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableStatistics {
    pub actual_row_count: u64,
    pub size_bytes: u64,
    pub index_size_bytes: u64,
    pub compression_ratio: Option<f64>,
    pub last_analyzed: Option<DateTime<Utc>>,
    pub last_modified: Option<DateTime<Utc>>,
    pub read_count_daily: Option<u64>,
    pub write_count_daily: Option<u64>,
}

/// Tier 2: Column statistics (Enhanced for Phase 1)
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ColumnStatistics {
    // Basic statistics
    pub distinct_count: Option<u64>,
    pub null_count: u64,
    pub null_percentage: f64,
    pub min_value: Option<String>,
    pub max_value: Option<String>,
    pub avg_length: Option<f64>,

    // Distribution information
    pub histogram: Option<Histogram>,
    pub most_common_values: Option<Vec<ValueFrequency>>,

    // PostgreSQL-specific (pg_stats)
    pub correlation: Option<f64>, // Correlation with physical row order
    pub n_distinct: Option<f64>,  // Negative means distinct count ratio
    pub avg_width: Option<i32>,   // Average storage width in bytes

    // Cardinality classification
    pub cardinality: Option<CardinalityClass>,

    // Statistical quality indicators
    pub sample_size: Option<u64>,
    pub last_analyzed: Option<DateTime<Utc>>,
    pub statistics_stale: bool,
}

/// Cardinality classification
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, ToSchema)]
pub enum CardinalityClass {
    /// 1-10 distinct values
    VeryLow,
    /// 11-100 distinct values
    Low,
    /// 101-1000 distinct values
    Medium,
    /// 1001-100000 distinct values
    High,
    /// > 100000 distinct values
    VeryHigh,
    /// Approaching row count (candidate for unique index)
    Unique,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Histogram {
    pub buckets: Vec<HistogramBucket>,
    pub method: HistogramMethod,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct HistogramBucket {
    pub lower_bound: String,
    pub upper_bound: String,
    pub frequency: u64,
    pub distinct_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub enum HistogramMethod {
    EquiWidth,
    EquiDepth,
    Hybrid,
}

/// Tier 2: Partitioning metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartitioningMetadata {
    pub strategy: PartitioningStrategy,
    pub columns: Vec<String>,
    pub partitions: Vec<PartitionInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PartitioningStrategy {
    Range,
    List,
    Hash,
    Composite,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartitionInfo {
    pub name: String,
    pub bounds: String,
    pub row_count: Option<u64>,
    pub size_bytes: Option<u64>,
}

/// Tier 3: Governance metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GovernanceMetadata {
    pub data_classification: DataClassification,
    pub steward: Option<String>,
    pub business_glossary_terms: Vec<String>,
    pub sensitivity_labels: Vec<SensitivityLabel>,
    pub retention_policy: Option<RetentionPolicy>,
    pub access_patterns: AccessPatterns,
    pub quality_metrics: DataQualityMetrics,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DataClassification {
    Public,
    Internal,
    Confidential,
    Restricted,
    HighlyRestricted,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SensitivityLabel {
    pub label_type: SensitivityType,
    pub confidence: f64,
    pub detected_columns: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SensitivityType {
    PII,
    PHI,
    PCI,
    Financial,
    Proprietary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetentionPolicy {
    pub retention_days: u32,
    pub archival_required: bool,
    pub deletion_method: DeletionMethod,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DeletionMethod {
    HardDelete,
    SoftDelete,
    Archive,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessPatterns {
    pub read_frequency: AccessFrequency,
    pub write_frequency: AccessFrequency,
    pub peak_hours: Vec<u8>,            // Hour of day 0-23
    pub primary_consumers: Vec<String>, // User/service IDs
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AccessFrequency {
    Realtime,      // < 1 min
    HighFrequency, // 1-60 min
    Moderate,      // 1-24 hours
    LowFrequency,  // > 24 hours
    Archive,       // Rarely accessed
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataQualityMetrics {
    pub completeness: f64,           // % non-null
    pub uniqueness: f64,             // % unique values
    pub validity: f64,               // % passing format checks
    pub consistency: f64,            // % consistent with constraints
    pub timeliness: f64,             // Based on freshness
    pub accuracy_score: Option<f64>, // If validation data available
}

/// Tier 3: PII detection results
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiiDetection {
    pub pii_type: PiiType,
    pub confidence: f64,
    pub detection_method: DetectionMethod,
    pub sample_matches: Vec<String>, // Redacted samples
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PiiType {
    Email,
    Phone,
    SSN,
    CreditCard,
    IPAddress,
    PersonName,
    Address,
    DateOfBirth,
    MedicalRecordNumber,
    Custom(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DetectionMethod {
    Regex,
    NamedEntityRecognition,
    ColumnName,
    ValuePattern,
    MachineLearning,
}

/// Semantic type classification (Phase 1: Extended Type System)
///
/// Broader than PII - includes business domain types, technical types, and patterns.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, ToSchema)]
pub enum SemanticType {
    // Identity & Contact
    Email,
    PhoneNumber,
    PersonName,
    OrganizationName,
    Username,
    UserId,

    // Geographic
    Address,
    City,
    State,
    PostalCode,
    Country,
    CountryCode,
    Coordinates,
    IPAddress,

    // Financial
    CreditCardNumber,
    BankAccountNumber,
    IBANNumber,
    CurrencyAmount,
    CurrencyCode,
    TaxIdentifier,

    // Healthcare
    SSN,
    MedicalRecordNumber,
    HealthInsuranceNumber,
    DrugCode,
    DiagnosisCode,

    // Temporal
    Timestamp,
    Date,
    Time,
    Duration,
    DateOfBirth,

    // Technical
    URL,
    URI,
    UUID,
    Hostname,
    MACAddress,
    FilePath,
    MimeType,

    // Business
    ProductCode,
    SKU,
    OrderNumber,
    InvoiceNumber,
    AccountNumber,
    VIN, // Vehicle Identification Number

    // Categorical
    Enum,
    Boolean,
    Flag,
    Status,
    Category,

    // Textual
    FreeText,
    Description,
    Comment,
    JsonBlob,
    XMLBlob,

    // Measurement
    Quantity,
    Percentage,
    Score,
    Rating,

    // Custom
    Custom(String),
    Unknown,
}

/// Tier 4: Deep value profiling
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfilingMetadata {
    pub sample_size: u64,
    pub sampling_method: SamplingMethod,
    pub profiled_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SamplingMethod {
    FirstN(u64),
    Random(f64), // Percentage
    Stratified,
    FullScan,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValueProfile {
    pub top_values: Vec<ValueFrequency>,
    pub pattern_distribution: HashMap<String, u64>,
    pub length_distribution: HashMap<usize, u64>,
    pub format_violations: Vec<FormatViolation>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ValueFrequency {
    pub value: String,
    pub count: u64,
    pub percentage: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormatViolation {
    pub expected_format: String,
    pub violating_value: String,
    pub count: u64,
}

/// Inference job for async execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceJob {
    pub job_id: String,
    pub source_id: String,
    pub schemas: Vec<String>, // Empty = all
    pub tier: InferenceTier,
    pub status: JobStatus,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub error: Option<String>,
    pub result_uri: Option<String>, // RDF graph URI
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum JobStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
}
