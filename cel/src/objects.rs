use std::borrow::Cow;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::convert::{Infallible, TryFrom, TryInto};
use std::fmt::{Debug, Display, Formatter};
use std::hash::{Hash, Hasher};
use std::ops::Deref;
use std::sync::{Arc, LazyLock};
use std::{ops, slice};

use bytes::Bytes;
#[cfg(feature = "chrono")]
use chrono::TimeZone;
use hashbrown::Equivalent;

use crate::common::ast::{operators, EntryExpr, Expr};
use crate::common::value::CelVal;
use crate::context::{Context, SingleVarResolver, VariableResolver};
use crate::functions::FunctionContext;
pub use crate::types::object::ObjectValue;
use crate::{ExecutionError, Expression};

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
pub enum MapValue<'a> {
    Owned(Arc<hashbrown::HashMap<Key, Value<'static>>>),
    Borrow(vector_map::VecMap<KeyRef<'a>, Value<'a>>),
}

impl PartialOrd for MapValue<'_> {
    fn partial_cmp(&self, _: &Self) -> Option<Ordering> {
        None
    }
}

impl<'a> MapValue<'a> {
    pub fn iter_keys(&self) -> impl Iterator<Item = KeyRef<'a>> + use<'a, '_> {
        use itertools::Either;
        match self {
            MapValue::Owned(m) => Either::Left(m.keys().map(|k| KeyRef::from(k.clone()))),
            MapValue::Borrow(m) => Either::Right(m.keys().cloned()),
        }
    }
    pub fn iter(&'a self) -> impl Iterator<Item = (KeyRef<'a>, &'a Value<'a>)> {
        use itertools::Either;
        match self {
            MapValue::Owned(m) => Either::Left(m.iter().map(|(k, v)| (KeyRef::from(k), v))),
            MapValue::Borrow(m) => Either::Right(m.iter().map(|(k, v)| (k.clone(), v))),
        }
    }
    pub fn iter_owned(&'a self) -> impl Iterator<Item = (Key, Value<'static>)> + use<'a> {
        use itertools::Either;
        match self {
            MapValue::Owned(m) => Either::Left(m.iter().map(|(k, v)| (k.clone(), v.clone()))),
            MapValue::Borrow(m) => {
                Either::Right(m.iter().map(|(k, v)| (Key::from(k), v.as_static())))
            }
        }
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn len(&self) -> usize {
        match self {
            MapValue::Owned(m) => m.len(),
            MapValue::Borrow(m) => m.len(),
        }
    }
    pub fn contains_key(&self, key: &KeyRef) -> bool {
        match self {
            MapValue::Owned(m) => m.contains_key(key),
            MapValue::Borrow(m) => m.contains_key(key),
        }
    }
    fn get_raw<'r>(&'r self, key: &KeyRef<'r>) -> Option<&'r Value<'a>> {
        match self {
            MapValue::Owned(m) => m.get(key),
            MapValue::Borrow(m) => m.get(key),
        }
    }
    /// Returns a reference to the value corresponding to the key. Implicitly converts between int
    /// and uint keys.
    pub fn get<'r>(&'r self, key: &KeyRef<'r>) -> Option<&'r Value<'a>> {
        self.get_raw(key).or_else(|| match key {
            KeyRef::Int(k) => {
                let converted = u64::try_from(*k).ok()?;
                self.get_raw(&KeyRef::Uint(converted))
            }
            KeyRef::Uint(k) => {
                let converted = i64::try_from(*k).ok()?;
                self.get_raw(&KeyRef::Int(converted))
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

impl<'a> PartialEq<KeyRef<'a>> for Key {
    fn eq(&self, key: &KeyRef) -> bool {
        &KeyRef::from(self) == key
    }
}

/// A borrowed version of [`Key`] that avoids allocating for lookups.
#[derive(Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub enum KeyRef<'a> {
    Int(i64),
    Uint(u64),
    Bool(bool),
    String(StringValue<'a>),
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

impl<'a> From<&KeyRef<'a>> for Key {
    fn from(value: &KeyRef<'a>) -> Self {
        match value {
            KeyRef::Int(v) => Key::Int(*v),
            KeyRef::Uint(v) => Key::Uint(*v),
            KeyRef::Bool(v) => Key::Bool(*v),
            KeyRef::String(v) => Key::String(v.as_owned()),
        }
    }
}
impl<'a> From<KeyRef<'a>> for Value<'a> {
    fn from(value: KeyRef<'a>) -> Self {
        match value {
            KeyRef::Int(v) => Value::Int(v),
            KeyRef::Uint(v) => Value::UInt(v),
            KeyRef::Bool(v) => Value::Bool(v),
            KeyRef::String(v) => Value::String(v),
        }
    }
}

impl Display for KeyRef<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            KeyRef::Int(v) => write!(f, "{v}"),
            KeyRef::Uint(v) => write!(f, "{v}"),
            KeyRef::Bool(v) => write!(f, "{v}"),
            KeyRef::String(v) => f.write_str(v.as_ref()),
        }
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
            Key::String(v) => KeyRef::String(StringValue::Borrowed(v.as_ref())),
            Key::Bool(v) => KeyRef::Bool(*v),
        }
    }
}
impl From<Key> for KeyRef<'static> {
    fn from(value: Key) -> Self {
        match value {
            Key::Int(v) => KeyRef::Int(v),
            Key::Uint(v) => KeyRef::Uint(v),
            Key::String(v) => KeyRef::String(StringValue::Owned(v.clone())),
            Key::Bool(v) => KeyRef::Bool(v),
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
            Value::String(v) => Ok(KeyRef::String(v.as_ref().into())),
            Value::Bool(v) => Ok(KeyRef::Bool(*v)),
            _ => Err(value.clone()),
        }
    }
}

