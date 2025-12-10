//! Runtime operator implementations for compiled CEL expressions.
//!
//! These functions are called from compiled bytecode to perform operations
//! on CEL values. All functions follow the convention of returning
//! `(value: u64, error_flag: u64)` where `error_flag` is 0 for success.
//!
//! # Safety
//!
//! All extern "C" functions in this module are designed to be called from
//! JIT-generated machine code. They accept raw pointers that are guaranteed
//! to be valid by the JIT compiler. The functions are not marked `unsafe`
//! because they are public API entry points, but callers must ensure:
//! - `ctx` pointers are valid `RuntimeContext` pointers
//! - String pointers (`name_ptr`, `field_ptr`, etc.) point to valid UTF-8
//! - Array pointers (`elements`, `keys`, `values`, etc.) are valid for the given length

#![allow(clippy::not_unsafe_ptr_arg_deref)]

use super::{rt_err, rt_ok, BoxedValue, RuntimeContext, RuntimeResult};
use cel::{ExecutionError, Value};
use std::cmp::Ordering;
use std::sync::Arc;

/// Add two values.
/// This function CONSUMES both input BoxedValues.
#[no_mangle]
pub extern "C" fn rt_add(ctx: *mut RuntimeContext, left: u64, right: u64) -> RuntimeResult {
    let left_box = unsafe { BoxedValue::from_raw(left) };
    let right_box = unsafe { BoxedValue::from_raw(right) };

    // Fast path: both are inline small integers (no heap to free)
    if let (Some(l), Some(r)) = (left_box.try_as_int(), right_box.try_as_int()) {
        if let Some(result) = l.checked_add(r) {
            if let Some(boxed) = BoxedValue::small_int(result) {
                return rt_ok(boxed);
            }
            // Result doesn't fit in small int, fall back to heap allocation
            return rt_ok(BoxedValue::from_value(Value::Int(result)));
        }
        // Overflow - fall through to full path for error handling
    }

    let ctx = unsafe { &*ctx };
    // Consume both operands
    let left_val = unsafe { left_box.into_value() };
    let right_val = unsafe { right_box.into_value() };

    match left_val + right_val {
        Ok(result) => rt_ok(BoxedValue::from_value(result)),
        Err(e) => {
            ctx.set_error(e);
            rt_err()
        }
    }
}

/// Subtract two values.
/// This function CONSUMES both input BoxedValues.
#[no_mangle]
pub extern "C" fn rt_sub(ctx: *mut RuntimeContext, left: u64, right: u64) -> RuntimeResult {
    let left_box = unsafe { BoxedValue::from_raw(left) };
    let right_box = unsafe { BoxedValue::from_raw(right) };

    // Fast path: both are inline small integers
    if let (Some(l), Some(r)) = (left_box.try_as_int(), right_box.try_as_int()) {
        if let Some(result) = l.checked_sub(r) {
            if let Some(boxed) = BoxedValue::small_int(result) {
                return rt_ok(boxed);
            }
            return rt_ok(BoxedValue::from_value(Value::Int(result)));
        }
    }

    let ctx = unsafe { &*ctx };
    let left_val = unsafe { left_box.into_value() };
    let right_val = unsafe { right_box.into_value() };

    match left_val - right_val {
        Ok(result) => rt_ok(BoxedValue::from_value(result)),
        Err(e) => {
            ctx.set_error(e);
            rt_err()
        }
    }
}

/// Multiply two values.
/// This function CONSUMES both input BoxedValues.
#[no_mangle]
pub extern "C" fn rt_mul(ctx: *mut RuntimeContext, left: u64, right: u64) -> RuntimeResult {
    let left_box = unsafe { BoxedValue::from_raw(left) };
    let right_box = unsafe { BoxedValue::from_raw(right) };

    // Fast path: both are inline small integers
    if let (Some(l), Some(r)) = (left_box.try_as_int(), right_box.try_as_int()) {
        if let Some(result) = l.checked_mul(r) {
            if let Some(boxed) = BoxedValue::small_int(result) {
                return rt_ok(boxed);
            }
            return rt_ok(BoxedValue::from_value(Value::Int(result)));
        }
    }

    let ctx = unsafe { &*ctx };
    let left_val = unsafe { left_box.into_value() };
    let right_val = unsafe { right_box.into_value() };

    match left_val * right_val {
        Ok(result) => rt_ok(BoxedValue::from_value(result)),
        Err(e) => {
            ctx.set_error(e);
            rt_err()
        }
    }
}

/// Divide two values.
/// This function CONSUMES both input BoxedValues.
#[no_mangle]
pub extern "C" fn rt_div(ctx: *mut RuntimeContext, left: u64, right: u64) -> RuntimeResult {
    let left_box = unsafe { BoxedValue::from_raw(left) };
    let right_box = unsafe { BoxedValue::from_raw(right) };

    // Fast path: both are inline small integers (but must check for div by zero)
    if let (Some(l), Some(r)) = (left_box.try_as_int(), right_box.try_as_int()) {
        if r != 0 {
            if let Some(result) = l.checked_div(r) {
                if let Some(boxed) = BoxedValue::small_int(result) {
                    return rt_ok(boxed);
                }
                return rt_ok(BoxedValue::from_value(Value::Int(result)));
            }
        }
        // Division by zero or overflow - fall through for error handling
    }

    let ctx = unsafe { &*ctx };
    let left_val = unsafe { left_box.into_value() };
    let right_val = unsafe { right_box.into_value() };

    match left_val / right_val {
        Ok(result) => rt_ok(BoxedValue::from_value(result)),
        Err(e) => {
            ctx.set_error(e);
            rt_err()
        }
    }
}

/// Remainder of two values.
/// This function CONSUMES both input BoxedValues.
#[no_mangle]
pub extern "C" fn rt_rem(ctx: *mut RuntimeContext, left: u64, right: u64) -> RuntimeResult {
    let left_box = unsafe { BoxedValue::from_raw(left) };
    let right_box = unsafe { BoxedValue::from_raw(right) };

    // Fast path: both are inline small integers
    if let (Some(l), Some(r)) = (left_box.try_as_int(), right_box.try_as_int()) {
        if r != 0 {
            if let Some(result) = l.checked_rem(r) {
                if let Some(boxed) = BoxedValue::small_int(result) {
                    return rt_ok(boxed);
                }
                return rt_ok(BoxedValue::from_value(Value::Int(result)));
            }
        }
    }

    let ctx = unsafe { &*ctx };
    let left_val = unsafe { left_box.into_value() };
    let right_val = unsafe { right_box.into_value() };

    match left_val % right_val {
        Ok(result) => rt_ok(BoxedValue::from_value(result)),
        Err(e) => {
            ctx.set_error(e);
            rt_err()
        }
    }
}

