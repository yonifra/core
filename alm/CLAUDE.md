# ALM Compiler — Development Guide

## Build

```bash
export LLVM_SYS_180_PREFIX=/opt/homebrew/opt/llvm@18
export LIBRARY_PATH="/opt/homebrew/lib:$LIBRARY_PATH"
cargo build --workspace
cargo test --workspace
```

LLVM 18 required. Install: `brew install llvm@18`

## Test

```bash
cargo test --workspace                    # all 122 tests
cargo test -p alm-lexer                   # single crate
cargo run -p alm-cli -- test examples/hello.alm   # ALM tests
cargo run -p alm-cli -- ci               # full CI pipeline from alm.yaml
```

## Architecture

```
Source (.alm) → Lexer (alm-lexer) → Parser (alm-parser) → AST
                                                            ↓
                                                    IR (alm-ir, SSA form)
                                                            ↓
                                        ┌───────────────────┼───────────────┐
                                        ↓                   ↓               ↓
                                   LLVM (alm-llvm)    WASM (alm-wasm)  Interp (alm-interp)
```

## Crate Responsibilities

| Change needed | Modify crate |
|---------------|-------------|
| New token/operator | alm-lexer |
| New syntax construct | alm-parser (+ ast.rs) |
| New IR instruction | alm-ir |
| Native codegen change | alm-llvm |
| WASM codegen change | alm-wasm/compile.rs |
| New builtin function | alm-interp (call_builtin) |
| New CLI command | alm-cli/main.rs |
| Config field | alm-config |
| Lint rule | alm-lint |
| Agent prompt | alm-agent/prompt.rs |
| New target | alm-target |
| Sandbox/effects | alm-wasm/sandbox.rs, effects.rs |

## Adding New ALM Syntax

1. **Lexer** (`alm-lexer/src/lib.rs`): Add token to `TokenKind` enum, handle in `next_token()`
2. **Parser** (`alm-parser/src/lib.rs` + `ast.rs`): Add AST node, parse in appropriate precedence level
3. **Interpreter** (`alm-interp/src/lib.rs`): Add eval case in `eval_expr()`
4. **IR** (`alm-ir/src/lower.rs`): Add lowering in `lower_expr()`
5. **LLVM** (`alm-llvm/src/lib.rs`): Add codegen in `emit_inst()` dispatch
6. **WASM** (`alm-wasm/src/compile.rs`): Add WASM emission in `emit_inst()`
7. Tests in each crate

## Adding New Builtin

1. Register in `alm-interp/src/lib.rs` → `Env::new()` → `global.insert(...)`
2. Implement in `call_builtin()` match arm
3. Add to GRAMMAR_REF in `alm-agent/src/prompt.rs`
4. Add to README.md builtins table

## Code Conventions

**Rust (compiler code):**
- Standard rustfmt
- Errors return `Result<T, String>` in alpha (will migrate to proper error types)
- Tests in `#[cfg(test)] mod tests` at bottom of each file
- Error codes: E000-E099 lexer, E100-E199 parser, E200-E299 runtime, E300+ CLI

**ALM (.alm files):**
- camelCase identifiers
- ≤12 chars per identifier
- `;` after statements
- `//` comments only
- No imports — deps in alm.yaml
- `@test(name){...}` for test blocks

## Environment

- Rust 1.95+ (2024 edition)
- LLVM 18 (via Homebrew)
- macOS ARM64 primary dev platform
- wasmtime for WASM sandbox
