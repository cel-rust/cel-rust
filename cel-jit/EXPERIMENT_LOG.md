# cel-jit Experiment Log

## Overview

This document chronicles the development of `cel-jit`, a JIT-compiled execution backend for CEL (Common Expression Language) expressions in Rust. The goal was to experiment with bytecode compilation as an alternative to the tree-walking interpreter in the main `cel` crate.

## Project Goals

- Create a JIT compilation backend for CEL expressions using Cranelift
- Compare performance against the existing tree-walking interpreter
- Explore inline optimizations for common operations

## Architecture

### Directory Structure

```
cel-jit/
├── Cargo.toml
├── src/
│   ├── lib.rs              # Public API: CompiledProgram, eval()
│   ├── error.rs            # CompileError type
│   ├── compiler/
│   │   ├── mod.rs          # Cranelift JIT compiler setup
│   │   ├── lowering.rs     # CEL AST → Cranelift IR
│   │   └── runtime.rs      # Runtime symbol registration
│   └── runtime/
│       ├── mod.rs          # RuntimeContext
│       ├── value.rs        # BoxedValue (tagged pointers)
│       └── ops.rs          # Runtime operator implementations
└── benches/
    └── comparison.rs       # Benchmark suite
```

### Key Design Decisions

#### 1. Tagged Pointer Representation

CEL values are dynamically typed, but Cranelift requires static types. Solution: tagged pointers using low 3 bits (8-byte aligned):

| Tag | Value Type |
|-----|------------|
| `0b000` | Heap-allocated `Value` pointer |
| `0b001` | Inline small integer (shifted left 3 bits) |
| `0b010` | Inline boolean |
| `0b011` | Null |

Small integers can hold values from -2^60 to 2^60-1 without heap allocation.

#### 2. Dual Return Convention

Every compiled function returns `(value: u64, error_flag: u64)`:
- `error_flag = 0`: Success, value contains result
- `error_flag = 1`: Error occurred, check RuntimeContext for details

#### 3. Runtime Context

All compiled code receives a pointer to `RuntimeContext` containing:
- Reference to CEL's `Context` (variables, functions)
- Error storage for propagating errors back to Rust

#### 4. Runtime Functions

All operators and built-in functions are implemented as `extern "C"` functions callable from compiled code:
- Binary operators: `rt_add`, `rt_sub`, `rt_mul`, `rt_div`, `rt_rem`
- Comparisons: `rt_eq`, `rt_ne`, `rt_lt`, `rt_le`, `rt_gt`, `rt_ge`
- Unary: `rt_not`, `rt_neg`
- Built-ins: `rt_size`, `rt_contains`, `rt_to_bool`
- Collections: `rt_make_list`, `rt_make_map`, `rt_index`, `rt_member`, `rt_in`
- Boxing: `rt_box_int`, `rt_box_uint`, `rt_box_float`, `rt_box_string`, `rt_box_bytes`

## Implementation Phases

### Phase 1: Foundation

- Created crate structure with Cranelift dependencies
- Implemented `BoxedValue` with tagged pointer encoding
- Implemented `RuntimeContext` for CEL context access
- Implemented core runtime operators

### Phase 2: Basic Compiler

- Set up Cranelift JIT with native ISA
- Implemented literal lowering (Int, Bool, Null, String, etc.)
- Implemented identifier resolution via `rt_get_variable`
- Implemented binary/unary operators
- Implemented short-circuit evaluation for `&&` and `||`
- Implemented ternary conditional with CLIF branching

### Phase 3: Complex Expressions

- Implemented `List` construction via `rt_make_list`
- Implemented `Map` construction via `rt_make_map`
- Implemented field access via `rt_member`
- Implemented index operator via `rt_index`

### Phase 4: Inline Optimizations

- Implemented inline integer arithmetic for small integers
- Implemented inline integer comparisons
- Implemented inline negation
- Added `size()` and `contains()` built-in support

### Phase 5: Built-in Functions and FFI Safety

#### Additional Built-in Functions
- `startsWith(prefix)` - Check if string starts with prefix
- `endsWith(suffix)` - Check if string ends with suffix
- `string(val)` - Convert value to string
- `int(val)` - Convert value to integer
- `uint(val)` - Convert value to unsigned integer
- `double(val)` - Convert value to float
- `bytes(val)` - Convert string to bytes
- `type(val)` - Get type name as string

#### FFI Safety Improvements
Changed `RuntimeResult` from a tuple `(u64, u64)` to a proper `#[repr(C)]` struct:
```rust
#[repr(C)]
pub struct RuntimeResult {
    pub value: u64,
    pub error: u64,
}
```

### Phase 6: Comprehension and Advanced Features

Comprehensions use a combination of:
- Runtime variables (`rt_set_variable`, `rt_get_variable`) for accumulator and iteration variable tracking
- List helpers (`rt_list_len`, `rt_list_get`) for iteration
- CLIF loop blocks with proper SSA handling

Additional features:
- `has()` macro via `rt_has` runtime function
- `max()`/`min()` variadic functions via `rt_max`/`rt_min`
- `@not_strictly_false` internal operator for comprehension conditions

### Phase 7: Host Function Support and Feature Flags

#### Custom Function Support

Implemented support for user-registered functions via `Context::add_function()`:
- Created `RuntimeContext::call_function()` method using synthetic CallExpr approach
- Stores pre-evaluated arguments as temporary variables in a child context
- Creates synthetic identifier expressions pointing to stored values
- Resolves through the normal CEL resolution path via `Context::resolve()`

