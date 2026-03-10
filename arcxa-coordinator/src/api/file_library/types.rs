//! File Library API Types
//!
//! Domain types for the enterprise file management system.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use utoipa::ToSchema;

// ============================================================================
// Core Domain Types
// ============================================================================

/// Data file in the library
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DataFile {
    pub id: String,
    pub name: String,
    pub file_path: String, // Server-side storage path
    pub folder_id: Option<String>,
    pub description: Option<String>,
    pub owner: FileOwner,

    // File metadata
    pub size_bytes: u64,
    pub encoding: String,
    pub delimiter: String,
    pub has_header: bool,

    // Schema
    pub schema: Option<FileSchema>,

    // Ontology mappings (field name -> ontology concept)
    #[serde(default)]
    pub ontology_mappings: Vec<FieldOntologyMapping>,

    // Status
    pub status: FileStatus,
    pub validation_errors: Vec<String>,
    pub validation_warnings: Vec<String>,

    // Organization
    pub tags: Vec<String>,
    pub metadata: HashMap<String, serde_json::Value>,

    // Governance
    pub sensitivity_level: Option<SensitivityLevel>,
    pub retention_policy: Option<String>,
    pub access_control: Option<AccessControl>,

    // Timestamps
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_accessed: Option<DateTime<Utc>>,

    // Versioning (optional)
    pub version: Option<u32>,
    pub previous_versions: Vec<String>,
}

/// File owner information
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct FileOwner {
    pub user_id: String,
    pub email: String,
    pub name: String,
}

/// File schema with field definitions
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct FileSchema {
    pub fields: Vec<SchemaField>,
    pub total_rows: u64,
    pub estimated_rows: Option<u64>,
    pub last_scanned: DateTime<Utc>,
}

/// Individual field in schema
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SchemaField {
    pub name: String,
    #[serde(rename = "type")]
    pub field_type: FieldType,
    pub nullable: bool,
    pub sample_values: Vec<String>,
    pub is_pii: Option<bool>,
    pub pii_type: Option<PiiType>,
}

/// Mapping of a field to an ontology concept
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct FieldOntologyMapping {
    pub field_name: String,
    pub ontology_id: String,
    pub concept_uri: String,
    pub concept_label: String,
    pub similarity: f64,
    pub confidence: f64,
    pub method: String, // "exact", "synonym", "embedding", etc.
    pub mapped_at: DateTime<Utc>,
}

/// File status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum FileStatus {
    Validated,
    Warning,
    Error,
    Processing,
    Pending,
}

/// Field data types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FieldType {
    String,
    Integer,
    Float,
    Boolean,
    Timestamp,
    Date,
}

/// PII types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum PiiType {
    Email,
    Phone,
    Ssn,
    CreditCard,
    Custom,
}

/// Sensitivity classification
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum SensitivityLevel {
    Public,
    Internal,
    Confidential,
    Restricted,
}

/// Access control configuration
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AccessControl {
    pub owner_only: Option<bool>,
    pub groups: Option<Vec<String>>,
    pub users: Option<Vec<String>>,
}

// ============================================================================
// Folder Management
// ============================================================================

/// Folder for organizing files
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Folder {
    pub id: String,
    pub name: String,
    pub parent_id: Option<String>,
    pub description: Option<String>,
    pub path: String, // Full path, e.g., "/Sales/Q1-2024"
    pub file_count: usize,
    pub subfolder_count: usize,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub children: Option<Vec<Folder>>, // For tree view
}

// ============================================================================
// Scan Results
// ============================================================================

/// Result of file scanning
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ScanResult {
    pub detected_fields: Vec<SchemaField>,
    pub total_rows: Option<u64>,
    pub estimated_rows: Option<u64>,
    pub delimiter_detected: Option<String>,
    pub encoding_detected: Option<String>,
    pub has_header_detected: Option<bool>,
    pub scan_timestamp: DateTime<Utc>,
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
}

// ============================================================================
// Import Jobs
// ============================================================================

