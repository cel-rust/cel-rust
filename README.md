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

### Examples

Check out these other examples to learn how to use this library:

- [Simple](./example/src/simple.rs) - A simple example of how to use the library.
- [Variables](./example/src/variables.rs) - Passing variables and using them in your program.
- [Functions](./example/src/functions.rs) - Defining and using custom functions in your program.
- [Concurrent Execution](./example/src/threads.rs) - Executing the same program concurrently.
- [Interrupting Evaluation](./example/src/interrupt.rs) - Cancelling an evaluation from a deadline or another thread, and capping the work it may do.

### Bounding evaluation

An evaluation can be cancelled cooperatively and bounded in the amount of work it does. Both are checked
while iterating comprehensions (`all`, `exists`, `map`, `filter`, ...), and neither can be swallowed by
`||`, `&&` or optional accessors: once tripped, the evaluation as a whole fails.

Cancellation is per evaluation, so the handle is set on the `Context`. Anything implementing
`cel::Interrupt` works: an `AtomicBool`, a `cel::Deadline`, or any `Fn() -> bool + Send + Sync` closure.

```rust
use cel::{Context, Deadline, ExecutionError, Program};
use std::time::Duration;

let program = Program::compile("list.all(x, list.exists(y, y > x))").unwrap();
let deadline = Deadline::after(Duration::from_millis(50));
let mut context = Context::default();
context.add_variable("list", (0..100_000).collect::<Vec<i64>>()).unwrap();
context.set_interrupt(&deadline);
assert_eq!(program.execute(&context), Err(ExecutionError::Interrupted));
```

The iteration budget is runtime policy, so it lives on the `Env`. It is off by default.

```rust
use cel::{Context, Env, ExecutionError, Program, RuntimeOptions};
use std::sync::Arc;

let mut env = Env::stdlib();
env.set_options(RuntimeOptions::default().with_max_iterations(10_000));
let context = Context::with_env(Arc::new(env));
let program = Program::compile("[1, 2, 3].all(x, x > 0)").unwrap();
assert_eq!(program.execute(&context), Ok(true.into()));
```

Custom functions can poll the handle through `FunctionContext::is_interrupted()` to return early from
long-running work.