/// Check equality of two values.
/// This function CONSUMES both input BoxedValues.
#[no_mangle]
pub extern "C" fn rt_eq(_ctx: *mut RuntimeContext, left: u64, right: u64) -> RuntimeResult {
    // Fast path: if raw bits are equal and both are inline, they're equal
    if left == right {
        let left_box = unsafe { BoxedValue::from_raw(left) };
        // Only for inline values (small int, bool, null) can we short-circuit
        if left_box.is_small_int() || left_box.is_bool() || left_box.is_null() {
            return rt_ok(BoxedValue::bool(true));
        }
    }

    let left_box = unsafe { BoxedValue::from_raw(left) };
    let right_box = unsafe { BoxedValue::from_raw(right) };

    // Fast path: both are inline small integers
    if let (Some(l), Some(r)) = (left_box.try_as_int(), right_box.try_as_int()) {
        return rt_ok(BoxedValue::bool(l == r));
    }

    // Fast path: both are inline booleans
    if let (Some(l), Some(r)) = (left_box.try_as_bool(), right_box.try_as_bool()) {
        return rt_ok(BoxedValue::bool(l == r));
    }

    let left_val = unsafe { left_box.into_value() };
    let right_val = unsafe { right_box.into_value() };
    let result = left_val == right_val;
    rt_ok(BoxedValue::bool(result))
}

/// Check inequality of two values.
/// This function CONSUMES both input BoxedValues.
#[no_mangle]
pub extern "C" fn rt_ne(_ctx: *mut RuntimeContext, left: u64, right: u64) -> RuntimeResult {
    // Fast path: if raw bits are equal and both are inline, they're equal (so not not-equal)
    if left == right {
        let left_box = unsafe { BoxedValue::from_raw(left) };
        if left_box.is_small_int() || left_box.is_bool() || left_box.is_null() {
            return rt_ok(BoxedValue::bool(false));
        }
    }

    let left_box = unsafe { BoxedValue::from_raw(left) };
    let right_box = unsafe { BoxedValue::from_raw(right) };

    // Fast path: both are inline small integers
    if let (Some(l), Some(r)) = (left_box.try_as_int(), right_box.try_as_int()) {
        return rt_ok(BoxedValue::bool(l != r));
    }

    // Fast path: both are inline booleans
    if let (Some(l), Some(r)) = (left_box.try_as_bool(), right_box.try_as_bool()) {
        return rt_ok(BoxedValue::bool(l != r));
    }

    let left_val = unsafe { left_box.into_value() };
    let right_val = unsafe { right_box.into_value() };
    let result = left_val != right_val;
    rt_ok(BoxedValue::bool(result))
}

/// Compare less than.
/// This function CONSUMES both input BoxedValues.
#[no_mangle]
pub extern "C" fn rt_lt(ctx: *mut RuntimeContext, left: u64, right: u64) -> RuntimeResult {
    let left_box = unsafe { BoxedValue::from_raw(left) };
    let right_box = unsafe { BoxedValue::from_raw(right) };

    // Fast path: both are inline small integers
    if let (Some(l), Some(r)) = (left_box.try_as_int(), right_box.try_as_int()) {
        return rt_ok(BoxedValue::bool(l < r));
    }

    let ctx = unsafe { &*ctx };
    let left_val = unsafe { left_box.into_value() };
    let right_val = unsafe { right_box.into_value() };

    match left_val.partial_cmp(&right_val) {
        Some(Ordering::Less) => rt_ok(BoxedValue::bool(true)),
        Some(_) => rt_ok(BoxedValue::bool(false)),
        None => {
            ctx.set_error(ExecutionError::ValuesNotComparable(
                left_val.clone(),
                right_val.clone(),
            ));
            rt_err()
        }
    }
}

/// Compare less than or equal.
/// This function CONSUMES both input BoxedValues.
#[no_mangle]
pub extern "C" fn rt_le(ctx: *mut RuntimeContext, left: u64, right: u64) -> RuntimeResult {
    let left_box = unsafe { BoxedValue::from_raw(left) };
    let right_box = unsafe { BoxedValue::from_raw(right) };

    // Fast path: both are inline small integers
    if let (Some(l), Some(r)) = (left_box.try_as_int(), right_box.try_as_int()) {
        return rt_ok(BoxedValue::bool(l <= r));
    }

    let ctx = unsafe { &*ctx };
    let left_val = unsafe { left_box.into_value() };
    let right_val = unsafe { right_box.into_value() };

    match left_val.partial_cmp(&right_val) {
        Some(Ordering::Less | Ordering::Equal) => rt_ok(BoxedValue::bool(true)),
        Some(_) => rt_ok(BoxedValue::bool(false)),
        None => {
            ctx.set_error(ExecutionError::ValuesNotComparable(
                left_val.clone(),
                right_val.clone(),
            ));
            rt_err()
        }
    }
}

/// Compare greater than.
/// This function CONSUMES both input BoxedValues.
#[no_mangle]
pub extern "C" fn rt_gt(ctx: *mut RuntimeContext, left: u64, right: u64) -> RuntimeResult {
    let left_box = unsafe { BoxedValue::from_raw(left) };
    let right_box = unsafe { BoxedValue::from_raw(right) };

    // Fast path: both are inline small integers
    if let (Some(l), Some(r)) = (left_box.try_as_int(), right_box.try_as_int()) {
        return rt_ok(BoxedValue::bool(l > r));
    }

    let ctx = unsafe { &*ctx };
    let left_val = unsafe { left_box.into_value() };
    let right_val = unsafe { right_box.into_value() };

    match left_val.partial_cmp(&right_val) {
        Some(Ordering::Greater) => rt_ok(BoxedValue::bool(true)),
        Some(_) => rt_ok(BoxedValue::bool(false)),
        None => {
            ctx.set_error(ExecutionError::ValuesNotComparable(
                left_val.clone(),
                right_val.clone(),
            ));
            rt_err()
        }
    }
}

