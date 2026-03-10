//! GDPR Data Export Module
//!
//! Implements GDPR Article 20: Right to Data Portability
//!
//! This module provides comprehensive data export capabilities,
//! allowing users to obtain their personal data in structured,
//! machine-readable formats.

pub mod converters;
pub mod discovery;
pub mod executor;
pub mod storage;
pub mod types;

// Re-export commonly used types
pub use converters::{ExportPackage, FormatConverter};
pub use discovery::{DataDiscoveryService, DataReference, DiscoveryResult};
pub use executor::ExportExecutor;
pub use storage::ExportJobStore;
pub use types::{
    DataCategory, DataSource, ExportError, ExportErrorCode, ExportErrorInfo, ExportFormat,
    ExportJob, ExportMetadata, ExportPhase, ExportProgressInfo, ExportRequest,
    ExportRequestResponse, ExportResult, ExportStatus, ExportStatusResponse, TimeRange,
};
