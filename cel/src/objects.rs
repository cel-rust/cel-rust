use crate::common::ast::{operators, EntryExpr, Expr};
use crate::common::value::CelVal;
use crate::context::{Context, SingleVarResolver, VariableResolver};
use crate::functions::FunctionContext;
use crate::{ExecutionError, Expression};
use bytes::Bytes;
#[cfg(feature = "chrono")]
use chrono::TimeZone;
use hashbrown::Equivalent;
use std::any::Any;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::convert::{Infallible, TryFrom, TryInto};
use std::fmt::{Debug, Display, Formatter};
use std::hash::Hash;
use std::marker::PhantomData;
use std::ops::Deref;
use std::ptr::NonNull;
#[cfg(feature = "chrono")]
use std::sync::LazyLock;
use std::sync::{Arc, OnceLock};
use std::{ops, slice};
use std::borrow::Cow;

/// Timestamp values are limited to the range of values which can be serialized as a string:
/// `["0001-01-01T00:00:00Z", "9999-12-31T23:59:59.999999999Z"]`. Since the max is a smaller
/// and the min is a larger timestamp than what is possible to represent with [`DateTime`],
/// we need to perform our own spec-compliant overflow checks.
///
/// https://github.com/google/cel-spec/blob/master/doc/langdef.md#overflow
#[cfg(feature = "chrono")]
static MAX_TIMESTAMP: LazyLock<chrono::DateTime<chrono::FixedOffset>> = LazyLock::new(|| {
    let naive = chrono::NaiveDate::from_ymd_opt(9999, 12, 31)
      .unwrap()
      .and_hms_nano_opt(23, 59, 59, 999_999_999)
      .unwrap();
    chrono::FixedOffset::east_opt(0)
      .unwrap()
      .from_utc_datetime(&naive)
});

#[cfg(feature = "chrono")]
static MIN_TIMESTAMP: LazyLock<chrono::DateTime<chrono::FixedOffset>> = LazyLock::new(|| {
    let naive = chrono::NaiveDate::from_ymd_opt(1, 1, 1)
      .unwrap()
      .and_hms_opt(0, 0, 0)
      .unwrap();
    chrono::FixedOffset::east_opt(0)
      .unwrap()
      .from_utc_datetime(&naive)
});

#[derive(Debug, PartialEq, Clone)]
pub struct Map {
    pub map: Arc<hashbrown::HashMap<Key, Value<'static>>>,
}

impl PartialOrd for Map {
    fn partial_cmp(&self, _: &Self) -> Option<Ordering> {
        None
    }
}

impl Map {
    pub(crate) fn contains_key<Q>(&self, key: &Q) -> bool
    where
      Q: Hash + Equivalent<Key> + ?Sized,
    {
        self.map.contains_key(key)
    }
    /// Returns a reference to the value corresponding to the key. Implicitly converts between int
    /// and uint keys.
    pub fn get<'a>(&'a self, key: &KeyRef) -> Option<&'a Value<'static>> {
        self.map.get(key).or_else(|| match key {
            KeyRef::Int(k) => {
                let converted = u64::try_from(*k).ok()?;
                self.map.get(&Key::Uint(converted))
            }
            KeyRef::Uint(k) => {
                let converted = i64::try_from(*k).ok()?;
                self.map.get(&Key::Int(converted))
            }
            _ => None,
        })
    }
}

#[derive(Debug, Eq, PartialEq, Hash, Ord, Clone, PartialOrd)]
pub enum Key {
    Int(i64),
    Uint(u64),
    Bool(bool),
    String(Arc<str>),
}

/// A borrowed version of [`Key`] that avoids allocating for lookups.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub enum KeyRef<'a> {
    Int(i64),
    Uint(u64),
    Bool(bool),
    String(&'a str),
}

impl Equivalent<Key> for KeyRef<'_> {
    fn equivalent(&self, key: &Key) -> bool {
        self == &KeyRef::from(key)
    }
}

/// Implement conversions from primitive types to [`Key`]
impl From<String> for Key {
    fn from(v: String) -> Self {
        Key::String(v.into())
    }
}

impl From<Arc<str>> for Key {
    fn from(v: Arc<str>) -> Self {
        Key::String(v)
    }
}

impl<'a> From<&'a str> for Key {
    fn from(v: &'a str) -> Self {
        Key::String(Arc::from(v))
    }
}

impl From<bool> for Key {
    fn from(v: bool) -> Self {
        Key::Bool(v)
    }
}

impl From<i64> for Key {
    fn from(v: i64) -> Self {
        Key::Int(v)
    }
}

impl From<i32> for Key {
    fn from(v: i32) -> Self {
        Key::Int(v as i64)
    }
}

impl From<u64> for Key {
    fn from(v: u64) -> Self {
        Key::Uint(v)
    }
}

impl From<u32> for Key {
    fn from(v: u32) -> Self {
        Key::Uint(v as u64)
    }
}

impl serde::Serialize for Key {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
      S: serde::Serializer,
    {
        match self {
            Key::Int(v) => v.serialize(serializer),
            Key::Uint(v) => v.serialize(serializer),
            Key::Bool(v) => v.serialize(serializer),
            Key::String(v) => v.serialize(serializer),
        }
    }
}

impl Display for Key {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Key::Int(v) => write!(f, "{v}"),
            Key::Uint(v) => write!(f, "{v}"),
            Key::Bool(v) => write!(f, "{v}"),
            Key::String(v) => write!(f, "{v}"),
        }
    }
}

/// Implement conversions from [`Key`] into [`Value`]
impl<'a> TryInto<Key> for Value<'a> {
    type Error = Value<'static>;

    #[inline(always)]
    fn try_into(self) -> Result<Key, Self::Error> {
        match self {
            Value::Int(v) => Ok(Key::Int(v)),
            Value::UInt(v) => Ok(Key::Uint(v)),
            Value::String(v) => Ok(Key::String(v.as_owned())),
            Value::Bool(v) => Ok(Key::Bool(v)),
            _ => Err(self.as_static()),
        }
    }
}

impl<'a> From<&'a Key> for KeyRef<'a> {
    fn from(value: &'a Key) -> Self {
        match value {
            Key::Int(v) => KeyRef::Int(*v),
            Key::Uint(v) => KeyRef::Uint(*v),
            Key::String(v) => KeyRef::String(v.as_ref()),
            Key::Bool(v) => KeyRef::Bool(*v),
        }
    }
}
/// Implement conversions from [`KeyRef`] into [`Value`]
impl<'a> TryFrom<&'a Value<'a>> for KeyRef<'a> {
    type Error = Value<'a>;

    fn try_from(value: &'a Value) -> Result<Self, Self::Error> {
        match value {
            Value::Int(v) => Ok(KeyRef::Int(*v)),
            Value::UInt(v) => Ok(KeyRef::Uint(*v)),
            Value::String(v) => Ok(KeyRef::String(v.as_ref())),
            Value::Bool(v) => Ok(KeyRef::Bool(*v)),
            _ => Err(value.clone()),
        }
    }
}

impl<K: Into<Key>, V: Into<Value<'static>>> From<HashMap<K, V>> for Map {
    fn from(map: HashMap<K, V>) -> Self {
        let mut new_map = hashbrown::HashMap::with_capacity(map.len());
        for (k, v) in map {
            new_map.insert(k.into(), v.into());
        }
        Map {
            map: Arc::new(new_map),
        }
    }
}
impl<K: Into<Key>, V: Into<Value<'static>>> From<hashbrown::HashMap<K, V>> for Map {
    fn from(map: hashbrown::HashMap<K, V>) -> Self {
        let mut new_map = hashbrown::HashMap::with_capacity(map.len());
        for (k, v) in map {
            new_map.insert(k.into(), v.into());
        }
        Map {
            map: Arc::new(new_map),
        }
    }
}

/// Equality helper for [`Opaque`] values.
///
/// Implementors define how two values of the same runtime type compare for
/// equality when stored as [`Value::Opaque`].
///
/// You normally don't implement this trait manually. It is automatically
/// provided for any `T: Eq + PartialEq + Any + Opaque` (see the blanket impl
/// below). The runtime will first ensure the two values have the same
/// [`Opaque::runtime_type_name`], and only then attempt a downcast and call
/// `Eq::eq`.
pub trait OpaqueEq {
    /// Compare with another [`Opaque`] erased value.
    ///
    /// Implementations should return `false` if `other` does not have the same
    /// runtime type, or if it cannot be downcast to the concrete type of `self`.
    fn opaque_eq(&self, other: &dyn Opaque) -> bool;
}

impl<T> OpaqueEq for T
where
  T: Eq + PartialEq + Any + Opaque,
{
    fn opaque_eq(&self, other: &dyn Opaque) -> bool {
        if self.runtime_type_name() != other.runtime_type_name() {
            return false;
        }
        if let Some(other) = other.downcast_ref::<T>() {
            self.eq(other)
        } else {
            false
        }
    }
}

/// Helper trait to obtain a `&dyn Debug` view.
///
/// This is auto-implemented for any `T: Debug` and is used by the runtime to
/// format [`Opaque`] values without knowing their concrete type.
pub trait AsDebug {
    /// Returns `self` as a `&dyn Debug` trait object.
    fn as_debug(&self) -> &dyn Debug;
}

impl<T> AsDebug for T
where
  T: Debug,
{
    fn as_debug(&self) -> &dyn Debug {
        self
    }
}
use crate::magic::Function;

pub trait StructValue<'a>: std::fmt::Debug {
    fn get_member(&self, name: &str) -> Option<Value<'a>>;
    fn resolve_function(&self, _name: &str) -> Option<&Function> {
        None
    }
    #[cfg(feature = "json")]
    fn json(&self) -> Option<serde_json::Value> {
        None
    }
}

/// Trait for user-defined opaque values stored inside [`Value::Opaque`].
///
/// Implement this trait for types that should participate in CEL evaluation as
/// opaque/user-defined values. An opaque value:
/// - must report a stable runtime type name via [`runtime_type_name`];
/// - participates in equality via the blanket [`OpaqueEq`] implementation;
/// - can be formatted via [`AsDebug`];
/// - must be thread-safe (`Send + Sync`).
///
/// When the `json` feature is enabled you may optionally provide a JSON
/// representation for diagnostics, logging or interop. Returning `None` keeps the
/// value non-serializable for JSON.
///
/// Example
/// ```rust
/// use std::fmt::{Debug, Formatter, Result as FmtResult};
/// use std::sync::Arc;
/// use cel::objects::{Opaque, Value};
///
/// #[derive(Eq, PartialEq)]
/// struct MyId(u64);
///
/// impl Debug for MyId {
///     fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult { write!(f, "MyId({})", self.0) }
/// }
///
/// impl Opaque for MyId {
///     fn runtime_type_name(&self) -> &str { "example.MyId" }
/// }
///
/// // Values of `MyId` can now be wrapped in `Value::Opaque` and compared.
/// let a = Value::Opaque(Arc::new(MyId(7)));
/// let b = Value::Opaque(Arc::new(MyId(7)));
/// assert_eq!(a, b);
/// ```
pub trait Opaque: Any + OpaqueEq + AsDebug + Send + Sync {
    /// Returns a stable, fully-qualified type name for this value's runtime type.
    ///
    /// This name is used to check type compatibility before attempting downcasts
    /// during equality checks and other operations. It should be stable across
    /// versions and unique within your application or library (e.g., a package
    /// qualified name like `my.pkg.Type`).
    fn runtime_type_name(&self) -> &str;
    fn resolve_variable(&self, _name: &str) -> Option<Value<'static>> {
        None
    }

    // fn resolve_function(&self, _name: &str) -> Option<&Function> {
    //     None
    // }
    /// Optional JSON representation (requires the `json` feature).
    ///
    /// The default implementation returns `None`, indicating that the value
    /// cannot be represented as JSON.
    #[cfg(feature = "json")]
    fn json(&self) -> Option<serde_json::Value> {
        None
    }
}

impl dyn Opaque {
    pub fn downcast_ref<T: Any>(&self) -> Option<&T> {
        let any: &dyn Any = self;
        any.downcast_ref()
    }
}

// TODO: in their current form, Opaque must be 'static.

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OptionalValue {
    value: Option<Value<'static>>,
}

impl OptionalValue {
    pub fn of(value: Value<'static>) -> Self {
        OptionalValue { value: Some(value) }
    }
    pub fn none() -> Self {
        OptionalValue { value: None }
    }
    pub fn value(&self) -> Option<&Value<'static>> {
        self.value.as_ref()
    }
}

