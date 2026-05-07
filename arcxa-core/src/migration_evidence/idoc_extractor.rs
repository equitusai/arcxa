use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use super::{ApprovalEvent, ControlResult, ExceptionRecord, ExecutionEvent};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SapIdocExtractorDataFormat {
    JsonDocuments,
    Csv,
    Tsv,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SapExtractorFamily {
    Idoc,
    Odp,
    Generic,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SapExtractorMode {
    Full,
    Delta,
    Snapshot,
    Init,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SapIdocExtractorDataSet {
    pub format: SapIdocExtractorDataFormat,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inline_payload: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_row_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SapIdocExtractorManifest {
    pub schema_version: String,
    pub package_id: String,
    pub program_id: String,
    pub object_id: String,
    pub object_name: String,
    pub source_system_id: String,
    pub source_client: String,
    #[serde(default = "default_extractor_family")]
    pub extractor_family: SapExtractorFamily,
    pub extractor_name: String,
    pub extractor_run_id: String,
    pub extracted_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extractor_object: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extractor_context: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extraction_mode: Option<SapExtractorMode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delta_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subscriber_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub queue_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub idoc_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_type: Option<String>,
    #[serde(default)]
    pub segment_counts: BTreeMap<String, u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data_set: Option<SapIdocExtractorDataSet>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SapIdocExtractorExecutionEvidence {
    pub execution: ExecutionEvent,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SapIdocExtractorExceptionEvidence {
    pub exception: ExceptionRecord,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SapIdocExtractorControlEvidence {
    pub control: ControlResult,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SapIdocExtractorApprovalEvidence {
    pub approval: ApprovalEvent,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SapIdocExtractorBundle {
    pub manifest: SapIdocExtractorManifest,
    #[serde(default)]
    pub executions: Vec<SapIdocExtractorExecutionEvidence>,
    #[serde(default)]
    pub exceptions: Vec<SapIdocExtractorExceptionEvidence>,
    #[serde(default)]
    pub controls: Vec<SapIdocExtractorControlEvidence>,
    #[serde(default)]
    pub approvals: Vec<SapIdocExtractorApprovalEvidence>,
}

fn default_extractor_family() -> SapExtractorFamily {
    SapExtractorFamily::Idoc
}