/// Compare greater than or equal.
/// This function CONSUMES both input BoxedValues.
#[no_mangle]
pub extern "C" fn rt_ge(ctx: *mut RuntimeContext, left: u64, right: u64) -> RuntimeResult {
    let left_box = unsafe { BoxedValue::from_raw(left) };
    let right_box = unsafe { BoxedValue::from_raw(right) };

    // Fast path: both are inline small integers
    if let (Some(l), Some(r)) = (left_box.try_as_int(), right_box.try_as_int()) {
        return rt_ok(BoxedValue::bool(l >= r));
    }

    let ctx = unsafe { &*ctx };
    let left_val = unsafe { left_box.into_value() };
    let right_val = unsafe { right_box.into_value() };

    match left_val.partial_cmp(&right_val) {
        Some(Ordering::Greater | Ordering::Equal) => rt_ok(BoxedValue::bool(true)),
        Some(_) => rt_ok(BoxedValue::bool(false)),
        None => {
            ctx.set_error(ExecutionError::ValuesNotComparable(
                left_val.clone(),
                right_val.clone(),
            ));
            rt_err()
        }
    }
}

/// Logical NOT.
/// This function CONSUMES the input BoxedValue.
#[no_mangle]
pub extern "C" fn rt_not(ctx: *mut RuntimeContext, val: u64) -> RuntimeResult {
    let boxed = unsafe { BoxedValue::from_raw(val) };

    // Fast path: inline boolean (no heap allocation to free)
    if let Some(b) = boxed.try_as_bool() {
        return rt_ok(BoxedValue::bool(!b));
    }

    let ctx = unsafe { &*ctx };
    // Consume the value to free heap allocation
    let val = unsafe { boxed.into_value() };

    match val {
        Value::Bool(b) => rt_ok(BoxedValue::bool(!b)),
        _ => {
            ctx.set_error(ExecutionError::UnsupportedUnaryOperator("not", val));
            rt_err()
        }
    }
}

/// Numeric negation.
/// This function CONSUMES the input BoxedValue.
#[no_mangle]
pub extern "C" fn rt_neg(ctx: *mut RuntimeContext, val: u64) -> RuntimeResult {
    let boxed = unsafe { BoxedValue::from_raw(val) };

    // Fast path: inline small integer (no heap allocation to free)
    if let Some(i) = boxed.try_as_int() {
        if let Some(result) = i.checked_neg() {
            if let Some(boxed) = BoxedValue::small_int(result) {
                return rt_ok(boxed);
            }
            return rt_ok(BoxedValue::from_value(Value::Int(result)));
        }
        // Overflow - fall through
    }

    let ctx = unsafe { &*ctx };
    // Consume the value to free heap allocation
    let val = unsafe { boxed.into_value() };

    match val {
        Value::Int(i) => match i.checked_neg() {
            Some(result) => rt_ok(BoxedValue::from_value(Value::Int(result))),
            None => {
                ctx.set_error(ExecutionError::Overflow("neg", val.clone(), Value::Null));
                rt_err()
            }
        },
        Value::Float(f) => rt_ok(BoxedValue::from_value(Value::Float(-f))),
        _ => {
            ctx.set_error(ExecutionError::UnsupportedUnaryOperator("neg", val));
            rt_err()
        }
    }
}

/// Convert value to boolean for conditional checks.
/// Returns 1 for true, 0 for false.
/// This function CONSUMES the input BoxedValue.
#[no_mangle]
pub extern "C" fn rt_to_bool(ctx: *mut RuntimeContext, val: u64) -> u64 {
    let boxed = unsafe { BoxedValue::from_raw(val) };

    // Fast path: inline boolean (no heap allocation to free)
    if let Some(b) = boxed.try_as_bool() {
        return b as u64;
    }

    // Consume the value to free heap allocation
    let val = unsafe { boxed.into_value() };

    match val {
        Value::Bool(b) => b as u64,
        _ => {
            let ctx = unsafe { &*ctx };
            ctx.set_error(ExecutionError::UnexpectedType {
                got: format!("{:?}", val),
                want: "bool".to_string(),
            });
            0
        }
    }
}

/// Get a variable from context by name.
/// First checks comprehension-scoped variables, then falls back to CEL context.
#[no_mangle]
pub extern "C" fn rt_get_variable(
    ctx: *mut RuntimeContext,
    name_ptr: *const u8,
    name_len: usize,
) -> RuntimeResult {
    let ctx = unsafe { &*ctx };
    let name = unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(name_ptr, name_len)) };

    // First check comprehension-scoped variables
    if let Some(val) = ctx.get_comprehension_var(name) {
        return rt_ok(BoxedValue::from_value(val));
    }

    // Fall back to CEL context
    match ctx.cel_context.get_variable(name) {
        Ok(val) => rt_ok(BoxedValue::from_value(val)),
        Err(e) => {
            ctx.set_error(e);
            rt_err()
        }
    }
}

/// Access a member field on a value.
/// This function CONSUMES the target BoxedValue.
#[no_mangle]
pub extern "C" fn rt_member(
    ctx: *mut RuntimeContext,
    target: u64,
    field_ptr: *const u8,
    field_len: usize,
) -> RuntimeResult {
    use cel::objects::Key;
    use std::sync::Arc;

    let ctx = unsafe { &*ctx };
    let target_val = unsafe { BoxedValue::from_raw(target).into_value() };
    let field = unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(field_ptr, field_len)) };

    match &target_val {
        Value::Map(map) => {
            let key = Key::String(Arc::new(field.to_string()));
            match map.get(&key) {
                Some(val) => rt_ok(BoxedValue::from_value(val.clone())),
                None => {
                    ctx.set_error(ExecutionError::no_such_key(field));
                    rt_err()
                }
            }
        }
        _ => {
            ctx.set_error(ExecutionError::no_such_key(field));
            rt_err()
        }
    }
}

