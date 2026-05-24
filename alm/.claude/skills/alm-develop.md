# Skill: Develop the ALM Compiler

Use this skill when modifying the ALM compiler itself — adding syntax, builtins, targets, or fixing compiler bugs.

## Crate Dependency Graph

```
alm-lexer (0 deps)
    ↓
alm-parser (depends: lexer)
    ↓
alm-ir (depends: lexer, parser)
    ↓
├── alm-llvm (depends: ir, target; + inkwell/LLVM)
├── alm-wasm (depends: ir; + wasm-encoder, wasmtime)
└── alm-interp (depends: parser, lexer)

alm-config (depends: serde, serde_yaml)
alm-target (0 deps)
alm-lint (depends: lexer, parser)
alm-agent (depends: lexer, parser, interp, config, lint; + serde_json)
alm-cli (depends: all above)
```

## Adding New Syntax

Full pipeline — touch 6 crates:

### 1. Lexer (`crates/alm-lexer/src/lib.rs`)
- Add variant to `TokenKind` enum
- Handle in `next_token()` match block
- Use `tok!()` macro for span-tracked tokens
- For multi-char: check `self.peek()` then `self.advance()`

### 2. Parser (`crates/alm-parser/src/ast.rs` + `lib.rs`)
- Add AST node to `Expr` enum in `ast.rs`
- Add `span()` match arm in `Expr::span()`
- Parse in appropriate precedence level:
  - Prefix unary → `parse_unary()`
  - Postfix → `parse_postfix()`
  - Primary → `parse_primary()`
  - Match-level → `parse_expr()`

### 3. Interpreter (`crates/alm-interp/src/lib.rs`)
- Add match arm in `eval_expr()`
- Return `Flow::Value(val)` or `Flow::Return(val)`

### 4. IR Lowering (`crates/alm-ir/src/lower.rs`)
- Add to `Inst` enum if new instruction needed (`crates/alm-ir/src/lib.rs`)
- Add match arm in `lower_expr()`

### 5. LLVM Codegen (`crates/alm-llvm/src/lib.rs`)
- Handle new `Inst` variant in the `emit_inst` match within `emit_function()`

### 6. WASM Codegen (`crates/alm-wasm/src/compile.rs`)
- Handle new `Inst` variant in `emit_inst()`

### 7. Tests
- Each crate: add test in `#[cfg(test)] mod tests`
- Naming: `test_<feature_name>`

## Adding New Builtin Function

Only 2 files:

1. **Register** in `alm-interp/src/lib.rs` → `Env::new()`:
```rust
global.insert("myFunc".into(), Value::BuiltIn("myFunc".into()));
```

2. **Implement** in `call_builtin()` match:
```rust
"myFunc" => {
    // implementation
    Ok(Flow::Value(result))
}
```

3. **Document** in:
   - `alm-agent/src/prompt.rs` → GRAMMAR_REF builtins line
   - README.md builtins table
   - `.claude/skills/alm-write.md`

## Adding New CLI Command

1. Add match arm in `main()` dispatch
2. Add to `print_usage()` help text
3. Write `fn cmd_name(args: &[String])` function
4. Pattern: read file → process → print result or exit(1)

## Adding New Target

1. Add variant to `Target` enum in `alm-target/src/lib.rs`
2. Add to `Target::ALL` array
3. Implement all methods: `from_name()`, `name()`, `llvm_triple()`, `object_format()`, etc.
4. LLVM must support the triple — check with `Target::initialize_all()`

## Key Patterns

**Borrow checker in lexer:** Use `tok!()` macro (not closures) to avoid borrowing `self` issues.

**Parser backtracking:** Save `self.pos`, try parsing, restore on failure:
```rust
let saved = self.pos;
// try something
if fails { self.pos = saved; }
```

**IR terminator check:** Before emitting `Ret`, check `current_block_terminated()` to avoid "terminator in middle of block" LLVM errors.

**WASM limitations (alpha):**
- Control flow linearized (no proper block/loop/br yet)
- External calls emit `i64.const 0` placeholder
- Strings stored as i64 pointers

## Error Code Ranges

| Range | Crate | Category |
|-------|-------|----------|
| E000-E099 | alm-lexer | Lex errors |
| E100-E199 | alm-parser | Parse errors |
| E200-E299 | alm-interp | Runtime errors |
| E250+ | alm-wasm | Effect violations |
| E300+ | alm-cli | CLI errors |
| W001-W005 | alm-lint | Warnings |
| E101 | alm-lint | Lint errors |