impl Opaque for OptionalValue {
    fn runtime_type_name(&self) -> &str {
        "optional_type"
    }
}
impl<'a> TryFrom<Value<'a>> for OptionalValue {
    type Error = ExecutionError;

    fn try_from(value: Value<'a>) -> Result<Self, Self::Error> {
        match value {
            Value::Opaque(opaque) if opaque.as_ref().runtime_type_name() == "optional_type" => {
                opaque
                  .as_ref()
                  .downcast_ref::<OptionalValue>()
                  .ok_or_else(|| ExecutionError::function_error("optional", "failed to downcast"))
                  .cloned()
            }
            Value::Opaque(opaque) => Err(ExecutionError::UnexpectedType {
                got: opaque.as_ref().runtime_type_name().to_string(),
                want: "optional_type".to_string(),
            }),
            v => Err(ExecutionError::UnexpectedType {
                got: v.type_of().to_string(),
                want: "optional_type".to_string(),
            }),
        }
    }
}
impl<'a, 'b: 'a> TryFrom<&'b Value<'a>> for &'b OptionalValue {
    type Error = ExecutionError;

    fn try_from(value: &'b Value<'a>) -> Result<Self, Self::Error> {
        match value {
            Value::Opaque(opaque) if opaque.as_ref().runtime_type_name() == "optional_type" => {
                opaque
                  .as_ref()
                  .downcast_ref::<OptionalValue>()
                  .ok_or_else(|| ExecutionError::function_error("optional", "failed to downcast"))
            }
            Value::Opaque(opaque) => Err(ExecutionError::UnexpectedType {
                got: opaque.as_ref().runtime_type_name().to_string(),
                want: "optional_type".to_string(),
            }),
            v => Err(ExecutionError::UnexpectedType {
                got: v.type_of().to_string(),
                want: "optional_type".to_string(),
            }),
        }
    }
}

pub trait TryIntoValue<'a> {
    type Error: std::error::Error + 'static + Send + Sync;
    fn try_into_value(self) -> Result<Value<'a>, Self::Error>;
}

impl<'a, T: serde::Serialize> TryIntoValue<'a> for T {
    type Error = crate::ser::SerializationError;
    fn try_into_value(self) -> Result<Value<'a>, Self::Error> {
        crate::ser::to_value(self)
    }
}
impl<'a> TryIntoValue<'a> for Value<'a> {
    type Error = Infallible;
    fn try_into_value(self) -> Result<Value<'a>, Self::Error> {
        Ok(self)
    }
}

#[derive(Clone)]
pub enum StringValue<'a> {
    Borrowed(&'a str),
    Owned(Arc<str>),
}

impl From<String> for StringValue<'static> {
    fn from(v: String) -> Self {
        StringValue::Owned(v.into())
    }
}

impl<'a> From<&'a str> for StringValue<'a> {
    fn from(v: &'a str) -> Self {
        StringValue::Borrowed(v)
    }
}

impl From<Arc<str>> for StringValue<'static> {
    fn from(v: Arc<str>) -> Self {
        StringValue::Owned(v)
    }
}
impl<'a> Deref for StringValue<'a> {
    type Target = str;
    fn deref(&self) -> &str {
        self.as_ref()
    }
}
impl AsRef<str> for StringValue<'_> {
    fn as_ref(&self) -> &str {
        match self {
            StringValue::Borrowed(s) => s,
            StringValue::Owned(s) => s.as_ref(),
        }
    }
}
impl<'a> StringValue<'a> {
    pub fn as_owned(&self) -> Arc<str> {
        match self {
            StringValue::Borrowed(v) => Arc::from(*v),
            StringValue::Owned(o) => Arc::clone(o),
        }
    }
}

pub enum OpaqueValue<'a> {
    Borrowed(&'a dyn Opaque),
    Arc(Arc<dyn Opaque>),
}

impl<T: Opaque> From<Arc<T>> for OpaqueValue<'static> {
    fn from(value: Arc<T>) -> Self {
        Self::Arc(value)
    }
}

impl AsRef<dyn Opaque> for OpaqueValue<'_> {
    fn as_ref(&self) -> &dyn Opaque {
        match self {
            OpaqueValue::Borrowed(v) => *v,
            OpaqueValue::Arc(v) => v.as_ref(),
        }
    }
}

pub enum ListValue<'a> {
    Borrowed(&'a [Value<'a>]),
    PartiallyOwned(Arc<[Value<'a>]>),
    Owned(Arc<[Value<'static>]>),
}

impl From<Arc<[Value<'static>]>> for ListValue<'static> {
    fn from(v: Arc<[Value<'static>]>) -> Self {
        ListValue::Owned(v)
    }
}

impl<'a> Clone for ListValue<'a> {
    fn clone(&self) -> Self {
        match self {
            ListValue::Borrowed(items) => ListValue::Borrowed(items),
            ListValue::PartiallyOwned(items) => ListValue::PartiallyOwned(items.clone()),
            ListValue::Owned(items) => ListValue::Owned(items.clone()),
        }
    }
}

impl<'a> AsRef<[Value<'a>]> for ListValue<'a> {
    fn as_ref(&self) -> &[Value<'a>] {
        match self {
            ListValue::Borrowed(a) => a,
            ListValue::PartiallyOwned(a) => a.as_ref(),
            ListValue::Owned(a) => a.as_ref(),
        }
    }
}

impl<'a> ListValue<'a> {
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn len(&self) -> usize {
        match self {
            ListValue::Borrowed(items) => items.len(),
            ListValue::PartiallyOwned(items) => items.len(),
            ListValue::Owned(items) => items.len(),
        }
    }

    pub fn iter(&'a self) -> slice::Iter<'a, Value<'a>> {
        self.as_ref().iter()
    }
}

#[derive(Clone, Debug)]
pub enum BytesValue<'a> {
    Borrowed(&'a [u8]),
    Owned(Arc<[u8]>),
    Bytes(Bytes),
}

impl From<Arc<[u8]>> for BytesValue<'static> {
    fn from(v: Arc<[u8]>) -> Self {
        BytesValue::Owned(v)
    }
}
impl<'a> AsRef<[u8]> for BytesValue<'a> {
    fn as_ref(&self) -> &[u8] {
        match self {
            BytesValue::Borrowed(b) => b,
            BytesValue::Owned(v) => v.as_ref(),
            BytesValue::Bytes(b) => b.as_ref(),
        }
    }
}

pub enum Value<'a> {
    List(ListValue<'a>),
    Map(Map),

    // Atoms
    Int(i64),
    UInt(u64),
    Float(f64),
    Bool(bool),
    #[cfg(feature = "chrono")]
    Duration(chrono::Duration),
    #[cfg(feature = "chrono")]
    Timestamp(chrono::DateTime<chrono::FixedOffset>),

    Opaque(OpaqueValue<'a>),
    Struct(OpaqueBox<'a>),

    String(StringValue<'a>),
    Bytes(BytesValue<'a>),

    Null,
}

impl Value<'_> {
    pub fn as_number(&self) -> Result<usize, ExecutionError> {
        match self {
            Value::Int(i) => usize::try_from(*i).map_err(|_e| ExecutionError::Conversion("usize", self.as_static())),
            Value::UInt(u) => usize::try_from(*u).map_err(|_e| ExecutionError::Conversion("usize", self.as_static())),
            _ => {
                Err(ExecutionError::Conversion("usize", self.as_static()))
            }
        }
    }
    pub fn as_bool(&self) -> Result<bool, ExecutionError> {
        match self {
            Value::Bool(b) => Ok(*b),
            _ => {
                Err(ExecutionError::Conversion("bool", self.as_static()))
            }
        }
    }
    pub fn as_bytes(&self) -> Result<&[u8], ExecutionError> {
        match self {
            Value::String(b) => Ok(b.as_ref().as_bytes()),
            Value::Bytes(b) => Ok(b.as_ref()),
            _ => {
                Err(ExecutionError::Conversion("bytes", self.as_static()))
            }
        }
    }
    // Note: may allocate
    pub fn as_str(&self) -> Result<Cow<'_, str>, ExecutionError> {
        match self {
            Value::String(v) => Ok(Cow::Borrowed(v.as_ref())),
            Value::Bool(v) => if *v {
                Ok(Cow::Borrowed("true"))
            } else {
                Ok(Cow::Borrowed("false"))
            }
            Value::Int(v) => Ok(Cow::Owned(v.to_string())),
            Value::UInt(v) => Ok(Cow::Owned(v.to_string())),
            Value::Bytes(v) => {
                use base64::Engine;
                Ok(Cow::Owned(base64::prelude::BASE64_STANDARD.encode(v.as_ref())))
            },
            _ => {
                Err(ExecutionError::Conversion("string", self.as_static()))
            }
        }
    }
}

fn _assert_covariant<'short>(v: Value<'static>) -> Value<'short> {
    v // ✅ If this compiles, Value is covariant in 'a
}

impl PartialEq for Value<'_> {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::Map(a), Value::Map(b)) => a == b,
            (Value::List(a), Value::List(b)) => {
                a.len() == b.len() && a.iter().zip(b.iter()).all(|(a, b)| a == b)
            }
            (Value::Int(a), Value::Int(b)) => a == b,
            (Value::UInt(a), Value::UInt(b)) => a == b,
            (Value::Float(a), Value::Float(b)) => a == b,
            (Value::String(a), Value::String(b)) => a.as_ref() == b.as_ref(),
            (Value::Bytes(a), Value::Bytes(b)) => a.as_ref() == b.as_ref(),
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Null, Value::Null) => true,
            #[cfg(feature = "chrono")]
            (Value::Duration(a), Value::Duration(b)) => a == b,
            #[cfg(feature = "chrono")]
            (Value::Timestamp(a), Value::Timestamp(b)) => a == b,
            // Allow different numeric types to be compared without explicit casting.
            (Value::Int(a), Value::UInt(b)) => a
              .to_owned()
              .try_into()
              .map(|a: u64| a == *b)
              .unwrap_or(false),
            (Value::Int(a), Value::Float(b)) => (*a as f64) == *b,
            (Value::UInt(a), Value::Int(b)) => a
              .to_owned()
              .try_into()
              .map(|a: i64| a == *b)
              .unwrap_or(false),
            (Value::UInt(a), Value::Float(b)) => (*a as f64) == *b,
            (Value::Float(a), Value::Int(b)) => *a == (*b as f64),
            (Value::Float(a), Value::UInt(b)) => *a == (*b as f64),
            (Value::Opaque(a), Value::Opaque(b)) => match (a, b) {
                (OpaqueValue::Borrowed(a), OpaqueValue::Borrowed(b)) => a.opaque_eq(*b),
                (OpaqueValue::Borrowed(a), OpaqueValue::Arc(b)) => a.opaque_eq(b.as_ref()),
                (OpaqueValue::Arc(a), OpaqueValue::Borrowed(b)) => a.as_ref().opaque_eq(*b),
                (OpaqueValue::Arc(a), OpaqueValue::Arc(b)) => a.as_ref().opaque_eq(b.as_ref()),
            },
            (_, _) => false,
        }
    }
}

impl Eq for Value<'_> {}

impl PartialOrd for Value<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        match (self, other) {
            (Value::Int(a), Value::Int(b)) => Some(a.cmp(b)),
            (Value::UInt(a), Value::UInt(b)) => Some(a.cmp(b)),
            (Value::Float(a), Value::Float(b)) => a.partial_cmp(b),
            (Value::String(a), Value::String(b)) => Some(a.as_ref().cmp(b.as_ref())),
            (Value::Bool(a), Value::Bool(b)) => Some(a.cmp(b)),
            (Value::Null, Value::Null) => Some(Ordering::Equal),
            #[cfg(feature = "chrono")]
            (Value::Duration(a), Value::Duration(b)) => Some(a.cmp(b)),
            #[cfg(feature = "chrono")]
            (Value::Timestamp(a), Value::Timestamp(b)) => Some(a.cmp(b)),
            // Allow different numeric types to be compared without explicit casting.
            (Value::Int(a), Value::UInt(b)) => Some(
                a.to_owned()
                  .try_into()
                  .map(|a: u64| a.cmp(b))
                  // If the i64 doesn't fit into a u64 it must be less than 0.
                  .unwrap_or(Ordering::Less),
            ),
            (Value::Int(a), Value::Float(b)) => (*a as f64).partial_cmp(b),
            (Value::UInt(a), Value::Int(b)) => Some(
                a.to_owned()
                  .try_into()
                  .map(|a: i64| a.cmp(b))
                  // If the u64 doesn't fit into a i64 it must be greater than i64::MAX.
                  .unwrap_or(Ordering::Greater),
            ),
            (Value::UInt(a), Value::Float(b)) => (*a as f64).partial_cmp(b),
            (Value::Float(a), Value::Int(b)) => a.partial_cmp(&(*b as f64)),
            (Value::Float(a), Value::UInt(b)) => a.partial_cmp(&(*b as f64)),
            _ => None,
        }
    }
}

