//! # Graphica CLI Utilities
//!
//! Command-line tools for managing and operating the Graphica data governance platform.
//!
//! ## Available Commands
//!
//! - **migrate**: Database migration tool for upgrading storage formats
//! - **admin**: Operator-facing coordinator administration
//! - **backup**: Backup and restore utilities (future)
//! - **validate**: Workflow and configuration validation (future)

pub mod migration_evidence;
pub mod sos;

/// Common CLI utilities and helpers
pub mod utils {
    use anyhow::Result;
    use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

    /// Initialize tracing for CLI applications
    pub fn init_tracing() -> Result<()> {
        tracing_subscriber::registry()
            .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
            .with(tracing_subscriber::fmt::layer())
            .init();

        Ok(())
    }
}
