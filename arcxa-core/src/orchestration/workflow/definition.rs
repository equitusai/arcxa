//! Workflow definition schemas and parsing

use anyhow::{Context, Result};
use serde::de::{self, Deserializer};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use utoipa::ToSchema;

/// Complete workflow definition
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct WorkflowDefinition {
    pub steps: Vec<WorkflowStep>,
    #[serde(default)]
    pub fusion_threshold: f64,
    #[serde(default)]
    pub fallback: FallbackStrategy,
}

impl WorkflowDefinition {
    /// Parse workflow from YAML
    pub fn from_yaml(yaml: &str) -> Result<Self> {
        serde_yaml::from_str(yaml).context("Failed to parse workflow YAML")
    }

    /// Parse workflow from JSON
    pub fn from_json(json: &str) -> Result<Self> {
        serde_json::from_str(json).context("Failed to parse workflow JSON")
    }

    /// Validate workflow definition
    pub fn validate(&self) -> Result<()> {
        if self.steps.is_empty() {
            anyhow::bail!("Workflow must have at least one step");
        }

        // Check for duplicate step IDs
        let mut seen_ids = std::collections::HashSet::new();
        for step in &self.steps {
            if !seen_ids.insert(&step.id) {
                anyhow::bail!("Duplicate step ID: {}", step.id);
            }
        }

        // Validate each step
        for step in &self.steps {
            step.validate()?;
        }

        Ok(())
    }
}

/// Individual workflow step
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct WorkflowStep {
    pub id: String,
    pub step_type: StepType,
    pub config: StepConfig,
    #[serde(default)]
    pub depends_on: Vec<String>,
}

impl<'de> Deserialize<'de> for WorkflowStep {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct RawStep {
            id: String,
            step_type: StepType,
            config: serde_json::Value,
            #[serde(default)]
            depends_on: Vec<String>,
        }

        let raw = RawStep::deserialize(deserializer)?;
        let config =
            StepConfig::from_step_type(&raw.step_type, raw.config).map_err(de::Error::custom)?;

        Ok(WorkflowStep {
            id: raw.id,
            step_type: raw.step_type,
            config,
            depends_on: raw.depends_on,
        })
    }
}

impl WorkflowStep {
    pub fn validate(&self) -> Result<()> {
        if self.id.is_empty() {
            anyhow::bail!("Step ID cannot be empty");
        }
        self.config.validate(&self.step_type)
    }
}

/// Step type enum
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum StepType {
    // ML/Fusion step types
    MlPrediction,
    HeuristicRule,
    WasmRule,
    ConfidenceGate,
    WeightedVote,
    ConfidenceAggregate,

    // ETL step types (Phase 1 - Data Movement)
    CsvSource,
    DbExtract,
    FieldTransformer,
    DbLoader,
    RdfLoader,

    // ETL step types (Phase 2 - Data Quality)
    DataValidator,

    // ETL step types (Phase 3 - Advanced Features)
    DataJoiner,
    SemanticMapper,
    Deduplicator,
    Aggregator,
    SosValidation,

    // ETL step types (Phase 4 - Export)
    CsvExporter,
}

impl std::fmt::Display for StepType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            StepType::MlPrediction => "ml_prediction",
            StepType::HeuristicRule => "heuristic_rule",
            StepType::WasmRule => "wasm_rule",
            StepType::ConfidenceGate => "confidence_gate",
            StepType::WeightedVote => "weighted_vote",
            StepType::ConfidenceAggregate => "confidence_aggregate",
            // ETL step types
            StepType::CsvSource => "csv_source",
            StepType::DbExtract => "db_extract",
            StepType::FieldTransformer => "field_transformer",
            StepType::DbLoader => "db_loader",
            StepType::RdfLoader => "rdf_loader",
            StepType::DataValidator => "data_validator",
            StepType::DataJoiner => "data_joiner",
            StepType::SemanticMapper => "semantic_mapper",
            StepType::Deduplicator => "deduplicator",
            StepType::Aggregator => "aggregator",
            StepType::SosValidation => "sos_validation",
            StepType::CsvExporter => "csv_exporter",
        };
        write!(f, "{}", s)
    }
}

