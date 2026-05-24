# ALM — Assembly for Language Models

ALM is an AI-first programming language designed to be written and read by LLMs. It compiles to native code via LLVM and runs in a WASM sandbox for safe execution. Humans interact through `alm.yaml` config — agents handle the code.

## Why ALM?

Traditional languages waste LLM compute. Attention is O(n²) in token count — fewer tokens = quadratically less compute. ALM achieves **~22x attention savings** vs Rust through:

- **Zero keywords** — 24 single-character sigils instead of `fn`, `if`, `for`, `let`, etc.
- **BPE-aligned syntax** — every structural token is a single BPE token in cl100k/o200k
- **No whitespace sensitivity** — explicit `{}` delimiters, no indentation ambiguity
- **Built-in lifecycle** — `@test`, `@deploy`, `#metric` are native, not external tools

```
// Rust: ~85 tokens, ~7225 attention ops
async fn main() -> Result<(), Box<dyn Error>> {
    let listener = TcpListener::bind("0.0.0.0:8080")?;
    ...
}

// ALM: ~18 tokens, ~324 attention ops (22x less compute)
@svc(8080){~(r){#rq;p=proc(r)?;p|?=>retry(3,proc,r);>p}}
```

## Architecture

```
Human Intent → alm.yaml → LLM Agent → .alm source → ALM Compiler → Native Binary
                                                          │
                                            ┌──────────────┼──────────────┐
                                            ▼              ▼              ▼
                                         LLVM IR      WASM Module    Interpreter
                                            ▼              ▼              ▼
                                     Native Binary    Sandbox Exec   REPL/Test
                                   (x86/ARM/Mach-O)  (wasmtime)
```

**Pipeline:** Lexer → Parser → AST → ALM-IR (SSA) → Backend (LLVM | WASM | Interpreter)

## Quick Start

```bash
# Prerequisites
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
brew install llvm@18

# Build
export LLVM_SYS_180_PREFIX=/opt/homebrew/opt/llvm@18
export LIBRARY_PATH="/opt/homebrew/lib:$LIBRARY_PATH"
cd alm && cargo build --workspace

# Run
cargo run -p alm-cli -- run examples/hello.alm        # interpret
cargo run -p alm-cli -- build examples/native.alm     # compile to native
cargo run -p alm-cli -- sandbox examples/native.alm   # run in WASM sandbox
cargo run -p alm-cli -- test examples/hello.alm        # run @test blocks
```

## ALM Syntax Reference

### Structural Tokens (24 sigils — no keywords)

| Token | Meaning | Token | Meaning |
|-------|---------|-------|---------|
| `{` `}` | Block scope | `@` | Annotation / lifecycle |
| `(` `)` | Grouping / args | `#` | Metric / observe |
| `[` `]` | Collection | `!` | Effect / side-effect |
| `:` | Type annotation | `?` | Fallible / optional |
| `;` | Statement separator | `~` | Async |
| `.` | Member access | `$` | Environment ref |
| `,` | List separator | `^` | Move (ownership) |
| `=` | Binding (heap) | `&` | Borrow (read) |
| `>` | Return / emit | `&*` | Mutable borrow |
| `<` | Generic / constraint | `*` | Deref / loop |
| `|` | Match arm | `_` | Wildcard |
| `=>` | Transform (fat arrow) | `:=` | Stack binding |

### Construct Reference

