use crate::context::Context;
use crate::duration::{format_duration, parse_duration};
use crate::magic::{Arguments, Identifier, This};
use crate::objects::{Value, ValueType};
use crate::resolvers::{Argument, Resolver};
use crate::ExecutionError;
use cel_parser::Expression;
use chrono::{DateTime, Duration, FixedOffset};
use regex::Regex;
use std::cmp::Ordering;
use std::convert::TryInto;
use std::sync::Arc;

type Result<T> = std::result::Result<T, ExecutionError>;

/// `FunctionContext` is a context object passed to functions when they are called.
/// It contains references to the target object (if the function is called as
/// a method), the program context ([`Context`]) which gives functions access
/// to variables, and the arguments to the function call.
#[derive(Clone)]
pub struct FunctionContext<'context> {
    pub name: Arc<String>,
    pub this: Option<Value>,
    pub ptx: &'context Context<'context>,
    pub args: Vec<Expression>,
    pub arg_idx: usize,
}

impl<'context> FunctionContext<'context> {
    pub fn new(
        name: Arc<String>,
        this: Option<Value>,
        ptx: &'context Context<'context>,
        args: Vec<Expression>,
    ) -> Self {
        Self {
            name,
            this,
            ptx,
            args,
            arg_idx: 0,
        }
    }

    /// Resolves the given expression using the program's [`Context`].
    pub fn resolve<R>(&self, resolver: R) -> Result<Value>
    where
        R: Resolver,
    {
        resolver.resolve(self)
    }

    /// Returns an execution error for the currently execution function.
    pub fn error<M: ToString>(&self, message: M) -> ExecutionError {
        ExecutionError::function_error(self.name.as_str(), message)
    }
}

/// Calculates the size of either the target, or the provided args depending on how
/// the function is called. If called as a method, the target will be used. If called
/// as a function, the first argument will be used.
///
/// The following [`Value`] variants are supported:
/// * [`Value::List`]
/// * [`Value::Map`]
/// * [`Value::String`]
/// * [`Value::Bytes`]
///
/// # Examples
/// ```skip
/// size([1, 2, 3]) == 3
/// ```
/// ```skip
/// 'foobar'.size() == 6
/// ```
pub fn size(ftx: &FunctionContext, This(this): This<Value>) -> Result<i64> {
    let size = match this {
        Value::List(l) => l.len(),
        Value::Map(m) => m.map.len(),
        Value::String(s) => s.len(),
        Value::Bytes(b) => b.len(),
        value => return Err(ftx.error(format!("cannot determine the size of {:?}", value))),
    };
    Ok(size as i64)
}

/// Returns true if the target contains the provided argument. The actual behavior
/// depends mainly on the type of the target.
///
/// The following [`Value`] variants are supported:
/// * [`Value::List`] - Returns true if the list contains the provided value.
/// * [`Value::Map`] - Returns true if the map contains the provided key.
/// * [`Value::String`] - Returns true if the string contains the provided substring.
/// * [`Value::Bytes`] - Returns true if the bytes contain the provided byte.
///
/// # Example
///
/// ## List
/// ```cel
/// [1, 2, 3].contains(1) == true
/// ```
///
/// ## Map
/// ```cel
/// {"a": 1, "b": 2, "c": 3}.contains("a") == true
/// ```
///
/// ## String
/// ```cel
/// "abc".contains("b") == true
/// ```
///
/// ## Bytes
/// ```cel
/// b"abc".contains(b"c") == true
/// ```
pub fn contains(This(this): This<Value>, arg: Value) -> Result<Value> {
    Ok(match this {
        Value::List(v) => v.contains(&arg),
        Value::Map(v) => v
            .map
            .contains_key(&arg.try_into().map_err(ExecutionError::UnsupportedKeyType)?),
        Value::String(s) => {
            if let Value::String(arg) = arg {
                s.contains(arg.as_str())
            } else {
                false
            }
        }
        Value::Bytes(b) => {
            if let Value::Bytes(arg) = arg {
                let s = arg.as_slice();
                b.windows(arg.len()).any(|w| w == s)
            } else {
                false
            }
        }
        _ => false,
    }
    .into())
}

