use crate::common::ast::{operators, EntryExpr, Expr};
use crate::context::Context;
use crate::functions::FunctionContext;
use crate::{ExecutionError, Expression};
use std::cmp::Ordering;
use std::collections::HashMap;
use std::convert::{Infallible, TryFrom, TryInto};
use std::fmt::{Display, Formatter};
use std::ops;
use std::ops::Deref;
use std::sync::Arc;
#[cfg(feature = "chrono")]
use std::sync::LazyLock;

use crate::common::reference::CelVal;
#[cfg(feature = "chrono")]
use chrono::TimeZone;

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
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
pub struct Map {
    pub map: Arc<HashMap<Key, Value>>,
}

impl PartialOrd for Map {
    fn partial_cmp(&self, _: &Self) -> Option<Ordering> {
        None
    }
}

impl Map {
    /// Returns a reference to the value corresponding to the key. Implicitly converts between int
    /// and uint keys.
    pub fn get(&self, key: &Key) -> Option<&Value> {
        self.map.get(key).or_else(|| {
            // Also check keys that are cross type comparable.
            let converted = match key {
                Key::Int(k) => Key::Uint(u64::try_from(*k).ok()?),
                Key::Uint(k) => Key::Int(i64::try_from(*k).ok()?),
                _ => return None,
            };
            self.map.get(&converted)
        })
    }
}