| Pattern | ALM | Example |
|---------|-----|---------|
| Heap binding | `name = expr;` | `x = 42;` |
| Stack binding | `name := expr;` | `x := 42;` |
| Function call | `name(args)` | `proc(r, 3)` |
| Async | `~expr` | `~name(args){body}` |
| Return | `>expr` | `>42` |
| Try (propagate error) | `expr?` | `proc(r)?` |
| Elvis (default on error) | `expr?: default` | `val?: 0` |
| Match | `expr \| pat => body` | `x \| 1 => "one" \| _ => "other"` |
| Loop N times | `*(n){body}` | `*(10){#iter}` |
| Infinite loop | `*(){body}` | `*(){tick()}` |
| For-each | `*x <- col{body}` | `*item <- list{proc(item)}` |
| Metric counter | `#name` | `#requests` |
| Metric increment | `#name++` | `#errors++` |
| Struct literal | `Name{f:v}` | `Point{x:1, y:2}` |
| Field access | `expr.field` | `p.x` |
| Env variable | `$NAME` | `$HOME` |
| Annotation | `@name(args){body}` | `@test(math){assertEq(1,1)}` |
| Effect block | `!effect{body}` | `!io{print(42)}` |
| Borrow | `&expr` / `&*expr` | `&x` (read) / `&*x` (mut) |
| Move | `^expr` | `^x` |
| Comment | `// text` | `// single line only` |

### Built-in Functions

| Function | Args | Description |
|----------|------|-------------|
| `assert(val)` | 1 | Fails if falsy |
| `assertEq(a, b)` | 2 | Fails if a ≠ b |
| `assertNe(a, b)` | 2 | Fails if a = b |
| `assertGt(a, b)` | 2 | Fails if a ≤ b |
| `assertLt(a, b)` | 2 | Fails if a ≥ b |
| `print(args...)` | 1+ | Print to stdout (!io effect) |
| `len(val)` | 1 | String/list length |
| `retry(n, fn, args...)` | 2+ | Retry fn up to n times |
| `type(val)` | 1 | Returns type name as string |
| `str(val)` | 1 | Convert to string |

### Conventions

- **camelCase** identifiers (NOT snake_case) — BPE-efficient
- Identifiers **≤12 characters** — token density
- **No imports** — dependencies declared in `alm.yaml`
- **`;`** after statements
- `//` single-line comments only — LLMs rarely need comments

## CLI Reference

```
alm run <file.alm>                    Execute (interpreted)
alm build <file.alm>                  Compile to native binary
alm build <file.alm> --target wasm32  Cross-compile to target
alm build <file.alm> --all-targets    Build all targets from alm.yaml
alm check <file.alm>                  Parse and type-check only
alm emit-ir <file.alm>                Show LLVM IR output
alm test <file.alm>                   Run @test blocks with timing
alm lint <file.alm>                   Static analysis (W001-W005, E101)
alm lint <file.alm> --strict          Include annotation validation
alm ci                                Run CI pipeline from alm.yaml
alm generate <intent>                 Generate agent prompt for LLM
alm generate <intent> --json          Agent prompt as JSON
alm meta <file.alm>                   Generate .alm.meta summary
alm meta <file.alm> --json            Module summary as JSON
alm heal <file.alm>                   Self-heal compilation errors
alm sandbox <file.alm>                Run in WASM sandbox (strict)
alm effects <file.alm>                Check effect violations
alm effects <file.alm> --mode relaxed Allow !io, block !net/!fs
alm targets                           List all compilation targets
alm repl                              Interactive REPL
alm version                           Show version
```

## alm.yaml Specification

The universal config file. One file = entire project definition.

