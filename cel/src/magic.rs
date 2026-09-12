use crate::common::types::{CelBool, CelBytes, CelDouble, CelInt, CelNull, CelString, CelUInt};
#[cfg(feature = "chrono")]
use crate::common::types::{CelDuration, CelTimestamp};
use crate::common::value::{CowVal, FromVal as DowncastFrom, Val};
use crate::macros::{impl_conversions, impl_handler};
use crate::objects::Opaque;
use crate::resolvers::AllArguments;
use crate::{ExecutionError, FunctionContext, Value};
use std::collections::BTreeMap;
use std::sync::Arc;

impl_conversions!(
    i64 => Value::Int as CelInt,
    u64 => Value::UInt as CelUInt,
    f64 => Value::Float as CelDouble,
    Arc<String> => Value::String as CelString,
    Arc<Vec<u8>> => Value::Bytes as CelBytes,
    bool => Value::Bool as CelBool,
);

#[cfg(feature = "chrono")]
impl_conversions!(
    chrono::Duration => Value::Duration as CelDuration,
    chrono::DateTime<chrono::FixedOffset> => Value::Timestamp as CelTimestamp,
);

impl From<i32> for Value {
    fn from(value: i32) -> Self {
        Value::Int(value as i64)
    }
}

impl From<u32> for Value {
    fn from(value: u32) -> Self {
        Value::UInt(value as u64)
    }
}

impl From<f32> for Value {
    fn from(value: f32) -> Self {
        Value::Float(value as f64)
    }
}

/// Describes a type that can be extracted directly from a `&dyn Val`, without
/// materializing an intermediate [`Value`]. This is commonly used to convert a
/// resolved argument into a primitive type, e.g. `CelInt -> i64`. This trait is
/// auto-implemented for many CEL-primitive types.
pub(crate) trait FromVal: Sized {
    fn from_val(value: &dyn Val) -> Result<Self, ExecutionError>;
}

fn downcast_or_unexpected<'a, 'v, T: DowncastFrom<'a, 'v>>(
    value: &'a (dyn Val + 'v),
    want: &str,
) -> Result<&'a T, ExecutionError> {
    value
        .downcast_ref::<T>()
        .ok_or_else(|| ExecutionError::UnexpectedType {
            got: format!("{value:?}"),
            want: want.to_string(),
        })
}

impl FromVal for Value {
    fn from_val(value: &dyn Val) -> Result<Self, ExecutionError> {
        value.try_into()
    }
}

impl FromVal for i64 {
    fn from_val(value: &dyn Val) -> Result<Self, ExecutionError> {
        Ok(*downcast_or_unexpected::<CelInt>(value, "i64")?.inner())
    }
}

impl FromVal for u64 {
    fn from_val(value: &dyn Val) -> Result<Self, ExecutionError> {
        Ok(*downcast_or_unexpected::<CelUInt>(value, "u64")?.inner())
    }
}

impl FromVal for f64 {
    fn from_val(value: &dyn Val) -> Result<Self, ExecutionError> {
        Ok(*downcast_or_unexpected::<CelDouble>(value, "f64")?.inner())
    }
}

impl FromVal for bool {
    fn from_val(value: &dyn Val) -> Result<Self, ExecutionError> {
        Ok(*downcast_or_unexpected::<CelBool>(value, "bool")?.inner())
    }
}

impl FromVal for Arc<String> {
    fn from_val(value: &dyn Val) -> Result<Self, ExecutionError> {
        Ok(Arc::new(
            downcast_or_unexpected::<CelString>(value, "Arc<String>")?
                .inner()
                .to_string(),
        ))
    }
}

impl FromVal for Arc<Vec<u8>> {
    fn from_val(value: &dyn Val) -> Result<Self, ExecutionError> {
        Ok(Arc::new(
            downcast_or_unexpected::<CelBytes>(value, "Arc<Vec<u8>>")?
                .inner()
                .to_vec(),
        ))
    }
}

#[cfg(feature = "chrono")]
impl FromVal for chrono::Duration {
    fn from_val(value: &dyn Val) -> Result<Self, ExecutionError> {
        Ok(*downcast_or_unexpected::<CelDuration>(value, "chrono::Duration")?.inner())
    }
}

#[cfg(feature = "chrono")]
impl FromVal for chrono::DateTime<chrono::FixedOffset> {
    fn from_val(value: &dyn Val) -> Result<Self, ExecutionError> {
        Ok(
            *downcast_or_unexpected::<CelTimestamp>(
                value,
                "chrono::DateTime<chrono::FixedOffset>",
            )?
            .inner(),
        )
    }
}