impl Debug for Value<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::List(l) => {
                write!(f, "List([")?;
                let mut iter = l.iter();
                if let Some(first) = iter.next() {
                    write!(f, "{:?}", first)?;
                    for item in iter {
                        write!(f, ", {:?}", item)?;
                    }
                }
                write!(f, "])")
            }
            Value::Map(m) => write!(f, "Map({:?})", m),
            Value::Int(i) => write!(f, "Int({:?})", i),
            Value::UInt(u) => write!(f, "UInt({:?})", u),
            Value::Float(d) => write!(f, "Float({:?})", d),
            Value::String(s) => write!(f, "String({:?})", s.as_ref()),
            Value::Bytes(b) => write!(f, "Bytes({:?})", b.as_ref()),
            Value::Bool(b) => write!(f, "Bool({:?})", b),
            #[cfg(feature = "chrono")]
            Value::Duration(d) => write!(f, "Duration({:?})", d),
            #[cfg(feature = "chrono")]
            Value::Timestamp(t) => write!(f, "Timestamp({:?})", t),
            Value::Struct(t) => write!(f, "Struct({:?})", t),
            Value::Opaque(o) => match o {
                OpaqueValue::Borrowed(op) => {
                    write!(f, "Opaque<{}>({:?})", op.runtime_type_name(), op.as_debug())
                }
                OpaqueValue::Arc(arc) => {
                    write!(
                        f,
                        "Opaque<{}>({:?})",
                        arc.runtime_type_name(),
                        arc.as_debug()
                    )
                }
            },
            Value::Null => write!(f, "Null"),
        }
    }
}

impl<'a> Clone for Value<'a> {
    fn clone(&self) -> Self {
        match self {
            Value::List(l) => Value::List(match l {
                ListValue::Borrowed(items) => ListValue::Borrowed(items),
                ListValue::PartiallyOwned(items) => ListValue::PartiallyOwned(items.clone()),
                ListValue::Owned(items) => ListValue::Owned(items.clone()),
            }),
            Value::Map(m) => Value::Map(m.clone()),
            Value::Int(i) => Value::Int(*i),
            Value::UInt(u) => Value::UInt(*u),
            Value::Float(f) => Value::Float(*f),
            Value::Bool(b) => Value::Bool(*b),
            Value::Struct(b) => Value::Struct(b.clone()),
            #[cfg(feature = "chrono")]
            Value::Duration(d) => Value::Duration(*d),
            #[cfg(feature = "chrono")]
            Value::Timestamp(t) => Value::Timestamp(*t),
            Value::Opaque(o) => Value::Opaque(match o {
                OpaqueValue::Borrowed(op) => OpaqueValue::Borrowed(*op),
                OpaqueValue::Arc(arc) => OpaqueValue::Arc(arc.clone()),
            }),
            Value::String(s) => Value::String(match s {
                StringValue::Borrowed(str_ref) => StringValue::Borrowed(str_ref),
                StringValue::Owned(owned) => StringValue::Owned(owned.clone()),
            }),
            Value::Bytes(b) => Value::Bytes(match b {
                BytesValue::Borrowed(bytes) => BytesValue::Borrowed(bytes),
                BytesValue::Owned(vec) => BytesValue::Owned(vec.clone()),
                BytesValue::Bytes(bytes) => BytesValue::Bytes(bytes.clone()),
            }),
            Value::Null => Value::Null,
        }
    }
}

impl From<CelVal> for Value<'static> {
    fn from(val: CelVal) -> Self {
        match val {
            CelVal::String(s) => Value::String(StringValue::Owned(Arc::from(s.as_ref()))),
            CelVal::Boolean(b) => Value::Bool(b),
            CelVal::Int(i) => Value::Int(i),
            CelVal::UInt(u) => Value::UInt(u),
            CelVal::Double(d) => Value::Float(d),
            CelVal::Bytes(bytes) => Value::Bytes(BytesValue::Owned(bytes.into())),
            CelVal::Null => Value::Null,
            v => unimplemented!("{v:?}"),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum ValueType {
    List,
    Map,
    Function,
    Int,
    UInt,
    Float,
    String,
    Bytes,
    Bool,
    Duration,
    Timestamp,
    Opaque,
    Null,
}

impl Display for ValueType {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            ValueType::List => write!(f, "list"),
            ValueType::Map => write!(f, "map"),
            ValueType::Function => write!(f, "function"),
            ValueType::Int => write!(f, "int"),
            ValueType::UInt => write!(f, "uint"),
            ValueType::Float => write!(f, "float"),
            ValueType::String => write!(f, "string"),
            ValueType::Bytes => write!(f, "bytes"),
            ValueType::Bool => write!(f, "bool"),
            ValueType::Opaque => write!(f, "opaque"),
            ValueType::Duration => write!(f, "duration"),
            ValueType::Timestamp => write!(f, "timestamp"),
            ValueType::Null => write!(f, "null"),
        }
    }
}

impl<'a> Value<'a> {
    pub fn type_of(&self) -> ValueType {
        match self {
            Value::List(_) => ValueType::List,
            Value::Map(_) => ValueType::Map,
            Value::Int(_) => ValueType::Int,
            Value::UInt(_) => ValueType::UInt,
            Value::Float(_) => ValueType::Float,
            Value::String(_) => ValueType::String,
            Value::Bytes(_) => ValueType::Bytes,
            Value::Bool(_) => ValueType::Bool,
            Value::Opaque(_) => ValueType::Opaque,
            Value::Struct(_) => ValueType::Opaque,
            #[cfg(feature = "chrono")]
            Value::Duration(_) => ValueType::Duration,
            #[cfg(feature = "chrono")]
            Value::Timestamp(_) => ValueType::Timestamp,
            Value::Null => ValueType::Null,
        }
    }

    pub fn is_zero(&self) -> bool {
        match self {
            Value::List(v) => v.is_empty(),
            Value::Map(v) => v.map.is_empty(),
            Value::Int(0) => true,
            Value::UInt(0) => true,
            Value::Float(f) => *f == 0.0,
            Value::String(v) => v.is_empty(),
            Value::Bytes(v) => v.as_ref().is_empty(),
            Value::Bool(false) => true,
            #[cfg(feature = "chrono")]
            Value::Duration(v) => v.is_zero(),
            Value::Null => true,
            _ => false,
        }
    }

    pub fn error_expected_type(&self, expected: ValueType) -> ExecutionError {
        ExecutionError::UnexpectedType {
            got: self.type_of().to_string(),
            want: expected.to_string(),
        }
    }
}

impl From<&Key> for Value<'static> {
    fn from(value: &Key) -> Self {
        match value {
            Key::Int(v) => Value::Int(*v),
            Key::Uint(v) => Value::UInt(*v),
            Key::Bool(v) => Value::Bool(*v),
            Key::String(v) => Value::String(StringValue::Owned(v.clone())),
        }
    }
}

impl From<Key> for Value<'static> {
    fn from(value: Key) -> Self {
        match value {
            Key::Int(v) => Value::Int(v),
            Key::Uint(v) => Value::UInt(v),
            Key::Bool(v) => Value::Bool(v),
            Key::String(v) => Value::String(StringValue::Owned(v)),
        }
    }
}

impl From<&Key> for Key {
    fn from(key: &Key) -> Self {
        key.clone()
    }
}

// Convert Vec<T> to Value
impl<T: Into<Value<'static>>> From<Vec<T>> for Value<'static> {
    fn from(v: Vec<T>) -> Self {
        Value::List(ListValue::Owned(v.into_iter().map(|v| v.into()).collect()))
    }
}

// Convert Vec<u8> to Value
impl From<Vec<u8>> for Value<'static> {
    fn from(v: Vec<u8>) -> Self {
        Value::Bytes(BytesValue::Owned(v.into()))
    }
}

#[cfg(feature = "bytes")]
// Convert Bytes to Value
impl From<::bytes::Bytes> for Value<'static> {
    fn from(v: ::bytes::Bytes) -> Self {
        Value::Bytes(BytesValue::Bytes(v))
    }
}

#[cfg(feature = "bytes")]
// Convert &Bytes to Value
impl From<&::bytes::Bytes> for Value<'static> {
    fn from(v: &::bytes::Bytes) -> Self {
        Value::Bytes(BytesValue::Bytes(v.clone()))
    }
}

// Convert String to Value
impl From<String> for Value<'static> {
    fn from(v: String) -> Self {
        Value::String(StringValue::Owned(Arc::from(v.as_ref())))
    }
}

impl From<&str> for Value<'static> {
    fn from(v: &str) -> Self {
        Value::String(StringValue::Owned(Arc::from(v)))
    }
}

// Convert Option<T> to Value
impl<T: Into<Value<'static>>> From<Option<T>> for Value<'static> {
    fn from(v: Option<T>) -> Self {
        match v {
            Some(v) => v.into(),
            None => Value::Null,
        }
    }
}

// Convert HashMap<K, V> to Value
impl<K: Into<Key>, V: Into<Value<'static>>> From<HashMap<K, V>> for Value<'static> {
    fn from(v: HashMap<K, V>) -> Self {
        Value::Map(v.into())
    }
}

impl From<ExecutionError> for ResolveResult<'static> {
    fn from(value: ExecutionError) -> Self {
        Err(value)
    }
}

pub type ResolveResult<'a> = Result<Value<'a>, ExecutionError>;

impl<'a> From<Value<'a>> for ResolveResult<'a> {
    fn from(value: Value<'a>) -> Self {
        Ok(value)
    }
}

/// A custom vtable for our opaque values
struct OpaqueVtable {
    get_member: unsafe fn(NonNull<()>, &str) -> Option<Value<'static>>,
    resolve_function: unsafe fn(NonNull<()>, &str) -> Option<&Function>,
    #[cfg(feature = "json")]
    json: unsafe fn(NonNull<()>) -> Option<serde_json::Value>,
    debug: unsafe fn(NonNull<()>, &mut std::fmt::Formatter<'_>) -> std::fmt::Result,
    drop: unsafe fn(NonNull<()>),
    clone: unsafe fn(NonNull<()>) -> NonNull<()>,
}

/// A covariant box containing an opaque value.
///
/// This is covariant in 'a because:
/// - The actual data pointer doesn't carry lifetime info (it's erased)
/// - PhantomData<fn() -> Value<'a>> is covariant in 'a (function return types are covariant)
pub struct OpaqueBox<'a> {
    data: NonNull<()>,
    vtable: &'static OpaqueVtable,
    // fn() -> T is covariant in T, so fn() -> Value<'a> is covariant in 'a
    _marker: PhantomData<fn() -> Value<'a>>,
}

// Safety: OpaqueBox is Send/Sync if the underlying type is
unsafe impl<'a> Send for OpaqueBox<'a> {}
unsafe impl<'a> Sync for OpaqueBox<'a> {}

impl<'a> OpaqueBox<'a> {
    /// Create a new OpaqueBox from a value implementing OpaqueValue
    pub fn new<T>(value: T) -> Self
    where
      T: StructValue<'a> + Clone + 'a,
    {
        let boxed = Box::new(value);
        let ptr = NonNull::new(Box::into_raw(boxed) as *mut ()).unwrap();

        // Create the vtable with functions specialized for type T
        // We transmute the lifetime to 'static in the vtable, but the PhantomData
        // ensures we only use it with the correct lifetime 'a
        // let vtable: &'static OpaqueVtable = Self::make_vtable::<T>();

        static VTAB: OnceLock<OpaqueVtable> = OnceLock::new();
        let vtable = VTAB.get_or_init(|| Self::make_vtable::<T>());
        OpaqueBox {
            data: ptr,
            vtable,
            _marker: PhantomData,
        }
    }