impl StepType {
    /// Convert to string (same as Display but without allocation)
    pub fn as_str(&self) -> &'static str {
        match self {
            StepType::MlPrediction => "ml_prediction",
            StepType::HeuristicRule => "heuristic_rule",
            StepType::WasmRule => "wasm_rule",
            StepType::ConfidenceGate => "confidence_gate",
            StepType::WeightedVote => "weighted_vote",
            StepType::ConfidenceAggregate => "confidence_aggregate",
            // ETL step types
            StepType::CsvSource => "csv_source",
            StepType::DbExtract => "db_extract",
            StepType::FieldTransformer => "field_transformer",
            StepType::DbLoader => "db_loader",
            StepType::RdfLoader => "rdf_loader",
            StepType::DataValidator => "data_validator",
            StepType::DataJoiner => "data_joiner",
            StepType::SemanticMapper => "semantic_mapper",
            StepType::Deduplicator => "deduplicator",
            StepType::Aggregator => "aggregator",
            StepType::SosValidation => "sos_validation",
            StepType::CsvExporter => "csv_exporter",
        }
    }
}

/// Step configuration (tagged union)
///
/// This enum remains `#[serde(untagged)]` to keep the JSON shape stable.
/// `WorkflowStep` deserialization uses `step_type` to choose the variant, so
/// workflow definitions do not rely on enum ordering. If `StepConfig` is
/// deserialized directly, the usual untagged matching behavior still applies.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(untagged)]
pub enum StepConfig {
    // ML/Fusion configs
    MLPrediction(MLPredictionConfig),
    Heuristic(HeuristicConfig),
    WasmRule(WasmRuleConfig),
    ConfidenceGate(ConfidenceGateConfig),
    WeightedVote(WeightedVoteConfig),

    // ETL configs (Phase 1 - Data Movement)
    CsvSource(CsvSourceConfig),
    DbLoader(DbLoaderConfig), // More specific (datasource_id + table_name required)
    FieldTransformer(FieldTransformerConfig),
    DbExtract(DbExtractConfig), // Greedy (only datasource_id required, other fields optional)
    RdfLoader(RdfLoaderConfig),

    // ETL configs (Phase 2 - Data Quality)
    DataValidator(DataValidatorConfig),

    // ETL configs (Phase 3 - Advanced Features)
    DataJoiner(DataJoinerConfig),
    SemanticMapper(SemanticMapperConfig),
    Deduplicator(DeduplicatorConfig), // Has method + key_fields - more specific than ConfidenceAggregate
    Aggregator(AggregatorConfig),
    SosValidation(SosValidationConfig),

    // ETL configs (Phase 4 - Export)
    CsvExporter(CsvExporterConfig),

    // Greedy configs - keep near end (fewest required fields, will match broadly)
    ConfidenceAggregate(ConfidenceAggregateConfig), // Only has method: String - moved to end
}

impl StepConfig {
    fn from_step_type(step_type: &StepType, value: serde_json::Value) -> Result<Self> {
        fn parse<T: serde::de::DeserializeOwned>(
            value: serde_json::Value,
            step_type: &StepType,
        ) -> Result<T> {
            serde_json::from_value(value)
                .with_context(|| format!("Invalid config for step_type {}", step_type.as_str()))
        }

        Ok(match step_type {
            StepType::MlPrediction => StepConfig::MLPrediction(parse(value, step_type)?),
            StepType::HeuristicRule => StepConfig::Heuristic(parse(value, step_type)?),
            StepType::WasmRule => StepConfig::WasmRule(parse(value, step_type)?),
            StepType::ConfidenceGate => StepConfig::ConfidenceGate(parse(value, step_type)?),
            StepType::WeightedVote => StepConfig::WeightedVote(parse(value, step_type)?),
            StepType::ConfidenceAggregate => {
                StepConfig::ConfidenceAggregate(parse(value, step_type)?)
            }
            StepType::CsvSource => StepConfig::CsvSource(parse(value, step_type)?),
            StepType::DbExtract => StepConfig::DbExtract(parse(value, step_type)?),
            StepType::FieldTransformer => StepConfig::FieldTransformer(parse(value, step_type)?),
            StepType::DbLoader => StepConfig::DbLoader(parse(value, step_type)?),
            StepType::RdfLoader => StepConfig::RdfLoader(parse(value, step_type)?),
            StepType::DataValidator => StepConfig::DataValidator(parse(value, step_type)?),
            StepType::DataJoiner => StepConfig::DataJoiner(parse(value, step_type)?),
            StepType::SemanticMapper => StepConfig::SemanticMapper(parse(value, step_type)?),
            StepType::Deduplicator => StepConfig::Deduplicator(parse(value, step_type)?),
            StepType::Aggregator => StepConfig::Aggregator(parse(value, step_type)?),
            StepType::SosValidation => StepConfig::SosValidation(parse(value, step_type)?),
            StepType::CsvExporter => StepConfig::CsvExporter(parse(value, step_type)?),
        })
    }

