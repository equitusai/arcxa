//! Memory Monitoring and Adaptive Batching
//!
//! Production-grade memory pressure monitoring with adaptive batch sizing.
//!
//! ## Features
//!
//! - Real-time heap usage tracking (Linux /proc/self/statm)
//! - Memory pressure ratio calculation (0.0-1.0)
//! - Adaptive batch sizing based on pressure thresholds
//! - Prometheus metrics for observability
//! - Backpressure signaling for flow control
//!
//! ## Usage
//!
//! ```rust,no_run
//! use graphica_core::orchestration::workflow::memory_monitor::{MemoryMonitor, MemoryConfig};
//!
//! # async fn example() -> anyhow::Result<()> {
//! let config = MemoryConfig {
//!     max_heap_mb: 4096,           // 4GB
//!     warning_threshold: 0.70,      // 70%
//!     critical_threshold: 0.85,     // 85%
//!     min_batch_size: 100,
//!     max_batch_size: 100_000,
//!     default_batch_size: 10_000,
//! };
//!
//! let monitor = MemoryMonitor::new(config);
//!
//! // Update pressure and get adaptive batch size
//! monitor.update_pressure().await?;
//! let batch_size = monitor.get_adaptive_batch_size().await;
//!
//! if monitor.should_backpressure().await {
//!     println!("Applying backpressure due to memory pressure");
//! }
//! # Ok(())
//! # }
//! ```

use anyhow::{anyhow, Result};
use lazy_static::lazy_static;
use prometheus::{register_gauge, Gauge};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, warn};

lazy_static! {
    static ref MEMORY_PRESSURE_RATIO: Gauge = register_gauge!(
        "graphica_memory_pressure_ratio",
        "Current memory pressure ratio (0.0-1.0)"
    )
    .unwrap();
    static ref HEAP_USED_BYTES: Gauge = register_gauge!(
        "graphica_heap_used_bytes",
        "Current heap memory usage in bytes"
    )
    .unwrap();
    static ref BATCH_SIZE_ADAPTIVE: Gauge = register_gauge!(
        "graphica_batch_size_adaptive",
        "Current adaptive batch size"
    )
    .unwrap();
    static ref ROCKSDB_STATE_SIZE_BYTES: Gauge = register_gauge!(
        "graphica_rocksdb_state_size_bytes",
        "RocksDB state backend size in bytes"
    )
    .unwrap();
}

/// Memory monitoring configuration
///
/// Controls memory pressure thresholds and adaptive batch sizing behavior.
#[derive(Debug, Clone)]
pub struct MemoryConfig {
    /// Maximum heap usage before backpressure (MB)
    pub max_heap_mb: usize,
    /// Pressure ratio to trigger warnings (0.7 = 70%)
    pub warning_threshold: f64,
    /// Pressure ratio to trigger backpressure (0.85 = 85%)
    pub critical_threshold: f64,
    /// Minimum batch size under pressure
    pub min_batch_size: usize,
    /// Maximum batch size
    pub max_batch_size: usize,
    /// Default batch size
    pub default_batch_size: usize,
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            max_heap_mb: 4096,        // 4GB default
            warning_threshold: 0.70,  // 70%
            critical_threshold: 0.85, // 85%
            min_batch_size: 100,
            max_batch_size: 100_000,
            default_batch_size: 10_000,
        }
    }
}

/// Memory pressure monitor with adaptive batch sizing
///
/// Tracks heap usage and adjusts batch sizes to prevent OOM conditions.
pub struct MemoryMonitor {
    config: MemoryConfig,
    current_pressure: Arc<RwLock<f64>>,
    adaptive_batch_size: Arc<RwLock<usize>>,
}

impl MemoryMonitor {
    /// Create a new memory monitor
    pub fn new(config: MemoryConfig) -> Self {
        let initial_batch_size = config.default_batch_size;

        Self {
            config,
            current_pressure: Arc::new(RwLock::new(0.0)),
            adaptive_batch_size: Arc::new(RwLock::new(initial_batch_size)),
        }
    }

