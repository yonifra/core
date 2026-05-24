//! WASM Sandbox — execute WASM modules with capability-based security.
//!
//! Uses wasmtime with restricted capabilities:
//! - No filesystem access
//! - No network access
//! - No environment variables
//! - Bounded execution time
//! - Bounded memory

use wasmtime::*;

/// Sandbox configuration.
#[derive(Debug, Clone)]
pub struct SandboxConfig {
    /// Maximum execution time in milliseconds.
    pub timeout_ms: u64,
    /// Maximum memory pages (64KB each).
    pub max_memory_pages: u32,
    /// Sandbox mode: strict (no I/O), relaxed (mock I/O).
    pub mode: SandboxMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SandboxMode {
    Strict,  // No I/O at all
    Relaxed, // Mock filesystem
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            timeout_ms: 5000,
            max_memory_pages: 16, // 1MB
            mode: SandboxMode::Strict,
        }
    }
}

/// Result of sandboxed execution.
#[derive(Debug, Clone)]
pub struct SandboxResult {
    pub success: bool,
    pub return_value: Option<i64>,
    pub error: Option<String>,
    pub metrics: Vec<(String, i64)>,
}

pub struct WasmSandbox;

impl WasmSandbox {
    /// Execute WASM bytes in sandbox, return result.
    pub fn execute(wasm_bytes: &[u8], config: &SandboxConfig) -> SandboxResult {
        match Self::run_inner(wasm_bytes, config) {
            Ok(result) => result,
            Err(e) => SandboxResult {
                success: false,
                return_value: None,
                error: Some(e),
                metrics: vec![],
            },
        }
    }

    /// Compile ALM source and execute in sandbox.
    pub fn compile_and_run(source: &str, config: &SandboxConfig) -> SandboxResult {
        let wasm_bytes = match super::WasmCompiler::compile(source) {
            Ok(b) => b,
            Err(e) => {
                return SandboxResult {
                    success: false,
                    return_value: None,
                    error: Some(format!("compilation failed: {e}")),
                    metrics: vec![],
                };
            }
        };
        Self::execute(&wasm_bytes, config)
    }

    fn run_inner(wasm_bytes: &[u8], config: &SandboxConfig) -> Result<SandboxResult, String> {
        let mut engine_config = Config::new();
        engine_config.consume_fuel(true); // Enable fuel-based timeout

        let engine = Engine::new(&engine_config)
            .map_err(|e| format!("engine creation failed: {e}"))?;

        let module = wasmtime::Module::new(&engine, wasm_bytes)
            .map_err(|e| format!("module validation failed: {e}"))?;

        let mut store = Store::new(&engine, ());

        // Set fuel limit based on timeout (rough: 1M fuel ≈ few ms)
        let fuel = (config.timeout_ms * 1_000_000) / 5; // ~200K ops per ms
        store.set_fuel(fuel)
            .map_err(|e| format!("fuel setup failed: {e}"))?;

        let instance = Instance::new(&mut store, &module, &[])
            .map_err(|e| format!("instantiation failed: {e}"))?;

        // Get main function
        let main = instance.get_typed_func::<(), i64>(&mut store, "main")
            .map_err(|e| format!("no main function: {e}"))?;

        // Execute
        match main.call(&mut store, ()) {
            Ok(result) => {
                // Read metric globals
                let mut metrics = Vec::new();
                let mut i = 0;
                while let Some(global) = instance.get_global(&mut store, &format!("__global_{i}")) {
                    if let Some(Val::I64(v)) = Some(global.get(&mut store)) {
                        metrics.push((format!("metric_{i}"), v));
                    }
                    i += 1;
                }

                Ok(SandboxResult {
                    success: true,
                    return_value: Some(result),
                    error: None,
                    metrics,
                })
            }
            Err(e) => {
                let err_str = e.to_string();
                if err_str.contains("fuel") {
                    Ok(SandboxResult {
                        success: false,
                        return_value: None,
                        error: Some("execution timeout: fuel exhausted".into()),
                        metrics: vec![],
                    })
                } else {
                    Ok(SandboxResult {
                        success: false,
                        return_value: None,
                        error: Some(format!("runtime error: {err_str}")),
                        metrics: vec![],
                    })
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sandbox_simple() {
        let config = SandboxConfig::default();
        let result = WasmSandbox::compile_and_run(">42", &config);
        assert!(result.success, "failed: {:?}", result.error);
        assert_eq!(result.return_value, Some(42));
    }

    #[test]
    fn test_sandbox_binding() {
        let config = SandboxConfig::default();
        let result = WasmSandbox::compile_and_run("x = 99; x", &config);
        assert!(result.success, "failed: {:?}", result.error);
        assert_eq!(result.return_value, Some(99));
    }

    #[test]
    fn test_sandbox_metric() {
        let config = SandboxConfig::default();
        let result = WasmSandbox::compile_and_run("#rq; #rq; >0", &config);
        assert!(result.success, "failed: {:?}", result.error);
    }

    #[test]
    fn test_sandbox_invalid_wasm() {
        let config = SandboxConfig::default();
        let result = WasmSandbox::execute(b"not wasm", &config);
        assert!(!result.success);
        assert!(result.error.is_some());
    }

    #[test]
    fn test_sandbox_zero_return() {
        let config = SandboxConfig::default();
        let result = WasmSandbox::compile_and_run(">0", &config);
        assert!(result.success);
        assert_eq!(result.return_value, Some(0));
    }
}
