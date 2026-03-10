//! # Rule Execution Engine
//!
//! WASM-based sandboxed rule execution for quality checks and transformations.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use wasmtime::*;

/// WASM rule runtime with proper memory management and timeouts
pub struct WasmRuleEngine {
    engine: Engine,
    module_cache: std::sync::RwLock<hashbrown::HashMap<String, Module>>,
}

impl WasmRuleEngine {
    pub fn new() -> Result<Self> {
        let mut config = Config::new();
        config.wasm_multi_memory(true);
        config.wasm_simd(true);

        // CRITICAL: Enable fuel metering for timeouts/resource limits
        config.consume_fuel(true);

        let engine = Engine::new(&config)?;

        Ok(Self {
            engine,
            module_cache: std::sync::RwLock::new(hashbrown::HashMap::new()),
        })
    }

    /// Load and compile a WASM rule (with caching)
    pub fn load_rule(&self, rule_id: &str, wasm_bytes: &[u8]) -> Result<()> {
        let module =
            Module::new(&self.engine, wasm_bytes).context("Failed to compile WASM module")?;

        let mut cache = self.module_cache.write().unwrap();
        cache.insert(rule_id.to_string(), module);

        Ok(())
    }

    /// Unload a WASM rule from cache
    pub fn unload_rule(&self, rule_id: &str) -> Result<()> {
        let mut cache = self.module_cache.write().unwrap();
        cache
            .remove(rule_id)
            .ok_or_else(|| anyhow::anyhow!("Rule {} not found in cache", rule_id))?;
        Ok(())
    }

    /// Execute rule against data with proper memory management
    pub fn execute(
        &self,
        rule_id: &str,
        input_json: &str,
        timeout: Duration,
    ) -> Result<RuleExecutionResult> {
        // Get cached module
        let cache = self.module_cache.read().unwrap();
        let module = cache
            .get(rule_id)
            .ok_or_else(|| anyhow::anyhow!("Rule {} not loaded", rule_id))?;

        // Create store with fuel limit (approximate timeout)
        let mut store = Store::new(&self.engine, ());

        // Set fuel: ~1M instructions per millisecond (rough estimate)
        let fuel = timeout.as_millis() as u64 * 1_000_000;
        store.set_fuel(fuel)?;

        // Create linker with host functions
        let mut linker = Linker::new(&self.engine);

        // CRITICAL: Don't expose dangerous host functions
        // Only expose safe logging and result functions

        // Instantiate the module
        let instance = linker.instantiate(&mut store, module)?;

        // PROPER way: Allocate memory in WASM, copy data, call function

        // 1. Get memory export
        let memory = instance
            .get_memory(&mut store, "memory")
            .ok_or_else(|| anyhow::anyhow!("WASM module must export 'memory'"))?;

        // 2. Get allocator function (module must export this)
        let alloc_fn = instance
            .get_typed_func::<i32, i32>(&mut store, "alloc")
            .context("WASM module must export 'alloc(size: i32) -> i32'")?;

        // 3. Allocate memory in WASM
        let input_bytes = input_json.as_bytes();
        let input_len = input_bytes.len() as i32;
        let input_ptr = alloc_fn.call(&mut store, input_len)?;

        // 4. Write data to WASM memory
        memory
            .write(&mut store, input_ptr as usize, input_bytes)
            .context("Failed to write input to WASM memory")?;

        // 5. Call the check function: check(ptr: i32, len: i32) -> i32
        let check_fn = instance
            .get_typed_func::<(i32, i32), i32>(&mut store, "check")
            .context("WASM module must export 'check(ptr: i32, len: i32) -> i32'")?;

        let result_code = check_fn
            .call(&mut store, (input_ptr, input_len))
            .context("Rule execution failed")?;

        // 6. Read result from WASM (if module exports result functions)
        let passed = result_code == 1;

        // Optional: Get error message if check failed
        let message = if !passed && instance.get_func(&mut store, "get_error_message").is_some() {
            // Would need to call get_error_message() -> ptr, then read from memory
            Some("Rule validation failed".to_string())
        } else {
            None
        };

        Ok(RuleExecutionResult { passed, message })
    }

    /// Clear module cache
    pub fn clear_cache(&self) {
        let mut cache = self.module_cache.write().unwrap();
        cache.clear();
    }
}

impl Default for WasmRuleEngine {
    fn default() -> Self {
        Self::new().expect("Failed to initialize WASM engine")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompiledRule {
    pub id: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleExecutionResult {
    pub passed: bool,
    pub message: Option<String>,
}

/// Built-in rule types for common patterns (non-WASM)
pub mod builtin {
    use super::*;
    use once_cell::sync::Lazy;
    use regex::Regex;
    use std::sync::Mutex;

    // Regex cache to avoid recompiling
    static REGEX_CACHE: Lazy<Mutex<hashbrown::HashMap<String, Regex>>> =
        Lazy::new(|| Mutex::new(hashbrown::HashMap::new()));

    pub fn not_null(value: &serde_json::Value) -> bool {
        !value.is_null()
    }

    pub fn matches_regex(value: &str, pattern: &str) -> Result<bool> {
        let mut cache = REGEX_CACHE.lock().unwrap();

        let re = match cache.get(pattern) {
            Some(regex) => regex,
            None => {
                let regex = Regex::new(pattern)?;
                cache.insert(pattern.to_string(), regex);
                cache.get(pattern).unwrap()
            }
        };

        Ok(re.is_match(value))
    }

    pub fn in_range(value: f64, min: f64, max: f64) -> bool {
        value >= min && value <= max
    }

    pub fn is_email(value: &str) -> Result<bool> {
        matches_regex(value, r"^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}$")
    }

    pub fn is_phone_us(value: &str) -> Result<bool> {
        matches_regex(value, r"^\+?1?\d{10}$")
    }

    pub fn is_date_iso8601(value: &str) -> bool {
        chrono::DateTime::parse_from_rfc3339(value).is_ok()
    }

    pub fn string_length_between(value: &str, min: usize, max: usize) -> bool {
        let len = value.len();
        len >= min && len <= max
    }

    /// Uniqueness checker with state
    pub struct UniquenessChecker {
        seen: hashbrown::HashSet<String>,
    }

    impl UniquenessChecker {
        pub fn new() -> Self {
            Self {
                seen: hashbrown::HashSet::new(),
            }
        }

        pub fn is_unique(&mut self, value: &str) -> bool {
            self.seen.insert(value.to_string())
        }

        pub fn reset(&mut self) {
            self.seen.clear();
        }
    }

    impl Default for UniquenessChecker {
        fn default() -> Self {
            Self::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builtin_not_null() {
        assert!(builtin::not_null(&serde_json::json!("value")));
        assert!(!builtin::not_null(&serde_json::Value::Null));
    }

    #[test]
    fn test_builtin_email() {
        assert!(builtin::is_email("test@example.com").unwrap());
        assert!(!builtin::is_email("invalid-email").unwrap());
    }

    #[test]
    fn test_builtin_uniqueness() {
        let mut checker = builtin::UniquenessChecker::new();

        assert!(checker.is_unique("value1"));
        assert!(checker.is_unique("value2"));
        assert!(!checker.is_unique("value1")); // Duplicate
    }

    #[test]
    fn test_regex_caching() {
        // First call compiles regex
        let result1 = builtin::matches_regex("test123", r"\w+\d+").unwrap();
        // Second call uses cached regex
        let result2 = builtin::matches_regex("test456", r"\w+\d+").unwrap();

        assert!(result1);
        assert!(result2);
    }
}