impl FromVal for Arc<Vec<Value>> {
    fn from_val(value: &dyn Val) -> Result<Self, ExecutionError> {
        match Value::from_val(value)? {
            Value::List(list) => Ok(list),
            _ => Err(ExecutionError::UnexpectedType {
                got: format!("{value:?}"),
                want: "Arc<Vec<Value>>".to_string(),
            }),
        }
    }
}

impl FromVal for Arc<dyn Opaque> {
    fn from_val(value: &dyn Val) -> Result<Self, ExecutionError> {
        match Value::from_val(value)? {
            Value::Opaque(opaque) => Ok(opaque),
            _ => Err(ExecutionError::UnexpectedType {
                got: format!("{value:?}"),
                want: "Arc<dyn Opaque>".to_string(),
            }),
        }
    }
}

impl<T: FromVal> FromVal for Option<T> {
    fn from_val(value: &dyn Val) -> Result<Self, ExecutionError> {
        if value.downcast_ref::<CelNull>().is_some() {
            Ok(None)
        } else {
            T::from_val(value).map(Some)
        }
    }
}

impl From<Arc<Vec<Value>>> for Value {
    fn from(value: Arc<Vec<Value>>) -> Self {
        Value::List(value)
    }
}

impl<'context, 'call> IntoResolveResult<'context, 'call> for Arc<Vec<Value>> {
    fn into_resolve_result(self) -> Result<CowVal<'context, 'call>, ExecutionError> {
        Value::List(self).into_resolve_result()
    }
}

impl<'context, 'call> IntoResolveResult<'context, 'call>
    for Result<Arc<Vec<Value>>, ExecutionError>
{
    fn into_resolve_result(self) -> Result<CowVal<'context, 'call>, ExecutionError> {
        self?.into_resolve_result()
    }
}