macro_rules! return_external_conversion {
    ($v: ident as $ty: ident) => {{
        let $v = unsafe { $v.as_value_unchecked() };
        if let Some(it) = $v.to_value(ValueType::$ty) {
            if matches!(it, Value::$ty(_)) {
                return Ok(it);
            }
        }
    }};
}

/// Converts provided value into a [`string`][Value::String].
///
/// # Supported Conversions
///
/// - [`string`][Value::String] - will be returned as is.
/// - [`timestamp`][Value::Timestamp] - will be formatted into RFC3339 format.
/// - [`duration`][Value::Duration] - will be formatted to string representation
///   (e.g. "72h3m0.5s").
/// - [`int`][Value::Int] - will be formatted into a string.
/// - [`uint`][Value::UInt] - will be formatted into a string.
/// - [`float`][Value::Float] - will be formatted into a string.
/// - [`bytes`][Value::Bytes] - will be converted to string using
///   [`from_utf8_lossy`][String::from_utf8_lossy].
/// - [`external`][Value::External] - will be converted to
///   [`string`][Value::String] if it's not opaque (i.e. implements
///   [`AsValue`][crate::external::AsValue]), and value returned by
///   [`to_value(ValueType::String)`][crate::external::AsValue::to_value] is
///   [`string`][Value::String]. [`AsValue`][crate::external::AsValue]) and the
///   returned value is a [`string`][Value::String].
pub fn string(ftx: &FunctionContext, This(this): This<Value>) -> Result<Value> {
    match &this {
        Value::String(v) => return Ok(Value::String(v.clone())),
        Value::Timestamp(t) => return Ok(Value::String(t.to_rfc3339().into())),
        Value::Duration(v) => return Ok(Value::String(format_duration(&v).into())),
        Value::Int(v) => return Ok(Value::String(v.to_string().into())),
        Value::UInt(v) => return Ok(Value::String(v.to_string().into())),
        Value::Float(v) => return Ok(Value::String(v.to_string().into())),
        Value::Bytes(v) => {
            return Ok(Value::String(Arc::new(
                String::from_utf8_lossy(v.as_slice()).into(),
            )))
        }
        Value::External(v) if !v.is_opaque() => {
            return_external_conversion!(v as String);
        }
        _ => {}
    }

    Err(ftx.error(format!("cannot convert {:?} to string", this)))
}

/// Converts provided value into [`bytes`][Value::Bytes].
///
/// # Supported Conversions
///
/// - [`bytes`][Value::Bytes] - will be returned as is.
/// - [`string`][Value::String] - will return UTF8 bytes.
/// - [`external`][Value::External] - will be converted to
///   [`bytes`][Value::Bytes] if it's not opaque (i.e. implements
///   [`AsValue`][crate::external::AsValue]), and value returned by
///   [`to_value(ValueType::Bytes)`][crate::external::AsValue::to_value] is
///   [`bytes`][Value::Bytes].
pub fn bytes(Arguments(args): Arguments) -> Result<Value> {
    if args.len() > 1 {
        return Err(ExecutionError::InvalidArgumentCount {
            expected: 1,
            actual: args.len(),
        });
    }
    let value = match args.get(0) {
        Some(it) => it,
        None => return Err(ExecutionError::MissingArgumentOrTarget),
    };
    match value {
        Value::Bytes(v) => return Ok(Value::Bytes(v.clone())),
        Value::String(v) => return Ok(Value::Bytes(v.as_bytes().to_vec().into())),
        Value::External(v) if !v.is_opaque() => {
            return_external_conversion!(v as Bytes);
        }
        _ => {}
    }

    Err(ExecutionError::UnexpectedType {
        got: value.type_of().to_string(),
        want: "`string` or `external` convertible to `bytes`".to_string(),
    })
}

