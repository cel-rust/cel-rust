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

```toml
[dependencies]
cel = "0.11.6"
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