    fn validate(&self, step_type: &StepType) -> Result<()> {
        match (self, step_type) {
            // ML/Fusion validators
            (StepConfig::MLPrediction(cfg), StepType::MlPrediction) => cfg.validate(),
            (StepConfig::Heuristic(cfg), StepType::HeuristicRule) => cfg.validate(),
            (StepConfig::WasmRule(cfg), StepType::WasmRule) => cfg.validate(),
            (StepConfig::ConfidenceGate(cfg), StepType::ConfidenceGate) => cfg.validate(),
            (StepConfig::WeightedVote(cfg), StepType::WeightedVote) => cfg.validate(),

            // ETL validators (Phase 1 - Data Movement)
            (StepConfig::CsvSource(cfg), StepType::CsvSource) => cfg.validate(),
            (StepConfig::DbLoader(cfg), StepType::DbLoader) => cfg.validate(),
            (StepConfig::FieldTransformer(cfg), StepType::FieldTransformer) => cfg.validate(),
            (StepConfig::DbExtract(cfg), StepType::DbExtract) => cfg.validate(),
            (StepConfig::RdfLoader(cfg), StepType::RdfLoader) => cfg.validate(),

            // ETL validators (Phase 2 - Data Quality)
            (StepConfig::DataValidator(cfg), StepType::DataValidator) => cfg.validate(),

            // ETL validators (Phase 3 - Advanced Features)
            (StepConfig::DataJoiner(cfg), StepType::DataJoiner) => cfg.validate(),
            (StepConfig::SemanticMapper(cfg), StepType::SemanticMapper) => cfg.validate(),
            (StepConfig::Deduplicator(cfg), StepType::Deduplicator) => cfg.validate(),
            (StepConfig::Aggregator(cfg), StepType::Aggregator) => cfg.validate(),
            (StepConfig::SosValidation(cfg), StepType::SosValidation) => cfg.validate(),

            // ETL validators (Phase 4 - Export)
            (StepConfig::CsvExporter(cfg), StepType::CsvExporter) => cfg.validate(),

            // Greedy validators - must match enum order
            (StepConfig::ConfidenceAggregate(cfg), StepType::ConfidenceAggregate) => cfg.validate(),

            _ => anyhow::bail!("Step config type mismatch"),
        }
    }
}

/// ML prediction step configuration
///
/// This config is for governance and lineage tracking only - not actual ML inference.
/// Predictions are mocked/simulated to test the RDF lineage generation pipeline.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MLPredictionConfig {
    /// Model identifier for RDF linking
    pub model_id: String,

    /// Model version for lineage tracking
    #[serde(default = "default_model_version")]
    pub model_version: String,

    /// Legacy: Simple feature list (kept for backward compatibility)
    #[serde(default)]
    pub features: Vec<String>,

    /// Feature mappings: model feature name -> workflow field name
    #[serde(default)]
    pub feature_mappings: Vec<FeatureMapping>,

    /// Output predictions to generate
    #[serde(default)]
    pub predictions: Vec<PredictionSpec>,

    /// Minimum confidence threshold (step fails if below this)
    #[serde(default)]
    pub confidence_threshold: Option<f64>,

    #[serde(default = "default_timeout")]
    pub timeout_ms: u64,

    #[serde(default)]
    pub cache_ttl_secs: Option<u64>,
}

impl MLPredictionConfig {
    fn validate(&self) -> Result<()> {
        if self.model_id.is_empty() {
            anyhow::bail!("model_id cannot be empty");
        }

        // Must have either legacy features or new feature_mappings
        if self.features.is_empty() && self.feature_mappings.is_empty() {
            anyhow::bail!("Either features or feature_mappings must be provided");
        }

        // Must have predictions specified
        if self.predictions.is_empty() {
            anyhow::bail!("predictions cannot be empty - specify what attributes to predict");
        }

        // Validate confidence threshold
        if let Some(threshold) = self.confidence_threshold {
            if threshold < 0.0 || threshold > 1.0 {
                anyhow::bail!("confidence_threshold must be between 0.0 and 1.0");
            }
        }

        // Validate predictions
        for pred in &self.predictions {
            pred.validate()?;
        }

        Ok(())
    }
}

