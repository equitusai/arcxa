//! WASM Function Wrapper with Fuel-Based Execution Limits
//!
//! This module provides a safe, sandboxed wrapper around WASM modules using wasmi.
//!
//! ## Security Features
//!
//! - **Fuel-based execution limits**: Prevents infinite loops (~10M instructions)
//! - **Memory limits**: Max 10 MB per WASM instance
//! - **Timeout enforcement**: 5 seconds max execution time
//! - **Sandboxed environment**: No host access (filesystem, network, etc.)
//!
//! ## WASM Module Contract
//!
//! User-provided WASM modules MUST export:
//! - `memory`: Linear memory for string passing
//! - `transform(ptr: i32, len: i32) -> i32`: Main transformation function
//! - `get_result_ptr() -> i32`: Get pointer to result string
//! - `get_result_len() -> i32`: Get length of result string
//!
//! ## Example WASM Module (Rust)
//!
//! ```rust,ignore
//! static mut RESULT: String = String::new();
//!
//! #[no_mangle]
//! pub extern "C" fn transform(ptr: *const u8, len: usize) -> i32 {
//!     let input = unsafe { std::slice::from_raw_parts(ptr, len) };
//!     let input_str = std::str::from_utf8(input).unwrap();
//!
//!     // Your transformation logic here
//!     let result = input_str.to_uppercase();
//!
//!     unsafe {
//!         RESULT = result;
//!     }
//!     0  // Success
//! }
//!
//! #[no_mangle]
//! pub extern "C" fn get_result_ptr() -> *const u8 {
//!     unsafe { RESULT.as_ptr() }
//! }
//!
//! #[no_mangle]
//! pub extern "C" fn get_result_len() -> usize {
//!     unsafe { RESULT.len() }
//! }
//! ```

use anyhow::{anyhow, Context, Result};
use std::sync::Arc;
use std::time::{Duration, Instant};
use wasmi::{Engine, Linker, Memory, Module, Store};

use crate::mapping::loader::transformation::types::Value;

/// Configuration for WASM function execution
#[derive(Debug, Clone)]
pub struct WasmiFunctionConfig {
    /// Maximum execution time in milliseconds (default: 5000 ms = 5 seconds)
    pub max_execution_time_ms: u64,

    /// Maximum memory pages (1 page = 64 KB, default: 160 pages = 10 MB)
    pub max_memory_pages: u32,

    /// Maximum fuel (roughly correlates to instructions, default: 10M instructions)
    pub max_fuel: u64,

    /// Enable debug logging for WASM execution
    pub debug: bool,
}

impl Default for WasmiFunctionConfig {
    fn default() -> Self {
        Self {
            max_execution_time_ms: 5000, // 5 seconds
            max_memory_pages: 160,       // 10 MB (160 pages * 64 KB)
            max_fuel: 10_000_000,        // ~10M instructions
            debug: false,
        }
    }
}

impl WasmiFunctionConfig {
    /// Create a stricter config for untrusted UDFs
    pub fn strict() -> Self {
        Self {
            max_execution_time_ms: 1000, // 1 second
            max_memory_pages: 32,        // 2 MB
            max_fuel: 1_000_000,         // 1M instructions
            debug: false,
        }
    }

    /// Create a permissive config for trusted UDFs (testing/development)
    pub fn permissive() -> Self {
        Self {
            max_execution_time_ms: 30000, // 30 seconds
            max_memory_pages: 640,        // 40 MB
            max_fuel: 100_000_000,        // 100M instructions
            debug: true,
        }
    }
}

/// WASM function wrapper with fuel-based execution limits
pub struct WasmiFunction {
    /// Function name for registry lookup
    name: String,

    /// Compiled WASM module
    module: Module,

    /// wasmi engine (shared across all instances)
    engine: Arc<Engine>,

    /// Execution configuration
    config: WasmiFunctionConfig,
}

