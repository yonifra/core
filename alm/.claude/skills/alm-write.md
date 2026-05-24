# Skill: Write ALM Code

Use this skill when generating ALM source code. ALM is token-optimized for LLMs — every design choice minimizes attention compute.

## Syntax Quick Ref

**No keywords.** 24 sigils only:
```
{ } ( ) [ ] : ; . , = > < | @ # ! ? ~ $ ^ & * _
```

Multi-char: `=> := &* ?: == != >= <= ++ -> <- ..`

## Core Patterns

```alm
// Binding
x = 42;
name = "hello";
t := 42;              // stack allocation

// Return
>42

// Function call
result = proc(x, y);

// Try + elvis
val = proc(x)?;       // propagate error
val = proc(x)?: 0;    // default on error

// Match
x | 1 => "one" | 2 => "two" | _ => "other"

// Loop
*(10){#iter};          // 10 iterations with metric
*(){tick()};           // infinite

// Struct
p = Point{x: 1, y: 2};
p.x

// Metric
#requests;
#errors++;

// Annotation
@test(name){assertEq(1, 1)};
@svc(8080){~(r){proc(r)}};

// Async
~handler(r){#rq; >proc(r)?}

// Effect block
!io{print(42)};

// Env
home = $HOME;

// Borrow/move
ref = &x;             // read borrow
mref = &*x;           // mutable borrow
moved = ^x;           // ownership transfer
```

## Rules

1. **camelCase** always. Never snake_case. `procHttpReq` not `process_http_request`
2. **≤12 chars** per identifier. Abbreviate: `req` not `request`, `rq` not `requestCount`
3. **No keywords** — use sigils: `>` not `return`, `*()` not `while`, `|=>` not `match`
4. **`;`** after every statement
5. **No imports** — deps come from alm.yaml
6. **`//`** comments only when truly needed. LLMs don't need comments.

## Effect Safety

Pure functions (safe for sandbox):
- `assert`, `assertEq`, `assertNe`, `assertGt`, `assertLt`
- `len`, `type`, `str`, `retry`
- `#metric`, match, loops, bindings

Effectful (blocked in sandbox:strict):
- `print` → !io
- `fetch`, `connect` → !net
- `readFile`, `writeFile` → !fs
- `now`, `sleep` → !time
- `$ENV` → !io

## Validation Checklist

Before submitting ALM code:
- [ ] All identifiers camelCase and ≤12 chars
- [ ] No snake_case anywhere
- [ ] `;` after each statement
- [ ] `@test` blocks for testable logic
- [ ] No forbidden effects if targeting sandbox
- [ ] `#metric` for observable operations

## Builtins

`assert(v)` `assertEq(a,b)` `assertNe(a,b)` `assertGt(a,b)` `assertLt(a,b)` `print(v...)` `len(v)` `retry(n,fn,args...)` `type(v)` `str(v)`