/// Converts provided value into [`float`][Value::Float].
///
/// # Supported Conversions
///
/// - [`string`][Value::String] - will be parsed as `f64`.
/// - [`float`][Value::Float] - will be returned as is.
/// - [`int`][Value::Int] - will be cast into a float.
/// - [`uint`][Value::UInt] - will be cast into a float.
/// - [`external`][Value::External] - will be converted to
///   [`float`][Value::Float] if it's not opaque (i.e. implements
///   [`AsValue`][crate::external::AsValue]), and value returned by
///   [`to_value(ValueType::Float)`][crate::external::AsValue::to_value] is
///   a [`float`][Value::Float].
pub fn double(ftx: &FunctionContext, This(this): This<Value>) -> Result<Value> {
    match &this {
        Value::Float(v) => return Ok(Value::Float(*v)),
        Value::String(v) => {
            return v
                .parse::<f64>()
                .map(Value::Float)
                .map_err(|e| ftx.error(format!("string parse error: {e}")))
        }
        Value::Int(v) => return Ok(Value::Float(*v as f64)),
        Value::UInt(v) => return Ok(Value::Float(*v as f64)),
        Value::External(v) if !v.is_opaque() => {
            return_external_conversion!(v as Float);
        }
        _ => {}
    }

    Err(ftx.error(format!("cannot convert {:?} to float", this)))
}

/// Converts provided value into [`uint`][Value::UInt].
///
/// # Supported Conversions
///
/// - [`string`][Value::String] - will be parsed as `u64`.
/// - [`float`][Value::Float] - will be cast if there's no overflow.
/// - [`int`][Value::Int] - will be cast if there's no overflow.
/// - [`uint`][Value::UInt] - will be returned as is.
/// - [`external`][Value::External] - will be converted to [`uint`][Value::UInt]
///   if it's not opaque (i.e. implements
///   [`AsValue`][crate::external::AsValue]), and value returned by
///   [`to_value(ValueType::Float)`][crate::external::AsValue::to_value] is
///   an [`uint`][Value::UInt].
pub fn uint(ftx: &FunctionContext, This(this): This<Value>) -> Result<Value> {
    match &this {
        Value::UInt(v) => return Ok(Value::UInt(*v)),
        Value::String(v) => {
            return v
                .parse::<u64>()
                .map(Value::UInt)
                .map_err(|e| ftx.error(format!("string parse error: {e}")))
        }
        Value::Float(v) => {
            if *v > u64::MAX as f64 || *v < u64::MIN as f64 {
                return Err(ftx.error("unsigned integer overflow"));
            }
            return Ok(Value::UInt(*v as u64));
        }
        Value::Int(v) => {
            return (*v)
                .try_into()
                .map(Value::UInt)
                .map_err(|_| ftx.error("unsigned integer overflow"))
        }
        Value::External(v) if !v.is_opaque() => {
            return_external_conversion!(v as UInt);
        }
        _ => {}
    }

    Err(ftx.error(format!("cannot convert {:?} to uint", this)))
}

/// Converts provided value into [`int`][Value::Int].
///
/// # Supported Conversions
///
/// - [`string`][Value::String] - will be parsed as `i64`.
/// - [`float`][Value::Float] - will be cast if there's no overflow.
/// - [`int`][Value::Int] - will be returned as is.
/// - [`uint`][Value::UInt] - will be cast if there's no overflow.
/// - [`external`][Value::External] - Will be converted to [`int`][Value::Int]
///   if it's not opaque (i.e. implements
///   [`AsValue`][crate::external::AsValue]), and value returned by
///   [`to_value(ValueType::Int)`][crate::external::AsValue::to_value] is
///   an [`int`][Value::Int].
pub fn int(ftx: &FunctionContext, This(this): This<Value>) -> Result<Value> {
    match &this {
        Value::Int(v) => return Ok(Value::Int(*v)),
        Value::String(v) => {
            return v
                .parse::<i64>()
                .map(Value::Int)
                .map_err(|e| ftx.error(format!("string parse error: {e}")))
        }
        Value::Float(v) => {
            if *v > i64::MAX as f64 || *v < i64::MIN as f64 {
                return Err(ftx.error("integer overflow"));
            }
            return Ok(Value::Int(*v as i64));
        }
        Value::UInt(v) => {
            return TryInto::<i64>::try_into(*v)
                .map(Value::Int)
                .map_err(|_| ftx.error("integer overflow"))
        }
        Value::External(v) if !v.is_opaque() => {
            return_external_conversion!(v as Int);
        }
        _ => {}
    }

    Err(ftx.error(format!("cannot convert {:?} to int", this)))
}

