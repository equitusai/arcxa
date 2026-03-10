//! Loader Orchestration Module
//!
//! Background job management and execution for ETL loader operations.
//! Provides async task orchestration, state management, and lifecycle control.
//!
//! ## Architecture
//!
//! ```text
//! REST API → LoaderJobManager → LoaderWorker (tokio task)
//!                                     ↓
//!                            AsyncCsvReader (streaming)
//!                                     ↓
//!                   Transform → DEL → DB2 LOAD → Checkpoint → DLQ
//! ```
//!
//! ## Components
//!
//! - **LoaderJobManager**: Central orchestrator managing job lifecycle,
//!   spawning background tasks, tracking progress, handling cancellation
//!
//! - **LoaderJobState**: In-memory job state stored in thread-safe DashMap
//!
//! - **LoaderWorker**: Background async task that executes ETL pipeline
//!
//! - **LoaderJobConfig**: Configuration for job manager and workers
//!
//! ## Usage
//!
//! ```rust,ignore
//! use graphica_coordinator::mapping::loader::orchestration::*;
//!
//! // Create job manager
//! let config = LoaderJobConfig::default();
//! let manager = LoaderJobManager::new(metrics, db2_pool, config)?;
//!
//! // Register and start job
//! let job_id = manager.register_job("job_1", create_request)?;
//! manager.start_job(&job_id).await?;
//!
//! // Query status
//! let status = manager.get_job_status(&job_id)?;
//! println!("Progress: {:.1}%", status.progress.progress_percent);
//!
//! // Cancel if needed
//! manager.cancel_job(&job_id).await?;
//! ```

pub mod async_csv_reader;
pub mod config;
pub mod job_manager;
pub mod job_state;
pub mod worker;
pub mod workflow_adapter;

// Re-export primary types
pub use async_csv_reader::{
    detect_delimiter, detect_encoding, AsyncCsvReader, AsyncCsvReaderConfig, CsvError,
    ReaderProgress,
};
pub use config::{DmlMode, LoaderJobConfig, LoaderWorkerConfig};
pub use job_manager::LoaderJobManager;
pub use job_state::{JobProgress, JobResult, LoaderJobState, LoaderJobStatus, LoaderJobSummary};
pub use worker::LoaderWorker;