fn default_model_version() -> String {
    "1.0.0".to_string()
}

/// Feature mapping: maps model input feature to workflow field
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct FeatureMapping {
    /// Name in model's feature space (e.g., "normalized_email")
    pub feature_name: String,

    /// Field name in workflow context (e.g., "email")
    pub field_name: String,

    /// Optional transformation to apply (e.g., "normalize", "lower")
    #[serde(default)]
    pub transform: Option<String>,
}

/// Prediction specification for mock predictions
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PredictionSpec {
    /// Attribute name to create (e.g., "customer_segment")
    pub attribute_name: String,

    /// Mock value to return, or "auto" for deterministic generation based on features
    pub mock_value: String,

    /// Mock confidence score (0.0-1.0)
    pub mock_confidence: f64,
}

impl PredictionSpec {
    fn validate(&self) -> Result<()> {
        if self.attribute_name.is_empty() {
            anyhow::bail!("attribute_name cannot be empty");
        }

        if self.mock_confidence < 0.0 || self.mock_confidence > 1.0 {
            anyhow::bail!("mock_confidence must be between 0.0 and 1.0");
        }

        Ok(())
    }
}

fn default_timeout() -> u64 {
    500
}

/// Heuristic rule configuration
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct HeuristicConfig {
    pub rule_id: String,
    #[serde(default)]
    pub min_confidence: f64,
}

impl HeuristicConfig {
    fn validate(&self) -> Result<()> {
        if self.rule_id.is_empty() {
            anyhow::bail!("rule_id cannot be empty");
        }
        if self.min_confidence < 0.0 || self.min_confidence > 1.0 {
            anyhow::bail!("min_confidence must be between 0.0 and 1.0");
        }
        Ok(())
    }
}

/// WASM rule configuration
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct WasmRuleConfig {
    pub rule_id: String,
}

impl WasmRuleConfig {
    fn validate(&self) -> Result<()> {
        if self.rule_id.is_empty() {
            anyhow::bail!("rule_id cannot be empty");
        }
        Ok(())
    }
}

/// Confidence gate configuration
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ConfidenceGateConfig {
    pub threshold: f64,
    #[serde(default)]
    pub input_step: Option<String>,
}

impl ConfidenceGateConfig {
    fn validate(&self) -> Result<()> {
        if self.threshold < 0.0 || self.threshold > 1.0 {
            anyhow::bail!("threshold must be between 0.0 and 1.0");
        }
        Ok(())
    }
}

/// Weighted vote configuration
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct WeightedVoteConfig {
    pub weights: HashMap<String, f64>,
}

impl WeightedVoteConfig {
    fn validate(&self) -> Result<()> {
        if self.weights.is_empty() {
            anyhow::bail!("weights cannot be empty");
        }

        let total: f64 = self.weights.values().sum();
        if (total - 1.0).abs() > 0.01 {
            anyhow::bail!("weights must sum to 1.0, got {}", total);
        }

        Ok(())
    }
}

/// Confidence aggregation configuration
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ConfidenceAggregateConfig {
    pub method: String, // "weighted_average", "bayesian", "voting"
    #[serde(default)]
    pub inputs: Vec<String>,
}

impl ConfidenceAggregateConfig {
    fn validate(&self) -> Result<()> {
        let valid_methods = ["weighted_average", "bayesian", "voting"];
        if !valid_methods.contains(&self.method.as_str()) {
            anyhow::bail!("Invalid method: {}", self.method);
        }
        Ok(())
    }
}

// ============================================================================
// ETL Step Configurations (Phase 1 - Data Movement)
// ============================================================================

/// CSV Source configuration
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CsvSourceConfig {
    pub file_path: String,
    #[serde(default)]
    pub delimiter: Option<char>,
    #[serde(default)]
    pub has_header: Option<bool>,
    #[serde(default)]
    pub encoding: Option<String>,
    #[serde(default)]
    pub skip_rows: Option<usize>,
    #[serde(default)]
    pub max_rows: Option<usize>,
}