impl<K: Into<Key>, V: Into<Value<'static>>> From<HashMap<K, V>> for MapValue<'static> {
    fn from(map: HashMap<K, V>) -> Self {
        let mut new_map = hashbrown::HashMap::with_capacity(map.len());
        for (k, v) in map {
            new_map.insert(k.into(), v.into());
        }
        MapValue::Owned(Arc::new(new_map))
    }
}
impl<K: Into<Key>, V: Into<Value<'static>>> From<hashbrown::HashMap<K, V>> for MapValue<'static> {
    fn from(map: hashbrown::HashMap<K, V>) -> Self {
        let mut new_map = hashbrown::HashMap::with_capacity(map.len());
        for (k, v) in map {
            new_map.insert(k.into(), v.into());
        }
        MapValue::Owned(Arc::new(new_map))
    }
}

use crate::magic::Function;

/// Trait for user-defined object values stored inside [`Value::Object`].
///
/// Implement this trait for types that should participate in CEL evaluation as
/// user-defined values. An object value:
/// - must report a stable type name via [`type_name`];
/// - can expose members via [`get_member`];
/// - can expose methods via [`resolve_function`];
/// - must be thread-safe (`Send + Sync`).
///
/// When the `json` feature is enabled you may optionally provide a JSON
/// representation for diagnostics, logging or interop. Returning `None` keeps the
/// value non-serializable for JSON.
///
/// Example
/// ```rust
/// use std::fmt::{Debug, Formatter, Result as FmtResult};
/// use cel::objects::{ObjectType, ObjectValue, Value};
///
/// #[derive(Clone, Debug, Eq, PartialEq)]
/// struct MyId(u64);
///
/// impl ObjectType<'static> for MyId {
///     fn type_name(&self) -> &'static str { "example.MyId" }
/// }
///
/// // Values of `MyId` can now be wrapped in `Value::Object` and compared.
/// let a: Value = ObjectValue::new(MyId(7)).into();
/// let b: Value = ObjectValue::new(MyId(7)).into();
/// assert_eq!(a, b);
/// ```
pub trait ObjectType<'a>: std::fmt::Debug + Send + Sync + 'a {
    /// Returns a stable, fully-qualified type name for this value's runtime type.
    ///
    /// This name is used to check type compatibility before attempting downcasts
    /// during equality checks and other operations. It should be stable across
    /// versions and unique within your application or library (e.g., a package
    /// qualified name like `my.pkg.Type`).
    #[inline]
    fn type_name(&self) -> &'static str {
        std::any::type_name::<Self>()
    }

    /// Returns the value of a member/field/variable by name.
    fn get_member(&self, _name: &str) -> Option<Value<'a>> {
        None
    }

    /// Resolves a method function by name.
    fn resolve_function(&self, _name: &str) -> Option<&Function> {
        None
    }

    /// Optional JSON representation (requires the `json` feature).
    ///
    /// The default implementation returns `None`, indicating that the value
    /// cannot be represented as JSON.
    #[cfg(feature = "json")]
    fn json(&self) -> Option<serde_json::Value> {
        None
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OptionalValue {
    value: Option<Value<'static>>,
}
crate::register_type!(OptionalValue);

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

impl ObjectType<'static> for OptionalValue {
    fn type_name(&self) -> &'static str {
        "optional_type"
    }
}