#### Feature Flags

Added feature flags matching the original `cel` crate:
- `regex` (default): Enables `matches()` function for regex matching
- `chrono` (default): Enables timestamp and duration types
- `json`: Enables JSON-related functionality

### Phase 8: Performance Optimization

#### Zero-Copy Value Access
Added `as_value_ref()` method to `BoxedValue` for accessing heap-allocated values without cloning.

#### Inline Type Fast Paths
Added `try_as_int()` and `try_as_bool()` methods for extracting inline values without heap access.

Applied to:
- `rt_eq`, `rt_ne` - equality with inline int/bool fast path
- `rt_lt`, `rt_le`, `rt_gt`, `rt_ge` - comparison with inline int fast path
- `rt_to_bool` - bool extraction with inline fast path

## Challenges Encountered

### 1. String Constant Lifetimes

**Problem**: String constants in compiled code referenced stack-allocated strings that were deallocated.

**Solution**: Created `LoweringData` struct to hold `Box<str>` constants that outlive compilation. Stored in `CompiledProgram` alongside the compiled function.

### 2. Private CEL Methods

**Problem**: `Value::member()` and `Value::index()` methods are private in the `cel` crate.

**Solution**: Reimplemented the logic in `rt_member` and `rt_index` using public APIs like `Map::get()`.

### 3. Runtime Function Call Overhead

**Problem**: Initial benchmarks showed compiled code was *slower* than interpreter for simple arithmetic due to function call overhead.

**Solution**: Implemented inline operations that:
1. Check if operands are small integers (tagged)
2. If yes, perform native Cranelift operations directly
3. If no, fall back to runtime function
4. Handle overflow by boxing via runtime

### 4. Host Function Compatibility

**Problem**: CEL functions use `FunctionContext` with lazy argument evaluation via `Expression` references. JIT evaluates arguments eagerly.

**Solution**: Built-in functions (`size`, `contains`) are handled specially in the compiler. Custom functions use synthetic CallExpr with pre-evaluated arguments stored as temporary variables.

## Benchmark Results (2025-12-11)

### Core Operations

| Benchmark | Interpreted | Compiled | Speedup |
|-----------|-------------|----------|---------|
| simple_arithmetic | 108 ns | **13 ns** | **8.3x** |
| comparison | 110 ns | **13 ns** | **8.5x** |
| conditional | 126 ns | **61 ns** | **2.1x** |
| nested_expression | 355 ns | **265 ns** | **1.3x** |
| member_access | 340 ns | **306 ns** | **1.1x** |
| list_indexing | 201 ns | **183 ns** | **1.1x** |

### Comprehensions

| Benchmark | Interpreted | Compiled | Speedup |
|-----------|-------------|----------|---------|
| list_filter | 3.1 µs | **1.5 µs** | **2.1x** |
| list_map | 1.8 µs | **1.1 µs** | **1.6x** |
| all_comprehension | 1.3 µs | **255 ns** | **5.1x** |
| exists_comprehension | 824 ns | **200 ns** | **4.1x** |

### Key Insight

The JIT compiler shows significant speedups across **all** benchmark categories:

- **Simple arithmetic/comparison**: 8x faster (inline small integer fast path)
- **Logical operations**: Fast with inline bool fast path
- **Comprehensions**: 1.6-5x faster (loop overhead amortized over iterations)

## Dependencies

```toml
[dependencies]
cel = { path = "../cel" }
cranelift-codegen = "0.115"
cranelift-frontend = "0.115"
cranelift-module = "0.115"
cranelift-jit = "0.115"
cranelift-native = "0.115"
target-lexicon = "0.12"
thiserror = "1.0"

[dev-dependencies]
criterion = { version = "0.5", features = ["html_reports"] }
```

## Public API

```rust
use cel_jit::CompiledProgram;
use cel::Context;

// Compile a CEL expression
let program = CompiledProgram::compile("x + y > 10").unwrap();

// Execute with context
let mut ctx = Context::default();
ctx.add_variable_from_value("x", 5i64);
ctx.add_variable_from_value("y", 10i64);

let result = program.execute(&ctx).unwrap();
assert_eq!(result, cel::Value::Bool(true));

// Or use the convenience function
let result = cel_jit::eval("1 + 2", &ctx).unwrap();
```

## When to Use JIT vs Interpreter

**Use JIT (cel-jit) when:**
- Expression is primarily arithmetic/comparison (8x faster)
- Expression will be executed many times
- Compilation cost can be amortized
- Expression uses comprehensions (1.6-5x faster)

**Use Interpreter (cel) when:**
- Expression is executed only once or few times
- Compilation latency is a concern
- Memory footprint is critical (JIT requires Cranelift)

## Future Optimization Opportunities

1. **Stack-based comprehension variables**: Store loop variables on the native stack instead of in a HashMap, avoiding string lookups entirely.

2. **Inline list iteration**: Generate CLIF code that directly indexes into `Arc<Vec<Value>>` without FFI calls.

3. **Specialized comprehension codepaths**: Detect simple patterns like `list.filter(x, x > 5)` and generate optimized inline code.

4. **Arena allocation**: Use a bump allocator for temporary Values created during comprehension execution.

5. **Lazy evaluation caching**: Cache function call results when arguments haven't changed.