/// Returns true if a string starts with another string.
///
/// # Example
/// ```cel
/// "abc".startsWith("a") == true
/// ```
pub fn starts_with(This(this): This<Arc<String>>, prefix: Arc<String>) -> bool {
    this.starts_with(prefix.as_str())
}

/// Returns true if a string ends with another string.
///
/// # Example
/// ```cel
/// "abc".endsWith("c") == true
/// ```
pub fn ends_with(This(this): This<Arc<String>>, suffix: Arc<String>) -> bool {
    this.ends_with(suffix.as_str())
}

/// Returns true if a string matches the regular expression.
///
/// # Example
/// ```cel
/// "abc".matches("^[a-z]*$") == true
/// ```
pub fn matches(
    ftx: &FunctionContext,
    This(this): This<Arc<String>>,
    regex: Arc<String>,
) -> Result<bool> {
    match Regex::new(&regex) {
        Ok(re) => Ok(re.is_match(&this)),
        Err(err) => Err(ftx.error(format!("'{regex}' not a valid regex:\n{err}"))),
    }
}

/// Returns true if the provided argument can be resolved. This function is
/// useful for checking if a property exists on a type before attempting to
/// resolve it. Resolving a property that does not exist will result in a
/// [`ExecutionError::NoSuchKey`] error.
///
/// Operates similar to the `has` macro describe in the Go CEL implementation
/// spec: <https://github.com/google/cel-spec/blob/master/doc/langdef.md#macros>.
///
/// # Examples
/// ```cel
/// has(foo.bar.baz)
/// ```
pub fn has(ftx: &FunctionContext) -> Result<Value> {
    // We determine if a type has a property by attempting to resolve it.
    // If we get a NoSuchKey error, then we know the property does not exist
    match ftx.resolve(Argument(0)) {
        Ok(_) => Value::Bool(true),
        Err(err) => match err {
            ExecutionError::NoSuchKey(_) => Value::Bool(false),
            _ => return Err(err),
        },
    }
    .into()
}

/// Maps the provided list to a new list by applying an expression to each
/// input item. This function is intended to be used like the CEL-go `map`
/// macro: <https://github.com/google/cel-spec/blob/master/doc/langdef.md#macros>
///
/// # Examples
/// ```cel
/// [1, 2, 3].map(x, x * 2) == [2, 4, 6]
/// ```
pub fn map(
    ftx: &FunctionContext,
    This(this): This<Value>,
    ident: Identifier,
    expr: Expression,
) -> Result<Value> {
    match this {
        Value::List(items) => {
            let mut values = Vec::with_capacity(items.len());
            let mut ptx = ftx.ptx.new_inner_scope();
            for item in items.iter() {
                ptx.add_variable_from_value(ident.clone(), item.clone());
                let value = ptx.resolve(&expr)?;
                values.push(value);
            }
            Value::List(Arc::new(values))
        }
        _ => return Err(this.error_expected_type(ValueType::List)),
    }
    .into()
}

/// Filters the provided list by applying an expression to each input item
/// and including the input item in the resulting list, only if the expression
/// returned true.
///
/// This function is intended to be used like the CEL-go `filter` macro:
/// <https://github.com/google/cel-spec/blob/master/doc/langdef.md#macros>
///
/// # Example
/// ```cel
/// [1, 2, 3].filter(x, x > 1) == [2, 3]
/// ```
pub fn filter(
    ftx: &FunctionContext,
    This(this): This<Value>,
    ident: Identifier,
    expr: Expression,
) -> Result<Value> {
    match this {
        Value::List(items) => {
            let mut values = Vec::with_capacity(items.len());
            let mut ptx = ftx.ptx.new_inner_scope();
            for item in items.iter() {
                ptx.add_variable_from_value(ident.clone(), item.clone());
                if let Value::Bool(true) = ptx.resolve(&expr)? {
                    values.push(item.clone());
                }
            }
            Value::List(Arc::new(values))
        }
        _ => return Err(this.error_expected_type(ValueType::List)),
    }
    .into()
}

