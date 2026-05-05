use super::{
    ApprovalEvent, ControlResult, ExecutionEvent, ExceptionRecord, MigrationObject,
    MigrationProgram, TransformationRule,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SapEccStagedExportDataFormat {
    JsonRows,
    Csv,
    Tsv,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SapEccStagedExportDataSet {
    pub format: SapEccStagedExportDataFormat,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inline_payload: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_row_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SapEccStagedExportManifest {
    pub schema_version: String,
    pub export_id: String,
    pub program_id: String,
    pub object_id: String,
    pub object_name: String,
    pub source_system_id: String,
    pub source_client: String,
    pub extracted_at: DateTime<Utc>,
    #[serde(default)]
    pub key_fields: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data_set: Option<SapEccStagedExportDataSet>,
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SapEccStagedRuleEvidence {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value_key: Option<String>,
    pub rule: TransformationRule,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SapEccStagedExecutionEvidence {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value_key: Option<String>,
    pub execution: ExecutionEvent,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SapEccStagedExceptionEvidence {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value_key: Option<String>,
    pub exception: ExceptionRecord,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SapEccStagedControlEvidence {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value_key: Option<String>,
    pub control: ControlResult,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SapEccStagedApprovalEvidence {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value_key: Option<String>,
    pub approval: ApprovalEvent,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SapEccStagedExportBundle {
    pub manifest: SapEccStagedExportManifest,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub program: Option<MigrationProgram>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub object: Option<MigrationObject>,
    #[serde(default)]
    pub transformation_rules: Vec<SapEccStagedRuleEvidence>,
    #[serde(default)]
    pub executions: Vec<SapEccStagedExecutionEvidence>,
    #[serde(default)]
    pub exceptions: Vec<SapEccStagedExceptionEvidence>,
    #[serde(default)]
    pub controls: Vec<SapEccStagedControlEvidence>,
    #[serde(default)]
    pub approvals: Vec<SapEccStagedApprovalEvidence>,
}