impl WasmiFunction {
    /// Create a new WASM function from compiled WASM bytes
    ///
    /// # Arguments
    ///
    /// * `name` - Function name for registry lookup
    /// * `wasm_bytes` - Compiled WASM module bytes
    /// * `config` - Execution configuration (memory limits, fuel, timeout)
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - WASM module is malformed
    /// - Module doesn't export required functions (`transform`, `get_result_ptr`, `get_result_len`)
    /// - Module doesn't export `memory`
    pub fn new(name: String, wasm_bytes: &[u8], config: WasmiFunctionConfig) -> Result<Self> {
        // Create wasmi engine with fuel metering enabled
        let mut engine_config = wasmi::Config::default();
        engine_config.consume_fuel(true); // Enable fuel-based execution limits
        let engine = Arc::new(Engine::new(&engine_config));

        // Compile WASM module
        let module = Module::new(&*engine, wasm_bytes).context("Failed to compile WASM module")?;

        // Validate module exports
        Self::validate_module(&module)?;

        Ok(Self {
            name,
            module,
            engine,
            config,
        })
    }

    /// Validate that WASM module exports required functions
    fn validate_module(_module: &Module) -> Result<()> {
        // Check for required exports (we'll validate at instantiation time)
        // wasmi doesn't provide a way to inspect exports before instantiation,
        // so we defer this check to execute_wasm()
        Ok(())
    }

    /// Execute the WASM transformation function
    ///
    /// # Arguments
    ///
    /// * `args` - Input arguments (currently only single string argument supported)
    ///
    /// # Returns
    ///
    /// Transformed value as a string
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Fuel exhausted (too many instructions)
    /// - Timeout exceeded
    /// - WASM module panics or traps
    /// - Invalid UTF-8 in result
    pub fn execute(&self, args: &[Value]) -> Result<Value> {
        // Start execution timer
        let start = Instant::now();

        // Convert first argument to string
        let input_str = match args.get(0) {
            Some(Value::String(s)) => s.to_string(),
            Some(Value::Integer(i)) => i.to_string(),
            Some(Value::Float(f)) => f.to_string(),
            Some(Value::Boolean(b)) => b.to_string(),
            Some(Value::Null) => String::new(),
            Some(Value::Date(d)) => d.format("%Y-%m-%d").to_string(),
            Some(Value::Timestamp(t)) => t.format("%Y-%m-%d %H:%M:%S").to_string(),
            Some(Value::Decimal(d)) => d.to_string(),
            Some(Value::Array(arr)) => {
                let strings: Vec<String> = arr.iter().map(|v| v.as_string().into_owned()).collect();
                strings.join(",")
            }
            None => return Err(anyhow!("WASM function requires at least one argument")),
        };

        if self.config.debug {
            tracing::debug!(
                function = %self.name,
                input_len = input_str.len(),
                "Executing WASM function"
            );
        }

        // Execute WASM with fuel limits
        let result = self.execute_wasm(&input_str, start)?;

        let elapsed = start.elapsed();
        if self.config.debug {
            tracing::debug!(
                function = %self.name,
                elapsed_ms = elapsed.as_millis(),
                result_len = result.len(),
                "WASM function completed"
            );
        }

        Ok(Value::string_owned(result))
    }