    fn make_vtable<T: StructValue<'a> + Clone + 'a>() -> OpaqueVtable {
        // These functions are safe because we only call them with the correct type T
        unsafe fn get_member_impl<'a, T: StructValue<'a>>(
            ptr: NonNull<()>,
            name: &str,
        ) -> Option<Value<'static>> {
            unsafe {
                let value = &*(ptr.as_ptr() as *const T);
                // Safety: We're transmuting Value<'a> to Value<'static>
                // This is safe because:
                // 1. The caller (OpaqueBox::get_member) will immediately cast it back to Value<'a>
                // 2. The OpaqueBox's PhantomData ensures the correct lifetime is tracked
                std::mem::transmute(value.get_member(name))
            }
        }
        unsafe fn resolve_function_impl<'a, T: StructValue<'a>>(
            ptr: NonNull<()>,
            name: &str,
        ) -> Option<&Function> {
            unsafe {
                let value = &*(ptr.as_ptr() as *const T);
                // Safety: todo
                std::mem::transmute(value.resolve_function(name))
            }
        }

        #[cfg(feature = "json")]
        unsafe fn json_impl<'a, T: StructValue<'a>>(ptr: NonNull<()>) -> Option<serde_json::Value> {
            unsafe {
                let value = &*(ptr.as_ptr() as *const T);
                value.json()
            }
        }

        unsafe fn debug_impl<'a, T: StructValue<'a>>(
            ptr: NonNull<()>,
            f: &mut std::fmt::Formatter<'_>,
        ) -> std::fmt::Result {
            unsafe {
                let value = &*(ptr.as_ptr() as *const T);
                std::fmt::Debug::fmt(value, f)
            }
        }

        unsafe fn drop_impl<'a, T: StructValue<'a>>(ptr: NonNull<()>) {
            unsafe {
                let _ = Box::from_raw(ptr.as_ptr() as *mut T);
            }
        }

        unsafe fn clone_impl<'a, T: StructValue<'a> + Clone>(ptr: NonNull<()>) -> NonNull<()> {
            unsafe {
                let value = &*(ptr.as_ptr() as *const T);
                let cloned = Box::new(value.clone());
                NonNull::new(Box::into_raw(cloned) as *mut ()).unwrap()
            }
        }

        // Leak a static vtable (one per type T)
        // We use Box::leak to create a 'static reference
        unsafe {
            #[allow(clippy::missing_transmute_annotations)]
            OpaqueVtable {
                get_member: std::mem::transmute(
                    get_member_impl::<T> as unsafe fn(NonNull<()>, &str) -> Option<Value<'a>>,
                ),
                resolve_function: std::mem::transmute(
                    resolve_function_impl::<T> as unsafe fn(NonNull<()>, &str) -> Option<&Function>,
                ),
                #[cfg(feature = "json")]
                json: json_impl::<T>,
                debug: std::mem::transmute(
                    debug_impl::<T>
                      as unsafe fn(NonNull<()>, &mut std::fmt::Formatter<'_>) -> std::fmt::Result,
                ),
                drop: drop_impl::<T>,
                clone: clone_impl::<T>,
            }
        }
    }

    pub fn get_member(&self, name: &str) -> Option<Value<'a>> {
        unsafe {
            // Safety: vtable.get_member returns Value<'static> but we cast to Value<'a>
            // This is sound because the actual lifetime is 'a (enforced by PhantomData)
            std::mem::transmute((self.vtable.get_member)(self.data, name))
        }
    }

    pub fn resolve_function(&self, name: &str) -> Option<&Function> {
        unsafe {
            // Safety: vtable.get_member returns Value<'static> but we cast to Value<'a>
            // This is sound because the actual lifetime is 'a (enforced by PhantomData)
            std::mem::transmute((self.vtable.resolve_function)(self.data, name))
        }
    }

    #[cfg(feature = "json")]
    pub fn json(&self) -> Option<serde_json::Value> {
        unsafe { (self.vtable.json)(self.data) }
    }
}

impl<'a> std::fmt::Debug for OpaqueBox<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        unsafe { (self.vtable.debug)(self.data, f) }
    }
}

impl<'a> Drop for OpaqueBox<'a> {
    fn drop(&mut self) {
        unsafe { (self.vtable.drop)(self.data) }
    }
}

impl<'a> Clone for OpaqueBox<'a> {
    fn clone(&self) -> Self {
        OpaqueBox {
            data: unsafe { (self.vtable.clone)(self.data) },
            vtable: self.vtable,
            _marker: PhantomData,
        }
    }
}

