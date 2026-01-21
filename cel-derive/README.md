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

### Field-level attributes

- `#[dynamic(skip)]` - Skip this field in the generated implementation. The field will not be accessible from CEL expressions.
- `#[dynamic(rename = "new_name")]` - Use a different name for this field when accessed from CEL expressions.

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