#[derive(Debug, Eq, PartialEq, Hash, Ord, Clone, PartialOrd)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
pub enum Key {
    Int(i64),
    Uint(u64),
    Bool(bool),
    String(Arc<String>),
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

impl From<u64> for Key {
    fn from(v: u64) -> Self {
        Key::Uint(v)
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

#[derive(Debug, Clone)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
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
    Null,
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
            #[cfg(feature = "chrono")]
            Value::Duration(_) => ValueType::Duration,
            #[cfg(feature = "chrono")]
            Value::Timestamp(_) => ValueType::Timestamp,
            Value::Null => ValueType::Null,
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

    #[inline(always)]
    pub fn resolve(expr: &Expression, ctx: &Context) -> ResolveResult {
        match &expr.expr {
            Expr::Literal(val) => Ok(val.clone().into()),
            Expr::Call(call) => {
                if call.args.len() == 3 && call.func_name == operators::CONDITIONAL {
                    let cond = Value::resolve(&call.args[0], ctx)?;
                    return if cond.to_bool() {
                        Value::resolve(&call.args[1], ctx)
                    } else {
                        Value::resolve(&call.args[2], ctx)
                    };
                }
                if call.args.len() == 2 {
                    let left = Value::resolve(&call.args[0], ctx)?;
                    match call.func_name.as_str() {
                        operators::ADD => return left + Value::resolve(&call.args[1], ctx)?,
                        operators::SUBSTRACT => return left - Value::resolve(&call.args[1], ctx)?,
                        operators::DIVIDE => return left / Value::resolve(&call.args[1], ctx)?,
                        operators::MULTIPLY => return left * Value::resolve(&call.args[1], ctx)?,
                        operators::MODULO => return left % Value::resolve(&call.args[1], ctx)?,
                        operators::EQUALS => {
                            return Value::Bool(left.eq(&Value::resolve(&call.args[1], ctx)?))
                                .into()
                        }
                        operators::NOT_EQUALS => {
                            return Value::Bool(left.ne(&Value::resolve(&call.args[1], ctx)?))
                                .into()
                        }
                        operators::LESS => {
                            let right = Value::resolve(&call.args[1], ctx)?;
                            return Value::Bool(
                                left.partial_cmp(&right)
                                    .ok_or(ExecutionError::ValuesNotComparable(left, right))?
                                    == Ordering::Less,
                            )
                            .into();
                        }
                        operators::LESS_EQUALS => {
                            let right = Value::resolve(&call.args[1], ctx)?;
                            return Value::Bool(
                                left.partial_cmp(&right)
                                    .ok_or(ExecutionError::ValuesNotComparable(left, right))?
                                    != Ordering::Greater,
                            )
                            .into();
                        }
                        operators::GREATER => {
                            let right = Value::resolve(&call.args[1], ctx)?;
                            return Value::Bool(
                                left.partial_cmp(&right)
                                    .ok_or(ExecutionError::ValuesNotComparable(left, right))?
                                    == Ordering::Greater,
                            )
                            .into();
                        }
                        operators::GREATER_EQUALS => {
                            let right = Value::resolve(&call.args[1], ctx)?;
                            return Value::Bool(
                                left.partial_cmp(&right)
                                    .ok_or(ExecutionError::ValuesNotComparable(left, right))?
                                    != Ordering::Less,
                            )
                            .into();
                        }
                        operators::IN => {
                            let right = Value::resolve(&call.args[1], ctx)?;
                            match (left, right) {
                                (Value::String(l), Value::String(r)) => {
                                    return Value::Bool(r.contains(&*l)).into()
                                }
                                (any, Value::List(v)) => {
                                    return Value::Bool(v.contains(&any)).into()
                                }
                                (any, Value::Map(m)) => match any.try_into() {
                                    Ok(key) => return Value::Bool(m.map.contains_key(&key)).into(),
                                    Err(_) => return Value::Bool(false).into(),
                                },
                                (left, right) => {
                                    Err(ExecutionError::ValuesNotComparable(left, right))?
                                }
                            }
                        }
                        operators::LOGICAL_OR => {
                            return if left.to_bool() {
                                left.into()
                            } else {
                                Value::resolve(&call.args[1], ctx)
                            };
                        }
                        operators::LOGICAL_AND => {
                            return if !left.to_bool() {
                                Value::Bool(false)
                            } else {
                                let right = Value::resolve(&call.args[1], ctx)?;
                                Value::Bool(right.to_bool())
                            }
                            .into();
                        }
                        operators::INDEX => {
                            let value = left;
                            let idx = Value::resolve(&call.args[1], ctx)?;
                            return match (value, idx) {
                                (Value::List(items), Value::Int(idx)) => items
                                    .get(idx as usize)
                                    .cloned()
                                    .unwrap_or(Value::Null)
                                    .into(),
                                (Value::String(str), Value::Int(idx)) => {
                                    match str.get(idx as usize..(idx + 1) as usize) {
                                        None => Ok(Value::Null),
                                        Some(str) => Ok(Value::String(str.to_string().into())),
                                    }
                                }
                                (Value::Map(map), Value::String(property)) => map
                                    .get(&property.into())
                                    .cloned()
                                    .unwrap_or(Value::Null)
                                    .into(),
                                (Value::Map(map), Value::Bool(property)) => map
                                    .get(&property.into())
                                    .cloned()
                                    .unwrap_or(Value::Null)
                                    .into(),
                                (Value::Map(map), Value::Int(property)) => map
                                    .get(&property.into())
                                    .cloned()
                                    .unwrap_or(Value::Null)
                                    .into(),
                                (Value::Map(map), Value::UInt(property)) => map
                                    .get(&property.into())
                                    .cloned()
                                    .unwrap_or(Value::Null)
                                    .into(),
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
                        }
                        _ => (),
                    }
                }
                if call.args.len() == 1 {
                    let expr = Value::resolve(&call.args[0], ctx)?;
                    match call.func_name.as_str() {
                        operators::LOGICAL_NOT => return Ok(Value::Bool(!expr.to_bool())),
                        operators::NEGATE => {
                            return match expr {
                                Value::Int(i) => Ok(Value::Int(-i)),
                                Value::Float(f) => Ok(Value::Float(-f)),
                                value => {
                                    Err(ExecutionError::UnsupportedUnaryOperator("minus", value))
                                }
                            }
                        }
                        operators::NOT_STRICTLY_FALSE => {
                            return match expr {
                                Value::Bool(b) => Ok(Value::Bool(b)),
                                _ => Ok(Value::Bool(true)),
                            }
                        }
                        _ => (),
                    }
                }
                let func = ctx.get_function(call.func_name.as_str()).ok_or_else(|| {
                    ExecutionError::UndeclaredReference(call.func_name.clone().into())
                })?;
                match &call.target {
                    None => {
                        let mut ctx = FunctionContext::new(
                            call.func_name.clone().into(),
                            None,
                            ctx,
                            call.args.clone(),
                        );
                        (func)(&mut ctx)
                    }
                    Some(target) => {
                        let mut ctx = FunctionContext::new(
                            call.func_name.clone().into(),
                            Some(Value::resolve(target, ctx)?),
                            ctx,
                            call.args.clone(),
                        );
                        (func)(&mut ctx)
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
                    left.member(&select.field, ctx)
                }
            }
            Expr::List(list_expr) => {
                let list = list_expr
                    .elements
                    .iter()
                    .map(|i| Value::resolve(i, ctx))
                    .collect::<Result<Vec<_>, _>>()?;
                Value::List(list.into()).into()
            }
            Expr::Map(map_expr) => {
                let mut map = HashMap::with_capacity(map_expr.entries.len());
                for entry in map_expr.entries.iter() {
                    let (k, v) = match &entry.expr {
                        EntryExpr::StructField(_) => panic!("WAT?"),
                        EntryExpr::MapEntry(e) => (&e.key, &e.value),
                    };
                    let key = Value::resolve(k, ctx)?
                        .try_into()
                        .map_err(ExecutionError::UnsupportedKeyType)?;
                    let value = Value::resolve(v, ctx)?;
                    map.insert(key, value);
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
                            if !Value::resolve(&comprehension.loop_cond, &ctx)?.to_bool() {
                                break;
                            }
                            ctx.add_variable_from_value(&comprehension.iter_var, item.clone());
                            let accu = Value::resolve(&comprehension.loop_step, &ctx)?;
                            ctx.add_variable_from_value(&comprehension.accu_var, accu);
                        }
                    }
                    Value::Map(map) => {
                        for key in map.map.deref().keys() {
                            if !Value::resolve(&comprehension.loop_cond, &ctx)?.to_bool() {
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

    fn member(self, name: &str, ctx: &Context) -> ResolveResult {
        // todo! Ideally we would avoid creating a String just to create a Key for lookup in the
        // map, but this would require something like the `hashbrown` crate's `Equivalent` trait.
        let name: Arc<String> = name.to_owned().into();

        // This will always either be because we're trying to access
        // a property on self, or a method on self.
        let child = match self {
            Value::Map(ref m) => m.map.get(&name.clone().into()).cloned(),
            _ => None,
        };

        // If the property is both an attribute and a method, then we
        // give priority to the property. Maybe we can implement lookahead
        // to see if the next token is a function call?
        if let Some(child) = child {
            child.into()
        } else if ctx.has_function(&name) {
            Value::Function(name.clone(), Some(self.into())).into()
        } else {
            ExecutionError::NoSuchKey(name.clone()).into()
        }
    }

    #[inline(always)]
    fn to_bool(&self) -> bool {
        match self {
            Value::List(v) => !v.is_empty(),
            Value::Map(v) => !v.map.is_empty(),
            Value::Int(v) => *v != 0,
            Value::UInt(v) => *v != 0,
            Value::Float(v) => *v != 0.0,
            Value::String(v) => !v.is_empty(),
            Value::Bytes(v) => !v.is_empty(),
            Value::Bool(v) => *v,
            Value::Null => false,
            #[cfg(feature = "chrono")]
            Value::Duration(v) => v.num_nanoseconds().map(|n| n != 0).unwrap_or(false),
            #[cfg(feature = "chrono")]
            Value::Timestamp(v) => v.timestamp_nanos_opt().unwrap_or_default() > 0,
            Value::Function(_, _) => false,
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
        assert_eq!(result.unwrap(), Value::Null);
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
}