/// Bulk import job
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ImportJob {
    pub job_id: String,
    pub status: JobStatus,
    pub total_files: usize,
    pub processed_files: usize,
    pub successful_files: usize,
    pub failed_files: usize,
    pub progress_percent: f32,
    pub results: Vec<ImportResult>,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub duration_ms: Option<u64>,
}

/// Job status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum JobStatus {
    Processing,
    Completed,
    Failed,
    Partial,
}

/// Result of importing individual file
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ImportResult {
    pub file_name: String,
    pub file_id: Option<String>,
    pub status: ImportFileStatus,
    pub error: Option<String>,
    pub scan_result: Option<ScanResult>,
}

/// Status of individual file import
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum ImportFileStatus {
    Success,
    Error,
    Warning,
}

// ============================================================================
// API Request/Response Types
// ============================================================================

/// Request to list files
#[derive(Debug, Deserialize, ToSchema)]
pub struct ListFilesRequest {
    pub folder_id: Option<String>,
    pub tags: Option<Vec<String>>,
    pub search: Option<String>,
    pub status: Option<FileStatus>,
    pub owner: Option<String>,
    pub sort: Option<SortField>,
    pub order: Option<SortOrder>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

/// Sort field options
#[derive(Debug, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum SortField {
    Name,
    Modified,
    Size,
    Created,
}

/// Sort order
#[derive(Debug, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum SortOrder {
    Asc,
    Desc,
}

/// Response for list files
#[derive(Debug, Serialize, ToSchema)]
pub struct ListFilesResponse {
    pub files: Vec<DataFile>,
    pub total: usize,
    pub page: usize,
    pub page_size: usize,
}

/// Request to create/upload file
#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateFileRequest {
    pub folder_id: Option<String>,
    pub tags: Option<Vec<String>>,
    pub description: Option<String>,
    pub auto_scan: Option<bool>,
}

/// Response from creating file
#[derive(Debug, Serialize, ToSchema)]
pub struct CreateFileResponse {
    pub file_id: String,
    pub file_path: String,
    pub scan_result: Option<ScanResult>,
    pub status: FileStatus,
}

/// Request to update file metadata
#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateFileRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub tags: Option<Vec<String>>,
    pub folder_id: Option<String>,
    pub metadata: Option<HashMap<String, serde_json::Value>>,
    pub schema: Option<FileSchema>, // Allow persisting scanned schema
    pub ontology_mappings: Vec<FieldOntologyMapping>, // Allow persisting ontology mappings
}

/// Request to scan file
#[derive(Debug, Deserialize, ToSchema)]
pub struct ScanFileRequest {
    pub delimiter: Option<String>,
    pub encoding: Option<String>,
    pub has_header: Option<bool>,
    pub sample_rows: Option<usize>,
    pub auto_save: Option<bool>, // If true, automatically persist schema to file
    pub map_to_ontology: Option<bool>, // If true, automatically map fields to ontology
    pub ontology_id: Option<String>, // Which ontology to map to
}

/// Response from file preview
#[derive(Debug, Serialize, ToSchema)]
pub struct FilePreviewResponse {
    pub headers: Vec<String>,
    pub rows: Vec<Vec<serde_json::Value>>,
    pub total_rows: u64,
}

/// Request for bulk upload
#[derive(Debug, Deserialize, ToSchema)]
pub struct BulkUploadRequest {
    pub defaults: BulkUploadDefaults,
}

/// Default settings for bulk upload
#[derive(Debug, Deserialize, ToSchema)]
pub struct BulkUploadDefaults {
    pub folder_id: Option<String>,
    pub tags: Option<Vec<String>>,
    pub auto_scan: Option<bool>,
    pub delimiter: Option<String>,
    pub encoding: Option<String>,
    pub has_header: Option<bool>,
}

/// Response from bulk upload
#[derive(Debug, Serialize, ToSchema)]
pub struct BulkUploadResponse {
    pub job_id: String,
    pub status: JobStatus,
    pub total_files: usize,
}

/// Request for directory import
#[derive(Debug, Deserialize, ToSchema)]
pub struct BulkImportDirectoryRequest {
    pub directory_path: String,
    pub include_subdirectories: Option<bool>,
    pub maintain_structure: Option<bool>,
    pub folder_id: Option<String>,
    pub defaults: BulkUploadDefaults,
    pub filters: Option<DirectoryFilters>,
}

