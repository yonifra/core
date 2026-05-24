# Skill: Build & Test ALM Programs

Use this skill when compiling, testing, or running ALM programs.

## Environment Setup (required)

```bash
export LLVM_SYS_180_PREFIX=/opt/homebrew/opt/llvm@18
export LIBRARY_PATH="/opt/homebrew/lib:$LIBRARY_PATH"
```

Without these, LLVM linking fails with `ld: library 'zstd' not found`.

## Run Modes

### Interpret (fast, for development)
```bash
cargo run -p alm-cli -- run file.alm
```

### Compile to native binary
```bash
cargo run -p alm-cli -- build file.alm              # host platform
cargo run -p alm-cli -- build file.alm -o myapp      # custom output name
cargo run -p alm-cli -- build file.alm --target wasm32  # cross-compile
```

### WASM Sandbox (safe execution)
```bash
cargo run -p alm-cli -- sandbox file.alm
```
Runs in wasmtime. No I/O, no network, no filesystem. Fuel-limited (no infinite loops).

### REPL
```bash
cargo run -p alm-cli -- repl
```

## Testing

```bash
cargo run -p alm-cli -- test file.alm          # run @test blocks
cargo run -p alm-cli -- check file.alm         # parse-only check
cargo run -p alm-cli -- lint file.alm           # static analysis
cargo run -p alm-cli -- lint file.alm --strict  # + annotation validation
```

### Lint Codes
- **W001**: Identifier >12 chars (token efficiency)
- **W002**: snake_case (should be camelCase)
- **W003**: Unused binding
- **W004**: Empty block
- **W005**: Metric without name
- **E101**: Invalid annotation name (strict mode)

## Effect Checking

```bash
cargo run -p alm-cli -- effects file.alm                # strict mode (default)
cargo run -p alm-cli -- effects file.alm --mode relaxed  # allow !io
```

Effect violations are compile-time errors in sandbox:strict.

## Cross-Compilation

```bash
cargo run -p alm-cli -- targets                          # list all targets
cargo run -p alm-cli -- build file.alm --target linux-x86_64
cargo run -p alm-cli -- build file.alm --all-targets     # all from alm.yaml
```

6 targets: linux-x86_64, linux-aarch64, darwin-arm64, darwin-x86_64, windows-x86_64, wasm32.

Cross-OS builds produce object files only (no cross-linker). WASM always produces full .wasm.

## Agent Tools

```bash
cargo run -p alm-cli -- generate "build a counter service" --json  # LLM prompt
cargo run -p alm-cli -- meta file.alm --json                       # module summary
cargo run -p alm-cli -- heal file.alm                               # self-heal loop
cargo run -p alm-cli -- emit-ir file.alm                            # show LLVM IR
```

## CI Pipeline

```bash
cargo run -p alm-cli -- ci    # reads alm.yaml, runs stages in topological order
```

Stages execute sequentially respecting `needs` dependencies. Gate thresholds enforced.

## Compiler Tests

```bash
cargo test --workspace         # all 122 tests across 11 crates
cargo test -p alm-lexer        # single crate
```