/// Index into a collection (list or map).
/// This function CONSUMES both input BoxedValues.
#[no_mangle]
pub extern "C" fn rt_index(ctx: *mut RuntimeContext, target: u64, index: u64) -> RuntimeResult {
    let ctx = unsafe { &*ctx };
    let target_box = unsafe { BoxedValue::from_raw(target) };
    let index_box = unsafe { BoxedValue::from_raw(index) };

    // Consume both values - must happen regardless of path taken
    let target_val = unsafe { target_box.into_value() };
    let index_val = unsafe { index_box.into_value() };

    match (&target_val, &index_val) {
        (Value::List(list), Value::Int(i)) => {
            let idx = *i as usize;
            if idx < list.len() {
                rt_ok(BoxedValue::from_value(list[idx].clone()))
            } else {
                ctx.set_error(ExecutionError::IndexOutOfBounds(index_val));
                rt_err()
            }
        }
        (Value::List(list), Value::UInt(u)) => {
            let idx = *u as usize;
            if idx < list.len() {
                rt_ok(BoxedValue::from_value(list[idx].clone()))
            } else {
                ctx.set_error(ExecutionError::IndexOutOfBounds(index_val));
                rt_err()
            }
        }
        (Value::Map(map), _) => {
            if let Ok(key) = index_val.clone().try_into() {
                match map.get(&key) {
                    Some(val) => rt_ok(BoxedValue::from_value(val.clone())),
                    None => {
                        ctx.set_error(ExecutionError::no_such_key(&format!("{:?}", index_val)));
                        rt_err()
                    }
                }
            } else {
                ctx.set_error(ExecutionError::UnsupportedMapIndex(index_val));
                rt_err()
            }
        }
        _ => {
            ctx.set_error(ExecutionError::UnsupportedIndex(index_val, target_val));
            rt_err()
        }
    }
}

/// Check if a value is in a collection.
/// This function CONSUMES both input BoxedValues.
#[no_mangle]
pub extern "C" fn rt_in(ctx: *mut RuntimeContext, element: u64, collection: u64) -> RuntimeResult {
    let ctx = unsafe { &*ctx };
    let element_val = unsafe { BoxedValue::from_raw(element).into_value() };
    let collection_val = unsafe { BoxedValue::from_raw(collection).into_value() };

    match &collection_val {
        Value::List(list) => {
            let found = list.iter().any(|v| v == &element_val);
            rt_ok(BoxedValue::bool(found))
        }
        Value::Map(map) => {
            // For maps, check if the element is a key
            if let Ok(key) = element_val.clone().try_into() {
                let found = map.map.contains_key(&key);
                rt_ok(BoxedValue::bool(found))
            } else {
                ctx.set_error(ExecutionError::UnsupportedKeyType(element_val));
                rt_err()
            }
        }
        _ => {
            ctx.set_error(ExecutionError::UnsupportedIndex(
                element_val,
                collection_val,
            ));
            rt_err()
        }
    }
}

/// Create a list from an array of boxed values.
/// This function CONSUMES the element BoxedValues.
#[no_mangle]
pub extern "C" fn rt_make_list(
    _ctx: *mut RuntimeContext,
    elements: *const u64,
    len: usize,
) -> RuntimeResult {
    let elements = unsafe { std::slice::from_raw_parts(elements, len) };
    let values: Vec<Value> = elements
        .iter()
        .map(|&raw| unsafe { BoxedValue::from_raw(raw).into_value() })
        .collect();

    rt_ok(BoxedValue::from_value(Value::List(values.into())))
}

/// Create a map from arrays of keys and values.
/// This function CONSUMES the key and value BoxedValues.
#[no_mangle]
pub extern "C" fn rt_make_map(
    ctx: *mut RuntimeContext,
    keys: *const u64,
    values: *const u64,
    len: usize,
) -> RuntimeResult {
    use cel::objects::Key;
    use std::collections::HashMap;

    let ctx = unsafe { &*ctx };
    let keys = unsafe { std::slice::from_raw_parts(keys, len) };
    let values = unsafe { std::slice::from_raw_parts(values, len) };

    let mut map = HashMap::with_capacity(len);

    for i in 0..len {
        let key_val = unsafe { BoxedValue::from_raw(keys[i]).into_value() };
        let val = unsafe { BoxedValue::from_raw(values[i]).into_value() };

        let key: Key = match key_val.clone().try_into() {
            Ok(k) => k,
            Err(_) => {
                // Free remaining unprocessed keys and values
                for j in (i + 1)..len {
                    unsafe { BoxedValue::from_raw(keys[j]).into_value() };
                    unsafe { BoxedValue::from_raw(values[j]).into_value() };
                }
                ctx.set_error(ExecutionError::UnsupportedKeyType(key_val));
                return rt_err();
            }
        };

        map.insert(key, val);
    }

    rt_ok(BoxedValue::from_value(Value::Map(map.into())))
}

/// Box an integer value.
#[no_mangle]
pub extern "C" fn rt_box_int(_ctx: *mut RuntimeContext, val: i64) -> u64 {
    BoxedValue::from_value(Value::Int(val)).as_raw()
}

/// Box an unsigned integer value.
#[no_mangle]
pub extern "C" fn rt_box_uint(_ctx: *mut RuntimeContext, val: u64) -> u64 {
    BoxedValue::from_value(Value::UInt(val)).as_raw()
}

/// Box a float value.
#[no_mangle]
pub extern "C" fn rt_box_float(_ctx: *mut RuntimeContext, val: f64) -> u64 {
    BoxedValue::from_value(Value::Float(val)).as_raw()
}

/// Box a string value.
#[no_mangle]
pub extern "C" fn rt_box_string(_ctx: *mut RuntimeContext, ptr: *const u8, len: usize) -> u64 {
    let s = unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(ptr, len)) };
    BoxedValue::from_value(Value::String(std::sync::Arc::new(s.to_string()))).as_raw()
}

/// Box a bytes value.
#[no_mangle]
pub extern "C" fn rt_box_bytes(_ctx: *mut RuntimeContext, ptr: *const u8, len: usize) -> u64 {
    let bytes = unsafe { std::slice::from_raw_parts(ptr, len) }.to_vec();
    BoxedValue::from_value(Value::Bytes(std::sync::Arc::new(bytes))).as_raw()
}