/// Filters for directory import
#[derive(Debug, Deserialize, ToSchema)]
pub struct DirectoryFilters {
    pub pattern: Option<String>,
    pub min_size: Option<u64>,
    pub max_size: Option<u64>,
    pub modified_after: Option<DateTime<Utc>>,
}

/// Request for bulk update
#[derive(Debug, Deserialize, ToSchema)]
pub struct BulkUpdateRequest {
    pub file_ids: Vec<String>,
    pub updates: BulkUpdates,
}

/// Bulk update operations
#[derive(Debug, Deserialize, ToSchema)]
pub struct BulkUpdates {
    pub folder_id: Option<String>,
    pub tags: Option<TagOperation>,
    pub metadata: Option<HashMap<String, serde_json::Value>>,
}

/// Tag operation
#[derive(Debug, Deserialize, ToSchema)]
pub struct TagOperation {
    pub action: TagAction,
    pub values: Vec<String>,
}

/// Tag action type
#[derive(Debug, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum TagAction {
    Add,
    Remove,
    Set,
}

/// Response from bulk update
#[derive(Debug, Serialize, ToSchema)]
pub struct BulkUpdateResponse {
    pub success: bool,
    pub updated_count: usize,
    pub errors: Option<Vec<BulkOperationError>>,
}

/// Error in bulk operation
#[derive(Debug, Serialize, ToSchema)]
pub struct BulkOperationError {
    pub file_id: String,
    pub error: String,
}

/// Request for bulk delete
#[derive(Debug, Deserialize, ToSchema)]
pub struct BulkDeleteRequest {
    pub file_ids: Vec<String>,
    pub force: Option<bool>,
}

/// Response from bulk delete
#[derive(Debug, Serialize, ToSchema)]
pub struct BulkDeleteResponse {
    pub success: bool,
    pub deleted_count: usize,
    pub errors: Option<Vec<BulkDeleteError>>,
}

/// Error in bulk delete
#[derive(Debug, Serialize, ToSchema)]
pub struct BulkDeleteError {
    pub file_id: String,
    pub error: String,
    pub dependencies: Option<Dependencies>,
}

/// Dependencies for a file
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct Dependencies {
    pub workflows: Vec<String>,
    pub datasets: Vec<String>,
}

/// Request to bulk scan files
#[derive(Debug, Deserialize, ToSchema)]
pub struct BulkScanRequest {
    pub file_ids: Vec<String>,
    pub delimiter: Option<String>,
    pub encoding: Option<String>,
    pub has_header: Option<bool>,
    pub sample_rows: Option<usize>,
    pub auto_save: Option<bool>,
    pub map_to_ontology: Option<bool>, // Auto-map fields to ontology
    pub ontology_id: Option<String>,   // Which ontology to map to
}

/// Response from bulk scan (async job-based)
#[derive(Debug, Serialize, ToSchema)]
pub struct BulkScanResponse {
    pub job_id: String,
    pub status: JobStatus,
    pub total_files: usize,
}

/// Request to create folder
#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateFolderRequest {
    pub name: String,
    pub parent_id: Option<String>,
    pub description: Option<String>,
}

/// Request to update folder
#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateFolderRequest {
    pub name: Option<String>,
    pub parent_id: Option<String>,
    pub description: Option<String>,
}

/// Response from folder operations
#[derive(Debug, Serialize, ToSchema)]
pub struct FolderResponse {
    pub success: bool,
    pub error: Option<String>,
}

/// Response from listing folders
#[derive(Debug, Serialize, ToSchema)]
pub struct ListFoldersResponse {
    pub folders: Vec<Folder>,
}

/// Response from listing tags
#[derive(Debug, Serialize, ToSchema)]
pub struct ListTagsResponse {
    pub tags: Vec<TagInfo>,
}

/// Tag information with usage count
#[derive(Debug, Serialize, ToSchema)]
pub struct TagInfo {
    pub name: String,
    pub count: usize,
    pub color: Option<String>,
}