impl<'a, 'context, 'call> FromContext<'a, 'context, 'call> for Arc<Vec<Value>> {
    fn from_context(ctx: &'a mut FunctionContext<'context, 'call>) -> Result<Self, ExecutionError>
    where
        Self: Sized,
    {
        arg_val_from_context(ctx).and_then(|v| FromVal::from_val(v.as_ref()))
    }
}

impl From<Arc<dyn Opaque>> for Value {
    fn from(value: Arc<dyn Opaque>) -> Self {
        Value::Opaque(value)
    }
}

impl<'context, 'call> IntoResolveResult<'context, 'call> for Arc<dyn Opaque> {
    fn into_resolve_result(self) -> Result<CowVal<'context, 'call>, ExecutionError> {
        Value::Opaque(self).into_resolve_result()
    }
}

impl<'context, 'call> IntoResolveResult<'context, 'call>
    for Result<Arc<dyn Opaque>, ExecutionError>
{
    fn into_resolve_result(self) -> Result<CowVal<'context, 'call>, ExecutionError> {
        self?.into_resolve_result()
    }
}

impl<'a, 'context, 'call> FromContext<'a, 'context, 'call> for Arc<dyn Opaque> {
    fn from_context(ctx: &'a mut FunctionContext<'context, 'call>) -> Result<Self, ExecutionError>
    where
        Self: Sized,
    {
        arg_val_from_context(ctx).and_then(|v| FromVal::from_val(v.as_ref()))
    }
}

/// A trait for types that can be converted into the [`CowVal`] returned by a registered
/// function. Every function that can be registered to the CEL context must return a value that
/// implements this trait.
///
/// Most implementations (e.g. the CEL-primitive types, [`Value`] itself) produce an owned
/// [`Val`], since they have no connection to the calling [`FunctionContext`]'s data. A function
/// that wants to avoid cloning - for example one that returns one of its arguments, or `this`,
/// unchanged - can instead return a [`CowVal`] borrowed from the [`FunctionContext`] directly (see
/// [`FunctionContext::this`] and [`FunctionContext::args`]). `'context` is that borrow, and
/// `'call` bounds the data the borrowed values may themselves borrow.
pub trait IntoResolveResult<'context, 'call> {
    fn into_resolve_result(self) -> Result<CowVal<'context, 'call>, ExecutionError>;
}

impl<'context, 'call> IntoResolveResult<'context, 'call> for String {
    fn into_resolve_result(self) -> Result<CowVal<'context, 'call>, ExecutionError> {
        Ok(CowVal::owned(CelString::from(self)))
    }
}

impl<'context, 'call> IntoResolveResult<'context, 'call> for Value {
    fn into_resolve_result(self) -> Result<CowVal<'context, 'call>, ExecutionError> {
        Ok(CowVal::Owned(self.try_into()?))
    }
}

impl<'context, 'call> IntoResolveResult<'context, 'call> for Result<Value, ExecutionError> {
    fn into_resolve_result(self) -> Result<CowVal<'context, 'call>, ExecutionError> {
        self?.into_resolve_result()
    }
}

impl<'context, 'call> IntoResolveResult<'context, 'call> for CowVal<'context, 'call> {
    fn into_resolve_result(self) -> Result<CowVal<'context, 'call>, ExecutionError> {
        Ok(self)
    }
}

impl<'context, 'call> IntoResolveResult<'context, 'call>
    for Result<CowVal<'context, 'call>, ExecutionError>
{
    fn into_resolve_result(self) -> Result<CowVal<'context, 'call>, ExecutionError> {
        self
    }
}

/// Describes any type that can be converted from a [`FunctionContext`] into
/// itself, for example CEL primitives implement this trait to allow them to
/// be used as arguments to functions. This trait is core to the 'magic function
/// parameter' system. Every argument to a function that can be registered to
/// the CEL context must implement this type.
pub(crate) trait FromContext<'a, 'context, 'call> {
    fn from_context(ctx: &'a mut FunctionContext<'context, 'call>) -> Result<Self, ExecutionError>
    where
        Self: Sized;
}

/// A function argument abstraction enabling dynamic method invocation on a
/// target instance or on the first argument if the function is not called
/// as a method.
///
/// This is similar to how methods can be called as functions using the
/// [fully-qualified syntax](https://doc.rust-lang.org/book/ch19-03-advanced-traits.html#fully-qualified-syntax-for-disambiguation-calling-methods-with-the-same-name).
///
/// # Using `This`
/// ```
/// # use std::sync::Arc;
/// # use cel::{Program, Context};
/// use cel::extractors::This;
/// # let mut context = Context::default();
/// # context.add_function("startsWith", starts_with);
///
/// /// Notice how `This` refers to the target value when called as a method,
/// /// but the first argument when called as a function.
/// let program1 = "'foobar'.startsWith('foo') == true";
/// let program2 = "startsWith('foobar', 'foo') == true";
/// # let program1 = Program::compile(program1).unwrap();
/// # let program2 = Program::compile(program2).unwrap();
/// # let value = program1.execute(&context).unwrap();
/// # assert_eq!(value, true.into());
/// # let value = program2.execute(&context).unwrap();
/// # assert_eq!(value, true.into());
///
/// fn starts_with(This(this): This<Arc<String>>, prefix: Arc<String>) -> bool {
///     this.starts_with(prefix.as_str())
/// }
/// ```
///
/// # Type of `This`
/// This also accepts a type `T` which determines the specific type
/// that's extracted. Any type that supports [`FromVal`] can be used.
/// In the previous example, the method `startsWith` is only ever called
/// on a string, so we can use `This<Rc<String>>` to extract the string
/// automatically prior to our method actually being called.
///
/// In some cases, you may want access to the raw [`Value`] instead, for
/// example, the `contains` method works for several different types. In these
/// cases, you can use `This<Value>` to extract the raw value.
///
/// ```skip
/// pub fn contains(This(this): This<Value>, arg: Value) -> Result<Value> {
///     Ok(match this {
///         Value::List(v) => v.contains(&arg),
///         ...
///     }
/// }
/// ```
pub struct This<T>(pub T);

impl<'a, 'context, 'call, T> FromContext<'a, 'context, 'call> for This<T>
where
    T: FromVal,
{
    fn from_context(ctx: &'a mut FunctionContext<'context, 'call>) -> Result<Self, ExecutionError>
    where
        Self: Sized,
    {
        if let Some(ref this) = ctx.this {
            Ok(This(T::from_val(this.as_ref())?))
        } else {
            let arg = arg_val_from_context(ctx)
                .map_err(|_| ExecutionError::missing_argument_or_target())?;
            Ok(This(T::from_val(arg.as_ref())?))
        }
    }
}

/// Identifier is an argument extractor that attempts to extract an identifier
/// from an argument's expression.
///
/// It fails if the argument is not available, or if the argument cannot be
/// converted into an expression.
///
/// # Examples
/// Identifiers are useful for functions like `.map` or `.filter` where one
/// of the arguments is the declaration of a variable. In this case, as noted
/// below, the x is an identifier, and we want to be able to parse it
/// automatically.
///
/// ```javascript
/// //        Identifier
/// //            ↓
/// [1, 2, 3].map(x, x * 2) == [2, 4, 6]
/// ```
///
/// The function signature for the Rust implementation of `map` looks like this
///
/// ```skip
/// pub fn map(
///     ftx: &FunctionContext,
///     This(this): This<Value>, // <- [1, 2, 3]
///     ident: Identifier,       // <- x
///     expr: Expression,        // <- x * 2
/// ) -> Result<Value>;
/// ```
#[derive(Clone)]
pub struct Identifier(pub Arc<String>);

impl From<&Identifier> for String {
    fn from(value: &Identifier) -> Self {
        value.0.to_string()
    }
}

impl From<Identifier> for String {
    fn from(value: Identifier) -> Self {
        value.0.as_ref().clone()
    }
}

/// An argument extractor that extracts all the arguments passed to a function, resolves their
/// expressions and returns a vector of [`Value`].
///
/// This is useful for functions that accept a variable number of arguments rather than known
/// arguments and types (for example a `sum` function).
///
/// # Example
/// ```javascript
/// sum(1, 2.0, uint(3)) == 5.0
/// ```
///
/// ```rust
/// # use cel::{Value};
/// use cel::extractors::Arguments;
/// pub fn sum(Arguments(args): Arguments) -> Value {
///     args.iter().fold(0.0, |acc, val| match val {
///         Value::Int(x) => *x as f64 + acc,
///         Value::UInt(x) => *x as f64 + acc,
///         Value::Float(x) => *x + acc,
///         _ => acc,
///     }).into()
/// }
/// ```
#[derive(Clone)]
pub struct Arguments(pub Arc<Vec<Value>>);

impl<'a> FromContext<'a, '_, '_> for Arguments {
    fn from_context(ctx: &'a mut FunctionContext) -> Result<Self, ExecutionError>
    where
        Self: Sized,
    {
        match ctx.resolve(AllArguments)? {
            Value::List(list) => Ok(Arguments(list.clone())),
            _ => todo!(),
        }
    }
}

impl<'a, 'context, 'call> FromContext<'a, 'context, 'call> for Value {
    fn from_context(ctx: &'a mut FunctionContext<'context, 'call>) -> Result<Self, ExecutionError>
    where
        Self: Sized,
    {
        arg_val_from_context(ctx).and_then(|v| FromVal::from_val(v.as_ref()))
    }
}

/// Returns the next argument specified by the context's `arg_idx` field, without
/// resolving it into a [`Value`] - the caller extracts whatever concrete type it
/// needs directly from the returned `Val` via [`FromVal`]. Calling this multiple
/// times will increment the `arg_idx` which will return subsequent arguments
/// every time.
pub(crate) fn arg_val_from_context<'context, 'call>(
    ctx: &mut FunctionContext<'context, 'call>,
) -> Result<CowVal<'context, 'call>, ExecutionError> {
    let idx = ctx.arg_idx;
    ctx.arg_idx += 1;
    ctx.args
        .get(idx)
        .cloned()
        .ok_or_else(|| ExecutionError::invalid_argument_count(idx + 1, ctx.args.len()))
}

pub struct WithFunctionContext;

impl_handler!();
impl_handler!(C1);
impl_handler!(C1, C2);
impl_handler!(C1, C2, C3);
impl_handler!(C1, C2, C3, C4);
impl_handler!(C1, C2, C3, C4, C5);
impl_handler!(C1, C2, C3, C4, C5, C6);
impl_handler!(C1, C2, C3, C4, C5, C6, C7);
impl_handler!(C1, C2, C3, C4, C5, C6, C7, C8);
impl_handler!(C1, C2, C3, C4, C5, C6, C7, C8, C9);

// Heavily inspired by https://users.rust-lang.org/t/common-data-type-for-functions-with-different-parameters-e-g-axum-route-handlers/90207/6
// and https://play.rust-lang.org/?version=stable&mode=debug&edition=2021&gist=c6744c27c2358ec1d1196033a0ec11e4

#[derive(Default)]
pub struct FunctionRegistry {
    functions: BTreeMap<String, Function>,
}

impl FunctionRegistry {
    pub(crate) fn add<F, T>(&mut self, name: &str, function: F)
    where
        F: IntoFunction<T> + 'static + Send + Sync,
        T: 'static,
    {
        self.functions
            .insert(name.to_string(), function.into_function());
    }

    #[allow(dead_code)]
    pub(crate) fn get(&self, name: &str) -> Option<&Function> {
        self.functions.get(name)
    }
}

pub type Function = Box<
    dyn for<'context, 'call> Fn(
            &mut FunctionContext<'context, 'call>,
        ) -> Result<CowVal<'context, 'call>, ExecutionError>
        + Send
        + Sync,
>;

pub trait IntoFunction<T> {
    fn into_function(self) -> Function;
}

impl IntoFunction<Function> for Function {
    fn into_function(self) -> Function {
        self
    }
}