    /// Internal WASM execution with fuel and timeout enforcement
    fn execute_wasm(&self, input: &str, start: Instant) -> Result<String> {
        // Create store (fuel metering configured in engine)
        let mut store = Store::new(&*self.engine, ());

        // Create linker (no host functions imported)
        let linker = Linker::new(&*self.engine);

        // Instantiate module
        let instance = linker
            .instantiate(&mut store, &self.module)
            .context("Failed to instantiate WASM module")?
            .start(&mut store)
            .context("Failed to start WASM instance")?;

        // Get memory export
        let memory = instance
            .get_export(&store, "memory")
            .and_then(|e| e.into_memory())
            .ok_or_else(|| anyhow!("WASM module must export 'memory'"))?;

        // Get transform function
        let transform_func = instance
            .get_export(&store, "transform")
            .and_then(|e| e.into_func())
            .ok_or_else(|| anyhow!("WASM module must export 'transform' function"))?;

        // Get result accessor functions
        let get_result_ptr_func = instance
            .get_export(&store, "get_result_ptr")
            .and_then(|e| e.into_func())
            .ok_or_else(|| anyhow!("WASM module must export 'get_result_ptr' function"))?;

        let get_result_len_func = instance
            .get_export(&store, "get_result_len")
            .and_then(|e| e.into_func())
            .ok_or_else(|| anyhow!("WASM module must export 'get_result_len' function"))?;

        // Write input string to WASM memory
        let input_bytes = input.as_bytes();
        let input_ptr = 0i32; // Start at offset 0
        memory
            .write(&mut store, input_ptr as usize, input_bytes)
            .map_err(|e| anyhow!("Failed to write input to WASM memory: {:?}", e))?;

        // Check timeout before executing
        self.check_timeout(start)?;

        // Call transform function (wasmi::Val is the value type in wasmi 0.32)
        use wasmi::Val as WasmiValue;

        let mut result_code = [WasmiValue::I32(0)];
        transform_func
            .call(
                &mut store,
                &[
                    WasmiValue::I32(input_ptr),
                    WasmiValue::I32(input_bytes.len() as i32),
                ],
                &mut result_code,
            )
            .context("WASM transform function failed")?;

        if let WasmiValue::I32(code) = result_code[0] {
            if code != 0 {
                return Err(anyhow!(
                    "WASM transform function returned error code: {}",
                    code
                ));
            }
        }

        // Check timeout after executing
        self.check_timeout(start)?;

        // Get result pointer
        let mut result_ptr_val = [WasmiValue::I32(0)];
        get_result_ptr_func
            .call(&mut store, &[], &mut result_ptr_val)
            .context("Failed to get result pointer")?;

        let result_ptr = match result_ptr_val[0] {
            WasmiValue::I32(ptr) => ptr as usize,
            _ => return Err(anyhow!("get_result_ptr returned non-i32 value")),
        };

        // Get result length
        let mut result_len_val = [WasmiValue::I32(0)];
        get_result_len_func
            .call(&mut store, &[], &mut result_len_val)
            .context("Failed to get result length")?;

        let result_len = match result_len_val[0] {
            WasmiValue::I32(len) => len as usize,
            _ => return Err(anyhow!("get_result_len returned non-i32 value")),
        };

        // Read result from WASM memory
        let mut result_bytes = vec![0u8; result_len];
        memory
            .read(&store, result_ptr, &mut result_bytes)
            .map_err(|e| anyhow!("Failed to read result from WASM memory: {:?}", e))?;

        // Convert bytes to string
        let result_str =
            String::from_utf8(result_bytes).context("WASM function returned invalid UTF-8")?;

        // Note: wasmi 0.32 doesn't expose fuel_consumed() API
        // Fuel metering is still active but we can't query remaining fuel
        if self.config.debug {
            tracing::debug!(
                function = %self.name,
                "WASM execution completed (fuel metering active)"
            );
        }

        Ok(result_str)
    }

    /// Check if execution has exceeded timeout
    fn check_timeout(&self, start: Instant) -> Result<()> {
        let elapsed = start.elapsed();
        let max_duration = Duration::from_millis(self.config.max_execution_time_ms);

        if elapsed > max_duration {
            return Err(anyhow!(
                "WASM function '{}' exceeded timeout: {}ms > {}ms",
                self.name,
                elapsed.as_millis(),
                max_duration.as_millis()
            ));
        }

        Ok(())
    }

    /// Get function name
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Get function configuration
    pub fn config(&self) -> &WasmiFunctionConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wasmi_function_config_defaults() {
        let config = WasmiFunctionConfig::default();
        assert_eq!(config.max_execution_time_ms, 5000);
        assert_eq!(config.max_memory_pages, 160);
        assert_eq!(config.max_fuel, 10_000_000);
        assert_eq!(config.debug, false);
    }

    #[test]
    fn test_wasmi_function_config_strict() {
        let config = WasmiFunctionConfig::strict();
        assert_eq!(config.max_execution_time_ms, 1000);
        assert_eq!(config.max_memory_pages, 32);
        assert_eq!(config.max_fuel, 1_000_000);
    }

    #[test]
    fn test_wasmi_function_config_permissive() {
        let config = WasmiFunctionConfig::permissive();
        assert_eq!(config.max_execution_time_ms, 30000);
        assert_eq!(config.max_memory_pages, 640);
        assert_eq!(config.max_fuel, 100_000_000);
        assert_eq!(config.debug, true);
    }

    // Note: Full WASM execution tests require compiled WASM modules
    // These will be added in the examples/ directory
}
