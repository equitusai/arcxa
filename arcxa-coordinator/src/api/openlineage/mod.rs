//! OpenLineage API Module
//!
//! This module provides REST API endpoints compatible with the OpenLineage 1.0.0 specification.
//!
//! ## Endpoints
//!
//! - `POST /api/v1/lineage` - Ingest OpenLineage events
//! - `GET /api/v1/lineage/export` - Export lineage in OpenLineage format
//! - `GET /api/v1/namespaces` - List namespaces
//! - `GET /api/v1/namespaces/{namespace}/jobs` - List jobs in namespace
//! - `GET /api/v1/namespaces/{namespace}/jobs/{job}` - Get job details
//! - `GET /api/v1/namespaces/{namespace}/jobs/{job}/runs` - List runs for job
//! - `GET /api/v1/namespaces/{namespace}/jobs/{job}/runs/{run_id}` - Get run details

pub mod handlers;

pub use handlers::*;