/// Request for advanced search
#[derive(Debug, Deserialize, ToSchema)]
pub struct SearchRequest {
    pub query: String,
    pub filters: Option<SearchFilters>,
    pub sort: Option<SearchSort>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

/// Search filters
#[derive(Debug, Deserialize, ToSchema)]
pub struct SearchFilters {
    pub folder_ids: Option<Vec<String>>,
    pub tags: Option<Vec<String>>,
    pub status: Option<Vec<FileStatus>>,
    pub owner: Option<Vec<String>>,
    pub date_range: Option<DateRange>,
    pub has_pii: Option<bool>,
    pub min_rows: Option<u64>,
    pub max_rows: Option<u64>,
}

/// Date range for filtering
#[derive(Debug, Deserialize, ToSchema)]
pub struct DateRange {
    pub from: DateTime<Utc>,
    pub to: DateTime<Utc>,
}

/// Sort configuration for search
#[derive(Debug, Deserialize, ToSchema)]
pub struct SearchSort {
    pub field: String,
    pub order: SortOrder,
}

/// Response from search
#[derive(Debug, Serialize, ToSchema)]
pub struct SearchResponse {
    pub results: Vec<DataFile>,
    pub total: usize,
    pub facets: Option<SearchFacets>,
}

/// Search facets for filtering UI
#[derive(Debug, Serialize, ToSchema)]
pub struct SearchFacets {
    pub tags: HashMap<String, usize>,
    pub owners: HashMap<String, usize>,
    pub status: HashMap<String, usize>,
}

/// Lineage information
#[derive(Debug, Serialize, ToSchema)]
pub struct LineageResponse {
    pub file_id: String,
    pub downstream: Vec<LineageNode>,
    pub upstream: Vec<LineageNode>,
}

/// Node in lineage graph
#[derive(Debug, Serialize, ToSchema)]
pub struct LineageNode {
    #[serde(rename = "type")]
    pub node_type: LineageNodeType,
    pub id: String,
    pub name: String,
    pub status: Option<String>,
}

/// Type of lineage node
#[derive(Debug, Serialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum LineageNodeType {
    Workflow,
    Dataset,
    Model,
    Datasource,
}

/// Impact analysis response
#[derive(Debug, Serialize, ToSchema)]
pub struct ImpactAnalysisResponse {
    pub can_delete: bool,
    pub can_modify: bool,
    pub impact: ImpactDetails,
}

/// Impact details
#[derive(Debug, Serialize, ToSchema)]
pub struct ImpactDetails {
    pub workflows_affected: usize,
    pub datasets_affected: usize,
    pub critical_workflows: Vec<String>,
    pub recommendations: Vec<String>,
}

/// Library statistics
#[derive(Debug, Serialize, ToSchema)]
pub struct LibraryStatsResponse {
    pub total_files: usize,
    pub total_size_bytes: u64,
    pub total_rows: u64,
    pub files_by_status: HashMap<String, usize>,
    pub files_by_folder: HashMap<String, usize>,
    pub files_with_pii: usize,
    pub top_tags: Vec<TagInfo>,
    pub recent_uploads: Vec<DataFile>,
    pub most_used: Vec<DataFile>,
}

/// Usage statistics for a file
#[derive(Debug, Serialize, ToSchema)]
pub struct UsageStatsResponse {
    pub file_id: String,
    pub times_used: usize,
    pub workflows_count: usize,
    pub last_accessed: Option<DateTime<Utc>>,
    pub access_count_30d: usize,
    pub top_users: Vec<UserUsage>,
}

/// User usage information
#[derive(Debug, Serialize, ToSchema)]
pub struct UserUsage {
    pub user: String,
    pub count: usize,
}

// ============================================================================
// Datasource Registration Validation
// ============================================================================

/// Response from validating file for datasource registration
#[derive(Debug, Serialize, ToSchema)]
pub struct ValidateFileForRegistrationResponse {
    pub can_register: bool,
    pub issues: Vec<String>,
    pub inferred_config: InferredConfig,
}

/// Inferred configuration for datasource registration
#[derive(Debug, Serialize, ToSchema)]
pub struct InferredConfig {
    pub connector_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delimiter: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub has_header: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub row_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column_count: Option<usize>,
}