impl<'a> TryFrom<Value<'a>> for OptionalValue {
    type Error = ExecutionError;

    fn try_from(value: Value<'a>) -> Result<Self, Self::Error> {
        match value {
            Value::Object(obj) if obj.type_name() == "optional_type" => obj
                .downcast_ref::<OptionalValue>()
                .ok_or_else(|| ExecutionError::function_error("optional", "failed to downcast"))
                .cloned(),
            Value::Object(obj) => Err(ExecutionError::UnexpectedType {
                got: obj.type_name(),
                want: "optional_type",
            }),
            v => Err(ExecutionError::UnexpectedType {
                got: v.type_of().as_str(),
                want: "optional_type",
            }),
        }
    }
}

impl<'a, 'b: 'a> TryFrom<&'b Value<'a>> for &'b OptionalValue {
    type Error = ExecutionError;

    fn try_from(value: &'b Value<'a>) -> Result<Self, Self::Error> {
        match value {
            Value::Object(obj) if obj.type_name() == "optional_type" => obj
                .downcast_ref::<OptionalValue>()
                .ok_or_else(|| ExecutionError::function_error("optional", "failed to downcast")),
            Value::Object(obj) => Err(ExecutionError::UnexpectedType {
                got: obj.type_name(),
                want: "optional_type",
            }),
            v => Err(ExecutionError::UnexpectedType {
                got: v.type_of().as_str(),
                want: "optional_type",
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

#[derive(Clone, Debug, Ord, PartialOrd)]
pub enum StringValue<'a> {
    Borrowed(&'a str),
    Owned(Arc<str>),
}
impl Eq for StringValue<'_> {}
impl PartialEq for StringValue<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.as_ref() == other.as_ref()
    }
}
impl Hash for StringValue<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // Hash only the string content, ignoring Borrowed vs Owned
        self.as_ref().hash(state);
    }
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

#[derive(Debug)]
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
    Map(MapValue<'a>),

    // Atoms
    Int(i64),
    UInt(u64),
    Float(f64),
    Bool(bool),
    #[cfg(feature = "chrono")]
    Duration(chrono::Duration),
    #[cfg(feature = "chrono")]
    Timestamp(chrono::DateTime<chrono::FixedOffset>),

    /// User-defined object values implementing [`ObjectType`].
    Object(ObjectValue<'a>),

    String(StringValue<'a>),
    Bytes(BytesValue<'a>),

    Null,
}

impl Value<'_> {
    pub fn as_unsigned(&self) -> Result<usize, ExecutionError> {
        match self {
            Value::Int(i) => usize::try_from(*i)
                .map_err(|_e| ExecutionError::Conversion("usize", self.as_static())),
            Value::UInt(u) => usize::try_from(*u)
                .map_err(|_e| ExecutionError::Conversion("usize", self.as_static())),
            _ => Err(ExecutionError::Conversion("usize", self.as_static())),
        }
    }
    pub fn as_signed(&self) -> Result<i64, ExecutionError> {
        match self {
            Value::Int(i) => Ok(*i),
            Value::UInt(u) => {
                i64::try_from(*u).map_err(|_e| ExecutionError::Conversion("i64", self.as_static()))
            }
            _ => Err(ExecutionError::Conversion("i64", self.as_static())),
        }
    }
    pub fn as_bool(&self) -> Result<bool, ExecutionError> {
        match self {
            Value::Bool(b) => Ok(*b),
            _ => Err(ExecutionError::Conversion("bool", self.as_static())),
        }
    }
    pub fn as_bytes(&self) -> Result<&[u8], ExecutionError> {
        match self {
            Value::String(b) => Ok(b.as_ref().as_bytes()),
            Value::Bytes(b) => Ok(b.as_ref()),
            _ => Err(ExecutionError::Conversion("bytes", self.as_static())),
        }
    }
    // Note: may allocate
    pub fn as_str(&self) -> Result<Cow<'_, str>, ExecutionError> {
        match self {
            Value::String(v) => Ok(Cow::Borrowed(v.as_ref())),
            Value::Bool(v) => {
                if *v {
                    Ok(Cow::Borrowed("true"))
                } else {
                    Ok(Cow::Borrowed("false"))
                }
            }
            Value::Int(v) => Ok(Cow::Owned(v.to_string())),
            Value::UInt(v) => Ok(Cow::Owned(v.to_string())),
            Value::Bytes(v) => {
                use base64::Engine;
                Ok(Cow::Owned(
                    base64::prelude::BASE64_STANDARD.encode(v.as_ref()),
                ))
            }
            _ => Err(ExecutionError::Conversion("string", self.as_static())),
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
            (Value::Object(a), Value::Object(b)) => a.eq(b),
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
            Value::Object(obj) => write!(f, "Object<{}>({:?})", obj.type_name(), obj),
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
            Value::Object(obj) => Value::Object(obj.clone()),
            #[cfg(feature = "chrono")]
            Value::Duration(d) => Value::Duration(*d),
            #[cfg(feature = "chrono")]
            Value::Timestamp(t) => Value::Timestamp(*t),
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
    Object,
    Null,
}

impl ValueType {
    pub fn as_str(&self) -> &'static str {
        match self {
            ValueType::List => "list",
            ValueType::Map => "map",
            ValueType::Function => "function",
            ValueType::Int => "int",
            ValueType::UInt => "uint",
            ValueType::Float => "float",
            ValueType::String => "string",
            ValueType::Bytes => "bytes",
            ValueType::Bool => "bool",
            ValueType::Object => "object",
            ValueType::Duration => "duration",
            ValueType::Timestamp => "timestamp",
            ValueType::Null => "null",
        }
    }
}
impl Display for ValueType {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
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
            Value::Object(_) => ValueType::Object,
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
            Value::Map(v) => v.is_empty(),
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
            got: self.type_of().as_str(),
            want: expected.as_str(),
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
            Value::Map(m) => match m {
                MapValue::Owned(map) => Value::Map(MapValue::Owned(map.clone())),
                MapValue::Borrow(_) => {
                    Value::Map(MapValue::Owned(Arc::new(m.iter_owned().collect())))
                }
            },
            Value::Int(i) => Value::Int(*i),
            Value::UInt(u) => Value::UInt(*u),
            Value::Float(f) => Value::Float(*f),
            Value::Bool(b) => Value::Bool(*b),
            // Object values are Arc-backed, so cloning is cheap and we can transmute the lifetime
            // Safety: Object uses Arc internally, so the cloned value is independent of 'a
            Value::Object(obj) => {
                let cloned = obj.clone();
                // Safety: The Object's data is Arc-wrapped and self-contained.
                // The PhantomData marker is only for covariance; the actual data lives in the Arc.
                unsafe { std::mem::transmute::<Value<'a>, Value<'static>>(Value::Object(cloned)) }
            }
            #[cfg(feature = "chrono")]
            Value::Duration(d) => Value::Duration(*d),
            #[cfg(feature = "chrono")]
            Value::Timestamp(t) => Value::Timestamp(*t),
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
                                        return Ok(ObjectValue::new(OptionalValue::none()).into())
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
                                    .get(&KeyRef::String(StringValue::Borrowed(&property)))
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
                                    Ok(val) => {
                                        ObjectValue::new(OptionalValue::of(val.as_static())).into()
                                    }
                                    Err(_) => ObjectValue::new(OptionalValue::none()).into(),
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
                                    Some(inner) => Ok(ObjectValue::new(OptionalValue::of(
                                        inner.clone().member(&field)?,
                                    ))
                                    .into()),
                                    None => Ok(operand),
                                };
                            }
                            return Ok(ObjectValue::new(OptionalValue::of(
                                operand.member(&field)?.as_static(),
                            ))
                            .into());
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
                            Some(Value::Object(ref ob)) => {
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
                            let b = map.contains_key(&KeyRef::String(select.field.as_str().into()));
                            Ok(Value::Bool(b))
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
                Ok(Value::Map(MapValue::Owned(Arc::from(map))))
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
                        for key in map.iter_keys() {
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
            Value::Map(m) => m.get(&KeyRef::String(StringValue::Borrowed(name))).cloned(),
            Value::Object(obj) => obj.get_member(name),
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
    use std::collections::HashMap;
    use std::sync::Arc;

    use crate::context::{MapResolver, VariableResolver};
    use crate::objects::{Key, ListValue, StringValue, Value};
    use crate::parser::Expression;
    use crate::{Context, ExecutionError, Program};

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
        fn all(&self) -> &[&'static str] {
            self.base.all()
        }
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
        use std::collections::HashMap;
        use std::fmt::Debug;
        use std::sync::Arc;

        use serde::Serialize;

        use crate::context::MapResolver;
        use crate::objects::{
            ListValue, MapValue, ObjectType, ObjectValue, OptionalValue, StringValue,
        };
        use crate::parser::Parser;
        use crate::{Context, ExecutionError, FunctionContext, Program, Value};

        #[derive(Debug, Clone, Eq, PartialEq, Serialize)]
        struct MyStruct {
            field: String,
        }
        crate::register_type!(MyStruct);

        impl ObjectType<'static> for MyStruct {
            fn type_name(&self) -> &'static str {
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
                if let Some(Value::Object(obj)) = &ftx.this {
                    if obj.type_name() == "my_struct" {
                        Ok(obj.downcast_ref::<MyStruct>().unwrap().field.clone().into())
                    } else {
                        Err(ExecutionError::UnexpectedType {
                            got: obj.type_name(),
                            want: "my_struct",
                        })
                    }
                } else {
                    Err(ExecutionError::UnexpectedType {
                        got: if let Some(t) = &ftx.this {
                            t.type_of().as_str()
                        } else {
                            "None"
                        },
                        want: "Value::Object",
                    })
                }
            }

            let value = MyStruct {
                field: String::from("value"),
            };

            let mut vars = MapResolver::new();
            vars.add_variable_from_value("mine", Value::Object(ObjectValue::new(value)));
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
            let value_1 = MyStruct {
                field: String::from("1"),
            };
            let value_2 = MyStruct {
                field: String::from("2"),
            };

            let mut vars = MapResolver::new();
            vars.add_variable_from_value("v1", Value::Object(ObjectValue::new(value_1.clone())));
            vars.add_variable_from_value("v1b", Value::Object(ObjectValue::new(value_1)));
            vars.add_variable_from_value("v2", Value::Object(ObjectValue::new(value_2)));
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
            let opaque = MyStruct {
                field: "not so opaque".to_string(),
            };
            let opaque = Value::Object(ObjectValue::new(opaque));
            assert_eq!(
                "Object<my_struct>(MyStruct { field: \"not so opaque\" })",
                format!("{:?}", opaque)
            );
        }

        #[test]
        #[cfg(feature = "json")]
        fn test_json() {
            let value = MyStruct {
                field: String::from("value"),
            };
            let cel_value = Value::Object(ObjectValue::new(value));
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
                Ok(Value::Object(ObjectValue::new(OptionalValue::none())))
            );

            let expr = Parser::default()
                .enable_optional_syntax(true)
                .parse("optional.of(1)")
                .expect("Must parse");
            assert_eq!(
                Value::resolve(&expr, &ctx, &empty_vars),
                Ok(Value::Object(ObjectValue::new(OptionalValue::of(
                    Value::Int(1)
                ))))
            );

            let expr = Parser::default()
                .enable_optional_syntax(true)
                .parse("optional.ofNonZeroValue(0)")
                .expect("Must parse");
            assert_eq!(
                Value::resolve(&expr, &ctx, &empty_vars),
                Ok(Value::Object(ObjectValue::new(OptionalValue::none())))
            );

            let expr = Parser::default()
                .enable_optional_syntax(true)
                .parse("optional.ofNonZeroValue(1)")
                .expect("Must parse");
            assert_eq!(
                Value::resolve(&expr, &ctx, &empty_vars),
                Ok(Value::Object(ObjectValue::new(OptionalValue::of(
                    Value::Int(1)
                ))))
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
                Ok(Value::Object(ObjectValue::new(OptionalValue::of(
                    Value::Int(1)
                ))))
            );
            let expr = Parser::default()
                .enable_optional_syntax(true)
                .parse("optional.none().or(optional.of(2))")
                .expect("Must parse");
            assert_eq!(
                Value::resolve(&expr, &ctx, &empty_vars),
                Ok(Value::Object(ObjectValue::new(OptionalValue::of(
                    Value::Int(2)
                ))))
            );
            let expr = Parser::default()
                .enable_optional_syntax(true)
                .parse("optional.none().or(optional.none())")
                .expect("Must parse");
            assert_eq!(
                Value::resolve(&expr, &ctx, &empty_vars),
                Ok(Value::Object(ObjectValue::new(OptionalValue::none())))
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
                Ok(Value::Object(ObjectValue::new(OptionalValue::of(
                    Value::String(StringValue::Owned(Arc::from("value")))
                ))))
            );

            let expr = Parser::default()
                .enable_optional_syntax(true)
                .parse("optional.of(msg).?field")
                .expect("Must parse");
            assert_eq!(
                Value::resolve(&expr, &ctx, &msg_vars),
                Ok(Value::Object(ObjectValue::new(OptionalValue::of(
                    Value::String(StringValue::Owned(Arc::from("value")))
                ))))
            );

            let expr = Parser::default()
                .enable_optional_syntax(true)
                .parse("optional.none().?field")
                .expect("Must parse");
            assert_eq!(
                Value::resolve(&expr, &ctx, &msg_vars),
                Ok(Value::Object(ObjectValue::new(OptionalValue::none())))
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
                Ok(Value::Map(MapValue::Owned(Arc::from(expected_map))))
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
                Ok(Value::Map(MapValue::Owned(Arc::from(expected_map))))
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
                Ok(Value::Map(MapValue::Owned(Arc::from(expected_map))))
            );

            let expr = Parser::default()
                .enable_optional_syntax(true)
                .parse(r#"{"a": 1, ?"b": mymap[?"missing"]}"#)
                .expect("Must parse");
            let mut expected_map = hashbrown::HashMap::new();
            expected_map.insert("a".into(), Value::Int(1));
            assert_eq!(
                Value::resolve(&expr, &ctx, &map_vars),
                Ok(Value::Map(MapValue::Owned(Arc::from(expected_map))))
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
                Ok(Value::Map(MapValue::Owned(Arc::from(expected_map))))
            );

            let expr = Parser::default()
                .enable_optional_syntax(true)
                .parse(r#"{?"a": optional.none(), ?"b": optional.none()}"#)
                .expect("Must parse");
            assert_eq!(
                Value::resolve(&expr, &ctx, &empty_vars),
                Ok(Value::Map(MapValue::Owned(Arc::from(
                    hashbrown::HashMap::new()
                )))),
            );
        }
    }
}
