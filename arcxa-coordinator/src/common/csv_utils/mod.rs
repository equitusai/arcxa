//! Unified CSV Utilities
//!
//! Shared CSV parsing, detection, and analysis utilities used across:
//! - File Library (schema detection, PII analysis)
//! - Data Loader (streaming production reads)
//! - R2RML Executor (RDF mapping)
//! - ETL Sources (data extraction)
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────┐
//! │     Applications                     │
//! │  (File Library, Loader, R2RML)      │
//! └────────────┬────────────────────────┘
//!              │
//!    ┌─────────┴──────────┐
//!    │                    │
//! ┌──▼────────┐    ┌──────▼───────┐
//! │ Analysis  │    │  Streaming   │
//! │  Layer    │    │    Layer     │
//! │           │    │              │
//! │ • Schema  │    │ • Production │
//! │ • PII     │    │ • Progress   │
//! │ • Quality │    │ • Errors     │
//! └──┬────────┘    └──────┬───────┘
//!    │                    │
//!    └─────────┬──────────┘
//!              │
//!       ┌──────▼─────────┐
//!       │  Core Layer    │
//!       │                │
//!       │ • Detection    │
//!       │ • Parsing      │
//!       │ • Validation   │
//!       └────────────────┘
//! ```

pub mod analysis;
pub mod core;
pub mod streaming;

// Re-exports for convenience
pub use core::{
    detect_delimiter_advanced, detect_encoding_advanced, parse_csv_line_advanced,
    CsvDetectionConfig, CsvEncoding,
};

pub use analysis::{FieldTypeInference, HeaderDetection, PiiDetection, SchemaInferenceConfig};

pub use streaming::{CsvError, CsvReaderConfig, CsvStreamReader, ReaderProgress};
