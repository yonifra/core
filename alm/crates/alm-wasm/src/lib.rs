//! ALM WASM Backend — compiles ALM-IR to WASM and executes in sandbox.
//!
//! Two components:
//! 1. Compiler: ALM-IR → WASM bytecode via wasm-encoder
//! 2. Sandbox: Execute WASM in wasmtime with capability restrictions

pub mod compile;
pub mod sandbox;
pub mod effects;

pub use compile::WasmCompiler;
pub use sandbox::WasmSandbox;
pub use effects::{Effect, EffectChecker};
