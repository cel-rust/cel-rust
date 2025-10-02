use crate::common::ast::{operators, EntryExpr, Expr};
use crate::common::value::{CelVal, Val};
use crate::context::Context;
use crate::functions::FunctionContext;
use crate::{ExecutionError, Expression};
use std::any::Any;
use std::borrow::Borrow;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::convert::{Infallible, TryFrom, TryInto};
use std::fmt::{Debug, Display, Formatter};
use std::ops;
use std::ops::Deref;
use std::sync::Arc;
#[cfg(feature = "chrono")]
use std::sync::LazyLock;

#[cfg(feature = "chrono")]
use chrono::TimeZone;
use nom::combinator::{cond, map, value};
use paste::{expr, item};

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
    pub map: Arc<HashMap<Key, Value>>,
}

impl PartialOrd for Map {
    fn partial_cmp(&self, _: &Self) -> Option<Ordering> {
        None
    }
}

impl Map {
    pub(crate) fn contains_key(&self, key: &(dyn AsKeyRef + '_)) -> bool {
        self.map.contains_key(key)
    }
    /// Returns a reference to the value corresponding to the key. Implicitly converts between int
    /// and uint keys.
    pub fn get(&self, key: &(dyn AsKeyRef + '_)) -> Option<&Value> {
        self.map.get(key).or_else(|| {
            // Also check keys that are cross type comparable.
            let keyref = key.as_keyref();
            match keyref {
                KeyRef::Int(k) => {
                    let converted = u64::try_from(k).ok()?;
                    self.map.get(&Key::Uint(converted))
                }
                KeyRef::Uint(k) => {
                    let converted = i64::try_from(k).ok()?;
                    self.map.get(&Key::Int(converted))
                }
                _ => None,
            }
        })
    }
}

#[derive(Debug, Eq, PartialEq, Hash, Ord, Clone, PartialOrd)]
pub enum Key {
    Int(i64),
    Uint(u64),
    Bool(bool),
    String(Arc<String>),
}

/// A borrowed version of [`Key`] that avoids allocating for lookups.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub enum KeyRef<'a> {
    Int(i64),
    Uint(u64),
    Bool(bool),
    String(&'a str),
}

/// Trait for converting to a borrowed [`KeyRef`] for efficient lookups.
pub trait AsKeyRef {
    fn as_keyref(&self) -> KeyRef<'_>;
}

impl AsKeyRef for Key {
    fn as_keyref(&self) -> KeyRef<'_> {
        match self {
            Key::Int(i) => KeyRef::Int(*i),
            Key::Uint(u) => KeyRef::Uint(*u),
            Key::Bool(b) => KeyRef::Bool(*b),
            Key::String(s) => KeyRef::String(s.as_str()),
        }
    }
}

impl<'a> AsKeyRef for KeyRef<'a> {
    fn as_keyref(&self) -> KeyRef<'a> {
        *self
    }
}

/// Trait object implementations for `dyn AsKeyRef` to enable hashing and comparison.
impl<'a> PartialEq for dyn AsKeyRef + 'a {
    fn eq(&self, other: &Self) -> bool {
        self.as_keyref().eq(&other.as_keyref())
    }
}

impl<'a> Eq for dyn AsKeyRef + 'a {}

impl<'a> std::hash::Hash for dyn AsKeyRef + 'a {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.as_keyref().hash(state)
    }
}

impl<'a> PartialOrd for dyn AsKeyRef + 'a {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<'a> Ord for dyn AsKeyRef + 'a {
    fn cmp(&self, other: &Self) -> Ordering {
        self.as_keyref().cmp(&other.as_keyref())
    }
}

/// Implement `Borrow<dyn AsKeyRef>` for `Key` to enable efficient lookups.
impl<'a> Borrow<dyn AsKeyRef + 'a> for Key {
    fn borrow(&self) -> &(dyn AsKeyRef + 'a) {
        self
    }
}

/// Implement conversions from primitive types to [`Key`]
impl From<String> for Key {
    fn from(v: String) -> Self {
        Key::String(v.into())
    }
}

impl From<Arc<String>> for Key {
    fn from(v: Arc<String>) -> Self {
        Key::String(v)
    }
}

impl<'a> From<&'a str> for Key {
    fn from(v: &'a str) -> Self {
        Key::String(Arc::new(v.into()))
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
impl TryInto<Key> for Value {
    type Error = Value;

    #[inline(always)]
    fn try_into(self) -> Result<Key, Self::Error> {
        match self {
            Value::Int(v) => Ok(Key::Int(v)),
            Value::UInt(v) => Ok(Key::Uint(v)),
            Value::String(v) => Ok(Key::String(v)),
            Value::Bool(v) => Ok(Key::Bool(v)),
            _ => Err(self),
        }
    }
}

/// Implement conversions from [`KeyRef`] into [`Value`]
impl<'a> TryFrom<&'a Value> for KeyRef<'a> {
    type Error = Value;

    fn try_from(value: &'a Value) -> Result<Self, Self::Error> {
        match value {
            Value::Int(v) => Ok(KeyRef::Int(*v)),
            Value::UInt(v) => Ok(KeyRef::Uint(*v)),
            Value::String(v) => Ok(KeyRef::String(v.as_str())),
            Value::Bool(v) => Ok(KeyRef::Bool(*v)),
            _ => Err(value.clone()),
        }
    }
}

// Implement conversion from HashMap<K, V> into CelMap
impl<K: Into<Key>, V: Into<Value>> From<HashMap<K, V>> for Map {
    fn from(map: HashMap<K, V>) -> Self {
        let mut new_map = HashMap::with_capacity(map.len());
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

#[derive(Debug, Eq, PartialEq)]
pub struct OptionalValue {
    value: Option<Value>,
}

impl OptionalValue {
    pub fn of(value: Value) -> Self {
        OptionalValue { value: Some(value) }
    }
    pub fn none() -> Self {
        OptionalValue { value: None }
    }
    pub fn value(&self) -> Option<&Value> {
        self.value.as_ref()
    }
}

impl Opaque for OptionalValue {
    fn runtime_type_name(&self) -> &str {
        "optional_type"
    }
}

impl<'a> TryFrom<&'a Value> for &'a OptionalValue {
    type Error = ExecutionError;

    fn try_from(value: &'a Value) -> Result<Self, Self::Error> {
        match value {
            Value::Opaque(opaque) if opaque.runtime_type_name() == "optional_type" => opaque
                .downcast_ref::<OptionalValue>()
                .ok_or_else(|| ExecutionError::function_error("optional", "failed to downcast")),
            Value::Opaque(opaque) => Err(ExecutionError::UnexpectedType {
                got: opaque.runtime_type_name().to_string(),
                want: "optional_type".to_string(),
            }),
            v => Err(ExecutionError::UnexpectedType {
                got: v.type_of().to_string(),
                want: "optional_type".to_string(),
            }),
        }
    }
}

pub trait TryIntoValue {
    type Error: std::error::Error + 'static + Send + Sync;
    fn try_into_value(self) -> Result<Value, Self::Error>;
}

impl<T: serde::Serialize> TryIntoValue for T {
    type Error = crate::ser::SerializationError;
    fn try_into_value(self) -> Result<Value, Self::Error> {
        crate::ser::to_value(self)
    }
}
impl TryIntoValue for Value {
    type Error = Infallible;
    fn try_into_value(self) -> Result<Value, Self::Error> {
        Ok(self)
    }
}

#[derive(Clone)]
pub enum Value {
    List(Arc<Vec<Value>>),
    Map(Map),