/// Get the size of a collection or string.
/// This function CONSUMES the input BoxedValue.
#[no_mangle]
pub extern "C" fn rt_size(ctx: *mut RuntimeContext, val: u64) -> RuntimeResult {
    let ctx = unsafe { &*ctx };
    let val = unsafe { BoxedValue::from_raw(val).into_value() };

    match &val {
        Value::String(s) => rt_ok(BoxedValue::from_value(Value::Int(s.len() as i64))),
        Value::Bytes(b) => rt_ok(BoxedValue::from_value(Value::Int(b.len() as i64))),
        Value::List(list) => rt_ok(BoxedValue::from_value(Value::Int(list.len() as i64))),
        Value::Map(map) => rt_ok(BoxedValue::from_value(Value::Int(map.map.len() as i64))),
        _ => {
            ctx.set_error(ExecutionError::FunctionError {
                function: "size".to_string(),
                message: format!("size() not supported for {:?}", val),
            });
            rt_err()
        }
    }
}

/// Check if a string contains a substring, or if a list/map contains an element.
/// This function CONSUMES both input BoxedValues.
#[no_mangle]
pub extern "C" fn rt_contains(ctx: *mut RuntimeContext, target: u64, arg: u64) -> RuntimeResult {
    let ctx = unsafe { &*ctx };
    let target_val = unsafe { BoxedValue::from_raw(target).into_value() };
    let arg_val = unsafe { BoxedValue::from_raw(arg).into_value() };

    match (&target_val, &arg_val) {
        (Value::String(s), Value::String(sub)) => {
            rt_ok(BoxedValue::bool(s.contains(sub.as_str())))
        }
        (Value::List(list), _) => {
            rt_ok(BoxedValue::bool(list.contains(&arg_val)))
        }
        (Value::Map(map), _) => {
            if let Ok(key) = arg_val.clone().try_into() {
                rt_ok(BoxedValue::bool(map.map.contains_key(&key)))
            } else {
                ctx.set_error(ExecutionError::UnsupportedKeyType(arg_val));
                rt_err()
            }
        }
        (Value::Bytes(b), Value::Bytes(sub)) => {
            let found = b.windows(sub.len()).any(|w| w == sub.as_slice());
            rt_ok(BoxedValue::bool(found))
        }
        _ => {
            ctx.set_error(ExecutionError::FunctionError {
                function: "contains".to_string(),
                message: format!("contains() not supported for {:?}", target_val),
            });
            rt_err()
        }
    }
}

/// Check if a string starts with a prefix.
/// This function CONSUMES both input BoxedValues.
#[no_mangle]
pub extern "C" fn rt_starts_with(ctx: *mut RuntimeContext, target: u64, prefix: u64) -> RuntimeResult {
    let ctx = unsafe { &*ctx };
    let target_val = unsafe { BoxedValue::from_raw(target).into_value() };
    let prefix_val = unsafe { BoxedValue::from_raw(prefix).into_value() };

    match (&target_val, &prefix_val) {
        (Value::String(s), Value::String(prefix)) => {
            rt_ok(BoxedValue::bool(s.starts_with(prefix.as_str())))
        }
        _ => {
            ctx.set_error(ExecutionError::FunctionError {
                function: "startsWith".to_string(),
                message: format!("startsWith() requires string arguments, got {:?} and {:?}", target_val, prefix_val),
            });
            rt_err()
        }
    }
}

/// Check if a string ends with a suffix.
/// This function CONSUMES both input BoxedValues.
#[no_mangle]
pub extern "C" fn rt_ends_with(ctx: *mut RuntimeContext, target: u64, suffix: u64) -> RuntimeResult {
    let ctx = unsafe { &*ctx };
    let target_val = unsafe { BoxedValue::from_raw(target).into_value() };
    let suffix_val = unsafe { BoxedValue::from_raw(suffix).into_value() };

    match (&target_val, &suffix_val) {
        (Value::String(s), Value::String(suffix)) => {
            rt_ok(BoxedValue::bool(s.ends_with(suffix.as_str())))
        }
        _ => {
            ctx.set_error(ExecutionError::FunctionError {
                function: "endsWith".to_string(),
                message: format!("endsWith() requires string arguments, got {:?} and {:?}", target_val, suffix_val),
            });
            rt_err()
        }
    }
}

/// Convert a value to a string.
/// This function CONSUMES the input BoxedValue.
#[no_mangle]
pub extern "C" fn rt_string(ctx: *mut RuntimeContext, val: u64) -> RuntimeResult {
    let ctx = unsafe { &*ctx };
    let val = unsafe { BoxedValue::from_raw(val).into_value() };

    let result = match &val {
        Value::String(s) => Value::String(s.clone()),
        Value::Int(i) => Value::String(i.to_string().into()),
        Value::UInt(u) => Value::String(u.to_string().into()),
        Value::Float(f) => Value::String(f.to_string().into()),
        Value::Bool(b) => Value::String(b.to_string().into()),
        Value::Bytes(b) => Value::String(String::from_utf8_lossy(b).into_owned().into()),
        _ => {
            ctx.set_error(ExecutionError::FunctionError {
                function: "string".to_string(),
                message: format!("cannot convert {:?} to string", val),
            });
            return rt_err();
        }
    };

    rt_ok(BoxedValue::from_value(result))
}

/// Convert a value to an integer.
/// This function CONSUMES the input BoxedValue.
#[no_mangle]
pub extern "C" fn rt_int(ctx: *mut RuntimeContext, val: u64) -> RuntimeResult {
    let ctx = unsafe { &*ctx };
    let val = unsafe { BoxedValue::from_raw(val).into_value() };

    let result = match &val {
        Value::Int(i) => Value::Int(*i),
        Value::UInt(u) => {
            if *u > i64::MAX as u64 {
                ctx.set_error(ExecutionError::FunctionError {
                    function: "int".to_string(),
                    message: "integer overflow".to_string(),
                });
                return rt_err();
            }
            Value::Int(*u as i64)
        }
        Value::Float(f) => {
            if *f > i64::MAX as f64 || *f < i64::MIN as f64 {
                ctx.set_error(ExecutionError::FunctionError {
                    function: "int".to_string(),
                    message: "integer overflow".to_string(),
                });
                return rt_err();
            }
            Value::Int(*f as i64)
        }
        Value::String(s) => match s.parse::<i64>() {
            Ok(i) => Value::Int(i),
            Err(e) => {
                ctx.set_error(ExecutionError::FunctionError {
                    function: "int".to_string(),
                    message: format!("string parse error: {}", e),
                });
                return rt_err();
            }
        },
        _ => {
            ctx.set_error(ExecutionError::FunctionError {
                function: "int".to_string(),
                message: format!("cannot convert {:?} to int", val),
            });
            return rt_err();
        }
    };

    rt_ok(BoxedValue::from_value(result))
}

