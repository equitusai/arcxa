//! WASM Function Registry
//!
//! Manages a collection of WASM-based User-Defined Functions (UDFs) for transformations.
//!
//! ## Features
//!
//! - Thread-safe function registration and lookup
//! - Function versioning support
//! - Metadata tracking (author, description, version)
//! - Function statistics (execution count, total time, errors)
//!
//! ## Example Usage
//!
//! ```rust,ignore
//! use graphica_coordinator::mapping::loader::transformation::wasm::{
//!     WasmiFunction, WasmiFunctionConfig, WasmiFunctionRegistry
//! };
//!
//! // Create registry
//! let registry = WasmiFunctionRegistry::new();
//!
//! // Load and register WASM module
//! let wasm_bytes = std::fs::read("email_obfuscator.wasm")?;
//! let config = WasmiFunctionConfig::default();
//! let function = WasmiFunction::new("obfuscate_email".to_string(), &wasm_bytes, config)?;
//! registry.register(function)?;
//!
//! // Execute function
//! let input = Value::String("test@example.com".to_string());
//! let result = registry.execute("obfuscate_email", &[input])?;
//! assert_eq!(result, Value::String("t***@example.com".to_string()));
//!
//! // Get statistics
//! let stats = registry.get_stats("obfuscate_email")?;
//! println!("Executions: {}, Avg time: {}ms", stats.execution_count, stats.avg_execution_time_ms);
//! ```

use anyhow::{anyhow, Result};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::function::WasmiFunction;
use crate::mapping::loader::transformation::types::Value;

/// Function metadata for tracking and documentation
#[derive(Debug, Clone)]
pub struct FunctionMetadata {
    /// Function name
    pub name: String,

    /// Function version (e.g., "1.0.0")
    pub version: String,

    /// Function author
    pub author: Option<String>,

    /// Function description
    pub description: Option<String>,

    /// Registration timestamp
    pub registered_at: Instant,
}

/// Function execution statistics
#[derive(Debug, Clone, Default)]
pub struct FunctionStats {
    /// Number of successful executions
    pub execution_count: u64,

    /// Number of failed executions
    pub error_count: u64,

    /// Total execution time across all invocations
    pub total_execution_time: Duration,

    /// Average execution time per invocation
    pub avg_execution_time_ms: f64,

    /// Last execution timestamp
    pub last_executed_at: Option<Instant>,
}

impl FunctionStats {
    /// Record a successful execution
    pub fn record_success(&mut self, duration: Duration) {
        self.execution_count += 1;
        self.total_execution_time += duration;
        self.avg_execution_time_ms =
            self.total_execution_time.as_secs_f64() * 1000.0 / self.execution_count as f64;
        self.last_executed_at = Some(Instant::now());
    }

    /// Record a failed execution
    pub fn record_error(&mut self, duration: Duration) {
        self.error_count += 1;
        self.total_execution_time += duration;
        self.avg_execution_time_ms = self.total_execution_time.as_secs_f64() * 1000.0
            / (self.execution_count + self.error_count) as f64;
        self.last_executed_at = Some(Instant::now());
    }
}

/// Registry entry combining function, metadata, and statistics
struct RegistryEntry {
    function: Arc<WasmiFunction>,
    metadata: FunctionMetadata,
    stats: FunctionStats,
}

/// Thread-safe registry for WASM User-Defined Functions (UDFs)
pub struct WasmiFunctionRegistry {
    /// Map of function name to registry entry
    functions: Arc<RwLock<HashMap<String, RegistryEntry>>>,
}