impl CsvSourceConfig {
    fn validate(&self) -> Result<()> {
        if self.file_path.is_empty() {
            anyhow::bail!("file_path cannot be empty");
        }
        Ok(())
    }
}

/// Database Extract configuration
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DbExtractConfig {
    pub datasource_id: String,
    #[serde(default)]
    pub table_name: Option<String>,
    /// Optional table name override for schema introspection
    #[serde(default)]
    pub schema_table: Option<String>,
    #[serde(default)]
    pub query: Option<String>,
    #[serde(default)]
    pub incremental: Option<bool>,
    #[serde(default)]
    pub incremental_column: Option<String>,
    #[serde(default)]
    #[schema(value_type = Option<Object>)]
    pub last_value: Option<serde_json::Value>,
    #[serde(default = "default_batch_size")]
    pub batch_size: usize,
    #[serde(default)]
    pub columns: Option<Vec<String>>,
    /// Include schema metadata in the output payload for ontology mapping
    #[serde(default)]
    pub include_schema: Option<bool>,
    /// Sample size for schema inference (defaults to 1000)
    #[serde(default)]
    pub schema_sample_size: Option<usize>,
}

impl DbExtractConfig {
    fn validate(&self) -> Result<()> {
        if self.datasource_id.is_empty() {
            anyhow::bail!("datasource_id cannot be empty");
        }
        if self.table_name.is_none() && self.query.is_none() {
            anyhow::bail!("Either table_name or query must be provided");
        }
        if self.query.is_some() && self.incremental.unwrap_or(false) {
            anyhow::bail!("incremental extraction is only supported in table mode");
        }
        if self.incremental.unwrap_or(false) && self.incremental_column.is_none() {
            anyhow::bail!("incremental_column required when incremental is true");
        }
        if self.incremental.unwrap_or(false) && self.last_value.is_none() {
            anyhow::bail!("last_value required when incremental is true");
        }
        if self.include_schema.unwrap_or(false)
            && self.schema_table.is_none()
            && self.table_name.is_none()
        {
            anyhow::bail!("schema_table or table_name required when include_schema is true");
        }
        Ok(())
    }
}

fn default_batch_size() -> usize {
    50000 // Increased from 10K to 50K for better throughput
}