/// Returns a boolean value indicating whether every value in the provided
/// list or map met the predicate defined by the provided expression. If
/// called on a map, the predicate is applied to the map keys.
///
/// This function is intended to be used like the CEL-go `all` macro:
/// <https://github.com/google/cel-spec/blob/master/doc/langdef.md#macros>
///
/// # Example
/// ```cel
/// [1, 2, 3].all(x, x > 0) == true
/// [{1:true, 2:true, 3:false}].all(x, x > 0) == true
/// ```
pub fn all(
    ftx: &FunctionContext,
    This(this): This<Value>,
    ident: Identifier,
    expr: Expression,
) -> Result<bool> {
    return match this {
        Value::List(items) => {
            let mut ptx = ftx.ptx.new_inner_scope();
            for item in items.iter() {
                ptx.add_variable_from_value(&ident, item);
                if let Value::Bool(false) = ptx.resolve(&expr)? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        Value::Map(value) => {
            let mut ptx = ftx.ptx.new_inner_scope();
            for key in value.map.keys() {
                ptx.add_variable_from_value(&ident, key);
                if let Value::Bool(false) = ptx.resolve(&expr)? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        _ => return Err(this.error_expected_type(ValueType::List)),
    };
}

/// Returns a boolean value indicating whether a or more values in the provided
/// list or map meet the predicate defined by the provided expression. If
/// called on a map, the predicate is applied to the map keys.
///
/// This function is intended to be used like the CEL-go `exists` macro:
/// <https://github.com/google/cel-spec/blob/master/doc/langdef.md#macros>
///
/// # Example
/// ```cel
/// [1, 2, 3].exists(x, x > 0) == true
/// [{1:true, 2:true, 3:false}].exists(x, x > 0) == true
/// ```
pub fn exists(
    ftx: &FunctionContext,
    This(this): This<Value>,
    ident: Identifier,
    expr: Expression,
) -> Result<bool> {
    match this {
        Value::List(items) => {
            let mut ptx = ftx.ptx.new_inner_scope();
            for item in items.iter() {
                ptx.add_variable_from_value(&ident, item);
                if let Value::Bool(true) = ptx.resolve(&expr)? {
                    return Ok(true);
                }
            }
            Ok(false)
        }
        Value::Map(value) => {
            let mut ptx = ftx.ptx.new_inner_scope();
            for key in value.map.keys() {
                ptx.add_variable_from_value(&ident, key);
                if let Value::Bool(true) = ptx.resolve(&expr)? {
                    return Ok(true);
                }
            }
            Ok(false)
        }
        _ => Err(this.error_expected_type(ValueType::List)),
    }
}

/// Returns a boolean value indicating whether only one value in the provided
/// list or map meets the predicate defined by the provided expression. If
/// called on a map, the predicate is applied to the map keys.
///
/// This function is intended to be used like the CEL-go `exists` macro:
/// <https://github.com/google/cel-spec/blob/master/doc/langdef.md#macros>
///
/// # Example
/// ```cel
/// [1, 2, 3].exists_one(x, x > 0) == false
/// [1, 2, 3].exists_one(x, x == 1) == true
/// [{1:true, 2:true, 3:false}].exists_one(x, x > 0) == false
/// ```
pub fn exists_one(
    ftx: &FunctionContext,
    This(this): This<Value>,
    ident: Identifier,
    expr: Expression,
) -> Result<bool> {
    match this {
        Value::List(items) => {
            let mut ptx = ftx.ptx.new_inner_scope();
            let mut exists = false;
            for item in items.iter() {
                ptx.add_variable_from_value(&ident, item);
                if let Value::Bool(true) = ptx.resolve(&expr)? {
                    if exists {
                        return Ok(false);
                    }
                    exists = true;
                }
            }
            Ok(exists)
        }
        Value::Map(value) => {
            let mut ptx = ftx.ptx.new_inner_scope();
            let mut exists = false;
            for key in value.map.keys() {
                ptx.add_variable_from_value(&ident, key);
                if let Value::Bool(true) = ptx.resolve(&expr)? {
                    if exists {
                        return Ok(false);
                    }
                    exists = true;
                }
            }
            Ok(exists)
        }
        _ => Err(this.error_expected_type(ValueType::List)),
    }
}

/// Duration converts the provided argument into a
/// [`duration`][Value::Duration].
///
/// # Supported Conversions
///
/// - [`duration`][Value::Duration] - will be returned as is.
/// - [`string`][Value::String] - will be parsed with [`parse_duration`].
/// - [`external`][Value::External] - will be converted to bytes if it's not
///   opaque (i.e. implements [`AsValue`][crate::external::AsValue]), and value
///   returned by
///   [`to_value(ValueType::Duration)`][crate::external::AsValue::to_value] is a
///   [`duration`][Value::Duration].
///
/// # Parsing Result Examples
///
/// - `1h` parses as 1 hour
/// - `1.5h` parses as 1 hour and 30 minutes
/// - `1h30m` parses as 1 hour and 30 minutes
/// - `1h30m1s` parses as 1 hour, 30 minutes, and 1 second
/// - `1ms` parses as 1 millisecond
/// - `1.5ms` parses as 1 millisecond and 500 microseconds
/// - `1ns` parses as 1 nanosecond
/// - `1.5ns` parses as 1 nanosecond (sub-nanosecond durations not supported)
pub fn duration(Arguments(args): Arguments) -> Result<Value> {
    if args.len() > 1 {
        return Err(ExecutionError::InvalidArgumentCount {
            expected: 1,
            actual: args.len(),
        });
    }
    let value = match args.get(0) {
        Some(it) => it,
        None => return Err(ExecutionError::MissingArgumentOrTarget),
    };
    match value {
        Value::Duration(value) => return Ok(Value::Duration(*value)),
        Value::String(value) => return Ok(Value::Duration(_duration(value.as_str())?)),
        Value::External(external) if !external.is_opaque() => {
            return_external_conversion!(external as Duration);
        }
        _ => {}
    }

    Err(ExecutionError::UnexpectedType {
        got: value.type_of().to_string(),
        want: "`string` or `external` convertible to `duration`".to_string(),
    })
}

/// Timestamp converts the provided argument into a
/// [`timestamp`][Value::Timestamp].
///
/// # Supported Conversions
///
/// - [`timestamp`][Value::Timestamp] - will be returned as is.
/// - [`string`][Value::String] - will be parsed as RFC3339 formatted timestamp.
/// - [`external`][Value::External] - will be converted to
///   [`timestamp`][Value::Timestamp] if it's not opaque (i.e. implements
///   [`AsValue`][crate::external::AsValue]), and value returned by
///   [`to_value(ValueType::Timestamp)`][crate::external::AsValue::to_value] is
///   a [`timestamp`][Value::Timestamp].
pub fn timestamp(Arguments(args): Arguments) -> Result<Value> {
    if args.len() > 1 {
        return Err(ExecutionError::InvalidArgumentCount {
            expected: 1,
            actual: args.len(),
        });
    }
    let value = match args.get(0) {
        Some(it) => it,
        None => return Err(ExecutionError::MissingArgumentOrTarget),
    };
    match value {
        Value::Timestamp(value) => return Ok(Value::Timestamp(*value)),
        Value::String(value) => {
            return Ok(Value::Timestamp(
                DateTime::parse_from_rfc3339(value.as_str()).map_err(|e| {
                    ExecutionError::function_error("timestamp", e.to_string().as_str())
                })?,
            ))
        }
        Value::External(external) if !external.is_opaque() => {
            return_external_conversion!(external as Timestamp);
        }
        _ => {}
    }

    Err(ExecutionError::UnexpectedType {
        got: value.type_of().to_string(),
        want: "`string` or `external` convertible to `timestamp`".to_string(),
    })
}

pub fn max(Arguments(args): Arguments) -> Result<Value> {
    // If items is a list of values, then operate on the list
    let items = if args.len() == 1 {
        match &args[0] {
            Value::List(values) => values,
            _ => return Ok(args[0].clone()),
        }
    } else {
        &args
    };

    items
        .iter()
        .skip(1)
        .try_fold(items.first().unwrap_or(&Value::Null), |acc, x| {
            match acc.partial_cmp(x) {
                Some(Ordering::Greater) => Ok(acc),
                Some(_) => Ok(x),
                None => Err(ExecutionError::ValuesNotComparable(acc.clone(), x.clone())),
            }
        })
        .cloned()
}

/// A wrapper around [`parse_duration`] that converts errors into [`ExecutionError`].
/// and only returns the duration, rather than returning the remaining input.
fn _duration(i: &str) -> Result<Duration> {
    let (_, duration) =
        parse_duration(i).map_err(|e| ExecutionError::function_error("duration", e.to_string()))?;
    Ok(duration)
}

fn _timestamp(i: &str) -> Result<DateTime<FixedOffset>> {
    DateTime::parse_from_rfc3339(i)
        .map_err(|e| ExecutionError::function_error("timestamp", e.to_string()))
}

#[cfg(test)]
mod tests {
    use crate::context::Context;
    use crate::testing::test_script;
    use crate::{Program, Value};
    use chrono::{DateTime, FixedOffset};
    use std::collections::HashMap;

    fn assert_script(input: &(&str, &str)) {
        assert_eq!(test_script(input.1, None), Ok(true.into()), "{}", input.0);
    }

    #[test]
    fn test_size() {
        [
            ("size of list", "size([1, 2, 3]) == 3"),
            ("size of map", "size({'a': 1, 'b': 2, 'c': 3}) == 3"),
            ("size of string", "size('foo') == 3"),
            ("size of bytes", "size(b'foo') == 3"),
            ("size as a list method", "[1, 2, 3].size() == 3"),
            ("size as a string method", "'foobar'.size() == 6"),
        ]
        .iter()
        .for_each(assert_script);
    }

    #[test]
    fn test_has() {
        let tests = vec![
            ("map has", "has(foo.bar) == true"),
            ("map has", "has(foo.bar) == true"),
            ("map not has", "has(foo.baz) == false"),
            ("map deep not has", "has(foo.baz.bar) == false"),
        ];

        for (name, script) in tests {
            let mut ctx = Context::default();
            ctx.add_variable_from_value("foo", HashMap::from([("bar", 1)]));
            assert_eq!(test_script(script, Some(ctx)), Ok(true.into()), "{}", name);
        }
    }

    #[test]
    fn test_map() {
        [
            ("map list", "[1, 2, 3].map(x, x * 2) == [2, 4, 6]"),
            ("map list 2", "[1, 2, 3].map(y, y + 1) == [2, 3, 4]"),
            (
                "nested map",
                "[[1, 2], [2, 3]].map(x, x.map(x, x * 2)) == [[2, 4], [4, 6]]",
            ),
        ]
        .iter()
        .for_each(assert_script);
    }

    #[test]
    fn test_filter() {
        [("filter list", "[1, 2, 3].filter(x, x > 2) == [3]")]
            .iter()
            .for_each(assert_script);
    }

    #[test]
    fn test_all() {
        [
            ("all list #1", "[0, 1, 2].all(x, x >= 0)"),
            ("all list #2", "[0, 1, 2].all(x, x > 0) == false"),
            ("all map", "{0: 0, 1:1, 2:2}.all(x, x >= 0) == true"),
        ]
        .iter()
        .for_each(assert_script);
    }

    #[test]
    fn test_exists() {
        [
            ("exist list #1", "[0, 1, 2].exists(x, x > 0)"),
            ("exist list #2", "[0, 1, 2].exists(x, x == 3) == false"),
            ("exist list #3", "[0, 1, 2, 2].exists(x, x == 2)"),
            ("exist map", "{0: 0, 1:1, 2:2}.exists(x, x > 0)"),
        ]
        .iter()
        .for_each(assert_script);
    }

    #[test]
    fn test_exists_one() {
        [
            ("exist list #1", "[0, 1, 2].exists_one(x, x > 0) == false"),
            ("exist list #2", "[0, 1, 2].exists_one(x, x == 0)"),
            ("exist map", "{0: 0, 1:1, 2:2}.exists_one(x, x == 2)"),
        ]
        .iter()
        .for_each(assert_script);
    }

    #[test]
    fn test_max() {
        [
            ("max single", "max(1) == 1"),
            ("max multiple", "max(1, 2, 3) == 3"),
            ("max negative", "max(-1, 0) == 0"),
            ("max float", "max(-1.0, 0.0) == 0.0"),
            ("max list", "max([1, 2, 3]) == 3"),
            ("max empty list", "max([]) == null"),
            ("max no args", "max() == null"),
        ]
        .iter()
        .for_each(assert_script);
    }

    #[test]
    fn test_duration() {
        [
            ("duration equal 1", "duration('1s') == duration('1000ms')"),
            ("duration equal 2", "duration('1m') == duration('60s')"),
            ("duration equal 3", "duration('1h') == duration('60m')"),
            ("duration comparison 1", "duration('1m') > duration('1s')"),
            ("duration comparison 2", "duration('1m') < duration('1h')"),
            (
                "duration subtraction",
                "duration('1h') - duration('1m') == duration('59m')",
            ),
            (
                "duration addition",
                "duration('1h') + duration('1m') == duration('1h1m')",
            ),
        ]
        .iter()
        .for_each(assert_script);
    }

    #[test]
    fn test_starts_with() {
        [
            ("starts with true", "'foobar'.startsWith('foo') == true"),
            ("starts with false", "'foobar'.startsWith('bar') == false"),
        ]
        .iter()
        .for_each(assert_script);
    }

    #[test]
    fn test_ends_with() {
        [
            ("ends with true", "'foobar'.endsWith('bar') == true"),
            ("ends with false", "'foobar'.endsWith('foo') == false"),
        ]
        .iter()
        .for_each(assert_script);
    }

    #[test]
    fn test_timestamp() {
        [(
                "comparison",
                "timestamp('2023-05-29T00:00:00Z') > timestamp('2023-05-28T00:00:00Z')",
            ),
            (
                "comparison",
                "timestamp('2023-05-29T00:00:00Z') < timestamp('2023-05-30T00:00:00Z')",
            ),
            (
                "subtracting duration",
                "timestamp('2023-05-29T00:00:00Z') - duration('24h') == timestamp('2023-05-28T00:00:00Z')",
            ),
            (
                "subtracting date",
                "timestamp('2023-05-29T00:00:00Z') - timestamp('2023-05-28T00:00:00Z') == duration('24h')",
            ),
            (
                "adding duration",
                "timestamp('2023-05-28T00:00:00Z') + duration('24h') == timestamp('2023-05-29T00:00:00Z')",
            ),
            (
                "timestamp string",
                "timestamp('2023-05-28T00:00:00Z').string() == '2023-05-28T00:00:00+00:00'",
            )]
        .iter()
        .for_each(assert_script);
    }

    #[test]
    fn test_timestamp_variable() {
        let mut context = Context::default();
        let ts: DateTime<FixedOffset> =
            DateTime::parse_from_rfc3339("2023-05-29T00:00:00Z").unwrap();
        context.add_variable("ts", Value::Timestamp(ts)).unwrap();

        let program = Program::compile("ts == timestamp('2023-05-29T00:00:00Z')").unwrap();
        let result = program.execute(&context).unwrap();
        assert_eq!(result, true.into());
    }

    #[test]
    fn test_string() {
        [
            ("duration", "duration('1h30m').string() == '1h30m0s'"),
            (
                "timestamp",
                "timestamp('2023-05-29T00:00:00Z').string() == '2023-05-29T00:00:00+00:00'",
            ),
            ("string", "'foo'.string() == 'foo'"),
            ("int", "10.string() == '10'"),
            ("float", "10.5.string() == '10.5'"),
            ("bytes", "b'foo'.string() == 'foo'"),
        ]
        .iter()
        .for_each(assert_script);
    }

    #[test]
    fn test_bytes() {
        [
            ("string", "bytes('abc') == b'abc'"),
            ("bytes", "bytes('abc') == b'\\x61b\\x63'"),
        ]
        .iter()
        .for_each(assert_script);
    }

    #[test]
    fn test_double() {
        [
            ("string", "'10'.double() == 10.0"),
            ("int", "10.double() == 10.0"),
            ("double", "10.0.double() == 10.0"),
        ]
        .iter()
        .for_each(assert_script);
    }

    #[test]
    fn test_uint() {
        [
            ("string", "'10'.uint() == 10.uint()"),
            ("double", "10.5.uint() == 10.uint()"),
        ]
        .iter()
        .for_each(assert_script);
    }

    #[test]
    fn test_int() {
        [
            ("string", "'10'.int() == 10"),
            ("int", "10.int() == 10"),
            ("uint", "10.uint().int() == 10"),
            ("double", "10.5.int() == 10"),
        ]
        .iter()
        .for_each(assert_script);
    }
}