/// Convert a value to an unsigned integer.
/// This function CONSUMES the input BoxedValue.
#[no_mangle]
pub extern "C" fn rt_uint(ctx: *mut RuntimeContext, val: u64) -> RuntimeResult {
    let ctx = unsafe { &*ctx };
    let val = unsafe { BoxedValue::from_raw(val).into_value() };

    let result = match &val {
        Value::UInt(u) => Value::UInt(*u),
        Value::Int(i) => {
            if *i < 0 {
                ctx.set_error(ExecutionError::FunctionError {
                    function: "uint".to_string(),
                    message: "unsigned integer overflow".to_string(),
                });
                return rt_err();
            }
            Value::UInt(*i as u64)
        }
        Value::Float(f) => {
            if *f > u64::MAX as f64 || *f < 0.0 {
                ctx.set_error(ExecutionError::FunctionError {
                    function: "uint".to_string(),
                    message: "unsigned integer overflow".to_string(),
                });
                return rt_err();
            }
            Value::UInt(*f as u64)
        }
        Value::String(s) => match s.parse::<u64>() {
            Ok(u) => Value::UInt(u),
            Err(e) => {
                ctx.set_error(ExecutionError::FunctionError {
                    function: "uint".to_string(),
                    message: format!("string parse error: {}", e),
                });
                return rt_err();
            }
        },
        _ => {
            ctx.set_error(ExecutionError::FunctionError {
                function: "uint".to_string(),
                message: format!("cannot convert {:?} to uint", val),
            });
            return rt_err();
        }
    };

    rt_ok(BoxedValue::from_value(result))
}

/// Convert a value to a double (float).
/// This function CONSUMES the input BoxedValue.
#[no_mangle]
pub extern "C" fn rt_double(ctx: *mut RuntimeContext, val: u64) -> RuntimeResult {
    let ctx = unsafe { &*ctx };
    let val = unsafe { BoxedValue::from_raw(val).into_value() };

    let result = match &val {
        Value::Float(f) => Value::Float(*f),
        Value::Int(i) => Value::Float(*i as f64),
        Value::UInt(u) => Value::Float(*u as f64),
        Value::String(s) => match s.parse::<f64>() {
            Ok(f) => Value::Float(f),
            Err(e) => {
                ctx.set_error(ExecutionError::FunctionError {
                    function: "double".to_string(),
                    message: format!("string parse error: {}", e),
                });
                return rt_err();
            }
        },
        _ => {
            ctx.set_error(ExecutionError::FunctionError {
                function: "double".to_string(),
                message: format!("cannot convert {:?} to double", val),
            });
            return rt_err();
        }
    };

    rt_ok(BoxedValue::from_value(result))
}

/// Convert a string to bytes.
/// This function CONSUMES the input BoxedValue.
#[no_mangle]
pub extern "C" fn rt_bytes(ctx: *mut RuntimeContext, val: u64) -> RuntimeResult {
    let ctx = unsafe { &*ctx };
    let val = unsafe { BoxedValue::from_raw(val).into_value() };

    match &val {
        Value::String(s) => {
            rt_ok(BoxedValue::from_value(Value::Bytes(s.as_bytes().to_vec().into())))
        }
        Value::Bytes(b) => rt_ok(BoxedValue::from_value(Value::Bytes(b.clone()))),
        _ => {
            ctx.set_error(ExecutionError::FunctionError {
                function: "bytes".to_string(),
                message: format!("cannot convert {:?} to bytes", val),
            });
            rt_err()
        }
    }
}

/// Get the type of a value as a string.
/// This function CONSUMES the input BoxedValue.
#[no_mangle]
pub extern "C" fn rt_type(_ctx: *mut RuntimeContext, val: u64) -> RuntimeResult {
    let val = unsafe { BoxedValue::from_raw(val).into_value() };

    let type_name = match &val {
        Value::Int(_) => "int",
        Value::UInt(_) => "uint",
        Value::Float(_) => "double",
        Value::Bool(_) => "bool",
        Value::String(_) => "string",
        Value::Bytes(_) => "bytes",
        Value::List(_) => "list",
        Value::Map(_) => "map",
        Value::Null => "null",
        #[cfg(feature = "chrono")]
        Value::Duration(_) => "duration",
        #[cfg(feature = "chrono")]
        Value::Timestamp(_) => "timestamp",
        Value::Function(_, _) => "function",
        Value::Opaque(_) => "opaque",
    };

    rt_ok(BoxedValue::from_value(Value::String(type_name.to_string().into())))
}

/// Get list length (for comprehension iteration).
/// Uses zero-copy access when possible.
#[no_mangle]
pub extern "C" fn rt_list_len(_ctx: *mut RuntimeContext, list: u64) -> u64 {
    let boxed = unsafe { BoxedValue::from_raw(list) };

    // Try zero-copy access first
    if let Some(val_ref) = unsafe { boxed.as_value_ref() } {
        return match val_ref {
            Value::List(list) => list.len() as u64,
            Value::Map(map) => map.map.len() as u64,
            _ => 0,
        };
    }

    // Fall back to conversion for inline values (which can't be lists/maps anyway)
    0
}

/// Get list element at index (for comprehension iteration).
/// Uses zero-copy access when possible.
#[no_mangle]
pub extern "C" fn rt_list_get(_ctx: *mut RuntimeContext, list: u64, index: u64) -> u64 {
    let boxed = unsafe { BoxedValue::from_raw(list) };

    // Try zero-copy access first
    if let Some(val_ref) = unsafe { boxed.as_value_ref() } {
        return match val_ref {
            Value::List(list) => {
                if (index as usize) < list.len() {
                    BoxedValue::from_value(list[index as usize].clone()).as_raw()
                } else {
                    BoxedValue::null().as_raw()
                }
            }
            Value::Map(map) => {
                // For maps, iteration is over keys
                if let Some((key, _)) = map.map.iter().nth(index as usize) {
                    BoxedValue::from_value(key.clone().into()).as_raw()
                } else {
                    BoxedValue::null().as_raw()
                }
            }
            _ => BoxedValue::null().as_raw(),
        };
    }

    BoxedValue::null().as_raw()
}