```yaml
version: "0.1.0"

project:
  name: "my-service"             # Project name
  lang_version: "0.1-alpha"      # ALM version
  targets:                       # Compilation targets
    - linux-x86_64
    - darwin-arm64
    - wasm32

modules:
  - path: src/                   # Source directory
    entry: main.alm              # Entry point
    deps:                        # Dependencies
      - name: http
        ver: "0.1"
      - name: db
        ver: "0.2"
        features: [postgres, pool]

build:
  opt_level: 3                   # 0=debug 1=size 2=speed 3=aggressive
  lto: true                      # Link-time optimization
  sanitizers: [address, thread]  # Debug build only

test:
  strategy: property             # unit | integration | property | fuzz
  coverage_min: 85               # Minimum test coverage %
  sandbox: strict                # strict=no I/O, relaxed=mock FS
  timeout_ms: 5000               # Per-test timeout
  parallel: auto                 # auto = num_cores
  on_fail:
    retry: 2
    then: block_deploy

ci:
  trigger: [push, pr]
  stages:
    - name: lint
      run: alm lint --strict
    - name: test
      run: alm test --all
      needs: [lint]
    - name: build
      run: alm build --release
      needs: [test]
      gate:
        regression_threshold: "5%"

deploy:
  staging:
    provider: container          # container | binary | lambda | edge-wasm
    health_check: /healthz
    rollback: auto
  production:
    strategy: canary
    canary_percent: 10
    canary_duration: 15m

metrics:
  backend: otlp                  # otlp | prometheus | custom
  endpoint: "https://otel:4317"
  alerts:
    - name: high_error_rate
      expr: "rate(errors[5m]) > 0.05"
      severity: critical
      notify: [pagerduty, slack]

agent:
  model: claude-4                # LLM for code generation
  context_budget: 200000         # Max tokens per task
  self_heal: true                # Auto-fix compile errors (3 attempts)
  review_mode: diff              # diff | full | none
```

## Crate Map

```
alm-lexer     Zero-copy streaming tokenizer (24 sigils + idents + literals)
    ↓
alm-parser    LL(1) recursive descent → typed AST
    ↓
alm-ir        SSA-form intermediate representation + AST→IR lowering
    ↓
├── alm-llvm      LLVM codegen via inkwell → native binaries (6 targets)
├── alm-wasm      WASM compiler + wasmtime sandbox + effect system
└── alm-interp    Tree-walking interpreter for REPL/tests

alm-config    alm.yaml parser (serde) → typed Rust structs
alm-target    Target registry, LLVM triple mapping, platform detection
alm-lint      Static analysis (W001-W005 warnings, E101 errors)
alm-agent     Agent prompt builder, self-heal loop, .alm.meta summaries
alm-cli       CLI entry point (15 commands)
```

## For AI Agents

### Generating ALM Code

Use `alm generate <intent>` to get a structured prompt. The prompt contains:
- Full syntax reference
- Project context from alm.yaml
- Code examples
- Constraints (sandbox mode, test strategy)

```bash
alm generate "HTTP echo server with metrics" --json
```

### Self-Healing Loop

When code has errors, the agent loop works:
1. Generate ALM code
2. `alm check` → parse errors? → repair prompt → retry
3. `alm lint` → warnings? → fix and retry
4. `alm effects` → sandbox violations? → remove effects → retry  
5. `alm sandbox` → WASM execution → safe
6. `alm build` → native binary → ship

Use `alm heal <file>` to run the loop. Config `agent.self_heal: true` enables 3 retries.

### Module Summaries (.alm.meta)

For large codebases, agents read summaries instead of full source:

```bash
alm meta src/main.alm --json
```

Output: binding names/types, annotations, metrics, test count. ~10x smaller than source.

### Effect System Constraints

In `sandbox: strict` mode (default for tests):
- `print()`, `$ENV` → **blocked** (!io)
- `fetch()`, `connect()` → **blocked** (!net)
- `readFile()`, `writeFile()` → **blocked** (!fs)
- `now()`, `sleep()` → **blocked** (!time)
- `assert()`, `assertEq()`, `#metric` → **allowed** (pure)

Check before submitting: `alm effects <file> --mode strict`

## Compilation Targets

| Target | Format | LLVM Triple | Output |
|--------|--------|-------------|--------|
| linux-x86_64 | ELF | x86_64-unknown-linux-gnu | Binary/Object |
| linux-aarch64 | ELF | aarch64-unknown-linux-gnu | Binary/Object |
| darwin-arm64 | Mach-O | aarch64-apple-darwin | Binary |
| darwin-x86_64 | Mach-O | x86_64-apple-darwin | Binary |
| windows-x86_64 | PE/COFF | x86_64-pc-windows-msvc | Object |
| wasm32 | WASM | wasm32-unknown-unknown | .wasm |

## License

MIT
