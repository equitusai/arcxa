//! WASM-based User-Defined Functions (UDFs) for Transformations
//!
//! This module provides sandboxed execution of custom transformation functions
//! using WebAssembly (WASM) via the wasmi runtime.
//!
//! ## Why wasmi?
//!
//! - **Multi-platform support**: Works on Power10, ARM64, RISC-V, x86_64
//! - **Pure Rust**: No C++ dependencies, easier builds
//! - **Smaller binaries**: ~9.5 MB smaller than wasmtime
//! - **Consistent behavior**: Interpreter-based execution across all platforms
//!
//! ## Architecture
//!
//! ```text
//! User WASM Module (.wasm)
//!         ↓
//! WasmiFunction (wrapper with fuel limits)
//!         ↓
//! WasmiFunctionRegistry (function management)
//!         ↓
//! TransformationEngine Integration
//! ```
//!
//! ## Security Model
//!
//! - **Fuel-based execution limits**: ~10M instructions per invocation
//! - **Memory limits**: 10 MB max per WASM instance
//! - **Timeout enforcement**: 5 seconds max execution time
//! - **Sandboxed environment**: No host access (filesystem, network, etc.)
//!
//! ## Example Usage
//!
//! ```rust,ignore
//! use graphica_coordinator::mapping::loader::transformation::wasm::{
//!     WasmiFunction, WasmiFunctionConfig, WasmiFunctionRegistry
//! };
//!
//! // Load WASM module
//! let wasm_bytes = std::fs::read("custom_transform.wasm")?;
//! let config = WasmiFunctionConfig::default();
//! let function = WasmiFunction::new("my_transform".to_string(), &wasm_bytes, config)?;
//!
//! // Register with registry
//! let registry = WasmiFunctionRegistry::new();
//! registry.register(function)?;
//!
//! // Execute transformation
//! let input = Value::String("test@example.com".to_string());
//! let result = registry.execute("my_transform", &[input])?;
//! ```

pub mod function;
pub mod registry;

pub use function::{WasmiFunction, WasmiFunctionConfig};
pub use registry::WasmiFunctionRegistry;
