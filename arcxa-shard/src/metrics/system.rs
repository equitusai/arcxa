//! System Metrics Collection
//!
//! Collects system-level metrics including memory, disk, and CPU usage.

use anyhow::{Context, Result};
use std::path::Path;
use tracing::warn;

/// System-level resource metrics
#[derive(Debug, Clone)]
pub struct SystemMetrics {
    /// Total system memory in MB
    pub total_memory_mb: u64,

    /// Available memory in MB
    pub available_memory_mb: u64,

    /// Total disk space in MB
    pub total_disk_mb: u64,

    /// Available disk space in MB
    pub available_disk_mb: u64,

    /// Number of CPU cores
    pub cpu_cores: u32,

    /// Current memory usage in bytes
    pub memory_usage_bytes: u64,

    /// Current disk usage in bytes
    pub disk_usage_bytes: u64,
}

/// Collector for system metrics
pub struct SystemMetricsCollector {
    /// Path to monitor for disk usage
    data_path: std::path::PathBuf,
}

impl SystemMetricsCollector {
    /// Create a new system metrics collector
    pub fn new(data_path: impl AsRef<Path>) -> Self {
        Self {
            data_path: data_path.as_ref().to_path_buf(),
        }
    }

    /// Collect current system metrics
    pub fn collect(&self) -> Result<SystemMetrics> {
        let total_memory_mb = self.get_total_memory_mb()?;
        let available_memory_mb = self.get_available_memory_mb()?;
        let (total_disk_mb, available_disk_mb) = self.get_disk_info()?;
        let cpu_cores = num_cpus::get() as u32;

        // Calculate usage
        let memory_usage_bytes = (total_memory_mb.saturating_sub(available_memory_mb)) * 1024 * 1024;
        let disk_usage_bytes = (total_disk_mb.saturating_sub(available_disk_mb)) * 1024 * 1024;

        Ok(SystemMetrics {
            total_memory_mb,
            available_memory_mb,
            total_disk_mb,
            available_disk_mb,
            cpu_cores,
            memory_usage_bytes,
            disk_usage_bytes,
        })
    }

    /// Get total system memory in MB
    fn get_total_memory_mb(&self) -> Result<u64> {
        #[cfg(target_os = "linux")]
        {
            self.get_memory_from_proc().map(|(total, _)| total)
        }

        #[cfg(not(target_os = "linux"))]
        {
            // Fallback: estimate 8GB
            warn!("Memory detection not supported on this platform, using default 8192 MB");
            Ok(8192)
        }
    }

    /// Get available system memory in MB
    fn get_available_memory_mb(&self) -> Result<u64> {
        #[cfg(target_os = "linux")]
        {
            self.get_memory_from_proc().map(|(_, available)| available)
        }

        #[cfg(not(target_os = "linux"))]
        {
            // Fallback: estimate 4GB available
            warn!("Memory detection not supported on this platform, using default 4096 MB available");
            Ok(4096)
        }
    }

    /// Read memory info from /proc/meminfo (Linux)
    #[cfg(target_os = "linux")]
    fn get_memory_from_proc(&self) -> Result<(u64, u64)> {
        let meminfo = std::fs::read_to_string("/proc/meminfo")
            .context("Failed to read /proc/meminfo")?;

        let mut total_kb = 0u64;
        let mut available_kb = 0u64;

        for line in meminfo.lines() {
            if line.starts_with("MemTotal:") {
                total_kb = Self::parse_meminfo_value(line)?;
            } else if line.starts_with("MemAvailable:") {
                available_kb = Self::parse_meminfo_value(line)?;
            }
        }

        Ok((total_kb / 1024, available_kb / 1024))
    }

    /// Parse a value from /proc/meminfo line (e.g., "MemTotal:  16384000 kB")
    #[cfg(target_os = "linux")]
    fn parse_meminfo_value(line: &str) -> Result<u64> {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 2 {
            parts[1]
                .parse::<u64>()
                .with_context(|| format!("Failed to parse meminfo value from: {}", line))
        } else {
            anyhow::bail!("Invalid meminfo line format: {}", line);
        }
    }

    /// Get disk space information (total and available) in MB
    fn get_disk_info(&self) -> Result<(u64, u64)> {
        #[cfg(target_os = "linux")]
        {
            self.get_disk_info_linux()
        }

        #[cfg(not(target_os = "linux"))]
        {
            // Fallback: estimate 500GB total, 250GB available
            warn!("Disk info detection not supported on this platform, using defaults");
            Ok((500_000, 250_000))
        }
    }