    /// Update memory pressure based on current heap usage
    ///
    /// Returns the current pressure ratio (0.0-1.0).
    pub async fn update_pressure(&self) -> Result<f64> {
        // Get current heap usage (platform-specific)
        #[cfg(target_os = "linux")]
        let heap_used_mb = self.get_heap_usage_linux()?;

        #[cfg(not(target_os = "linux"))]
        let heap_used_mb = self.get_heap_usage_fallback()?;

        let pressure = heap_used_mb as f64 / self.config.max_heap_mb as f64;
        let pressure = pressure.min(1.0).max(0.0);

        // Update internal state
        *self.current_pressure.write().await = pressure;

        // Update metrics
        MEMORY_PRESSURE_RATIO.set(pressure);
        HEAP_USED_BYTES.set((heap_used_mb * 1024 * 1024) as f64);

        // Adjust batch size based on pressure
        self.adjust_batch_size(pressure).await;

        debug!(
            "Memory pressure updated: {:.1}% ({} MB / {} MB)",
            pressure * 100.0,
            heap_used_mb,
            self.config.max_heap_mb
        );

        Ok(pressure)
    }

    /// Get current memory pressure ratio (0.0-1.0)
    pub async fn get_pressure(&self) -> f64 {
        *self.current_pressure.read().await
    }

    /// Check if we should apply backpressure
    ///
    /// Returns true when pressure exceeds critical threshold.
    pub async fn should_backpressure(&self) -> bool {
        let pressure = self.get_pressure().await;
        pressure >= self.config.critical_threshold
    }

    /// Get adaptive batch size based on current memory pressure
    pub async fn get_adaptive_batch_size(&self) -> usize {
        *self.adaptive_batch_size.read().await
    }

    /// Adjust batch size based on memory pressure
    ///
    /// Implements linear scaling between warning and critical thresholds.
    async fn adjust_batch_size(&self, pressure: f64) {
        let new_size = if pressure < self.config.warning_threshold {
            // Low pressure: use default or max batch size
            self.config.default_batch_size
        } else if pressure < self.config.critical_threshold {
            // Warning: scale down linearly
            let range = self.config.critical_threshold - self.config.warning_threshold;
            let scale = (self.config.critical_threshold - pressure) / range;
            let size = (self.config.default_batch_size as f64 * scale) as usize;
            size.max(self.config.min_batch_size)
        } else {
            // Critical: use minimum batch size
            self.config.min_batch_size
        };

        *self.adaptive_batch_size.write().await = new_size;
        BATCH_SIZE_ADAPTIVE.set(new_size as f64);

        if new_size < self.config.default_batch_size {
            warn!(
                "Reduced batch size to {} due to memory pressure ({:.1}%)",
                new_size,
                pressure * 100.0
            );
        }
    }

    /// Get heap usage on Linux using /proc/self/statm
    #[cfg(target_os = "linux")]
    fn get_heap_usage_linux(&self) -> Result<usize> {
        use std::fs;

        // Read /proc/self/statm for memory usage
        let statm = fs::read_to_string("/proc/self/statm")
            .map_err(|e| anyhow!("Failed to read /proc/self/statm: {}", e))?;

        let fields: Vec<&str> = statm.split_whitespace().collect();
        if fields.len() < 2 {
            return Err(anyhow!("Invalid /proc/self/statm format"));
        }

        // Second field is resident set size in pages
        let rss_pages: usize = fields[1]
            .parse()
            .map_err(|e| anyhow!("Failed to parse RSS: {}", e))?;

        // Convert pages to MB (assume 4KB pages)
        let rss_mb = (rss_pages * 4) / 1024;

        Ok(rss_mb)
    }

    /// Fallback heap usage estimation for non-Linux platforms
    #[cfg(not(target_os = "linux"))]
    fn get_heap_usage_fallback(&self) -> Result<usize> {
        // Fallback: estimate based on RocksDB size (rough approximation)
        warn!("Memory monitoring not fully supported on this platform, using estimation");
        Ok(512) // Return conservative estimate
    }

