# Common Expression Language (Rust)

[![Rust](https://github.com/cel-rust/cel-rust/actions/workflows/rust.yml/badge.svg)](https://github.com/cel-rust/cel-rust/actions/workflows/rust.yml)

The [Common Expression Language (CEL)](https://github.com/google/cel-spec) is a non-Turing complete language designed
for simplicity, speed, safety, and
portability. CEL's C-like syntax looks nearly identical to equivalent expressions in C++, Go, Java, and TypeScript. CEL
is ideal for lightweight expression evaluation when a fully sandboxed scripting language is too resource intensive.

```java
// Check whether a resource name starts with a group name.
resource.name.startsWith("/groups/" + auth.claims.group)
```

```go
// Determine whether the request is in the permitted time window.
request.time - resource.age < duration("24h")
```

```typescript
// Check whether all resource names in a list match a given filter.
auth.claims.email_verified && resources.all(r, r.startsWith(auth.claims.email))
```

## Getting Started

Add `cel` to your `Cargo.toml`:

```shell
cargo add cel
```

Create and execute a simple CEL expression:

```rust
use cel::{Context, Program};

fn main() {
    let program = Program::compile("add(2, 3) == 5").unwrap();
    let mut context = Context::default();
    context.add_function("add", |a: i64, b: i64| a + b);
    let value = program.execute(&context).unwrap();
    assert_eq!(value, true.into());
}
```

### Execution budgets

Evaluation can take an optional per-invocation monotonic [`ExecutionBudget`](https://docs.rs/cel/latest/cel/struct.ExecutionBudget.html).
The budget covers evaluation only (not compilation), is checked cooperatively by
the interpreter, and does not mutate a compiled `Program`:

```rust
use cel::{Context, ExecutionBudget, Program};
use std::time::Duration;

let program = Program::compile("items.map(x, x + 1)").unwrap();
let mut context = Context::default();
context.add_variable_from_value("items", vec![1i64; 10_000]);

let budget = ExecutionBudget::with_timeout(Duration::from_millis(5));
match program.execute_with_budget(&context, budget) {
    Ok(value) => println!("{value:?}"),
    Err(cel::ExecutionError::DeadlineExceeded) => println!("timed out"),
    Err(err) => panic!("{err}"),
}
```

Host callbacks and other work that does not return to the interpreter cannot be
preempted mid-call. This is complementary to CEL cost limiting (see
[cel-rust#56](https://github.com/cel-rust/cel-rust/issues/56)).

### Examples

Check out these other examples to learn how to use this library:

- [Simple](./example/src/simple.rs) - A simple example of how to use the library.
- [Variables](./example/src/variables.rs) - Passing variables and using them in your program.
- [Functions](./example/src/functions.rs) - Defining and using custom functions in your program.
- [Concurrent Execution](./example/src/threads.rs) - Executing the same program concurrently.
