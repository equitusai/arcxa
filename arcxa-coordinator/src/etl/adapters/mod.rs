//! ETL Adapters
//!
//! This module provides adapters that bridge the new ETL abstractions
//! (FormatReader, DataDestination, etc.) with the existing workflow system.
//!
//! ## Design Pattern: Layered Adapter
//!
//! The adapter pattern allows gradual migration from old to new abstractions
//! while maintaining 100% backward compatibility with existing workflows.
//!
//! ```text
//! ┌─────────────────────────────────────────────────────┐
//! │         Existing Workflow System                    │
//! │         (YAML workflows, ActionExecutor)            │
//! └─────────────────────────────────────────────────────┘
//!                        ↓
//! ┌─────────────────────────────────────────────────────┐
//! │              Adapter Layer                          │
//! │  FormatReaderAdapter, DataDestinationAdapter, etc.  │
//! └─────────────────────────────────────────────────────┘
//!                        ↓
//! ┌─────────────────────────────────────────────────────┐
//! │           New ETL Abstractions                      │
//! │  FormatReader, DataDestination, PipelineExecutor    │
//! └─────────────────────────────────────────────────────┘
//! ```
//!
//! ## Adapters Provided
//!
//! - **FormatReaderAdapter**: Wraps FormatReader as a workflow Transformer
//! - **DataDestinationAdapter**: Wraps DataDestination as a workflow Transformer
//! - **PipelineTransformerAdapter**: Wraps entire Pipeline as a workflow Transformer
//!
//! ## Usage Example
//!
//! ```rust,ignore
//! use graphica_coordinator::etl::adapters::FormatReaderAdapter;
//! use graphica_coordinator::etl::readers::CsvReader;
//!
//! // Create a CSV reader
//! let csv_reader = CsvReader::new(file_store, "file_123", CsvOptions::default());
//!
//! // Wrap it as a workflow transformer
//! let adapter = FormatReaderAdapter::new(
//!     Box::new(csv_reader),
//!     "csv_parser".to_string()
//! );
//!
//! // Use in existing workflow execution
//! let mut data = json!({});
//! adapter.transform(&config, &mut data, Some(&context)).await?;
//! ```

pub mod destination_adapter;
pub mod format_reader_adapter;
pub mod pipeline_adapter;

pub use destination_adapter::DataDestinationAdapter;
pub use format_reader_adapter::FormatReaderAdapter;
pub use pipeline_adapter::PipelineTransformerAdapter;