    /// Get disk info using statvfs (Linux)
    #[cfg(target_os = "linux")]
    fn get_disk_info_linux(&self) -> Result<(u64, u64)> {
        use std::os::unix::fs::MetadataExt;

        // Get filesystem stats for the data path
        let metadata = std::fs::metadata(&self.data_path)
            .with_context(|| format!("Failed to get metadata for path: {:?}", self.data_path))?;

        // Use statvfs syscall to get filesystem information
        let stat = nix::sys::statvfs::statvfs(&self.data_path)
            .with_context(|| format!("Failed to get statvfs for path: {:?}", self.data_path))?;

        let block_size = stat.block_size();
        let total_blocks = stat.blocks();
        let available_blocks = stat.blocks_available();

        // Convert to MB
        let total_mb = (total_blocks * block_size) / (1024 * 1024);
        let available_mb = (available_blocks * block_size) / (1024 * 1024);

        Ok((total_mb, available_mb))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_system_metrics_collection() {
        let temp_dir = TempDir::new().unwrap();
        let collector = SystemMetricsCollector::new(temp_dir.path());

        let metrics = collector.collect().unwrap();

        // Basic sanity checks
        assert!(metrics.total_memory_mb > 0, "Total memory should be positive");
        assert!(
            metrics.available_memory_mb <= metrics.total_memory_mb,
            "Available memory should not exceed total"
        );
        assert!(metrics.total_disk_mb > 0, "Total disk should be positive");
        assert!(
            metrics.available_disk_mb <= metrics.total_disk_mb,
            "Available disk should not exceed total"
        );
        assert!(metrics.cpu_cores > 0, "Should have at least one CPU core");

        // Usage should be calculated correctly
        let expected_memory_usage =
            (metrics.total_memory_mb - metrics.available_memory_mb) * 1024 * 1024;
        assert_eq!(
            metrics.memory_usage_bytes, expected_memory_usage,
            "Memory usage calculation should match"
        );

        let expected_disk_usage = (metrics.total_disk_mb - metrics.available_disk_mb) * 1024 * 1024;
        assert_eq!(
            metrics.disk_usage_bytes, expected_disk_usage,
            "Disk usage calculation should match"
        );
    }

    #[test]
    fn test_cpu_cores_detection() {
        let temp_dir = TempDir::new().unwrap();
        let collector = SystemMetricsCollector::new(temp_dir.path());

        let metrics = collector.collect().unwrap();

        // Should match num_cpus
        assert_eq!(metrics.cpu_cores, num_cpus::get() as u32);
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn test_memory_from_proc() {
        let temp_dir = TempDir::new().unwrap();
        let collector = SystemMetricsCollector::new(temp_dir.path());

        let (total, available) = collector.get_memory_from_proc().unwrap();

        assert!(total > 0, "Total memory should be positive");
        assert!(available > 0, "Available memory should be positive");
        assert!(
            available <= total,
            "Available memory should not exceed total"
        );
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn test_parse_meminfo_value() {
        let line = "MemTotal:        16384000 kB";
        let value = SystemMetricsCollector::parse_meminfo_value(line).unwrap();
        assert_eq!(value, 16384000);

        let line2 = "MemAvailable:     8192000 kB";
        let value2 = SystemMetricsCollector::parse_meminfo_value(line2).unwrap();
        assert_eq!(value2, 8192000);
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn test_disk_info_linux() {
        let temp_dir = TempDir::new().unwrap();
        let collector = SystemMetricsCollector::new(temp_dir.path());

        let (total, available) = collector.get_disk_info_linux().unwrap();

        assert!(total > 0, "Total disk should be positive");
        assert!(available > 0, "Available disk should be positive");
        assert!(
            available <= total,
            "Available disk should not exceed total"
        );
    }

    #[test]
    fn test_system_metrics_debug() {
        let metrics = SystemMetrics {
            total_memory_mb: 16384,
            available_memory_mb: 8192,
            total_disk_mb: 500_000,
            available_disk_mb: 250_000,
            cpu_cores: 8,
            memory_usage_bytes: 8192 * 1024 * 1024,
            disk_usage_bytes: 250_000 * 1024 * 1024,
        };

        let debug_str = format!("{:?}", metrics);
        assert!(debug_str.contains("16384"));
        assert!(debug_str.contains("cpu_cores: 8"));
    }

    #[test]
    fn test_collector_with_nonexistent_path() {
        let collector = SystemMetricsCollector::new("/nonexistent/path/that/does/not/exist");

        // Should fail gracefully when trying to get disk info
        let result = collector.get_disk_info();

        #[cfg(target_os = "linux")]
        assert!(result.is_err(), "Should fail for nonexistent path on Linux");

        #[cfg(not(target_os = "linux"))]
        assert!(
            result.is_ok(),
            "Should return defaults for nonexistent path on non-Linux"
        );
    }
}