/// Field Transformation operation
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct FieldTransformation {
    pub field: String,
    pub operations: Vec<TransformOperation>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(tag = "type", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TransformOperation {
    Trim,
    Lower,
    Upper,
    Regex {
        pattern: String,
        replacement: String,
    },
    Concat {
        separator: String,
        fields: Vec<String>,
    },
    Split {
        delimiter: String,
        index: usize,
    },
    Substring {
        start: usize,
        length: Option<usize>,
    },
    Replace {
        from: String,
        to: String,
    },
    Round {
        decimals: usize,
    },
    FormatDate {
        format: String,
    },
    Coalesce {
        fields: Vec<String>,
    },
    IfNull {
        default_value: String,
    },
    Custom {
        expression: String,
    },
}

/// Field Transformer configuration
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct FieldTransformerConfig {
    pub transformations: Vec<FieldTransformation>,
}

impl FieldTransformerConfig {
    fn validate(&self) -> Result<()> {
        if self.transformations.is_empty() {
            anyhow::bail!("transformations cannot be empty");
        }
        for transformation in &self.transformations {
            if transformation.field.is_empty() {
                anyhow::bail!("field name cannot be empty");
            }
            if transformation.operations.is_empty() {
                anyhow::bail!(
                    "operations cannot be empty for field {}",
                    transformation.field
                );
            }
        }
        Ok(())
    }
}

/// Database Loader configuration
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DbLoaderConfig {
    pub datasource_id: String,
    pub table_name: String,
    #[serde(default)]
    pub mode: LoadMode,
    #[serde(default)]
    pub key_fields: Option<Vec<String>>,
    #[serde(default = "default_batch_size")]
    pub batch_size: usize,
    #[serde(default)]
    pub create_table: bool,
    /// Optional entity URI for ontology-driven loading
    /// If provided, enables automatic schema generation from ontology
    #[serde(default)]
    pub entity_uri: Option<String>,
}

/// Load mode for workflow database loading operations
///
/// **Note:** This enum is kept in graphica-core for workflow definitions only.
/// For new ETL code, prefer using `graphica_coordinator::etl::traits::LoadMode` which
/// has additional modes (Append, Merge) and is the canonical definition.
///
/// This enum must remain here because graphica-core cannot depend on graphica-coordinator
/// (dependency flows the other way). Workflows deserialize this from YAML.
#[derive(Debug, Clone, Serialize, Deserialize, Default, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum LoadMode {
    #[default]
    Insert,
    Upsert,
    Replace,
}

impl DbLoaderConfig {
    fn validate(&self) -> Result<()> {
        if self.datasource_id.is_empty() {
            anyhow::bail!("datasource_id cannot be empty");
        }
        if self.table_name.is_empty() {
            anyhow::bail!("table_name cannot be empty");
        }
        if matches!(self.mode, LoadMode::Upsert) && self.key_fields.is_none() {
            anyhow::bail!("key_fields required for upsert mode");
        }
        Ok(())
    }
}

/// RDF Loader configuration
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RdfLoaderConfig {
    #[serde(default)]
    pub target_graph: Option<String>,
    pub entity_type: String,
    pub id_field: String,
    #[serde(default = "default_batch_size")]
    pub batch_size: usize,
    #[serde(default = "default_true")]
    pub capture_lineage: bool,
}

fn default_true() -> bool {
    true
}

impl RdfLoaderConfig {
    fn validate(&self) -> Result<()> {
        if self.entity_type.is_empty() {
            anyhow::bail!("entity_type cannot be empty");
        }
        if self.id_field.is_empty() {
            anyhow::bail!("id_field cannot be empty");
        }
        Ok(())
    }
}

// ============================================================================
// ETL Step Configurations (Phase 2 - Data Quality)
// ============================================================================

/// Validation Rule
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ValidationRule {
    pub field: String,
    pub rule_type: RuleType,
    #[serde(default)]
    #[schema(value_type = Option<Object>)]
    pub params: Option<serde_json::Value>,
    #[serde(default)]
    pub severity: Severity,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RuleType {
    NotNull,
    Regex {
        pattern: String,
    },
    Range {
        min: f64,
        max: f64,
    },
    InSet {
        values: Vec<String>,
    },
    Unique,
    Length {
        min: usize,
        max: usize,
    },
    DateRange {
        min: String,
        max: String,
    },
    CrossField {
        other_field: String,
        operator: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    #[default]
    Error,
    Warning,
}

/// Data Validator configuration
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DataValidatorConfig {
    pub rules: Vec<ValidationRule>,
    #[serde(default = "default_true")]
    pub fail_on_error: bool,
}

impl DataValidatorConfig {
    fn validate(&self) -> Result<()> {
        if self.rules.is_empty() {
            anyhow::bail!("rules cannot be empty");
        }
        for rule in &self.rules {
            if rule.field.is_empty() {
                anyhow::bail!("field name cannot be empty");
            }
        }
        Ok(())
    }
}

// ============================================================================
// ETL Step Configurations (Phase 3 - Advanced Features)
// ============================================================================

/// Data Joiner configuration
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DataJoinerConfig {
    pub join_type: JoinType,
    pub left_key: Vec<String>,
    pub right_key: Vec<String>,
    #[serde(default)]
    pub output_columns: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum JoinType {
    Inner,
    Left,
    Right,
    Full,
}

impl DataJoinerConfig {
    fn validate(&self) -> Result<()> {
        if self.left_key.is_empty() {
            anyhow::bail!("left_key cannot be empty");
        }
        if self.right_key.is_empty() {
            anyhow::bail!("right_key cannot be empty");
        }
        if self.left_key.len() != self.right_key.len() {
            anyhow::bail!("left_key and right_key must have same length");
        }
        Ok(())
    }
}

/// Semantic Mapper configuration
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SemanticMapperConfig {
    pub target_ontology: Vec<String>,
    #[serde(default = "default_auto_approve_threshold")]
    pub auto_approve_threshold: f64,
    #[serde(default)]
    pub mapping_mode: MappingMode,
    #[serde(default)]
    pub mapping_session_id: Option<String>,
    /// Preserve the source field names alongside ontology-aligned aliases.
    ///
    /// This is useful when downstream steps still load into a canonical table
    /// schema but we also want ontology mapping and lineage in the same run.
    #[serde(default)]
    pub preserve_original_fields: bool,
    /// Optional datasource ID for lineage and mapping context
    #[serde(default)]
    pub source_id: Option<String>,
    /// Optional table name for lineage and mapping context
    #[serde(default)]
    pub table_name: Option<String>,
    /// Optional entity URI to inject into mapped rows (enables ontology-driven loading with DDL auto-generation)
    #[serde(default)]
    pub entity_uri: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum MappingMode {
    Auto,
    Manual,
    #[default]
    Hybrid,
}

fn default_auto_approve_threshold() -> f64 {
    0.95
}

impl SemanticMapperConfig {
    fn validate(&self) -> Result<()> {
        if self.target_ontology.is_empty() {
            anyhow::bail!("target_ontology cannot be empty");
        }
        if self.auto_approve_threshold < 0.0 || self.auto_approve_threshold > 1.0 {
            anyhow::bail!("auto_approve_threshold must be between 0.0 and 1.0");
        }
        Ok(())
    }
}

/// Deduplicator configuration
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DeduplicatorConfig {
    pub method: DedupMethod,
    pub key_fields: Vec<String>,
    #[serde(default)]
    pub threshold: Option<f64>,
    #[serde(default)]
    pub keep: KeepStrategy,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum DedupMethod {
    Exact,
    Fuzzy { algorithm: FuzzyAlgorithm },
    Semantic { model: String },
}

impl std::fmt::Display for DedupMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            DedupMethod::Exact => write!(f, "exact"),
            DedupMethod::Fuzzy { algorithm } => write!(f, "fuzzy({})", algorithm),
            DedupMethod::Semantic { model } => write!(f, "semantic({})", model),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum FuzzyAlgorithm {
    Levenshtein,
    JaroWinkler,
    Soundex,
}

impl std::fmt::Display for FuzzyAlgorithm {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            FuzzyAlgorithm::Levenshtein => write!(f, "levenshtein"),
            FuzzyAlgorithm::JaroWinkler => write!(f, "jaro-winkler"),
            FuzzyAlgorithm::Soundex => write!(f, "soundex"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum KeepStrategy {
    #[default]
    First,
    Last,
    Merge,
    HighestQuality,
}

impl DeduplicatorConfig {
    fn validate(&self) -> Result<()> {
        if self.key_fields.is_empty() {
            anyhow::bail!("key_fields cannot be empty");
        }
        Ok(())
    }
}

/// Aggregation function
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Aggregation {
    pub field: String,
    pub function: AggFunction,
    #[serde(default)]
    pub alias: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AggFunction {
    Sum,
    Avg,
    Count,
    Min,
    Max,
    Stddev,
    Variance,
    Median,
}

/// Aggregator configuration
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AggregatorConfig {
    pub group_by: Vec<String>,
    pub aggregations: Vec<Aggregation>,
}

/// Systems-of-Systems validation configuration.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SosValidationConfig {
    pub validation: SosValidationSpec,
    #[serde(default = "default_blocking_severities")]
    pub blocking_severities: Vec<String>,
    #[serde(default = "default_true")]
    pub persist_report: bool,
    #[serde(default = "default_true")]
    pub emit_graph_lineage: bool,
}

impl SosValidationConfig {
    fn validate(&self) -> Result<()> {
        self.validation.validate()?;

        if self.blocking_severities.is_empty() {
            anyhow::bail!("blocking_severities cannot be empty");
        }

        for severity in &self.blocking_severities {
            match severity.to_ascii_lowercase().as_str() {
                "error" | "warning" | "info" => {}
                _ => anyhow::bail!(
                    "Unsupported blocking severity '{}'; expected error, warning, or info",
                    severity
                ),
            }
        }

        Ok(())
    }
}

fn default_blocking_severities() -> Vec<String> {
    vec!["error".to_string()]
}

/// Shared SoS validation spec used by workflow steps.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SosValidationSpec {
    InterfaceCompatibility {
        provider_interface_id: String,
        consumer_interface_id: String,
    },
    ContractCompliance {
        contract_id: String,
    },
    SystemIntegration {
        source_system_id: String,
        target_system_id: String,
    },
    PolicyCheck {
        sparql_query: String,
        #[serde(default)]
        context: HashMap<String, serde_json::Value>,
    },
    DataValidation {
        interface_id: String,
        data: serde_json::Value,
    },
}

impl SosValidationSpec {
    fn validate(&self) -> Result<()> {
        match self {
            SosValidationSpec::InterfaceCompatibility {
                provider_interface_id,
                consumer_interface_id,
            } => {
                if provider_interface_id.is_empty() || consumer_interface_id.is_empty() {
                    anyhow::bail!(
                        "provider_interface_id and consumer_interface_id cannot be empty"
                    );
                }
            }
            SosValidationSpec::ContractCompliance { contract_id } => {
                if contract_id.is_empty() {
                    anyhow::bail!("contract_id cannot be empty");
                }
            }
            SosValidationSpec::SystemIntegration {
                source_system_id,
                target_system_id,
            } => {
                if source_system_id.is_empty() || target_system_id.is_empty() {
                    anyhow::bail!("source_system_id and target_system_id cannot be empty");
                }
            }
            SosValidationSpec::PolicyCheck { sparql_query, .. } => {
                if sparql_query.trim().is_empty() {
                    anyhow::bail!("sparql_query cannot be empty");
                }
            }
            SosValidationSpec::DataValidation { interface_id, .. } => {
                if interface_id.is_empty() {
                    anyhow::bail!("interface_id cannot be empty");
                }
            }
        }

        Ok(())
    }
}

impl AggregatorConfig {
    fn validate(&self) -> Result<()> {
        if self.aggregations.is_empty() {
            anyhow::bail!("aggregations cannot be empty");
        }
        for agg in &self.aggregations {
            if agg.field.is_empty() {
                anyhow::bail!("aggregation field cannot be empty");
            }
        }
        Ok(())
    }
}

// ============================================================================
// ETL Step Configurations (Phase 4 - Export)
// ============================================================================

/// CSV Exporter configuration
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CsvExporterConfig {
    pub output_path: String,
    #[serde(default)]
    pub delimiter: Option<char>,
    #[serde(default = "default_true")]
    pub include_header: bool,
    #[serde(default)]
    pub encoding: Option<String>,
}

impl CsvExporterConfig {
    fn validate(&self) -> Result<()> {
        if self.output_path.is_empty() {
            anyhow::bail!("output_path cannot be empty");
        }
        Ok(())
    }
}

/// Fallback strategy
#[derive(Debug, Clone, Serialize, Deserialize, Default, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum FallbackStrategy {
    #[default]
    ManualReview,
    RejectFusion,
    AcceptFusion,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_parse_workflow_yaml() {
        let yaml = r#"
steps:
  - id: ml_similarity
    step_type: ml_prediction
    config:
      model_id: address_bert_v2
      features: [street, city, zip]
      timeout_ms: 500
      predictions:
        - attribute_name: similarity_score
          mock_value: "0.92"
          mock_confidence: 0.92
  - id: confidence_check
    step_type: confidence_gate
    config:
      threshold: 0.85
fusion_threshold: 0.80
fallback: manual_review
"#;

        let workflow = WorkflowDefinition::from_yaml(yaml).unwrap();
        assert_eq!(workflow.steps.len(), 2);
        assert_eq!(workflow.fusion_threshold, 0.80);

        workflow.validate().unwrap();
    }

    #[test]
    fn test_step_deserialize_uses_step_type_for_config() {
        let value = json!({
            "id": "step_1",
            "step_type": "db_extract",
            "config": {
                "datasource_id": "urn:graphica:datasource:test",
                "table_name": "UPLOADED_DATA"
            }
        });

        let step: WorkflowStep = serde_json::from_value(value).unwrap();
        assert_eq!(step.step_type, StepType::DbExtract);
        assert!(matches!(step.config, StepConfig::DbExtract(_)));
    }

    #[test]
    fn test_validate_duplicate_step_ids() {
        let workflow = WorkflowDefinition {
            steps: vec![
                WorkflowStep {
                    id: "step1".to_string(),
                    step_type: StepType::ConfidenceGate,
                    config: StepConfig::ConfidenceGate(ConfidenceGateConfig {
                        threshold: 0.8,
                        input_step: None,
                    }),
                    depends_on: vec![],
                },
                WorkflowStep {
                    id: "step1".to_string(), // Duplicate!
                    step_type: StepType::ConfidenceGate,
                    config: StepConfig::ConfidenceGate(ConfidenceGateConfig {
                        threshold: 0.9,
                        input_step: None,
                    }),
                    depends_on: vec![],
                },
            ],
            fusion_threshold: 0.8,
            fallback: FallbackStrategy::ManualReview,
        };

        assert!(workflow.validate().is_err());
    }
}