    Function(Arc<String>, Option<Box<Value>>),

    // Atoms
    Int(i64),
    UInt(u64),
    Float(f64),
    String(Arc<String>),
    Bytes(Arc<Vec<u8>>),
    Bool(bool),
    #[cfg(feature = "chrono")]
    Duration(chrono::Duration),
    #[cfg(feature = "chrono")]
    Timestamp(chrono::DateTime<chrono::FixedOffset>),
    Opaque(Arc<dyn Opaque>),
    Null,
}

impl Debug for Value {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::List(l) => write!(f, "List({:?})", l),
            Value::Map(m) => write!(f, "Map({:?})", m),
            Value::Function(name, func) => write!(f, "Function({:?}, {:?})", name, func),
            Value::Int(i) => write!(f, "Int({:?})", i),
            Value::UInt(u) => write!(f, "UInt({:?})", u),
            Value::Float(d) => write!(f, "Float({:?})", d),
            Value::String(s) => write!(f, "String({:?})", s),
            Value::Bytes(b) => write!(f, "Bytes({:?})", b),
            Value::Bool(b) => write!(f, "Bool({:?})", b),
            #[cfg(feature = "chrono")]
            Value::Duration(d) => write!(f, "Duration({:?})", d),
            #[cfg(feature = "chrono")]
            Value::Timestamp(t) => write!(f, "Timestamp({:?})", t),
            Value::Opaque(o) => write!(f, "Opaque<{}>({:?})", o.runtime_type_name(), o.as_debug()),
            Value::Null => write!(f, "Null"),
        }
    }
}

impl From<CelVal> for Value {
    fn from(val: CelVal) -> Self {
        match val {
            CelVal::String(s) => Value::String(Arc::new(s)),
            CelVal::Boolean(b) => Value::Bool(b),
            CelVal::Int(i) => Value::Int(i),
            CelVal::UInt(u) => Value::UInt(u),
            CelVal::Double(d) => Value::Float(d),
            CelVal::Bytes(bytes) => Value::Bytes(Arc::new(bytes)),
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

impl Value {
    pub fn type_of(&self) -> ValueType {
        match self {
            Value::List(_) => ValueType::List,
            Value::Map(_) => ValueType::Map,
            Value::Function(_, _) => ValueType::Function,
            Value::Int(_) => ValueType::Int,
            Value::UInt(_) => ValueType::UInt,
            Value::Float(_) => ValueType::Float,
            Value::String(_) => ValueType::String,
            Value::Bytes(_) => ValueType::Bytes,
            Value::Bool(_) => ValueType::Bool,
            Value::Opaque(_) => ValueType::Opaque,
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
            Value::Bytes(v) => v.is_empty(),
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

impl From<&Value> for Value {
    fn from(value: &Value) -> Self {
        value.clone()
    }
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::Map(a), Value::Map(b)) => a == b,
            (Value::List(a), Value::List(b)) => a == b,
            (Value::Function(a1, a2), Value::Function(b1, b2)) => a1 == b1 && a2 == b2,
            (Value::Int(a), Value::Int(b)) => a == b,
            (Value::UInt(a), Value::UInt(b)) => a == b,
            (Value::Float(a), Value::Float(b)) => a == b,
            (Value::String(a), Value::String(b)) => a == b,
            (Value::Bytes(a), Value::Bytes(b)) => a == b,
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
            (Value::Opaque(a), Value::Opaque(b)) => a.opaque_eq(b.deref()),
            (_, _) => false,
        }
    }
}

impl Eq for Value {}

impl PartialOrd for Value {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        match (self, other) {
            (Value::Int(a), Value::Int(b)) => Some(a.cmp(b)),
            (Value::UInt(a), Value::UInt(b)) => Some(a.cmp(b)),
            (Value::Float(a), Value::Float(b)) => a.partial_cmp(b),
            (Value::String(a), Value::String(b)) => Some(a.cmp(b)),
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

impl From<&Key> for Value {
    fn from(value: &Key) -> Self {
        match value {
            Key::Int(v) => Value::Int(*v),
            Key::Uint(v) => Value::UInt(*v),
            Key::Bool(v) => Value::Bool(*v),
            Key::String(v) => Value::String(v.clone()),
        }
    }
}

impl From<Key> for Value {
    fn from(value: Key) -> Self {
        match value {
            Key::Int(v) => Value::Int(v),
            Key::Uint(v) => Value::UInt(v),
            Key::Bool(v) => Value::Bool(v),
            Key::String(v) => Value::String(v),
        }
    }
}

impl From<&Key> for Key {
    fn from(key: &Key) -> Self {
        key.clone()
    }
}

// Convert Vec<T> to Value
impl<T: Into<Value>> From<Vec<T>> for Value {
    fn from(v: Vec<T>) -> Self {
        Value::List(v.into_iter().map(|v| v.into()).collect::<Vec<_>>().into())
    }
}

// Convert Vec<u8> to Value
impl From<Vec<u8>> for Value {
    fn from(v: Vec<u8>) -> Self {
        Value::Bytes(v.into())
    }
}

#[cfg(feature = "bytes")]
// Convert Bytes to Value
impl From<::bytes::Bytes> for Value {
    fn from(v: ::bytes::Bytes) -> Self {
        Value::Bytes(v.to_vec().into())
    }
}

#[cfg(feature = "bytes")]
// Convert &Bytes to Value
impl From<&::bytes::Bytes> for Value {
    fn from(v: &::bytes::Bytes) -> Self {
        Value::Bytes(v.to_vec().into())
    }
}

// Convert String to Value
impl From<String> for Value {
    fn from(v: String) -> Self {
        Value::String(v.into())
    }
}

impl From<&str> for Value {
    fn from(v: &str) -> Self {
        Value::String(v.to_string().into())
    }
}

// Convert Option<T> to Value
impl<T: Into<Value>> From<Option<T>> for Value {
    fn from(v: Option<T>) -> Self {
        match v {
            Some(v) => v.into(),
            None => Value::Null,
        }
    }
}

// Convert HashMap<K, V> to Value
impl<K: Into<Key>, V: Into<Value>> From<HashMap<K, V>> for Value {
    fn from(v: HashMap<K, V>) -> Self {
        Value::Map(v.into())
    }
}

impl From<ExecutionError> for ResolveResult {
    fn from(value: ExecutionError) -> Self {
        Err(value)
    }
}

pub type ResolveResult = Result<Value, ExecutionError>;

impl From<Value> for ResolveResult {
    fn from(value: Value) -> Self {
        Ok(value)
    }
}

impl Value {
    pub fn resolve_all(expr: &[Expression], ctx: &Context) -> ResolveResult {
        let mut res = Vec::with_capacity(expr.len());
        for expr in expr {
            res.push(Value::resolve(expr, ctx)?);
        }
        Ok(Value::List(res.into()))
    }

    pub fn resolve(expr: &Expression, ctx: &Context) -> ResolveResult {
        todo!()
    }

    #[inline(always)]
    pub fn resolve_val(expr: &Expression, ctx: &Context) -> Result<Box<dyn Val>, ExecutionError> {
        match &expr.expr {
            Expr::Literal(val) => Ok(val.to_value()),
            Expr::Call(call) => {
                // START OF SPECIAL CASES FOR operators::...
                if call.args.len() == 3 && call.func_name == operators::CONDITIONAL {
                    let cond = Value::resolve(&call.args[0], ctx)?;
                    return if cond.to_bool()? {
                        Value::resolve_val(&call.args[1], ctx)
                    } else {
                        Value::resolve_val(&call.args[2], ctx)
                    };
                }
                if call.args.len() == 2 {
                    match call.func_name.as_str() {
                        operators::LOGICAL_OR => {
                            let left = Value::resolve_val(&call.args[0], ctx)?;
                            return if left.to_bool()? {
                                Ok(left)
                            } else {
                                Value::resolve_val(&call.args[1], ctx)
                            };
                        }
                        operators::LOGICAL_AND => {
                            let left = Value::resolve(&call.args[0], ctx)?;
                            return if !left.to_bool()? {
                                Value::Bool(false)
                            } else {
                                let right = Value::resolve(&call.args[1], ctx)?;
                                Value::Bool(right.to_bool()?)
                            }
                            .into();
                        }
                        operators::EQUALS => {
                            return Value::Bool(
                                Value::resolve_val(&call.args[0], ctx)?
                                    .eq(&Value::resolve_val(&call.args[1], ctx)?),
                            )
                            .into()
                        }
                        operators::NOT_EQUALS => {
                            return Value::Bool(
                                Value::resolve(&call.args[0], ctx)?
                                    .ne(&Value::resolve(&call.args[1], ctx)?),
                            )
                            .into()
                        }
                        operators::INDEX | operators::OPT_INDEX => {
                            let mut value = Value::resolve(&call.args[0], ctx)?;
                            let idx = Value::resolve(&call.args[1], ctx)?;
                            let mut is_optional = call.func_name == operators::OPT_INDEX;

                            if let Ok(opt_val) = <&OptionalValue>::try_from(&value) {
                                is_optional = true;
                                value = match opt_val.value() {
                                    Some(inner) => inner.clone(),
                                    None => {
                                        return Ok(Value::Opaque(Arc::new(OptionalValue::none())))
                                    }
                                };
                            }

                            let result = match (value, idx) {
                                (Value::List(items), Value::Int(idx)) => {
                                    if idx >= 0 && (idx as usize) < items.len() {
                                        items[idx as usize].clone().into()
                                    } else {
                                        Err(ExecutionError::IndexOutOfBounds(idx.into()))
                                    }
                                }
                                (Value::List(items), Value::UInt(idx)) => {
                                    if (idx as usize) < items.len() {
                                        items[idx as usize].clone().into()
                                    } else {
                                        Err(ExecutionError::IndexOutOfBounds(idx.into()))
                                    }
                                }
                                (Value::String(_), Value::Int(idx)) => {
                                    Err(ExecutionError::NoSuchKey(idx.to_string().into()))
                                }
                                (Value::Map(map), Value::String(property)) => map
                                    .get(&KeyRef::String(property.as_str()))
                                    .cloned()
                                    .ok_or_else(|| ExecutionError::NoSuchKey(property)),
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
                                    Err(ExecutionError::UnsupportedMapIndex(index))
                                }
                                (Value::List(_), index) => {
                                    Err(ExecutionError::UnsupportedListIndex(index))
                                }
                                (value, index) => {
                                    Err(ExecutionError::UnsupportedIndex(value, index))
                                }
                            };

                            return if is_optional {
                                Ok(match result {
                                    Ok(val) => Value::Opaque(Arc::new(OptionalValue::of(val))),
                                    Err(_) => Value::Opaque(Arc::new(OptionalValue::none())),
                                })
                            } else {
                                result
                            };
                        }
                        operators::OPT_SELECT => {
                            let operand = Value::resolve(&call.args[0], ctx)?;
                            let field_literal = Value::resolve(&call.args[1], ctx)?;
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
                                    Some(inner) => Ok(Value::Opaque(Arc::new(OptionalValue::of(
                                        inner.clone().member(&field)?,
                                    )))),
                                    None => Ok(operand),
                                };
                            }
                            return Ok(Value::Opaque(Arc::new(OptionalValue::of(
                                operand.member(&field)?,
                            ))));
                        }
                        // END OF SPECIAL CASES

                        // all below is NOT special in the interpreter
                        operators::ADD => {
                            return Value::resolve(&call.args[0], ctx)?
                                + Value::resolve(&call.args[1], ctx)?
                        }
                        operators::SUBSTRACT => {
                            return Value::resolve(&call.args[0], ctx)?
                                - Value::resolve(&call.args[1], ctx)?
                        }
                        operators::DIVIDE => {
                            return Value::resolve(&call.args[0], ctx)?
                                / Value::resolve(&call.args[1], ctx)?
                        }
                        operators::MULTIPLY => {
                            return Value::resolve(&call.args[0], ctx)?
                                * Value::resolve(&call.args[1], ctx)?
                        }
                        operators::MODULO => {
                            return Value::resolve(&call.args[0], ctx)?
                                % Value::resolve(&call.args[1], ctx)?
                        }
                        operators::LESS => {
                            let left = Value::resolve(&call.args[0], ctx)?;
                            let right = Value::resolve(&call.args[1], ctx)?;
                            return Value::Bool(
                                left.partial_cmp(&right)
                                    .ok_or(ExecutionError::ValuesNotComparable(left, right))?
                                    == Ordering::Less,
                            )
                            .into();
                        }
                        operators::LESS_EQUALS => {
                            let left = Value::resolve(&call.args[0], ctx)?;
                            let right = Value::resolve(&call.args[1], ctx)?;
                            return Value::Bool(
                                left.partial_cmp(&right)
                                    .ok_or(ExecutionError::ValuesNotComparable(left, right))?
                                    != Ordering::Greater,
                            )
                            .into();
                        }
                        operators::GREATER => {
                            let left = Value::resolve(&call.args[0], ctx)?;
                            let right = Value::resolve(&call.args[1], ctx)?;
                            return Value::Bool(
                                left.partial_cmp(&right)
                                    .ok_or(ExecutionError::ValuesNotComparable(left, right))?
                                    == Ordering::Greater,
                            )
                            .into();
                        }
                        operators::GREATER_EQUALS => {
                            let left = Value::resolve(&call.args[0], ctx)?;
                            let right = Value::resolve(&call.args[1], ctx)?;
                            return Value::Bool(
                                left.partial_cmp(&right)
                                    .ok_or(ExecutionError::ValuesNotComparable(left, right))?
                                    != Ordering::Less,
                            )
                            .into();
                        }
                        operators::IN => {
                            let left = Value::resolve(&call.args[0], ctx)?;
                            let right = Value::resolve(&call.args[1], ctx)?;
                            match (left, right) {
                                (Value::String(l), Value::String(r)) => {
                                    return Value::Bool(r.contains(&*l)).into()
                                }
                                (any, Value::List(v)) => {
                                    return Value::Bool(v.contains(&any)).into()
                                }
                                (any, Value::Map(m)) => match KeyRef::try_from(&any) {
                                    Ok(key) => return Value::Bool(m.contains_key(&key)).into(),
                                    Err(_) => return Value::Bool(false).into(),
                                },
                                (left, right) => {
                                    Err(ExecutionError::ValuesNotComparable(left, right))?
                                }
                            }                        }
                        _ => (),
                    }
                }
                if call.args.len() == 1 {
                    match call.func_name.as_str() {
                        operators::LOGICAL_NOT => {
                            let expr = Value::resolve(&call.args[0], ctx)?;
                            return Ok(Value::Bool(!expr.to_bool()?));
                        }
                        operators::NEGATE => {
                            return match Value::resolve(&call.args[0], ctx)? {
                                Value::Int(i) => Ok(Value::Int(-i)),
                                Value::Float(f) => Ok(Value::Float(-f)),
                                value => {
                                    Err(ExecutionError::UnsupportedUnaryOperator("minus", value))
                                }
                            }
                        }
                        operators::NOT_STRICTLY_FALSE => {
                            return match Value::resolve(&call.args[0], ctx)? {
                                Value::Bool(b) => Ok(Value::Bool(b)),
                                _ => Ok(Value::Bool(true)),
                            }
                        }
                        _ => (),
                    }
                }
                match &call.target {
                    None => {
                        let func = ctx.get_function(call.func_name.as_str()).ok_or_else(|| {
                            ExecutionError::UndeclaredReference(call.func_name.clone().into())
                        })?;
                        let mut ctx = FunctionContext::new(&call.func_name, None, ctx, &call.args);
                        (func)(&mut ctx)
                    }
                    Some(target) => {
                        let qualified_func = match &target.expr {
                            Expr::Ident(prefix) => {
                                let qualified_name = format!("{prefix}.{}", &call.func_name);
                                ctx.get_function(&qualified_name)
                            }
                            _ => None,
                        };
                        match qualified_func {
                            None => {
                                let func =
                                    ctx.get_function(call.func_name.as_str()).ok_or_else(|| {
                                        ExecutionError::UndeclaredReference(
                                            call.func_name.clone().into(),
                                        )
                                    })?;
                                let mut ctx = FunctionContext::new(
                                    &call.func_name,
                                    Some(Value::resolve(target, ctx)?),
                                    ctx,
                                    &call.args,
                                );
                                (func)(&mut ctx)
                            }
                            Some(func) => {
                                let mut ctx =
                                    FunctionContext::new(&call.func_name, None, ctx, &call.args);
                                (func)(&mut ctx)
                            }
                        }
                    }
                }
            }
            Expr::Ident(name) => ctx.get_variable(name),
            Expr::Select(select) => {
                let left = Value::resolve(select.operand.deref(), ctx)?;
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
                    left.member(&select.field)
                }
            }
            Expr::List(list_expr) => {
                let list = list_expr
                    .elements
                    .iter()
                    .enumerate()
                    .map(|(idx, element)| {
                        Value::resolve(element, ctx).map(|value| {
                            if list_expr.optional_indices.contains(&idx) {
                                if let Ok(opt_val) = <&OptionalValue>::try_from(&value) {
                                    opt_val.value().cloned()
                                } else {
                                    Some(value)
                                }
                            } else {
                                Some(value)
                            }
                        })
                    })
                    .collect::<Result<Vec<_>, _>>()?
                    .into_iter()
                    .flatten()
                    .collect::<Vec<_>>();
                Value::List(list.into()).into()
            }
            Expr::Map(map_expr) => {
                let mut map = HashMap::with_capacity(map_expr.entries.len());
                for entry in map_expr.entries.iter() {
                    let (k, v, is_optional) = match &entry.expr {
                        EntryExpr::StructField(_) => panic!("WAT?"),
                        EntryExpr::MapEntry(e) => (&e.key, &e.value, e.optional),
                    };
                    let key = Value::resolve(k, ctx)?
                        .try_into()
                        .map_err(ExecutionError::UnsupportedKeyType)?;
                    let value = Value::resolve(v, ctx)?;

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
                let accu_init = Value::resolve(&comprehension.accu_init, ctx)?;
                let iter = Value::resolve(&comprehension.iter_range, ctx)?;
                let mut ctx = ctx.new_inner_scope();
                ctx.add_variable(&comprehension.accu_var, accu_init)
                    .expect("Failed to add accu variable");

                match iter {
                    Value::List(items) => {
                        for item in items.deref() {
                            if !Value::resolve(&comprehension.loop_cond, &ctx)?.to_bool()? {
                                break;
                            }
                            ctx.add_variable_from_value(&comprehension.iter_var, item.clone());
                            let accu = Value::resolve(&comprehension.loop_step, &ctx)?;
                            ctx.add_variable_from_value(&comprehension.accu_var, accu);
                        }
                    }
                    Value::Map(map) => {
                        for key in map.map.deref().keys() {
                            if !Value::resolve(&comprehension.loop_cond, &ctx)?.to_bool()? {
                                break;
                            }
                            ctx.add_variable_from_value(&comprehension.iter_var, key.clone());
                            let accu = Value::resolve(&comprehension.loop_step, &ctx)?;
                            ctx.add_variable_from_value(&comprehension.accu_var, accu);
                        }
                    }
                    t => todo!("Support {t:?}"),
                }
                Value::resolve(&comprehension.result, &ctx)
            }
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

    fn member(self, name: &str) -> ResolveResult {
        // This will always either be because we're trying to access
        // a property on self, or a method on self.
        let child = match self {
            Value::Map(ref m) => m.get(&KeyRef::String(name)).cloned(),
            _ => None,
        };

        // If the property is both an attribute and a method, then we
        // give priority to the property. Maybe we can implement lookahead
        // to see if the next token is a function call?
        if let Some(child) = child {
            child.into()
        } else {
            ExecutionError::NoSuchKey(Arc::new(name.to_owned())).into()
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

impl ops::Add<Value> for Value {
    type Output = ResolveResult;

    #[inline(always)]
    fn add(self, rhs: Value) -> Self::Output {
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

            (Value::List(mut l), Value::List(mut r)) => {
                {
                    // If this is the only reference to `l`, we can append to it in place.
                    // `l` is replaced with a clone otherwise.
                    let l = Arc::make_mut(&mut l);

                    // Likewise, if this is the only reference to `r`, we can move its values
                    // instead of cloning them.
                    match Arc::get_mut(&mut r) {
                        Some(r) => l.append(r),
                        None => l.extend(r.iter().cloned()),
                    }
                }

                Ok(Value::List(l))
            }
            (Value::String(mut l), Value::String(r)) => {
                // If this is the only reference to `l`, we can append to it in place.
                // `l` is replaced with a clone otherwise.
                Arc::make_mut(&mut l).push_str(&r);
                Ok(Value::String(l))
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
                "add", left, right,
            )),
        }
    }
}

impl ops::Sub<Value> for Value {
    type Output = ResolveResult;

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
                "sub", left, right,
            )),
        }
    }
}

impl ops::Div<Value> for Value {
    type Output = ResolveResult;

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
                "div", left, right,
            )),
        }
    }
}

