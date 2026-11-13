# Common Expression Language (Rust)

[![Rust](https://github.com/cel-rust/cel-rust/actions/workflows/rust.yml/badge.svg)](https://github.com/cel-rust/cel-rust/actions/workflows/rust.yml)

The [Common Expression Language (CEL)](https://github.com/cel-expr/cel-spec) is a non-Turing complete language designed
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

### With CEL & this interpreter

If you want to try CEL out first, before integrating it in a project, we've set up a
[playground](https://cel-rust.github.io/playground/), where you can look at different expressions and their usages,
explore extensions to the language and the libraries currently being shipped.

### With `cel-rust`

Add `cel` to your `Cargo.toml`:

```shell
cargo add cel
```

Create and execute a simple CEL expression:

```rust
use cel::{Context, Env};
use std::sync::Arc;

fn main() {
    let env = Arc::new(Env::stdlib());
    let program = env.compile("add(2, 3) == 5").unwrap();
    let mut context = Context::with_env(env);
    context.add_function("add", |a: i64, b: i64| a + b).unwrap();
    let value = program.execute(&context).unwrap();
    assert_eq!(value, true.into());
}
```

#### Examples

Check out these other examples to learn how to use this library:

- [Simple](./example/src/simple.rs) - A simple example of how to use the library.
- [Variables](./example/src/variables.rs) - Passing variables and using them in your program.
- [Functions](./example/src/functions.rs) - Defining and using custom functions in your program.
- [Concurrent Execution](./example/src/threads.rs) - Executing the same program concurrently.
- [Interrupting Evaluation](./example/src/interrupt.rs) - Cancelling an evaluation from a deadline or another thread, and capping the work it may do.
- [Iteration Budget](./example/src/budget.rs) - Capping how many comprehension iterations an evaluation may perform.

### Bounding evaluation

An evaluation can be cancelled cooperatively and bounded in the amount of work it does. Both are checked
while iterating comprehensions (`all`, `exists`, `map`, `filter`, ...), and neither can be swallowed by
`||`, `&&` or optional accessors: once tripped, the evaluation as a whole fails. This differs from cel-go,
where an interrupted comprehension is an ordinary error that `||` and `&&` can absorb. The `cel::runtime`
docs compare both mechanisms with cel-go's and cel-cpp's.

Cancellation is per evaluation, so the handle is set on the `Context`. Anything implementing
`cel::Interrupt` works: an `AtomicBool`, a `cel::Deadline`, or any `Fn() -> bool + Send + Sync` closure.

```rust
use cel::{Context, Deadline, Env, ExecutionError};
use std::sync::Arc;
use std::time::Duration;

let env = Arc::new(Env::stdlib());
let program = env.compile("list.all(x, list.exists(y, y > x))").unwrap();
let deadline = Deadline::after(Duration::from_millis(50));
let mut context = Context::with_env(env);
context.add_variable("list", (0..100_000).collect::<Vec<i64>>()).unwrap();
context.set_interrupt(&deadline);
assert_eq!(program.execute(&context), Err(ExecutionError::Interrupted));
```

The iteration budget is runtime policy, so it lives on the `Env`. It is off by default.

```rust
use cel::{Context, Env, RuntimeOptions};
use std::sync::Arc;

let mut env = Env::stdlib();
env.set_options(RuntimeOptions::default().with_max_iterations(10_000));
let env = Arc::new(env);
let program = env.compile("[1, 2, 3].all(x, x > 0)").unwrap();
let context = Context::with_env(env);
assert_eq!(program.execute(&context), Ok(true.into()));
```

Custom functions can poll the handle through `FunctionContext::is_interrupted()` to return early from
long-running work.

## Goals

This project aims to be a fully conformant Rust implementation of the
[CEL specification](https://github.com/cel-expr/cel-spec). Conformance comes before every other concern, including
performance and API ergonomics. See [GOVERNANCE.md](./GOVERNANCE.md) for how the project is run.

### What This Means

* **The specification is the reference:** Syntax, semantics, standard definitions and extension libraries behave as
  cel-spec defines them. Where another CEL implementation disagrees with the specification, cel-rust follows the
  specification.
* **Conformance is tested:** CI runs the cel-spec conformance suite on every pull request and every push to `master`.
  It is the same suite other CEL implementations, such as cel-go, cel-cpp and cel-java, test against.
* **Known gaps are tracked:** Conformance tests cel-rust does not pass yet are listed and tracked until fixed; see
  [conformance](./conformance/README.md).

### What This Does Not Mean

* We do not add syntax, change semantics, or extend the standard definitions, even where doing so would suit Rust.
* Changes to CEL itself are proposed to [cel-spec](https://github.com/cel-expr/cel-spec), following its
  [governance process](https://github.com/cel-expr/cel-spec/blob/master/GOVERNANCE.md). cel-rust adopts them once they
  are part of a cel-spec release.
* It does not stop applications from registering their own functions: the specification leaves that mechanism to
  implementations.