impl<'a> Value<'a> {
    pub fn as_static(&self) -> Value<'static> {
        match self {
            Value::List(l) => match l {
                ListValue::Borrowed(items) => Value::List(ListValue::Owned(
                    items.iter().map(|v| v.as_static()).collect(),
                )),
                ListValue::PartiallyOwned(items) => Value::List(ListValue::Owned(
                    items.iter().map(|v| v.as_static()).collect(),
                )),
                ListValue::Owned(items) => Value::List(ListValue::Owned(items.clone())),
            },
            Value::Map(m) => Value::Map(m.clone()),
            Value::Int(i) => Value::Int(*i),
            Value::UInt(u) => Value::UInt(*u),
            Value::Float(f) => Value::Float(*f),
            Value::Bool(b) => Value::Bool(*b),
            Value::Struct(_b) => todo!(),
            #[cfg(feature = "chrono")]
            Value::Duration(d) => Value::Duration(*d),
            #[cfg(feature = "chrono")]
            Value::Timestamp(t) => Value::Timestamp(*t),
            Value::Opaque(o) => match o {
                OpaqueValue::Borrowed(_op) => {
                    // Cannot convert borrowed opaque trait object to owned without concrete type
                    // This is a fundamental limitation - trait objects can't be cloned/moved
                    // without knowing their concrete type. Opaque values should be Arc-wrapped
                    // when they need to be converted to 'static lifetime.
                    panic!("Cannot convert borrowed Opaque value to owned Value<'static>. Opaque values must be Arc-wrapped to convert to 'static lifetime.")
                }
                OpaqueValue::Arc(arc) => Value::Opaque(OpaqueValue::Arc(arc.clone())),
            },
            Value::String(s) => match s {
                StringValue::Borrowed(str_ref) => {
                    Value::String(StringValue::Owned(Arc::from(*str_ref)))
                }
                StringValue::Owned(owned) => Value::String(StringValue::Owned(owned.clone())),
            },
            Value::Bytes(b) => match b {
                BytesValue::Borrowed(bytes) => Value::Bytes(BytesValue::Owned(Arc::from(*bytes))),
                BytesValue::Owned(vec) => Value::Bytes(BytesValue::Owned(vec.clone())),
                BytesValue::Bytes(bytes) => Value::Bytes(BytesValue::Bytes(bytes.clone())),
            },
            Value::Null => Value::Null,
        }
    }
    // pub fn resolve_all(expr: &'a [Expression], ctx: &'a Context) -> ResolveResult<'a> {
    //     let mut res = Vec::with_capacity(expr.len());
    //     for expr in expr {
    //         res.push(Value::resolve(expr, ctx)?);
    //     }
    //     Ok(Value::List(ListValue::Owned(Arc::new(res))))
    // }

    #[inline(always)]
    pub fn resolve<'vars: 'a, 'rf>(
        expr: &'vars Expression,
        ctx: &'vars Context,
        resolver: &'rf dyn VariableResolver<'vars>,
    ) -> ResolveResult<'a> {
        let resolve = |e| Value::resolve(e, ctx, resolver);
        match &expr.expr {
            Expr::Literal(val) => Ok(val.clone().into()),
            Expr::Inline(val) => Ok(val.clone()),
            Expr::Call(call) => {
                if call.args.len() == 3 && call.func_name == operators::CONDITIONAL {
                    let cond = Value::resolve(&call.args[0], ctx, resolver)?;
                    return if cond.to_bool()? {
                        Value::resolve(&call.args[1], ctx, resolver)
                    } else {
                        Value::resolve(&call.args[2], ctx, resolver)
                    };
                }
                if call.args.len() == 2 {
                    match call.func_name.as_str() {
                        operators::ADD => return resolve(&call.args[0])? + resolve(&call.args[1])?,
                        operators::SUBSTRACT => {
                            return resolve(&call.args[0])? - resolve(&call.args[1])?
                        }
                        operators::DIVIDE => {
                            return resolve(&call.args[0])? / resolve(&call.args[1])?
                        }
                        operators::MULTIPLY => {
                            return resolve(&call.args[0])? * resolve(&call.args[1])?
                        }
                        operators::MODULO => {
                            return resolve(&call.args[0])? % resolve(&call.args[1])?
                        }
                        operators::EQUALS => {
                            return Value::Bool(
                                resolve(&call.args[0])?.eq(&resolve(&call.args[1])?),
                            )
                              .into()
                        }
                        operators::NOT_EQUALS => {
                            return Value::Bool(
                                resolve(&call.args[0])?.ne(&resolve(&call.args[1])?),
                            )
                              .into()
                        }
                        operators::LESS => {
                            let left = resolve(&call.args[0])?;
                            let right = resolve(&call.args[1])?;
                            return Value::Bool(
                                left.partial_cmp(&right).ok_or(
                                    ExecutionError::ValuesNotComparable(
                                        left.as_static(),
                                        right.as_static(),
                                    ),
                                )? == Ordering::Less,
                            )
                              .into();
                        }
                        operators::LESS_EQUALS => {
                            let left = resolve(&call.args[0])?;
                            let right = resolve(&call.args[1])?;
                            return Value::Bool(
                                left.partial_cmp(&right).ok_or(
                                    ExecutionError::ValuesNotComparable(
                                        left.as_static(),
                                        right.as_static(),
                                    ),
                                )? != Ordering::Greater,
                            )
                              .into();
                        }
                        operators::GREATER => {
                            let left = resolve(&call.args[0])?;
                            let right = resolve(&call.args[1])?;
                            return Value::Bool(
                                left.partial_cmp(&right).ok_or(
                                    ExecutionError::ValuesNotComparable(
                                        left.as_static(),
                                        right.as_static(),
                                    ),
                                )? == Ordering::Greater,
                            )
                              .into();
                        }
                        operators::GREATER_EQUALS => {
                            let left = resolve(&call.args[0])?;
                            let right = resolve(&call.args[1])?;
                            return Value::Bool(
                                left.partial_cmp(&right).ok_or(
                                    ExecutionError::ValuesNotComparable(
                                        left.as_static(),
                                        right.as_static(),
                                    ),
                                )? != Ordering::Less,
                            )
                              .into();
                        }
                        operators::IN => {
                            let left = resolve(&call.args[0])?;
                            let right = resolve(&call.args[1])?;
                            match (left, right) {
                                (Value::String(l), Value::String(r)) => {
                                    return Value::Bool(r.as_ref().contains(l.as_ref())).into()
                                }
                                (any, Value::List(v)) => {
                                    return Value::Bool(v.as_ref().contains(&any)).into()
                                }
                                (any, Value::Map(m)) => match KeyRef::try_from(&any) {
                                    Ok(key) => return Value::Bool(m.contains_key(&key)).into(),
                                    Err(_) => return Value::Bool(false).into(),
                                },
                                (left, right) => Err(ExecutionError::ValuesNotComparable(
                                    left.as_static(),
                                    right.as_static(),
                                ))?,
                            }
                        }
                        operators::LOGICAL_OR => {
                            let left = resolve(&call.args[0])?;
                            return if left.to_bool()? {
                                left.into()
                            } else {
                                resolve(&call.args[1])
                            };
                        }
                        operators::LOGICAL_AND => {
                            let left = resolve(&call.args[0])?;
                            return if !left.to_bool()? {
                                Value::Bool(false)
                            } else {
                                let right = resolve(&call.args[1])?;
                                Value::Bool(right.to_bool()?)
                            }
                              .into();
                        }
                        operators::INDEX | operators::OPT_INDEX => {
                            let mut value: Value<'a> = resolve(&call.args[0])?;
                            let idx = resolve(&call.args[1])?;
                            let mut is_optional = call.func_name == operators::OPT_INDEX;

                            if let Ok(opt_val) = <&OptionalValue>::try_from(&value) {
                                is_optional = true;
                                value = match opt_val.value() {
                                    Some(inner) => inner.clone(),
                                    None => {
                                        return Ok(Value::Opaque(
                                            Arc::new(OptionalValue::none()).into(),
                                        ))
                                    }
                                };
                            }

                            let result = match (&value, idx) {
                                (Value::List(items), Value::Int(idx)) => {
                                    if idx >= 0 && (idx as usize) < items.len() {
                                        let x: Value<'a> = items.as_ref()[idx as usize].clone();
                                        x.into()
                                    } else {
                                        Err(ExecutionError::IndexOutOfBounds(idx.into()))
                                    }
                                }
                                (Value::List(items), Value::UInt(idx)) => {
                                    if (idx as usize) < items.len() {
                                        items.as_ref()[idx as usize].clone().into()
                                    } else {
                                        Err(ExecutionError::IndexOutOfBounds(idx.into()))
                                    }
                                }
                                (Value::String(_), Value::Int(idx)) => {
                                    Err(ExecutionError::NoSuchKey(idx.to_string().into()))
                                }
                                (Value::Map(map), Value::String(property)) => map
                                  .get(&KeyRef::String(property.as_ref()))
                                  .cloned()
                                  .ok_or_else(|| ExecutionError::NoSuchKey(property.as_owned())),
                                (Value::Map(map), Value::Bool(property)) => {
                                    map.get(&KeyRef::Bool(property)).cloned().ok_or_else(|| {
                                        ExecutionError::NoSuchKey(property.to_string().into())
                                    })
                                }
                                (Value::Map(map), Value::Int(property)) => {
                                    map.get(&KeyRef::Int(property)).cloned().ok_or_else(|| {
                                        ExecutionError::NoSuchKey(property.to_string().into())
                                    })
                                }
                                (Value::Map(map), Value::UInt(property)) => {
                                    map.get(&KeyRef::Uint(property)).cloned().ok_or_else(|| {
                                        ExecutionError::NoSuchKey(property.to_string().into())
                                    })
                                }
                                (Value::Map(_), index) => {
                                    Err(ExecutionError::UnsupportedMapIndex(index.as_static()))
                                }
                                (Value::List(_), index) => {
                                    Err(ExecutionError::UnsupportedListIndex(index.as_static()))
                                }
                                (value, index) => Err(ExecutionError::UnsupportedIndex(
                                    value.as_static(),
                                    index.as_static(),
                                ))?,
                            };

                            return if is_optional {
                                Ok(match result {
                                    Ok(val) => Value::Opaque(
                                        Arc::new(OptionalValue::of(val.as_static())).into(),
                                    ),
                                    Err(_) => Value::Opaque(Arc::new(OptionalValue::none()).into()),
                                })
                            } else {
                                result
                            };
                        }
                        operators::OPT_SELECT => {
                            let operand = resolve(&call.args[0])?;
                            let field_literal = resolve(&call.args[1])?;
                            let field = match field_literal {
                                Value::String(s) => s,
                                _ => {
                                    return Err(ExecutionError::function_error(
                                        "_?._",
                                        "field must be string",
                                    ))
                                }
                            };
                            if let Ok(opt_val) = <&OptionalValue>::try_from(&operand) {
                                return match opt_val.value() {
                                    Some(inner) => Ok(Value::Opaque(
                                        Arc::new(OptionalValue::of(inner.clone().member(&field)?))
                                          .into(),
                                    )),
                                    None => Ok(operand),
                                };
                            }
                            return Ok(Value::Opaque(
                                Arc::new(OptionalValue::of(operand.member(&field)?.as_static()))
                                  .into(),
                            ));
                        }
                        _ => (),
                    }
                }
                if call.args.len() == 1 {
                    match call.func_name.as_str() {
                        operators::LOGICAL_NOT => {
                            let expr = resolve(&call.args[0])?;
                            return Ok(Value::Bool(!expr.to_bool()?));
                        }
                        operators::NEGATE => {
                            return match resolve(&call.args[0])? {
                                Value::Int(i) => Ok(Value::Int(-i)),
                                Value::Float(f) => Ok(Value::Float(-f)),
                                value => Err(ExecutionError::UnsupportedUnaryOperator(
                                    "minus",
                                    value.as_static(),
                                )),
                            }
                        }
                        operators::NOT_STRICTLY_FALSE => {
                            return match resolve(&call.args[0])? {
                                Value::Bool(b) => Ok(Value::Bool(b)),
                                _ => Ok(Value::Bool(true)),
                            }
                        }
                        _ => (),
                    }
                }

                match &call.target {
                    None => {
                        let Some(func) = ctx.get_function(call.func_name.as_str()) else {
                            return Err(ExecutionError::UndeclaredReference(
                                call.func_name.clone().into(),
                            ));
                        };
                        let mut ctx =
                          FunctionContext::new(&call.func_name, None, ctx, &call.args, resolver);
                        (func)(&mut ctx)
                    }
                    Some(target) => {
                        let qualified_func = if let Expr::Ident(prefix) = &target.expr {
                            ctx.get_qualified_function(prefix, call.func_name.as_str())
                        } else {
                            None
                        };
                        if let Some(func) = qualified_func {
                            let mut fctx = FunctionContext::new(
                                &call.func_name,
                                None,
                                ctx,
                                &call.args,
                                resolver,
                            );
                            return (func)(&mut fctx);
                        }
                        let tgt = Some(resolve(target)?);
                        let of = &match tgt {
                            Some(Value::Struct(ref ob)) => {
                                ob.resolve_function(call.func_name.as_str())
                            }
                            _ => None,
                        };
                        let Some(func) = of
                          .or(qualified_func)
                          .or_else(|| ctx.get_function(call.func_name.as_str()))
                        else {
                            return Err(ExecutionError::UndeclaredReference(
                                call.func_name.clone().into(),
                            ));
                        };
                        let mut fctx = FunctionContext::new(
                            &call.func_name,
                            tgt.clone(),
                            ctx,
                            &call.args,
                            resolver,
                        );
                        (func)(&mut fctx)
                    }
                }
            }
            Expr::Ident(name) => {
                if let Some(v) = resolver.resolve(name) {
                    return Ok(v);
                }
                Err(ExecutionError::UndeclaredReference(name.to_string().into()))
            }
            Expr::Select(select) => {
                let left_op = select.operand.deref();
                if !select.test {
                    if let Expr::Ident(name) = &left_op.expr {
                        if let Some(v) = resolver.resolve_member(name, &select.field) {
                            return Ok(v);
                        }
                    }
                }
                let left: Value<'a> = Value::resolve(left_op, ctx, resolver)?;
                if select.test {
                    match &left {
                        Value::Map(map) => {
                            for key in map.map.deref().keys() {
                                if key.to_string().eq(&select.field) {
                                    return Ok(Value::Bool(true));
                                }
                            }
                            Ok(Value::Bool(false))
                        }
                        _ => Ok(Value::Bool(false)),
                    }
                } else {
                    let res = left.member(&select.field);
                    res
                }
            }
            Expr::List(list_expr) => {
                let list = list_expr
                  .elements
                  .iter()
                  .enumerate()
                  .map(|(idx, element)| {
                      resolve(element).map(|value| {
                          if list_expr.optional_indices.contains(&idx) {
                              if let Ok(opt_val) = <&OptionalValue>::try_from(&value) {
                                  opt_val.value().cloned().map(|v| v.as_static())
                              } else {
                                  Some(value)
                              }
                          } else {
                              Some(value)
                          }
                      })
                  })
                  .filter_map(|r| r.transpose())
                  .collect::<Result<Arc<_>, _>>()?;
                Value::List(ListValue::PartiallyOwned(list)).into()
            }
            Expr::Map(map_expr) => {
                let mut map = hashbrown::HashMap::with_capacity(map_expr.entries.len());
                for entry in map_expr.entries.iter() {
                    let (k, v, is_optional) = match &entry.expr {
                        EntryExpr::StructField(_) => panic!("WAT?"),
                        EntryExpr::MapEntry(e) => (&e.key, &e.value, e.optional),
                    };
                    let key = resolve(k)?
                      .as_static()
                      .try_into()
                      .map_err(ExecutionError::UnsupportedKeyType)?;
                    let value = resolve(v)?.as_static();

                    if is_optional {
                        if let Ok(opt_val) = <&OptionalValue>::try_from(&value) {
                            if let Some(inner) = opt_val.value() {
                                map.insert(key, inner.clone());
                            }
                        } else {
                            map.insert(key, value);
                        }
                    } else {
                        map.insert(key, value);
                    }
                }
                Ok(Value::Map(Map {
                    map: Arc::from(map),
                }))
            }
            Expr::Comprehension(comprehension) => {
                let accu_init = resolve(&comprehension.accu_init)?;
                let iter = resolve(&comprehension.iter_range)?;
                let mut accu = accu_init;

                match iter {
                    Value::List(items) => {
                        for item in items.as_ref() {
                            let comp_resolver = SingleVarResolver::new(
                                resolver,
                                &comprehension.accu_var,
                                accu.clone(),
                            );
                            if !Value::resolve(&comprehension.loop_cond, ctx, &comp_resolver)?
                              .to_bool()?
                            {
                                break;
                            }
                            let with_iter = SingleVarResolver::new(
                                &comp_resolver,
                                &comprehension.iter_var,
                                item.clone(),
                            );
                            accu = Value::resolve(&comprehension.loop_step, ctx, &with_iter)?;
                        }
                    }
                    Value::Map(map) => {
                        for key in map.map.deref().keys() {
                            let comp_resolver = SingleVarResolver::new(
                                resolver,
                                &comprehension.accu_var,
                                accu.clone(),
                            );
                            if !Value::resolve(&comprehension.loop_cond, ctx, &comp_resolver)?
                              .to_bool()?
                            {
                                break;
                            }
                            let kv = Value::from(key);
                            let with_iter =
                              SingleVarResolver::new(&comp_resolver, &comprehension.iter_var, kv);
                            accu = Value::resolve(&comprehension.loop_step, ctx, &with_iter)?;
                        }
                    }
                    t => todo!("Support {t:?}"),
                }
                let comp_resolver = SingleVarResolver::new(resolver, &comprehension.accu_var, accu);
                Value::resolve(&comprehension.result, ctx, &comp_resolver)
            }
            //     let accu_init = Value::resolve(&comprehension.accu_init, ctx)?;
            //     let iter = Value::resolve(&comprehension.iter_range, ctx)?;
            //     let mut ctx = ctx.new_inner_scope();
            //     ctx.add_variable(&comprehension.accu_var, accu_init)
            //         .expect("Failed to add accu variable");
            //
            //     match iter {
            //         Value::List(items) => {
            //             for item in items.as_ref() {
            //                 if !Value::resolve(&comprehension.loop_cond, &ctx)?.to_bool()? {
            //                     break;
            //                 }
            //                 ctx.add_variable_from_value(&comprehension.iter_var, item.clone());
            //                 let accu = Value::resolve(&comprehension.loop_step, &ctx)?;
            //                 ctx.add_variable_from_value(&comprehension.accu_var, accu);
            //             }
            //         }
            //         // Value::Map(map) => {
            //         //     for key in map.map.deref().keys() {
            //         //         if !Value::resolve(&comprehension.loop_cond, &ctx)?.to_bool()? {
            //         //             break;
            //         //         }
            //         //         ctx.add_variable_from_value(&comprehension.iter_var, key.clone());
            //         //         let accu = Value::resolve(&comprehension.loop_step, &ctx)?;
            //         //         ctx.add_variable_from_value(&comprehension.accu_var, accu);
            //         //     }
            //         // }
            //         t => todo!("Support {t:?}"),
            //     }
            //     Value::resolve(&comprehension.result, &ctx)
            // }
            Expr::Struct(_) => todo!("Support structs!"),
            Expr::Unspecified => panic!("Can't evaluate Unspecified Expr"),
        }
    }

    // >> a(b)
    // Member(Ident("a"),
    //        FunctionCall([Ident("b")]))
    // >> a.b(c)
    // Member(Member(Ident("a"),
    //               Attribute("b")),
    //        FunctionCall([Ident("c")]))

    fn member(self, name: &str) -> ResolveResult<'a> {
        // This will always either be because we're trying to access
        // a property on self, or a method on self.
        let child = match self {
            Value::Map(m) => m.map.get(&KeyRef::String(name)).cloned(),
            Value::Struct(ov) => ov.get_member(name.as_ref()),
            Value::Opaque(ov) => match ov {
                OpaqueValue::Borrowed(ovr) => ovr.resolve_variable(name.as_ref()),
                OpaqueValue::Arc(ovr) => ovr.resolve_variable(name.as_ref()),
            },
            _ => None,
        };

        // If the property is both an attribute and a method, then we
        // give priority to the property. Maybe we can implement lookahead
        // to see if the next token is a function call?
        if let Some(child) = child {
            child.into()
        } else {
            ExecutionError::NoSuchKey(Arc::from(name)).into()
        }
    }

    #[inline(always)]
    fn to_bool(&self) -> Result<bool, ExecutionError> {
        match self {
            Value::Bool(v) => Ok(*v),
            _ => Err(ExecutionError::NoSuchOverload),
        }
    }
}