/// Append to a list (for comprehension result building).
/// This function consumes the old list but NOT the element.
/// The element is cloned, allowing the caller to free it separately.
#[no_mangle]
pub extern "C" fn rt_list_append(_ctx: *mut RuntimeContext, list: u64, elem: u64) -> u64 {
    // Consume and free the old list's BoxedValue
    let list_val = unsafe { BoxedValue::from_raw(list).into_value() };
    // Clone the element (don't consume - caller will free it)
    let elem_val = unsafe { BoxedValue::from_raw(elem) }.to_value();

    match list_val {
        Value::List(list) => {
            // Try to get mutable access if we're the only owner
            match Arc::try_unwrap(list) {
                Ok(mut vec) => {
                    // We have exclusive ownership, mutate in place
                    vec.push(elem_val);
                    BoxedValue::from_value(Value::List(Arc::new(vec))).as_raw()
                }
                Err(arc) => {
                    // Shared ownership, must clone
                    let mut new_list = (*arc).clone();
                    new_list.push(elem_val);
                    BoxedValue::from_value(Value::List(Arc::new(new_list))).as_raw()
                }
            }
        }
        _ => {
            // Create new list with element
            BoxedValue::from_value(Value::List(Arc::new(vec![elem_val]))).as_raw()
        }
    }
}

/// Get the maximum value from a list or multiple values.
/// This function CONSUMES the argument BoxedValues.
#[no_mangle]
pub extern "C" fn rt_max(ctx: *mut RuntimeContext, vals_ptr: *const u64, vals_len: usize) -> RuntimeResult {
    let ctx = unsafe { &*ctx };
    let vals = unsafe { std::slice::from_raw_parts(vals_ptr, vals_len) };

    if vals.is_empty() {
        return rt_ok(BoxedValue::null());
    }

    // Convert to values, consuming the BoxedValues
    let mut values: Vec<Value> = vals
        .iter()
        .map(|&raw| unsafe { BoxedValue::from_raw(raw).into_value() })
        .collect();

    // If single list argument, use the list contents
    if values.len() == 1 {
        if let Value::List(list) = &values[0] {
            values = list.iter().cloned().collect();
        }
    }

    if values.is_empty() {
        return rt_ok(BoxedValue::null());
    }

    let mut max_val = values[0].clone();
    for val in values.iter().skip(1) {
        match max_val.partial_cmp(val) {
            Some(std::cmp::Ordering::Less) => max_val = val.clone(),
            Some(_) => {}
            None => {
                ctx.set_error(ExecutionError::ValuesNotComparable(max_val, val.clone()));
                return rt_err();
            }
        }
    }

    rt_ok(BoxedValue::from_value(max_val))
}

/// Get the minimum value from a list or multiple values.
/// This function CONSUMES the argument BoxedValues.
#[no_mangle]
pub extern "C" fn rt_min(ctx: *mut RuntimeContext, vals_ptr: *const u64, vals_len: usize) -> RuntimeResult {
    let ctx = unsafe { &*ctx };
    let vals = unsafe { std::slice::from_raw_parts(vals_ptr, vals_len) };

    if vals.is_empty() {
        return rt_ok(BoxedValue::null());
    }

    // Convert to values, consuming the BoxedValues
    let mut values: Vec<Value> = vals
        .iter()
        .map(|&raw| unsafe { BoxedValue::from_raw(raw).into_value() })
        .collect();

    // If single list argument, use the list contents
    if values.len() == 1 {
        if let Value::List(list) = &values[0] {
            values = list.iter().cloned().collect();
        }
    }

    if values.is_empty() {
        return rt_ok(BoxedValue::null());
    }

    let mut min_val = values[0].clone();
    for val in values.iter().skip(1) {
        match min_val.partial_cmp(val) {
            Some(std::cmp::Ordering::Greater) => min_val = val.clone(),
            Some(_) => {}
            None => {
                ctx.set_error(ExecutionError::ValuesNotComparable(min_val, val.clone()));
                return rt_err();
            }
        }
    }

    rt_ok(BoxedValue::from_value(min_val))
}

/// Check if a field exists on a map (used by has() macro).
/// This function CONSUMES the target BoxedValue.
#[no_mangle]
pub extern "C" fn rt_has(
    _ctx: *mut RuntimeContext,
    target: u64,
    field_ptr: *const u8,
    field_len: usize,
) -> RuntimeResult {
    use cel::objects::Key;

    let target_val = unsafe { BoxedValue::from_raw(target).into_value() };
    let field = unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(field_ptr, field_len)) };

    match &target_val {
        Value::Map(map) => {
            let key = Key::String(Arc::new(field.to_string()));
            let has_field = map.map.contains_key(&key);
            rt_ok(BoxedValue::bool(has_field))
        }
        _ => rt_ok(BoxedValue::bool(false)),
    }
}

/// Not strictly false: returns true for non-bool values, or the bool value itself.
/// Used internally by comprehension macros.
/// This function CONSUMES the input BoxedValue.
#[no_mangle]
pub extern "C" fn rt_not_strictly_false(_ctx: *mut RuntimeContext, val: u64) -> RuntimeResult {
    let val = unsafe { BoxedValue::from_raw(val).into_value() };

    match val {
        Value::Bool(b) => rt_ok(BoxedValue::bool(b)),
        _ => rt_ok(BoxedValue::bool(true)),
    }
}

/// Set a scoped variable for comprehension evaluation.
/// This is used to bind the iteration variable during comprehension loops.
/// This function CONSUMES the input BoxedValue.
#[no_mangle]
pub extern "C" fn rt_set_variable(
    ctx: *mut RuntimeContext,
    name_ptr: *const u8,
    name_len: usize,
    val: u64,
) {
    let ctx = unsafe { &mut *ctx };
    let name = unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(name_ptr, name_len)) };
    let value = unsafe { BoxedValue::from_raw(val).into_value() };

    // Store in the comprehension scope
    ctx.set_comprehension_var(name, value);
}