    /// Monitor RocksDB state size
    ///
    /// Updates metrics for state backend size tracking.
    pub fn update_rocksdb_size(&self, size_bytes: u64) {
        ROCKSDB_STATE_SIZE_BYTES.set(size_bytes as f64);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_memory_monitor_creation() {
        let config = MemoryConfig::default();
        let monitor = MemoryMonitor::new(config.clone());

        // Initial pressure should be 0
        let pressure = monitor.get_pressure().await;
        assert_eq!(pressure, 0.0);

        // Initial batch size should be default
        let batch_size = monitor.get_adaptive_batch_size().await;
        assert_eq!(batch_size, config.default_batch_size);
    }

    #[tokio::test]
    async fn test_adaptive_batch_sizing_low_pressure() {
        let config = MemoryConfig {
            max_heap_mb: 1000,
            warning_threshold: 0.70,
            critical_threshold: 0.85,
            min_batch_size: 100,
            max_batch_size: 100_000,
            default_batch_size: 10_000,
        };

        let monitor = MemoryMonitor::new(config.clone());

        // Simulate low pressure (50%)
        monitor.adjust_batch_size(0.50).await;
        let batch_size = monitor.get_adaptive_batch_size().await;
        assert_eq!(batch_size, 10_000);
    }

    #[tokio::test]
    async fn test_adaptive_batch_sizing_warning_pressure() {
        let config = MemoryConfig {
            max_heap_mb: 1000,
            warning_threshold: 0.70,
            critical_threshold: 0.85,
            min_batch_size: 100,
            max_batch_size: 100_000,
            default_batch_size: 10_000,
        };

        let monitor = MemoryMonitor::new(config.clone());

        // Simulate warning pressure (77.5% - midpoint)
        monitor.adjust_batch_size(0.775).await;
        let batch_size = monitor.get_adaptive_batch_size().await;

        // Should be halfway between default and min (around 5000)
        assert!(batch_size < 10_000);
        assert!(batch_size >= 100);
    }

    #[tokio::test]
    async fn test_adaptive_batch_sizing_critical_pressure() {
        let config = MemoryConfig {
            max_heap_mb: 1000,
            warning_threshold: 0.70,
            critical_threshold: 0.85,
            min_batch_size: 100,
            max_batch_size: 100_000,
            default_batch_size: 10_000,
        };

        let monitor = MemoryMonitor::new(config.clone());

        // Simulate critical pressure (90%)
        monitor.adjust_batch_size(0.90).await;
        let batch_size = monitor.get_adaptive_batch_size().await;
        assert_eq!(batch_size, 100);
    }

    #[tokio::test]
    async fn test_should_backpressure() {
        let config = MemoryConfig {
            max_heap_mb: 1000,
            warning_threshold: 0.70,
            critical_threshold: 0.85,
            min_batch_size: 100,
            max_batch_size: 100_000,
            default_batch_size: 10_000,
        };

        let monitor = MemoryMonitor::new(config.clone());

        // Low pressure - no backpressure
        *monitor.current_pressure.write().await = 0.70;
        assert!(!monitor.should_backpressure().await);

        // Critical pressure - apply backpressure
        *monitor.current_pressure.write().await = 0.85;
        assert!(monitor.should_backpressure().await);

        // Extreme pressure - apply backpressure
        *monitor.current_pressure.write().await = 0.95;
        assert!(monitor.should_backpressure().await);
    }

    #[tokio::test]
    async fn test_rocksdb_size_update() {
        let monitor = MemoryMonitor::new(MemoryConfig::default());

        // Should not panic
        monitor.update_rocksdb_size(1024 * 1024 * 500); // 500 MB
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn test_heap_usage_linux() {
        let monitor = MemoryMonitor::new(MemoryConfig::default());

        // Should successfully read from /proc/self/statm
        let result = monitor.get_heap_usage_linux();
        assert!(result.is_ok());

        let usage_mb = result.unwrap();
        // Sanity check: should be > 0 and < max_heap_mb
        assert!(usage_mb > 0);
        assert!(usage_mb < monitor.config.max_heap_mb);
    }

    #[tokio::test]
    async fn test_update_pressure_integration() {
        let config = MemoryConfig {
            max_heap_mb: 4096,
            warning_threshold: 0.70,
            critical_threshold: 0.85,
            min_batch_size: 100,
            max_batch_size: 100_000,
            default_batch_size: 10_000,
        };

        let monitor = MemoryMonitor::new(config);

        // Update pressure (should succeed on all platforms)
        let result = monitor.update_pressure().await;
        assert!(result.is_ok());

        let pressure = result.unwrap();
        assert!(pressure >= 0.0);
        assert!(pressure <= 1.0);
    }
}