impl<'a> ops::Add<Value<'a>> for Value<'a> {
    type Output = ResolveResult<'a>;

    #[inline(always)]
    fn add(self, rhs: Value<'a>) -> Self::Output {
        match (self, rhs) {
            (Value::Int(l), Value::Int(r)) => l
              .checked_add(r)
              .ok_or(ExecutionError::Overflow("add", l.into(), r.into()))
              .map(Value::Int),

            (Value::UInt(l), Value::UInt(r)) => l
              .checked_add(r)
              .ok_or(ExecutionError::Overflow("add", l.into(), r.into()))
              .map(Value::UInt),

            (Value::Float(l), Value::Float(r)) => Value::Float(l + r).into(),

            (Value::List(l), Value::List(r)) => {
                let mut res = Vec::with_capacity(l.as_ref().len() + r.as_ref().len());
                res.extend_from_slice(l.as_ref());
                res.extend_from_slice(r.as_ref());
                Ok(Value::List(ListValue::PartiallyOwned(res.into())))
            }
            (Value::String(l), Value::String(r)) => {
                let mut res = String::with_capacity(l.as_ref().len() + r.as_ref().len());
                res.push_str(l.as_ref());
                res.push_str(r.as_ref());
                Ok(Value::String(res.into()))
            }
            #[cfg(feature = "chrono")]
            (Value::Duration(l), Value::Duration(r)) => l
              .checked_add(&r)
              .ok_or(ExecutionError::Overflow("add", l.into(), r.into()))
              .map(Value::Duration),
            #[cfg(feature = "chrono")]
            (Value::Timestamp(l), Value::Duration(r)) => checked_op(TsOp::Add, &l, &r),
            #[cfg(feature = "chrono")]
            (Value::Duration(l), Value::Timestamp(r)) => r
              .checked_add_signed(l)
              .ok_or(ExecutionError::Overflow("add", l.into(), r.into()))
              .map(Value::Timestamp),
            (left, right) => Err(ExecutionError::UnsupportedBinaryOperator(
                "add",
                left.as_static(),
                right.as_static(),
            )),
        }
    }
}

impl<'a> ops::Sub<Value<'a>> for Value<'a> {
    type Output = ResolveResult<'a>;

    #[inline(always)]
    fn sub(self, rhs: Value) -> Self::Output {
        match (self, rhs) {
            (Value::Int(l), Value::Int(r)) => l
              .checked_sub(r)
              .ok_or(ExecutionError::Overflow("sub", l.into(), r.into()))
              .map(Value::Int),

            (Value::UInt(l), Value::UInt(r)) => l
              .checked_sub(r)
              .ok_or(ExecutionError::Overflow("sub", l.into(), r.into()))
              .map(Value::UInt),

            (Value::Float(l), Value::Float(r)) => Value::Float(l - r).into(),

            #[cfg(feature = "chrono")]
            (Value::Duration(l), Value::Duration(r)) => l
              .checked_sub(&r)
              .ok_or(ExecutionError::Overflow("sub", l.into(), r.into()))
              .map(Value::Duration),
            #[cfg(feature = "chrono")]
            (Value::Timestamp(l), Value::Duration(r)) => checked_op(TsOp::Sub, &l, &r),
            #[cfg(feature = "chrono")]
            (Value::Timestamp(l), Value::Timestamp(r)) => {
                Value::Duration(l.signed_duration_since(r)).into()
            }
            (left, right) => Err(ExecutionError::UnsupportedBinaryOperator(
                "sub",
                left.as_static(),
                right.as_static(),
            )),
        }
    }
}

impl<'a> ops::Div<Value<'a>> for Value<'a> {
    type Output = ResolveResult<'a>;

    #[inline(always)]
    fn div(self, rhs: Value) -> Self::Output {
        match (self, rhs) {
            (Value::Int(l), Value::Int(r)) => {
                if r == 0 {
                    Err(ExecutionError::DivisionByZero(l.into()))
                } else {
                    l.checked_div(r)
                      .ok_or(ExecutionError::Overflow("div", l.into(), r.into()))
                      .map(Value::Int)
                }
            }

            (Value::UInt(l), Value::UInt(r)) => l
              .checked_div(r)
              .ok_or(ExecutionError::DivisionByZero(l.into()))
              .map(Value::UInt),

            (Value::Float(l), Value::Float(r)) => Value::Float(l / r).into(),

            (left, right) => Err(ExecutionError::UnsupportedBinaryOperator(
                "div",
                left.as_static(),
                right.as_static(),
            )),
        }
    }
}

impl<'a> ops::Mul<Value<'a>> for Value<'a> {
    type Output = ResolveResult<'a>;

    #[inline(always)]
    fn mul(self, rhs: Value) -> Self::Output {
        match (self, rhs) {
            (Value::Int(l), Value::Int(r)) => l
              .checked_mul(r)
              .ok_or(ExecutionError::Overflow("mul", l.into(), r.into()))
              .map(Value::Int),

            (Value::UInt(l), Value::UInt(r)) => l
              .checked_mul(r)
              .ok_or(ExecutionError::Overflow("mul", l.into(), r.into()))
              .map(Value::UInt),

            (Value::Float(l), Value::Float(r)) => Value::Float(l * r).into(),

            (left, right) => Err(ExecutionError::UnsupportedBinaryOperator(
                "mul",
                left.as_static(),
                right.as_static(),
            )),
        }
    }
}

impl<'a> ops::Rem<Value<'a>> for Value<'a> {
    type Output = ResolveResult<'a>;

    #[inline(always)]
    fn rem(self, rhs: Value) -> Self::Output {
        match (self, rhs) {
            (Value::Int(l), Value::Int(r)) => {
                if r == 0 {
                    Err(ExecutionError::RemainderByZero(l.into()))
                } else {
                    l.checked_rem(r)
                      .ok_or(ExecutionError::Overflow("rem", l.into(), r.into()))
                      .map(Value::Int)
                }
            }

            (Value::UInt(l), Value::UInt(r)) => l
              .checked_rem(r)
              .ok_or(ExecutionError::RemainderByZero(l.into()))
              .map(Value::UInt),

            (left, right) => Err(ExecutionError::UnsupportedBinaryOperator(
                "rem",
                left.as_static(),
                right.as_static(),
            )),
        }
    }
}

/// Op represents a binary arithmetic operation supported on a timestamp
///
#[cfg(feature = "chrono")]
enum TsOp {
    Add,
    Sub,
}

#[cfg(feature = "chrono")]
impl TsOp {
    fn str(&self) -> &'static str {
        match self {
            TsOp::Add => "add",
            TsOp::Sub => "sub",
        }
    }
}

/// Performs a checked arithmetic operation [`TsOp`] on a timestamp and a duration and ensures that
/// the resulting timestamp does not overflow the data type internal limits, as well as the timestamp
/// limits defined in the cel-spec. See [`MAX_TIMESTAMP`] and [`MIN_TIMESTAMP`] for more details.
#[cfg(feature = "chrono")]
fn checked_op<'a>(
    op: TsOp,
    lhs: &chrono::DateTime<chrono::FixedOffset>,
    rhs: &chrono::Duration,
) -> ResolveResult<'a> {
    // Add lhs and rhs together, checking for data type overflow
    let result = match op {
        TsOp::Add => lhs.checked_add_signed(*rhs),
        TsOp::Sub => lhs.checked_sub_signed(*rhs),
    }
      .ok_or(ExecutionError::Overflow(
          op.str(),
          (*lhs).into(),
          (*rhs).into(),
      ))?;

    // Check for cel-spec limits
    if result > *MAX_TIMESTAMP || result < *MIN_TIMESTAMP {
        Err(ExecutionError::Overflow(
            op.str(),
            (*lhs).into(),
            (*rhs).into(),
        ))
    } else {
        Value::Timestamp(result).into()
    }
}

#[cfg(test)]
mod tests {
    use crate::context::{MapResolver, VariableResolver};
    use crate::objects::{ListValue, StringValue, Value};
    use crate::parser::Expression;
    use crate::{objects::Key, Context, ExecutionError, Program};
    use std::collections::HashMap;
    use std::sync::Arc;

    #[test]
    fn test_indexed_map_access() {
        let mut headers = HashMap::new();
        headers.insert("Content-Type", "application/json".to_string());
        let mut vars = MapResolver::new();
        vars.add_variable_from_value("headers", headers);

        let program = Program::compile("headers[\"Content-Type\"]").unwrap();
        let ctx = Context::default();
        let value = program.execute_with(&ctx, &vars).unwrap();
        assert_eq!(value, "application/json".into());
    }

    #[test]
    fn test_numeric_map_access() {
        let mut numbers = HashMap::new();
        numbers.insert(Key::Uint(1), "one".to_string());
        let mut vars = MapResolver::new();
        vars.add_variable_from_value("numbers", numbers);

        let program = Program::compile("numbers[1]").unwrap();
        let ctx = Context::default();
        let value = program.execute_with(&ctx, &vars).unwrap();
        assert_eq!(value, "one".into());
    }

    #[test]
    fn test_heterogeneous_compare() {
        let context = Context::default();

        let program = Program::compile("1 < uint(2)").unwrap();
        let value = program.execute(&context).unwrap();
        assert_eq!(value, true.into());

        let program = Program::compile("1 < 1.1").unwrap();
        let value = program.execute(&context).unwrap();
        assert_eq!(value, true.into());

        let program = Program::compile("uint(0) > -10").unwrap();
        let value = program.execute(&context).unwrap();
        assert_eq!(
            value,
            true.into(),
            "negative signed ints should be less than uints"
        );
    }

    #[test]
    fn test_float_compare() {
        let context = Context::default();

        let program = Program::compile("1.0 > 0.0").unwrap();
        let value = program.execute(&context).unwrap();
        assert_eq!(value, true.into());

        let program = Program::compile("double('NaN') == double('NaN')").unwrap();
        let value = program.execute(&context).unwrap();
        assert_eq!(value, false.into(), "NaN should not equal itself");

        let program = Program::compile("1.0 > double('NaN')").unwrap();
        let result = program.execute(&context);
        assert!(
            result.is_err(),
            "NaN should not be comparable with inequality operators"
        );
    }

    #[test]
    fn test_invalid_compare() {
        let context = Context::default();

        let program = Program::compile("{} == []").unwrap();
        let value = program.execute(&context).unwrap();
        assert_eq!(value, false.into());
    }

    #[test]
    fn test_size_fn_var() {
        let program = Program::compile("size(requests) + size == 5").unwrap();
        let requests = vec![Value::Int(42), Value::Int(42)];
        let mut vars = MapResolver::new();
        vars.add_variable_from_value(
            "requests",
            Value::List(ListValue::PartiallyOwned(requests.into())),
        );
        vars.add_variable_from_value("size", Value::Int(3));
        let ctx = Context::default();
        assert_eq!(
            program.execute_with(&ctx, &vars).unwrap(),
            Value::Bool(true)
        );
    }

    fn test_execution_error(program: &str, expected: ExecutionError) {
        let program = Program::compile(program).unwrap();
        let ctx = Context::default();
        let result = program.execute(&ctx);
        assert_eq!(result.unwrap_err(), expected);
    }

    #[test]
    fn test_invalid_sub() {
        test_execution_error(
            "'foo' - 10",
            ExecutionError::UnsupportedBinaryOperator("sub", "foo".into(), Value::Int(10)),
        );
    }

    #[test]
    fn test_invalid_add() {
        test_execution_error(
            "'foo' + 10",
            ExecutionError::UnsupportedBinaryOperator("add", "foo".into(), Value::Int(10)),
        );
    }

    #[test]
    fn test_invalid_div() {
        test_execution_error(
            "'foo' / 10",
            ExecutionError::UnsupportedBinaryOperator("div", "foo".into(), Value::Int(10)),
        );
    }

