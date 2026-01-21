# cel-derive

Derive macros for the `cel` crate.

## Usage

Add `cel` to your dependencies (which will automatically include `cel-derive`):

```toml
[dependencies]
cel = "0.13"
```

Then use the `DynamicType` derive macro on your structs:

```rust
use cel::DynamicType;

#[derive(DynamicType)]
pub struct MyData<'a> {
    name: &'a str,
    value: i64,
    #[dynamic(skip)]
    internal_field: bool,
}
```

## Attributes

### Struct-level attributes

- `#[dynamic(auto_materialize)]` - Override `auto_materialize()` to return `true`. This is typically used for primitive-like types that should always be eagerly converted to CEL values.
- `#[dynamic(crate = "path")]` - Specify the path to the `cel` crate. Useful when using this derive macro inside the `cel` crate itself or when the crate is re-exported under a different name.
  - Use `#[dynamic(crate = "crate")]` when deriving inside the cel crate itself
  - Use `#[dynamic(crate = "::cel")]` or omit for normal external usage
  - Use `#[dynamic(crate = "::my_crate::cel")]` if cel is re-exported from another crate

### Field-level attributes

- `#[dynamic(skip)]` - Skip this field in the generated implementation. The field will not be accessible from CEL expressions.
- `#[dynamic(rename = "new_name")]` - Use a different name for this field when accessed from CEL expressions.
- `#[dynamic(with = "function")]` - Transform the field value using a helper function before passing to `maybe_materialize`. The function receives a reference to the field (note: if the field is already a reference like `&'a T`, the function receives `&&'a T`) and should return a reference to a type that implements `DynamicType + DynamicValueVtable`. Useful for newtype wrappers or extracting inner values.

  **Important**: Due to type inference limitations, you must use a named helper function with explicit lifetime annotations rather than inline closures.

## Example

```rust
use cel::DynamicType;

#[derive(DynamicType)]
pub struct HttpRequest<'a> {
    method: &'a str,
    path: &'a str,
    #[dynamic(rename = "statusCode")]
    status_code: i32,
    #[dynamic(skip)]
    internal_timestamp: u64,
}
```

### Using `with` attribute for newtype wrappers

```rust
use cel::DynamicType;

// Newtype wrapper around serde_json::Value
#[derive(Clone, Debug)]
pub struct Claims(pub serde_json::Value);

// Helper function to extract the inner value from Claims
// Note: the function receives &&Claims because claims field is &'a Claims
fn extract_claims<'a>(c: &'a &'a Claims) -> &'a serde_json::Value {
    &c.0
}

#[derive(DynamicType)]
pub struct HttpRequestRef<'a> {
    method: &'a str,
    path: &'a str,
    // Extract the inner serde_json::Value from the Claims newtype
    #[dynamic(with = "extract_claims")]
    claims: &'a Claims,
}
```

In this example, the `with` attribute uses a helper function to extract the inner `serde_json::Value` from the `Claims` newtype wrapper. The function receives `&&'a Claims` (a reference to the `&'a Claims` field) and returns `&serde_json::Value`. The explicit lifetime annotations are necessary for the compiler to properly infer the types.

### Using inside the cel crate

When using `#[derive(DynamicType)]` inside the `cel` crate itself, you need to either:

1. Use the `crate` attribute:
```rust
#[derive(DynamicType)]
#[dynamic(crate = "crate")]
pub struct InternalType {
    field: String,
}
```

2. Or add an extern crate alias at the module level:
```rust
extern crate self as cel;

#[derive(DynamicType)]
pub struct InternalType {
    field: String,
}
```

## For Foreign Types

If you need to implement `DynamicType` for a type you don't own (like types from other crates), you can manually implement `DynamicType` and use the `impl_dynamic_vtable!` macro to generate the boilerplate vtable implementation:

```rust
use cel::types::dynamic::{DynamicType, DynamicValueVtable};
use cel::impl_dynamic_vtable;

impl DynamicType for serde_json::Value {
    fn materialize(&self) -> cel::Value<'_> {
        cel::to_value(self).unwrap()
    }
    
    fn auto_materialize(&self) -> bool {
        false
    }
    
    fn field(&self, field: &str) -> Option<cel::Value<'_>> {
        // Custom field lookup logic
        None
    }
}

impl_dynamic_vtable!(serde_json::Value);
```