impl ops::Mul<Value> for Value {
    type Output = ResolveResult;

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
                "mul", left, right,
            )),
        }
    }
}

impl ops::Rem<Value> for Value {
    type Output = ResolveResult;

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
                "rem", left, right,
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
fn checked_op(
    op: TsOp,
    lhs: &chrono::DateTime<chrono::FixedOffset>,
    rhs: &chrono::Duration,
) -> ResolveResult {
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
    use crate::{objects::Key, Context, ExecutionError, Program, Value};
    use std::collections::HashMap;
    use std::sync::Arc;

    #[test]
    fn test_indexed_map_access() {
        let mut context = Context::default();
        let mut headers = HashMap::new();
        headers.insert("Content-Type", "application/json".to_string());
        context.add_variable_from_value("headers", headers);

        let program = Program::compile("headers[\"Content-Type\"]").unwrap();
        let value = program.execute(&context).unwrap();
        assert_eq!(value, "application/json".into());
    }

    #[test]
    fn test_numeric_map_access() {
        let mut context = Context::default();
        let mut numbers = HashMap::new();
        numbers.insert(Key::Uint(1), "one".to_string());
        context.add_variable_from_value("numbers", numbers);

        let program = Program::compile("numbers[1]").unwrap();
        let value = program.execute(&context).unwrap();
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
        let mut context = Context::default();
        let requests = vec![Value::Int(42), Value::Int(42)];
        context
            .add_variable("requests", Value::List(Arc::new(requests)))
            .unwrap();
        context.add_variable("size", Value::Int(3)).unwrap();
        assert_eq!(program.execute(&context).unwrap(), Value::Bool(true));
    }

    fn test_execution_error(program: &str, expected: ExecutionError) {
        let program = Program::compile(program).unwrap();
        let result = program.execute(&Context::default());
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
        let mut context = Context::default();
        context
            .add_variable("list", Value::List(Arc::new(vec![])))
            .unwrap();
        let result = program.execute(&context);
        assert_eq!(
            result,
            Err(ExecutionError::IndexOutOfBounds(Value::Int(10)))
        );
    }

    #[test]
    fn out_of_bound_list_access_negative() {
        let program = Program::compile("list[-1]").unwrap();
        let mut context = Context::default();
        context
            .add_variable("list", Value::List(Arc::new(vec![])))
            .unwrap();
        let result = program.execute(&context);
        assert_eq!(
            result,
            Err(ExecutionError::IndexOutOfBounds(Value::Int(-1)))
        );
    }

    #[test]
    fn list_access_uint() {
        let program = Program::compile("list[1u]").unwrap();
        let mut context = Context::default();
        context
            .add_variable("list", Value::List(Arc::new(vec![1.into(), 2.into()])))
            .unwrap();
        let result = program.execute(&context);
        assert_eq!(result, Ok(Value::Int(2.into())));
    }

    #[test]
    fn reference_to_value() {
        let test = "example".to_string();
        let direct: Value = test.as_str().into();
        assert_eq!(direct, Value::String(Arc::new(String::from("example"))));

        let vec = vec![test.as_str()];
        let indirect: Value = vec.into();
        assert_eq!(
            indirect,
            Value::List(Arc::new(vec![Value::String(Arc::new(String::from(
                "example"
            )))]))
        );
    }

    #[test]
    fn test_short_circuit_and() {
        let mut context = Context::default();
        let data: HashMap<String, String> = HashMap::new();
        context.add_variable_from_value("data", data);

        let program = Program::compile("has(data.x) && data.x.startsWith(\"foo\")").unwrap();
        let value = program.execute(&context);
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

    #[test]
    fn test_function_identifier() {
        fn with(
            ftx: &crate::FunctionContext,
            crate::extractors::This(this): crate::extractors::This<Value>,
            ident: crate::extractors::Identifier,
            expr: crate::parser::Expression,
        ) -> crate::ResolveResult {
            let mut ptx = ftx.ptx.new_inner_scope();
            ptx.add_variable_from_value(&ident, this);
            ptx.resolve(&expr)
        }
        let mut context = Context::default();
        context.add_function("with", with);

        let program = Program::compile("[1,2].with(a, a + a)").unwrap();
        let value = program.execute(&context);
        assert_eq!(
            value,
            Ok(Value::List(Arc::new(vec![
                Value::Int(1),
                Value::Int(2),
                Value::Int(1),
                Value::Int(2)
            ])))
        );
    }

    #[test]
    fn test_index_missing_map_key() {
        let mut ctx = Context::default();
        let mut map = HashMap::new();
        map.insert("a".to_string(), Value::Int(1));
        ctx.add_variable_from_value("mymap", map);

        let p = Program::compile(r#"mymap["missing"]"#).expect("Must compile");
        let result = p.execute(&ctx);

        assert!(result.is_err(), "Should error on missing map key");
    }

    mod opaque {
        use crate::objects::{Map, Opaque, OptionalValue};
        use crate::parser::Parser;
        use crate::{Context, ExecutionError, FunctionContext, Program, Value};
        use serde::Serialize;
        use std::collections::HashMap;
        use std::fmt::Debug;
        use std::ops::Deref;
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

        #[test]
        fn test_opaque_fn() {
            pub fn my_fn(ftx: &FunctionContext) -> Result<Value, ExecutionError> {
                if let Some(Value::Opaque(opaque)) = &ftx.this {
                    if opaque.runtime_type_name() == "my_struct" {
                        Ok(opaque
                            .deref()
                            .downcast_ref::<MyStruct>()
                            .unwrap()
                            .field
                            .clone()
                            .into())
                    } else {
                        Err(ExecutionError::UnexpectedType {
                            got: opaque.runtime_type_name().to_string(),
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

            let mut ctx = Context::default();
            ctx.add_variable_from_value("mine", Value::Opaque(value.clone()));
            ctx.add_function("myFn", my_fn);
            let prog = Program::compile("mine.myFn()").unwrap();
            assert_eq!(
                Ok(Value::String(Arc::new("value".into()))),
                prog.execute(&ctx)
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

            let mut ctx = Context::default();
            ctx.add_variable_from_value("v1", Value::Opaque(value_1.clone()));
            ctx.add_variable_from_value("v1b", Value::Opaque(value_1));
            ctx.add_variable_from_value("v2", Value::Opaque(value_2));
            assert_eq!(
                Program::compile("v2 == v1").unwrap().execute(&ctx),
                Ok(false.into())
            );
            assert_eq!(
                Program::compile("v1 == v1b").unwrap().execute(&ctx),
                Ok(true.into())
            );
            assert_eq!(
                Program::compile("v2 == v2").unwrap().execute(&ctx),
                Ok(true.into())
            );
        }

        #[test]
        fn test_value_holder_dbg() {
            let opaque = Arc::new(MyStruct {
                field: "not so opaque".to_string(),
            });
            let opaque = Value::Opaque(opaque);
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
            let cel_value = Value::Opaque(value);
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
            let expr = Parser::default()
                .enable_optional_syntax(true)
                .parse("optional.none()")
                .expect("Must parse");
            assert_eq!(
                Value::resolve(&expr, &Context::default()),
                Ok(Value::Opaque(Arc::new(OptionalValue::none())))
            );

            let expr = Parser::default()
                .enable_optional_syntax(true)
                .parse("optional.of(1)")
                .expect("Must parse");
            assert_eq!(
                Value::resolve(&expr, &Context::default()),
                Ok(Value::Opaque(Arc::new(OptionalValue::of(Value::Int(1)))))
            );

            let expr = Parser::default()
                .enable_optional_syntax(true)
                .parse("optional.ofNonZeroValue(0)")
                .expect("Must parse");
            assert_eq!(
                Value::resolve(&expr, &Context::default()),
                Ok(Value::Opaque(Arc::new(OptionalValue::none())))
            );

            let expr = Parser::default()
                .enable_optional_syntax(true)
                .parse("optional.ofNonZeroValue(1)")
                .expect("Must parse");
            assert_eq!(
                Value::resolve(&expr, &Context::default()),
                Ok(Value::Opaque(Arc::new(OptionalValue::of(Value::Int(1)))))
            );

            let expr = Parser::default()
                .enable_optional_syntax(true)
                .parse("optional.of(1).value()")
                .expect("Must parse");
            assert_eq!(
                Value::resolve(&expr, &Context::default()),
                Ok(Value::Int(1))
            );
            let expr = Parser::default()
                .enable_optional_syntax(true)
                .parse("optional.none().value()")
                .expect("Must parse");
            assert_eq!(
                Value::resolve(&expr, &Context::default()),
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
                Value::resolve(&expr, &Context::default()),
                Ok(Value::Bool(true))
            );
            let expr = Parser::default()
                .enable_optional_syntax(true)
                .parse("optional.none().hasValue()")
                .expect("Must parse");
            assert_eq!(
                Value::resolve(&expr, &Context::default()),
                Ok(Value::Bool(false))
            );

            let expr = Parser::default()
                .enable_optional_syntax(true)
                .parse("optional.of(1).or(optional.of(2))")
                .expect("Must parse");
            assert_eq!(
                Value::resolve(&expr, &Context::default()),
                Ok(Value::Opaque(Arc::new(OptionalValue::of(Value::Int(1)))))
            );
            let expr = Parser::default()
                .enable_optional_syntax(true)
                .parse("optional.none().or(optional.of(2))")
                .expect("Must parse");
            assert_eq!(
                Value::resolve(&expr, &Context::default()),
                Ok(Value::Opaque(Arc::new(OptionalValue::of(Value::Int(2)))))
            );
            let expr = Parser::default()
                .enable_optional_syntax(true)
                .parse("optional.none().or(optional.none())")
                .expect("Must parse");
            assert_eq!(
                Value::resolve(&expr, &Context::default()),
                Ok(Value::Opaque(Arc::new(OptionalValue::none())))
            );

            let expr = Parser::default()
                .enable_optional_syntax(true)
                .parse("optional.of(1).orValue(5)")
                .expect("Must parse");
            assert_eq!(
                Value::resolve(&expr, &Context::default()),
                Ok(Value::Int(1))
            );
            let expr = Parser::default()
                .enable_optional_syntax(true)
                .parse("optional.none().orValue(5)")
                .expect("Must parse");
            assert_eq!(
                Value::resolve(&expr, &Context::default()),
                Ok(Value::Int(5))
            );

            let mut ctx = Context::default();
            ctx.add_variable_from_value("msg", HashMap::from([("field", "value")]));

            let expr = Parser::default()
                .enable_optional_syntax(true)
                .parse("msg.?field")
                .expect("Must parse");
            assert_eq!(
                Value::resolve(&expr, &ctx),
                Ok(Value::Opaque(Arc::new(OptionalValue::of(Value::String(
                    Arc::new("value".to_string())
                )))))
            );

            let expr = Parser::default()
                .enable_optional_syntax(true)
                .parse("optional.of(msg).?field")
                .expect("Must parse");
            assert_eq!(
                Value::resolve(&expr, &ctx),
                Ok(Value::Opaque(Arc::new(OptionalValue::of(Value::String(
                    Arc::new("value".to_string())
                )))))
            );

            let expr = Parser::default()
                .enable_optional_syntax(true)
                .parse("optional.none().?field")
                .expect("Must parse");
            assert_eq!(
                Value::resolve(&expr, &ctx),
                Ok(Value::Opaque(Arc::new(OptionalValue::none())))
            );

            let expr = Parser::default()
                .enable_optional_syntax(true)
                .parse("optional.of(msg).?field.orValue('default')")
                .expect("Must parse");
            assert_eq!(
                Value::resolve(&expr, &ctx),
                Ok(Value::String(Arc::new("value".to_string())))
            );

            let expr = Parser::default()
                .enable_optional_syntax(true)
                .parse("optional.none().?field.orValue('default')")
                .expect("Must parse");
            assert_eq!(
                Value::resolve(&expr, &ctx),
                Ok(Value::String(Arc::new("default".to_string())))
            );

            let mut map_ctx = Context::default();
            let mut map = HashMap::new();
            map.insert("a".to_string(), Value::Int(1));
            map_ctx.add_variable_from_value("mymap", map);

            let expr = Parser::default()
                .enable_optional_syntax(true)
                .parse(r#"mymap[?"missing"].orValue(99)"#)
                .expect("Must parse");
            assert_eq!(Value::resolve(&expr, &map_ctx), Ok(Value::Int(99)));

            let expr = Parser::default()
                .enable_optional_syntax(true)
                .parse(r#"mymap[?"missing"].hasValue()"#)
                .expect("Must parse");
            assert_eq!(Value::resolve(&expr, &map_ctx), Ok(Value::Bool(false)));

            let expr = Parser::default()
                .enable_optional_syntax(true)
                .parse(r#"mymap[?"a"].orValue(99)"#)
                .expect("Must parse");
            assert_eq!(Value::resolve(&expr, &map_ctx), Ok(Value::Int(1)));

            let expr = Parser::default()
                .enable_optional_syntax(true)
                .parse(r#"mymap[?"a"].hasValue()"#)
                .expect("Must parse");
            assert_eq!(Value::resolve(&expr, &map_ctx), Ok(Value::Bool(true)));

            let mut list_ctx = Context::default();
            list_ctx.add_variable_from_value(
                "mylist",
                vec![Value::Int(1), Value::Int(2), Value::Int(3)],
            );

            let expr = Parser::default()
                .enable_optional_syntax(true)
                .parse("mylist[?10].orValue(99)")
                .expect("Must parse");
            assert_eq!(Value::resolve(&expr, &list_ctx), Ok(Value::Int(99)));

            let expr = Parser::default()
                .enable_optional_syntax(true)
                .parse("mylist[?1].orValue(99)")
                .expect("Must parse");
            assert_eq!(Value::resolve(&expr, &list_ctx), Ok(Value::Int(2)));

            let expr = Parser::default()
                .enable_optional_syntax(true)
                .parse("optional.of([1, 2, 3])[1].orValue(99)")
                .expect("Must parse");
            assert_eq!(
                Value::resolve(&expr, &Context::default()),
                Ok(Value::Int(2))
            );

            let expr = Parser::default()
                .enable_optional_syntax(true)
                .parse("optional.of([1, 2, 3])[4].orValue(99)")
                .expect("Must parse");
            assert_eq!(
                Value::resolve(&expr, &Context::default()),
                Ok(Value::Int(99))
            );

            let expr = Parser::default()
                .enable_optional_syntax(true)
                .parse("optional.none()[1].orValue(99)")
                .expect("Must parse");
            assert_eq!(
                Value::resolve(&expr, &Context::default()),
                Ok(Value::Int(99))
            );

            let expr = Parser::default()
                .enable_optional_syntax(true)
                .parse("optional.of([1, 2, 3])[?1].orValue(99)")
                .expect("Must parse");
            assert_eq!(
                Value::resolve(&expr, &Context::default()),
                Ok(Value::Int(2))
            );

            let expr = Parser::default()
                .enable_optional_syntax(true)
                .parse("[1, 2, ?optional.of(3), 4]")
                .expect("Must parse");
            assert_eq!(
                Value::resolve(&expr, &Context::default()),
                Ok(Value::List(Arc::new(vec![
                    Value::Int(1),
                    Value::Int(2),
                    Value::Int(3),
                    Value::Int(4)
                ])))
            );

            let expr = Parser::default()
                .enable_optional_syntax(true)
                .parse("[1, 2, ?optional.none(), 4]")
                .expect("Must parse");
            assert_eq!(
                Value::resolve(&expr, &Context::default()),
                Ok(Value::List(Arc::new(vec![
                    Value::Int(1),
                    Value::Int(2),
                    Value::Int(4)
                ])))
            );

            let expr = Parser::default()
                .enable_optional_syntax(true)
                .parse("[?optional.of(1), ?optional.none(), ?optional.of(3)]")
                .expect("Must parse");
            assert_eq!(
                Value::resolve(&expr, &Context::default()),
                Ok(Value::List(Arc::new(vec![Value::Int(1), Value::Int(3)])))
            );

            let expr = Parser::default()
                .enable_optional_syntax(true)
                .parse(r#"[1, ?mymap[?"missing"], 3]"#)
                .expect("Must parse");
            assert_eq!(
                Value::resolve(&expr, &map_ctx),
                Ok(Value::List(Arc::new(vec![Value::Int(1), Value::Int(3)])))
            );

            let expr = Parser::default()
                .enable_optional_syntax(true)
                .parse(r#"[1, ?mymap[?"a"], 3]"#)
                .expect("Must parse");
            assert_eq!(
                Value::resolve(&expr, &map_ctx),
                Ok(Value::List(Arc::new(vec![
                    Value::Int(1),
                    Value::Int(1),
                    Value::Int(3)
                ])))
            );

            let expr = Parser::default()
                .enable_optional_syntax(true)
                .parse("[?optional.none(), ?optional.none()]")
                .expect("Must parse");
            assert_eq!(
                Value::resolve(&expr, &Context::default()),
                Ok(Value::List(Arc::new(vec![])))
            );

            let expr = Parser::default()
                .enable_optional_syntax(true)
                .parse(r#"{"a": 1, "b": 2, ?"c": optional.of(3)}"#)
                .expect("Must parse");
            let mut expected_map = HashMap::new();
            expected_map.insert("a".into(), Value::Int(1));
            expected_map.insert("b".into(), Value::Int(2));
            expected_map.insert("c".into(), Value::Int(3));
            assert_eq!(
                Value::resolve(&expr, &Context::default()),
                Ok(Value::Map(Map {
                    map: Arc::from(expected_map)
                }))
            );

            let expr = Parser::default()
                .enable_optional_syntax(true)
                .parse(r#"{"a": 1, "b": 2, ?"c": optional.none()}"#)
                .expect("Must parse");
            let mut expected_map = HashMap::new();
            expected_map.insert("a".into(), Value::Int(1));
            expected_map.insert("b".into(), Value::Int(2));
            assert_eq!(
                Value::resolve(&expr, &Context::default()),
                Ok(Value::Map(Map {
                    map: Arc::from(expected_map)
                }))
            );

            let expr = Parser::default()
                .enable_optional_syntax(true)
                .parse(r#"{"a": 1, ?"b": optional.none(), ?"c": optional.of(3)}"#)
                .expect("Must parse");
            let mut expected_map = HashMap::new();
            expected_map.insert("a".into(), Value::Int(1));
            expected_map.insert("c".into(), Value::Int(3));
            assert_eq!(
                Value::resolve(&expr, &Context::default()),
                Ok(Value::Map(Map {
                    map: Arc::from(expected_map)
                }))
            );

            let expr = Parser::default()
                .enable_optional_syntax(true)
                .parse(r#"{"a": 1, ?"b": mymap[?"missing"]}"#)
                .expect("Must parse");
            let mut expected_map = HashMap::new();
            expected_map.insert("a".into(), Value::Int(1));
            assert_eq!(
                Value::resolve(&expr, &map_ctx),
                Ok(Value::Map(Map {
                    map: Arc::from(expected_map)
                }))
            );

            let expr = Parser::default()
                .enable_optional_syntax(true)
                .parse(r#"{"x": 10, ?"y": mymap[?"a"]}"#)
                .expect("Must parse");
            let mut expected_map = HashMap::new();
            expected_map.insert("x".into(), Value::Int(10));
            expected_map.insert("y".into(), Value::Int(1));
            assert_eq!(
                Value::resolve(&expr, &map_ctx),
                Ok(Value::Map(Map {
                    map: Arc::from(expected_map)
                }))
            );

            let expr = Parser::default()
                .enable_optional_syntax(true)
                .parse(r#"{?"a": optional.none(), ?"b": optional.none()}"#)
                .expect("Must parse");
            assert_eq!(
                Value::resolve(&expr, &Context::default()),
                Ok(Value::Map(Map {
                    map: Arc::from(HashMap::new())
                }))
            );
        }
    }
}