/// Free a BoxedValue.
/// Used for explicit cleanup of values that aren't consumed by other operations.
#[no_mangle]
pub extern "C" fn rt_free_value(_ctx: *mut RuntimeContext, val: u64) {
    // Consume and drop the value, freeing heap memory
    unsafe { BoxedValue::from_raw(val).into_value() };
}

/// Call a host function by name with pre-evaluated arguments.
///
/// This enables support for user-registered functions via `Context::add_function()`.
/// Arguments are passed as an array of BoxedValue raw representations.
/// This function CONSUMES the target and argument BoxedValues.
#[no_mangle]
pub extern "C" fn rt_call_function(
    ctx: *mut RuntimeContext,
    name_ptr: *const u8,
    name_len: usize,
    target: u64,
    has_target: u64,
    args_ptr: *const u64,
    args_len: usize,
) -> RuntimeResult {
    let ctx = unsafe { &*ctx };
    let name = unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(name_ptr, name_len)) };

    // Convert target if present, consuming the BoxedValue
    let target_value = if has_target != 0 {
        Some(unsafe { BoxedValue::from_raw(target).into_value() })
    } else {
        None
    };

    // Convert arguments array to Vec<Value>, consuming the BoxedValues
    let args: Vec<Value> = if args_len > 0 && !args_ptr.is_null() {
        let args_slice = unsafe { std::slice::from_raw_parts(args_ptr, args_len) };
        args_slice
            .iter()
            .map(|&raw| unsafe { BoxedValue::from_raw(raw).into_value() })
            .collect()
    } else {
        Vec::new()
    };

    // Call the function through RuntimeContext
    match ctx.call_function(name, target_value, args) {
        Ok(result) => rt_ok(BoxedValue::from_value(result)),
        Err(e) => {
            ctx.set_error(e);
            rt_err()
        }
    }
}

// ============================================================================
// Fast Slot Functions for Comprehension Variables
// These avoid HashMap lookups and string comparisons by using integer indices
// ============================================================================

/// Set a fast slot value (for comprehension loop variables).
/// This is much faster than rt_set_variable as it avoids HashMap operations.
#[no_mangle]
pub extern "C" fn rt_set_slot(ctx: *mut RuntimeContext, slot: u64, value: u64) {
    let ctx = unsafe { &*ctx };
    ctx.set_fast_slot(slot as usize, value);
}

/// Get a fast slot value (returns raw u64, no cloning).
/// This is much faster than rt_get_variable as it avoids HashMap operations.
/// WARNING: The returned value shares ownership with the slot. Caller must not consume it
/// without first cloning.
#[no_mangle]
pub extern "C" fn rt_get_slot(ctx: *mut RuntimeContext, slot: u64) -> u64 {
    let ctx = unsafe { &*ctx };
    ctx.get_fast_slot(slot as usize)
}

/// Get a fast slot value and return a clone.
/// This is slightly slower than rt_get_slot but safe for operations that consume the value.
#[no_mangle]
pub extern "C" fn rt_get_slot_cloned(ctx: *mut RuntimeContext, slot: u64) -> u64 {
    let ctx = unsafe { &*ctx };
    let raw = ctx.get_fast_slot(slot as usize);
    let boxed = unsafe { BoxedValue::from_raw(raw) };
    // Clone the value and return the clone's raw representation
    // For inline values (small int, bool, null), this is a no-op
    // For heap values, this clones the underlying Value
    let cloned_value = boxed.to_value();
    BoxedValue::from_value(cloned_value).as_raw()
}

#[cfg(test)]
mod tests {
    use super::*;
    use cel::Context;

    fn make_ctx() -> RuntimeContext<'static> {
        // Leak a context for testing purposes
        let ctx = Box::leak(Box::new(Context::default()));
        RuntimeContext::new(ctx)
    }

    #[test]
    fn test_add_ints() {
        let mut ctx = make_ctx();
        let left = BoxedValue::from_value(Value::Int(10)).as_raw();
        let right = BoxedValue::from_value(Value::Int(20)).as_raw();

        let result = rt_add(&mut ctx, left, right);
        assert_eq!(result.error, 0);

        let val = unsafe { BoxedValue::from_raw(result.value) }.to_value();
        assert_eq!(val, Value::Int(30));
    }

    #[test]
    fn test_mul_ints() {
        let mut ctx = make_ctx();
        let left = BoxedValue::from_value(Value::Int(5)).as_raw();
        let right = BoxedValue::from_value(Value::Int(7)).as_raw();

        let result = rt_mul(&mut ctx, left, right);
        assert_eq!(result.error, 0);

        let val = unsafe { BoxedValue::from_raw(result.value) }.to_value();
        assert_eq!(val, Value::Int(35));
    }

    #[test]
    fn test_div_by_zero() {
        let mut ctx = make_ctx();
        let left = BoxedValue::from_value(Value::Int(10)).as_raw();
        let right = BoxedValue::from_value(Value::Int(0)).as_raw();

        let result = rt_div(&mut ctx, left, right);
        assert_eq!(result.error, 1);
        assert!(ctx.has_error());
    }

    #[test]
    fn test_comparison() {
        let mut ctx = make_ctx();
        let five = BoxedValue::from_value(Value::Int(5)).as_raw();
        let ten = BoxedValue::from_value(Value::Int(10)).as_raw();

        let result = rt_lt(&mut ctx, five, ten);
        assert_eq!(result.error, 0);
        let val = unsafe { BoxedValue::from_raw(result.value) }.to_value();
        assert_eq!(val, Value::Bool(true));

        let result = rt_gt(&mut ctx, five, ten);
        assert_eq!(result.error, 0);
        let val = unsafe { BoxedValue::from_raw(result.value) }.to_value();
        assert_eq!(val, Value::Bool(false));
    }

    #[test]
    fn test_not() {
        let mut ctx = make_ctx();
        let true_val = BoxedValue::bool(true).as_raw();
        let false_val = BoxedValue::bool(false).as_raw();

        let result = rt_not(&mut ctx, true_val);
        assert_eq!(result.error, 0);
        let val = unsafe { BoxedValue::from_raw(result.value) }.to_value();
        assert_eq!(val, Value::Bool(false));

        let result = rt_not(&mut ctx, false_val);
        assert_eq!(result.error, 0);
        let val = unsafe { BoxedValue::from_raw(result.value) }.to_value();
        assert_eq!(val, Value::Bool(true));
    }
}