impl WasmiFunctionRegistry {
    /// Create a new empty registry
    pub fn new() -> Self {
        Self {
            functions: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register a WASM function with default metadata
    ///
    /// # Arguments
    ///
    /// * `function` - Compiled WASM function to register
    ///
    /// # Errors
    ///
    /// Returns error if a function with the same name already exists
    pub fn register(&self, function: WasmiFunction) -> Result<()> {
        let name = function.name().to_string();
        self.register_with_metadata(
            function,
            FunctionMetadata {
                name,
                version: "1.0.0".to_string(),
                author: None,
                description: None,
                registered_at: Instant::now(),
            },
        )
    }

    /// Register a WASM function with custom metadata
    ///
    /// # Arguments
    ///
    /// * `function` - Compiled WASM function to register
    /// * `metadata` - Function metadata (version, author, description)
    ///
    /// # Errors
    ///
    /// Returns error if a function with the same name already exists
    pub fn register_with_metadata(
        &self,
        function: WasmiFunction,
        metadata: FunctionMetadata,
    ) -> Result<()> {
        let mut functions = self.functions.write();

        let name = function.name().to_string();
        if functions.contains_key(&name) {
            return Err(anyhow!(
                "Function '{}' is already registered (version: {})",
                name,
                functions.get(&name).unwrap().metadata.version
            ));
        }

        functions.insert(
            name.clone(),
            RegistryEntry {
                function: Arc::new(function),
                metadata,
                stats: FunctionStats::default(),
            },
        );

        tracing::info!(
            function_name = %name,
            "Registered WASM UDF"
        );

        Ok(())
    }

    /// Unregister a WASM function
    ///
    /// # Arguments
    ///
    /// * `name` - Function name to unregister
    ///
    /// # Errors
    ///
    /// Returns error if function doesn't exist
    pub fn unregister(&self, name: &str) -> Result<()> {
        let mut functions = self.functions.write();

        if functions.remove(name).is_none() {
            return Err(anyhow!("Function '{}' is not registered", name));
        }

        tracing::info!(
            function_name = %name,
            "Unregistered WASM UDF"
        );

        Ok(())
    }

    /// Execute a registered WASM function
    ///
    /// # Arguments
    ///
    /// * `name` - Function name to execute
    /// * `args` - Input arguments for the function
    ///
    /// # Returns
    ///
    /// Transformed value
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Function doesn't exist
    /// - Execution fails (fuel exhausted, timeout, WASM trap, etc.)
    pub fn execute(&self, name: &str, args: &[Value]) -> Result<Value> {
        let start = Instant::now();

        // Get function (read lock)
        let function = {
            let functions = self.functions.read();
            let entry = functions
                .get(name)
                .ok_or_else(|| anyhow!("Function '{}' is not registered", name))?;
            Arc::clone(&entry.function)
        };

        // Execute function (no lock held)
        let result = function.execute(args);
        let duration = start.elapsed();

        // Update statistics (write lock)
        {
            let mut functions = self.functions.write();
            if let Some(entry) = functions.get_mut(name) {
                match &result {
                    Ok(_) => entry.stats.record_success(duration),
                    Err(_) => entry.stats.record_error(duration),
                }
            }
        }

        result
    }

    /// Check if a function is registered
    ///
    /// # Arguments
    ///
    /// * `name` - Function name to check
    pub fn contains(&self, name: &str) -> bool {
        let functions = self.functions.read();
        functions.contains_key(name)
    }

    /// Get function metadata
    ///
    /// # Arguments
    ///
    /// * `name` - Function name
    ///
    /// # Errors
    ///
    /// Returns error if function doesn't exist
    pub fn get_metadata(&self, name: &str) -> Result<FunctionMetadata> {
        let functions = self.functions.read();
        let entry = functions
            .get(name)
            .ok_or_else(|| anyhow!("Function '{}' is not registered", name))?;
        Ok(entry.metadata.clone())
    }

    /// Get function execution statistics
    ///
    /// # Arguments
    ///
    /// * `name` - Function name
    ///
    /// # Errors
    ///
    /// Returns error if function doesn't exist
    pub fn get_stats(&self, name: &str) -> Result<FunctionStats> {
        let functions = self.functions.read();
        let entry = functions
            .get(name)
            .ok_or_else(|| anyhow!("Function '{}' is not registered", name))?;
        Ok(entry.stats.clone())
    }

    /// List all registered function names
    pub fn list_functions(&self) -> Vec<String> {
        let functions = self.functions.read();
        functions.keys().cloned().collect()
    }

    /// Get total number of registered functions
    pub fn count(&self) -> usize {
        let functions = self.functions.read();
        functions.len()
    }

    /// Clear all registered functions
    pub fn clear(&self) {
        let mut functions = self.functions.write();
        functions.clear();
        tracing::info!("Cleared all registered WASM UDFs");
    }
}

impl Default for WasmiFunctionRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_creation() {
        let registry = WasmiFunctionRegistry::new();
        assert_eq!(registry.count(), 0);
        assert!(registry.list_functions().is_empty());
    }

    #[test]
    fn test_registry_contains() {
        let registry = WasmiFunctionRegistry::new();
        assert!(!registry.contains("test_func"));
    }

    #[test]
    fn test_function_stats_record_success() {
        let mut stats = FunctionStats::default();
        assert_eq!(stats.execution_count, 0);
        assert_eq!(stats.error_count, 0);

        stats.record_success(Duration::from_millis(100));
        assert_eq!(stats.execution_count, 1);
        assert_eq!(stats.error_count, 0);
        assert!(stats.avg_execution_time_ms > 0.0);
    }

    #[test]
    fn test_function_stats_record_error() {
        let mut stats = FunctionStats::default();
        assert_eq!(stats.execution_count, 0);
        assert_eq!(stats.error_count, 0);

        stats.record_error(Duration::from_millis(50));
        assert_eq!(stats.execution_count, 0);
        assert_eq!(stats.error_count, 1);
        assert!(stats.avg_execution_time_ms > 0.0);
    }

    // Note: Full integration tests with WASM modules will be added in examples/
}