    #[test]
    fn test_invalid_rem() {
        test_execution_error(
            "'foo' % 10",
            ExecutionError::UnsupportedBinaryOperator("rem", "foo".into(), Value::Int(10)),
        );
    }

    #[test]
    fn out_of_bound_list_access() {
        let program = Program::compile("list[10]").unwrap();
        let mut vars = MapResolver::new();
        vars.add_variable_from_value("list", Value::List(ListValue::Owned(vec![].into())));
        let ctx = Context::default();
        let result = program.execute_with(&ctx, &vars);
        assert_eq!(
            result,
            Err(ExecutionError::IndexOutOfBounds(Value::Int(10)))
        );
    }

    #[test]
    fn out_of_bound_list_access_negative() {
        let program = Program::compile("list[-1]").unwrap();
        let mut vars = MapResolver::new();
        vars.add_variable_from_value("list", Value::List(ListValue::Owned(vec![].into())));
        let ctx = Context::default();
        let result = program.execute_with(&ctx, &vars);
        assert_eq!(
            result,
            Err(ExecutionError::IndexOutOfBounds(Value::Int(-1)))
        );
    }

    #[test]
    fn list_access_uint() {
        let program = Program::compile("list[1u]").unwrap();
        let mut vars = MapResolver::new();
        vars.add_variable_from_value(
            "list",
            Value::List(ListValue::Owned(vec![1.into(), 2.into()].into())),
        );
        let ctx = Context::default();
        let result = program.execute_with(&ctx, &vars);
        assert_eq!(result, Ok(Value::Int(2.into())));
    }

    #[test]
    fn reference_to_value() {
        let test = "example".to_string();
        let direct: Value = test.as_str().into();
        assert_eq!(
            direct,
            Value::String(StringValue::Owned(Arc::from("example")))
        );

        let vec = vec![test.as_str()];
        let indirect: Value = vec.into();
        assert_eq!(
            indirect,
            Value::List(ListValue::Owned(
                vec![Value::String(StringValue::Owned(Arc::from("example")))].into()
            ))
        );
    }

    #[test]
    fn test_short_circuit_and() {
        let data: HashMap<String, String> = HashMap::new();
        let mut vars = MapResolver::new();
        vars.add_variable_from_value("data", data);

        let program = Program::compile("has(data.x) && data.x.startsWith(\"foo\")").unwrap();
        let ctx = Context::default();
        let value = program.execute_with(&ctx, &vars);
        println!("{value:?}");
        assert!(
            value.is_ok(),
            "The AND expression should support short-circuit evaluation."
        );
    }

    #[test]
    fn invalid_int_math() {
        use ExecutionError::*;

        let cases = [
            ("1 / 0", DivisionByZero(1.into())),
            ("1 % 0", RemainderByZero(1.into())),
            (
                &format!("{} + 1", i64::MAX),
                Overflow("add", i64::MAX.into(), 1.into()),
            ),
            (
                &format!("{} - 1", i64::MIN),
                Overflow("sub", i64::MIN.into(), 1.into()),
            ),
            (
                &format!("{} * 2", i64::MAX),
                Overflow("mul", i64::MAX.into(), 2.into()),
            ),
            (
                &format!("{} / -1", i64::MIN),
                Overflow("div", i64::MIN.into(), (-1).into()),
            ),
            (
                &format!("{} % -1", i64::MIN),
                Overflow("rem", i64::MIN.into(), (-1).into()),
            ),
        ];

        for (expr, err) in cases {
            test_execution_error(expr, err);
        }
    }

    #[test]
    fn invalid_uint_math() {
        use ExecutionError::*;

        let cases = [
            ("1u / 0u", DivisionByZero(1u64.into())),
            ("1u % 0u", RemainderByZero(1u64.into())),
            (
                &format!("{}u + 1u", u64::MAX),
                Overflow("add", u64::MAX.into(), 1u64.into()),
            ),
            ("0u - 1u", Overflow("sub", 0u64.into(), 1u64.into())),
            (
                &format!("{}u * 2u", u64::MAX),
                Overflow("mul", u64::MAX.into(), 2u64.into()),
            ),
        ];

        for (expr, err) in cases {
            test_execution_error(expr, err);
        }
    }

    struct CompositeResolver<'a, 'rf> {
        base: &'rf dyn VariableResolver<'a>,
        name: &'a str,
        val: Value<'a>,
    }

    impl<'a, 'rf> VariableResolver<'a> for CompositeResolver<'a, 'rf> {
        fn resolve(&self, expr: &str) -> Option<Value<'a>> {
            if expr == self.name {
                Some(self.val.clone())
            } else {
                self.base.resolve(expr)
            }
        }
    }

    #[test]
    fn test_function_identifier() {
        fn with<'a, 'rf, 'b>(
            ftx: &'b mut crate::FunctionContext<'a, 'rf>,
        ) -> crate::ResolveResult<'a> {
            let this = ftx.this.as_ref().unwrap();
            let ident = ftx.ident(0)?;
            let expr: &'a Expression = ftx.expr(1)?;
            let x: &'rf dyn VariableResolver<'a> = ftx.vars();
            let resolver = CompositeResolver::<'a, 'rf> {
                base: x,
                name: ident,
                val: this.clone(),
            };
            let v = Value::resolve(expr, ftx.ptx, &resolver)?;
            Ok(v.as_static())
        }
        let mut context = Context::default();
        context.add_function("with", with);

        let program = Program::compile("[1,2].with(a, a + a)").unwrap();
        let value = program.execute(&context);
        assert_eq!(
            value,
            Ok(Value::List(ListValue::Owned(
                vec![Value::Int(1), Value::Int(2), Value::Int(1), Value::Int(2)].into()
            )))
        );
    }

    #[test]
    fn test_index_missing_map_key() {
        let ctx = Context::default();
        let mut map = HashMap::new();
        let mut vars = MapResolver::new();
        map.insert("a".to_string(), Value::Int(1));
        vars.add_variable_from_value("mymap", map);

        let p = Program::compile(r#"mymap["missing"]"#).expect("Must compile");
        let result = p.execute_with(&ctx, &vars);

        assert!(result.is_err(), "Should error on missing map key");
    }

    mod opaque {
        use crate::context::MapResolver;
        use crate::objects::{ListValue, Map, Opaque, OpaqueValue, OptionalValue, StringValue};
        use crate::parser::Parser;
        use crate::{Context, ExecutionError, FunctionContext, Program, Value};
        use serde::Serialize;
        use std::collections::HashMap;
        use std::fmt::Debug;
        use std::sync::Arc;

        #[derive(Debug, Eq, PartialEq, Serialize)]
        struct MyStruct {
            field: String,
        }

        impl Opaque for MyStruct {
            fn runtime_type_name(&self) -> &str {
                "my_struct"
            }

            #[cfg(feature = "json")]
            fn json(&self) -> Option<serde_json::Value> {
                Some(serde_json::to_value(self).unwrap())
            }
        }

        // #[derive(Debug, Eq, PartialEq, Serialize)]
        // struct Reference<'a> {
        //     field: &'a str,
        // }
        //
        // impl<'a> Opaque for Reference<'a> {
        //     fn runtime_type_name(&self) -> &str {
        //         "reference"
        //     }
        //
        //     #[cfg(feature = "json")]
        //     fn json(&self) -> Option<serde_json::Value> {
        //         Some(serde_json::to_value(self).unwrap())
        //     }
        // }

        #[test]
        fn test_opaque_fn() {
            pub fn my_fn<'a>(
                ftx: &mut FunctionContext<'a, '_>,
            ) -> Result<Value<'a>, ExecutionError> {
                if let Some(Value::Opaque(opaque)) = &ftx.this {
                    if opaque.as_ref().runtime_type_name() == "my_struct" {
                        Ok(opaque
                          .as_ref()
                          .downcast_ref::<MyStruct>()
                          .unwrap()
                          .field
                          .clone()
                          .into())
                    } else {
                        Err(ExecutionError::UnexpectedType {
                            got: opaque.as_ref().runtime_type_name().to_string(),
                            want: "my_struct".to_string(),
                        })
                    }
                } else {
                    Err(ExecutionError::UnexpectedType {
                        got: format!("{:?}", ftx.this),
                        want: "Value::Opaque".to_string(),
                    })
                }
            }

            let value = Arc::new(MyStruct {
                field: String::from("value"),
            });

            let mut vars = MapResolver::new();
            vars.add_variable_from_value("mine", Value::Opaque(OpaqueValue::Arc(value.clone())));
            let mut ctx = Context::default();
            ctx.add_function("myFn", my_fn);
            let prog = Program::compile("mine.myFn()").unwrap();
            assert_eq!(
                Ok(Value::String(StringValue::Owned(Arc::from("value")))),
                prog.execute_with(&ctx, &vars)
            );
        }

        #[test]
        fn opaque_eq() {
            let value_1 = Arc::new(MyStruct {
                field: String::from("1"),
            });
            let value_2 = Arc::new(MyStruct {
                field: String::from("2"),
            });

            let mut vars = MapResolver::new();
            vars.add_variable_from_value("v1", Value::Opaque(value_1.clone().into()));
            vars.add_variable_from_value("v1b", Value::Opaque(value_1.into()));
            vars.add_variable_from_value("v2", Value::Opaque(value_2.into()));
            let ctx = Context::default();
            assert_eq!(
                Program::compile("v2 == v1")
                  .unwrap()
                  .execute_with(&ctx, &vars),
                Ok(false.into())
            );
            assert_eq!(
                Program::compile("v1 == v1b")
                  .unwrap()
                  .execute_with(&ctx, &vars),
                Ok(true.into())
            );
            assert_eq!(
                Program::compile("v2 == v2")
                  .unwrap()
                  .execute_with(&ctx, &vars),
                Ok(true.into())
            );
        }

        #[test]
        fn test_value_holder_dbg() {
            let opaque = Arc::new(MyStruct {
                field: "not so opaque".to_string(),
            });
            let opaque = Value::Opaque(opaque.into());
            assert_eq!(
                "Opaque<my_struct>(MyStruct { field: \"not so opaque\" })",
                format!("{:?}", opaque)
            );
        }

        #[test]
        #[cfg(feature = "json")]
        fn test_json() {
            let value = Arc::new(MyStruct {
                field: String::from("value"),
            });
            let cel_value = Value::Opaque(value.into());
            let mut map = serde_json::Map::new();
            map.insert(
                "field".to_string(),
                serde_json::Value::String("value".to_string()),
            );
            assert_eq!(
                cel_value.json().expect("Must convert"),
                serde_json::Value::Object(map)
            );
        }

        #[test]
        fn test_optional() {
            let ctx = Context::default();
            let empty_vars = MapResolver::new();
            let expr = Parser::default()
              .enable_optional_syntax(true)
              .parse("optional.none()")
              .expect("Must parse");
            assert_eq!(
                Value::resolve(&expr, &ctx, &empty_vars),
                Ok(Value::Opaque(Arc::new(OptionalValue::none()).into()))
            );

            let expr = Parser::default()
              .enable_optional_syntax(true)
              .parse("optional.of(1)")
              .expect("Must parse");
            assert_eq!(
                Value::resolve(&expr, &ctx, &empty_vars),
                Ok(Value::Opaque(
                    Arc::new(OptionalValue::of(Value::Int(1))).into()
                ))
            );

            let expr = Parser::default()
              .enable_optional_syntax(true)
              .parse("optional.ofNonZeroValue(0)")
              .expect("Must parse");
            assert_eq!(
                Value::resolve(&expr, &ctx, &empty_vars),
                Ok(Value::Opaque(Arc::new(OptionalValue::none()).into()))
            );

            let expr = Parser::default()
              .enable_optional_syntax(true)
              .parse("optional.ofNonZeroValue(1)")
              .expect("Must parse");
            assert_eq!(
                Value::resolve(&expr, &ctx, &empty_vars),
                Ok(Value::Opaque(
                    Arc::new(OptionalValue::of(Value::Int(1))).into()
                ))
            );

            let expr = Parser::default()
              .enable_optional_syntax(true)
              .parse("optional.of(1).value()")
              .expect("Must parse");
            assert_eq!(Value::resolve(&expr, &ctx, &empty_vars), Ok(Value::Int(1)));
            let expr = Parser::default()
              .enable_optional_syntax(true)
              .parse("optional.none().value()")
              .expect("Must parse");
            assert_eq!(
                Value::resolve(&expr, &ctx, &empty_vars),
                Err(ExecutionError::FunctionError {
                    function: "value".to_string(),
                    message: "optional.none() dereference".to_string()
                })
            );

            let expr = Parser::default()
              .enable_optional_syntax(true)
              .parse("optional.of(1).hasValue()")
              .expect("Must parse");
            assert_eq!(
                Value::resolve(&expr, &ctx, &empty_vars),
                Ok(Value::Bool(true))
            );
            let expr = Parser::default()
              .enable_optional_syntax(true)
              .parse("optional.none().hasValue()")
              .expect("Must parse");
            assert_eq!(
                Value::resolve(&expr, &ctx, &empty_vars),
                Ok(Value::Bool(false))
            );

            let expr = Parser::default()
              .enable_optional_syntax(true)
              .parse("optional.of(1).or(optional.of(2))")
              .expect("Must parse");
            assert_eq!(
                Value::resolve(&expr, &ctx, &empty_vars),
                Ok(Value::Opaque(
                    Arc::new(OptionalValue::of(Value::Int(1))).into()
                ))
            );
            let expr = Parser::default()
              .enable_optional_syntax(true)
              .parse("optional.none().or(optional.of(2))")
              .expect("Must parse");
            assert_eq!(
                Value::resolve(&expr, &ctx, &empty_vars),
                Ok(Value::Opaque(
                    Arc::new(OptionalValue::of(Value::Int(2))).into()
                ))
            );
            let expr = Parser::default()
              .enable_optional_syntax(true)
              .parse("optional.none().or(optional.none())")
              .expect("Must parse");
            assert_eq!(
                Value::resolve(&expr, &ctx, &empty_vars),
                Ok(Value::Opaque(Arc::new(OptionalValue::none()).into()))
            );

            let expr = Parser::default()
              .enable_optional_syntax(true)
              .parse("optional.of(1).orValue(5)")
              .expect("Must parse");
            assert_eq!(Value::resolve(&expr, &ctx, &empty_vars), Ok(Value::Int(1)));
            let expr = Parser::default()
              .enable_optional_syntax(true)
              .parse("optional.none().orValue(5)")
              .expect("Must parse");
            assert_eq!(Value::resolve(&expr, &ctx, &empty_vars), Ok(Value::Int(5)));

            let mut msg_vars = MapResolver::new();
            msg_vars.add_variable_from_value("msg", HashMap::from([("field", "value")]));

            let expr = Parser::default()
              .enable_optional_syntax(true)
              .parse("msg.?field")
              .expect("Must parse");
            assert_eq!(
                Value::resolve(&expr, &ctx, &msg_vars),
                Ok(Value::Opaque(
                    Arc::new(OptionalValue::of(Value::String(StringValue::Owned(
                        Arc::from("value")
                    ))))
                      .into()
                ))
            );

            let expr = Parser::default()
              .enable_optional_syntax(true)
              .parse("optional.of(msg).?field")
              .expect("Must parse");
            assert_eq!(
                Value::resolve(&expr, &ctx, &msg_vars),
                Ok(Value::Opaque(
                    Arc::new(OptionalValue::of(Value::String(StringValue::Owned(
                        Arc::from("value")
                    ))))
                      .into()
                ))
            );

            let expr = Parser::default()
              .enable_optional_syntax(true)
              .parse("optional.none().?field")
              .expect("Must parse");
            assert_eq!(
                Value::resolve(&expr, &ctx, &msg_vars),
                Ok(Value::Opaque(Arc::new(OptionalValue::none()).into()))
            );

            let expr = Parser::default()
              .enable_optional_syntax(true)
              .parse("optional.of(msg).?field.orValue('default')")
              .expect("Must parse");
            assert_eq!(
                Value::resolve(&expr, &ctx, &msg_vars),
                Ok(Value::String(StringValue::Owned(Arc::from("value"))))
            );

            let expr = Parser::default()
              .enable_optional_syntax(true)
              .parse("optional.none().?field.orValue('default')")
              .expect("Must parse");
            assert_eq!(
                Value::resolve(&expr, &ctx, &msg_vars),
                Ok(Value::String(StringValue::Owned(Arc::from("default"))))
            );

            let mut map_vars = MapResolver::new();
            let mut map = HashMap::new();
            map.insert("a".to_string(), Value::Int(1));
            map_vars.add_variable_from_value("mymap", map);

            let expr = Parser::default()
              .enable_optional_syntax(true)
              .parse(r#"mymap[?"missing"].orValue(99)"#)
              .expect("Must parse");
            assert_eq!(Value::resolve(&expr, &ctx, &map_vars), Ok(Value::Int(99)));

            let expr = Parser::default()
              .enable_optional_syntax(true)
              .parse(r#"mymap[?"missing"].hasValue()"#)
              .expect("Must parse");
            assert_eq!(
                Value::resolve(&expr, &ctx, &map_vars),
                Ok(Value::Bool(false))
            );

            let expr = Parser::default()
              .enable_optional_syntax(true)
              .parse(r#"mymap[?"a"].orValue(99)"#)
              .expect("Must parse");
            assert_eq!(Value::resolve(&expr, &ctx, &map_vars), Ok(Value::Int(1)));

            let expr = Parser::default()
              .enable_optional_syntax(true)
              .parse(r#"mymap[?"a"].hasValue()"#)
              .expect("Must parse");
            assert_eq!(
                Value::resolve(&expr, &ctx, &map_vars),
                Ok(Value::Bool(true))
            );

            let mut list_vars = MapResolver::new();
            list_vars.add_variable_from_value(
                "mylist",
                vec![Value::Int(1), Value::Int(2), Value::Int(3)],
            );

            let expr = Parser::default()
              .enable_optional_syntax(true)
              .parse("mylist[?10].orValue(99)")
              .expect("Must parse");
            assert_eq!(Value::resolve(&expr, &ctx, &list_vars), Ok(Value::Int(99)));

            let expr = Parser::default()
              .enable_optional_syntax(true)
              .parse("mylist[?1].orValue(99)")
              .expect("Must parse");
            assert_eq!(Value::resolve(&expr, &ctx, &list_vars), Ok(Value::Int(2)));

            let expr = Parser::default()
              .enable_optional_syntax(true)
              .parse("optional.of([1, 2, 3])[1].orValue(99)")
              .expect("Must parse");
            assert_eq!(Value::resolve(&expr, &ctx, &empty_vars), Ok(Value::Int(2)));

            let expr = Parser::default()
              .enable_optional_syntax(true)
              .parse("optional.of([1, 2, 3])[4].orValue(99)")
              .expect("Must parse");
            assert_eq!(Value::resolve(&expr, &ctx, &empty_vars), Ok(Value::Int(99)));

            let expr = Parser::default()
              .enable_optional_syntax(true)
              .parse("optional.none()[1].orValue(99)")
              .expect("Must parse");
            assert_eq!(Value::resolve(&expr, &ctx, &empty_vars), Ok(Value::Int(99)));

            let expr = Parser::default()
              .enable_optional_syntax(true)
              .parse("optional.of([1, 2, 3])[?1].orValue(99)")
              .expect("Must parse");
            assert_eq!(Value::resolve(&expr, &ctx, &empty_vars), Ok(Value::Int(2)));

            let expr = Parser::default()
              .enable_optional_syntax(true)
              .parse("[1, 2, ?optional.of(3), 4]")
              .expect("Must parse");
            assert_eq!(
                Value::resolve(&expr, &ctx, &empty_vars),
                Ok(Value::List(ListValue::Owned(
                    vec![Value::Int(1), Value::Int(2), Value::Int(3), Value::Int(4)].into(),
                )))
            );

            let expr = Parser::default()
              .enable_optional_syntax(true)
              .parse("[1, 2, ?optional.none(), 4]")
              .expect("Must parse");
            assert_eq!(
                Value::resolve(&expr, &ctx, &empty_vars),
                Ok(Value::List(ListValue::Owned(
                    vec![Value::Int(1), Value::Int(2), Value::Int(4)].into(),
                )))
            );

            let expr = Parser::default()
              .enable_optional_syntax(true)
              .parse("[?optional.of(1), ?optional.none(), ?optional.of(3)]")
              .expect("Must parse");
            assert_eq!(
                Value::resolve(&expr, &ctx, &empty_vars),
                Ok(Value::List(ListValue::Owned(
                    vec![Value::Int(1), Value::Int(3)].into(),
                )))
            );

            let expr = Parser::default()
              .enable_optional_syntax(true)
              .parse(r#"[1, ?mymap[?"missing"], 3]"#)
              .expect("Must parse");
            assert_eq!(
                Value::resolve(&expr, &ctx, &map_vars),
                Ok(Value::List(ListValue::Owned(
                    vec![Value::Int(1), Value::Int(3)].into(),
                )))
            );

            let expr = Parser::default()
              .enable_optional_syntax(true)
              .parse(r#"[1, ?mymap[?"a"], 3]"#)
              .expect("Must parse");
            assert_eq!(
                Value::resolve(&expr, &ctx, &map_vars),
                Ok(Value::List(ListValue::Owned(
                    vec![Value::Int(1), Value::Int(1), Value::Int(3)].into(),
                )))
            );

            let expr = Parser::default()
              .enable_optional_syntax(true)
              .parse("[?optional.none(), ?optional.none()]")
              .expect("Must parse");
            assert_eq!(
                Value::resolve(&expr, &ctx, &empty_vars),
                Ok(Value::List(ListValue::Owned(vec![].into())))
            );

            let expr = Parser::default()
              .enable_optional_syntax(true)
              .parse(r#"{"a": 1, "b": 2, ?"c": optional.of(3)}"#)
              .expect("Must parse");
            let mut expected_map = hashbrown::HashMap::new();
            expected_map.insert("a".into(), Value::Int(1));
            expected_map.insert("b".into(), Value::Int(2));
            expected_map.insert("c".into(), Value::Int(3));
            assert_eq!(
                Value::resolve(&expr, &ctx, &empty_vars),
                Ok(Value::Map(Map {
                    map: Arc::from(expected_map)
                }))
            );

            let expr = Parser::default()
              .enable_optional_syntax(true)
              .parse(r#"{"a": 1, "b": 2, ?"c": optional.none()}"#)
              .expect("Must parse");
            let mut expected_map = hashbrown::HashMap::new();
            expected_map.insert("a".into(), Value::Int(1));
            expected_map.insert("b".into(), Value::Int(2));
            assert_eq!(
                Value::resolve(&expr, &ctx, &empty_vars),
                Ok(Value::Map(Map {
                    map: Arc::from(expected_map)
                }))
            );

            let expr = Parser::default()
              .enable_optional_syntax(true)
              .parse(r#"{"a": 1, ?"b": optional.none(), ?"c": optional.of(3)}"#)
              .expect("Must parse");
            let mut expected_map = hashbrown::HashMap::new();
            expected_map.insert("a".into(), Value::Int(1));
            expected_map.insert("c".into(), Value::Int(3));
            assert_eq!(
                Value::resolve(&expr, &ctx, &empty_vars),
                Ok(Value::Map(Map {
                    map: Arc::from(expected_map)
                }))
            );

            let expr = Parser::default()
              .enable_optional_syntax(true)
              .parse(r#"{"a": 1, ?"b": mymap[?"missing"]}"#)
              .expect("Must parse");
            let mut expected_map = hashbrown::HashMap::new();
            expected_map.insert("a".into(), Value::Int(1));
            assert_eq!(
                Value::resolve(&expr, &ctx, &map_vars),
                Ok(Value::Map(Map {
                    map: Arc::from(expected_map)
                }))
            );

            let expr = Parser::default()
              .enable_optional_syntax(true)
              .parse(r#"{"x": 10, ?"y": mymap[?"a"]}"#)
              .expect("Must parse");
            let mut expected_map = hashbrown::HashMap::new();
            expected_map.insert("x".into(), Value::Int(10));
            expected_map.insert("y".into(), Value::Int(1));
            assert_eq!(
                Value::resolve(&expr, &ctx, &map_vars),
                Ok(Value::Map(Map {
                    map: Arc::from(expected_map)
                }))
            );

            let expr = Parser::default()
              .enable_optional_syntax(true)
              .parse(r#"{?"a": optional.none(), ?"b": optional.none()}"#)
              .expect("Must parse");
            assert_eq!(
                Value::resolve(&expr, &ctx, &empty_vars),
                Ok(Value::Map(Map {
                    map: Arc::from(hashbrown::HashMap::new())
                }))
            );
        }
    }
}
