use crate::common::traits::{Container, Indexer, Iterable, Sizer, Zeroer};
use crate::common::types::{CelBool, CelDouble, CelInt, CelString, CelUInt, Type};
use crate::common::value::{Builtin, BuiltinRef, CowVal, Val};
use crate::common::{traits, types};
use crate::ExecutionError;
use std::borrow::Borrow;
use std::cmp::Ordering;
use std::collections::hash_map::Keys;
use std::collections::HashMap;
use std::hash::Hash;
use std::ops::Deref;
use std::sync::Arc;

/// A CEL map whose keys and values may borrow data for `'v`.
#[derive(Debug, Default)]
pub struct DefaultMap<'v>(HashMap<Key<'v>, Box<dyn Val + 'v>>);

impl<'v> DefaultMap<'v> {
    pub fn into_inner(self) -> HashMap<Key<'v>, Box<dyn Val + 'v>> {
        self.0
    }

    pub fn inner(&self) -> &HashMap<Key<'v>, Box<dyn Val + 'v>> {
        &self.0
    }
}

impl<'v> Deref for DefaultMap<'v> {
    type Target = HashMap<Key<'v>, Box<dyn Val + 'v>>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<'v> Clone for DefaultMap<'v> {
    fn clone(&self) -> Self {
        Self(self.0.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
    }
}

impl<'v> Val for DefaultMap<'v> {
    fn get_type(&self) -> &Type {
        <Self as Val>::cel_type()
    }

    fn cel_type() -> &'static Type {
        &types::MAP_TYPE
    }

    fn as_container(&self) -> Option<&dyn Container> {
        Some(self)
    }

    fn as_indexer<'b, 'w>(&'b self) -> Option<&'b (dyn Indexer + 'w)>
    where
        Self: 'w,
    {
        Some(self)
    }

    fn into_indexer<'w>(self: Box<Self>) -> Option<Box<dyn Indexer + 'w>>
    where
        Self: 'w,
    {
        Some(self)
    }

    fn as_iterable<'b, 'w>(&'b self) -> Option<&'b (dyn Iterable + 'w)>
    where
        Self: 'w,
    {
        Some(self)
    }

    fn as_sizer(&self) -> Option<&dyn Sizer> {
        Some(self)
    }

    fn as_zeroer(&self) -> Option<&dyn Zeroer> {
        Some(self)
    }

    fn equals(&self, other: &dyn Val) -> bool {
        // As in cel-go, each of our keys is looked up in `other` with the numeric fallbacks of
        // `lookup`, so `{1: "a"} == {1u: "a"}`.
        other.downcast_ref::<DefaultMap>().is_some_and(|other| {
            self.0.len() == other.0.len()
                && self
                    .0
                    .iter()
                    .all(|(k, v)| other.find(k.inner()).is_some_and(|ov| v.equals(ov)))
        })
    }

    fn clone_as_boxed<'w>(&self) -> Box<dyn Val + 'w>
    where
        Self: 'w,
    {
        Box::new(self.clone())
    }

    fn as_builtin<'b, 'w>(&'b self) -> BuiltinRef<'b, 'w>
    where
        Self: 'w,
    {
        BuiltinRef::Map(self)
    }

    fn into_builtin<'w>(self: Box<Self>) -> Option<Builtin<'w>>
    where
        Self: 'w,
    {
        Some(Builtin::Map(*self))
    }
}

/// Views a key value as a hashable [`AsKeyRef`] without copying it.
fn key_ref(key: &dyn Val) -> Option<&dyn AsKeyRef> {
    if let Some(s) = key.downcast_ref::<CelString>() {
        Some(s)
    } else if let Some(i) = key.downcast_ref::<CelInt>() {
        Some(i)
    } else if let Some(u) = key.downcast_ref::<CelUInt>() {
        Some(u)
    } else if let Some(b) = key.downcast_ref::<CelBool>() {
        Some(b)
    } else {
        None
    }
}

/// Whether `key` can be looked up in a map: any key type, or a double.
fn is_lookup_key(key: &dyn Val) -> bool {
    key_ref(key).is_some() || key.downcast_ref::<CelDouble>().is_some()
}

/// Probes for `key` the way cel-go does: as is first and, on a miss, as its lossless conversion
/// to the other numeric key type, so `{1u: "One"}[1]` finds `"One"`. A double is only ever
/// probed that way, as an int first and then as a uint.
fn lookup<T>(key: &dyn Val, mut probe: impl FnMut(&dyn AsKeyRef) -> Option<T>) -> Option<T> {
    key_ref(key)
        .and_then(&mut probe)
        .or_else(|| fallback_keys(key).iter().flatten().find_map(|k| probe(k)))
}

/// Whether `map` has `key`, or the same number as the other numeric key type: the spec has map
/// keys compare with numeric equality, so `{0: 1, 0u: 2}` repeats a key.
pub(crate) fn has_key<V>(map: &HashMap<Key<'_>, V>, key: &Key<'_>) -> bool {
    lookup(key.inner(), |k| map.get(k)).is_some()
}

/// The keys a numeric `key` falls back to on a miss, in the order to try them.
fn fallback_keys(key: &dyn Val) -> [Option<KeyRef<'static>>; 2] {
    if let Some(i) = key.downcast_ref::<CelInt>() {
        [u64::try_from(*i.inner()).ok().map(KeyRef::Uint), None]
    } else if let Some(u) = key.downcast_ref::<CelUInt>() {
        [i64::try_from(*u.inner()).ok().map(KeyRef::Int), None]
    } else if let Some(d) = key.downcast_ref::<CelDouble>() {
        let d = *d.inner();
        [
            double_to_int_lossless(d).map(KeyRef::Int),
            double_to_uint_lossless(d).map(KeyRef::Uint),
        ]
    } else {
        [None, None]
    }
}

/// `d` as an `i64`, if it converts without loss. Mirrors cel-go, which also rejects -2^63.
fn double_to_int_lossless(d: f64) -> Option<i64> {
    // The range check comes first as `as` saturates: `2^63 as i64` would round-trip to 2^63.
    if d > i64::MIN as f64 && d < i64::MAX as f64 {
        let i = d as i64;
        (i as f64 == d).then_some(i)
    } else {
        None
    }
}

/// `d` as a `u64`, if it converts without loss. Mirrors cel-go.
fn double_to_uint_lossless(d: f64) -> Option<u64> {
    // `u64::MAX as f64` is 2^64, which is out of range.
    if d >= 0.0 && d < u64::MAX as f64 {
        let u = d as u64;
        (u as f64 == d).then_some(u)
    } else {
        None
    }
}

fn unsupported_key(key: &dyn Val) -> ExecutionError {
    ExecutionError::UnsupportedKeyType(key.try_into().unwrap_or(crate::Value::Null))
}

fn no_such_key(key: &dyn Val) -> ExecutionError {
    let key = if let Some(s) = key.downcast_ref::<CelString>() {
        s.inner().to_string()
    } else if let Some(i) = key.downcast_ref::<CelInt>() {
        i.inner().to_string()
    } else if let Some(u) = key.downcast_ref::<CelUInt>() {
        u.inner().to_string()
    } else if let Some(d) = key.downcast_ref::<CelDouble>() {
        d.inner().to_string()
    } else if let Some(b) = key.downcast_ref::<CelBool>() {
        b.inner().to_string()
    } else {
        String::new()
    };
    ExecutionError::NoSuchKey(Arc::new(key))
}

impl<'v> DefaultMap<'v> {
    /// The value for `key`, see [`lookup`].
    fn find<'b>(&'b self, key: &dyn Val) -> Option<&'b (dyn Val + 'v)> {
        lookup(key, |k| self.0.get(k)).map(|v| v.as_ref())
    }
}

impl Container for DefaultMap<'_> {
    fn contains(&self, key: &dyn Val) -> Result<bool, ExecutionError> {
        if self.find(key).is_some() {
            Ok(true)
        } else if is_lookup_key(key) {
            Ok(false)
        } else {
            Err(unsupported_key(key))
        }
    }
}

impl<'v> Indexer for DefaultMap<'v> {
    fn get<'b, 'w>(&'b self, key: &dyn Val) -> Result<CowVal<'b, 'w>, ExecutionError>
    where
        Self: 'w,
    {
        match self.find(key) {
            Some(v) => Ok(CowVal::Borrowed(v)),
            None if is_lookup_key(key) => Err(no_such_key(key)),
            None => Err(ExecutionError::UnexpectedType {
                got: key.get_type().name().to_owned(),
                want: "map key".to_owned(),
            }),
        }
    }

    fn steal<'w>(self: Box<Self>, key: &dyn Val) -> Result<Box<dyn Val + 'w>, ExecutionError>
    where
        Self: 'w,
    {
        let mut map = self;
        match lookup(key, |k| map.0.remove(k)) {
            Some(v) => Ok(v as Box<dyn Val + 'w>),
            None if is_lookup_key(key) => Err(no_such_key(key)),
            None => Err(unsupported_key(key)),
        }
    }
}

impl<'v> Iterable for DefaultMap<'v> {
    fn iter<'b, 'w>(&'b self) -> Box<dyn traits::Iterator<'b, 'w> + 'b>
    where
        Self: 'w,
    {
        Box::new(MapKeyIterator::new(self.0.keys()))
    }
}

impl Sizer for DefaultMap<'_> {
    fn size(&self) -> CelInt {
        (self.inner().len() as i64).into()
    }
}

impl Zeroer for DefaultMap<'_> {
    fn is_zero_value(&self) -> bool {
        self.inner().is_empty()
    }
}

impl<'v> From<HashMap<Key<'v>, Box<dyn Val + 'v>>> for DefaultMap<'v> {
    fn from(value: HashMap<Key<'v>, Box<dyn Val + 'v>>) -> Self {
        Self(value)
    }
}

/// A map key. A string key may borrow its bytes for `'v`.
#[derive(Debug, Eq, Clone)]
pub enum Key<'v> {
    Bool(CelBool),
    Int(CelInt),
    String(CelString<'v>),
    UInt(CelUInt),
}

impl Hash for Key<'_> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.as_keyref().hash(state);
    }
}

impl PartialEq for Key<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.as_keyref() == other.as_keyref()
    }
}

impl PartialOrd for Key<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Key<'_> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.as_keyref().cmp(&other.as_keyref())
    }
}

impl<'v> Key<'v> {
    pub fn inner<'b>(&'b self) -> &'b (dyn Val + 'v) {
        match self {
            Key::Bool(b) => b,
            Key::Int(i) => i,
            Key::String(s) => s,
            Key::UInt(u) => u,
        }
    }

    /// Copies a borrowed string key so the key owns its bytes.
    pub fn into_static(self) -> Key<'static> {
        match self {
            Key::Bool(b) => Key::Bool(b),
            Key::Int(i) => Key::Int(i),
            Key::String(s) => Key::String(s.into_static()),
            Key::UInt(u) => Key::UInt(u),
        }
    }
}

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

impl AsKeyRef for Key<'_> {
    fn as_keyref(&self) -> KeyRef<'_> {
        match self {
            Key::Int(i) => KeyRef::Int(*i.inner()),
            Key::UInt(u) => KeyRef::Uint(*u.inner()),
            Key::Bool(b) => KeyRef::Bool(*b.inner()),
            Key::String(s) => KeyRef::String(s.inner()),
        }
    }
}

impl AsKeyRef for CelString<'_> {
    fn as_keyref(&self) -> KeyRef<'_> {
        KeyRef::String(self.inner())
    }
}

impl AsKeyRef for CelInt {
    fn as_keyref(&self) -> KeyRef<'_> {
        KeyRef::Int(*self.inner())
    }
}

impl AsKeyRef for CelUInt {
    fn as_keyref(&self) -> KeyRef<'_> {
        KeyRef::Uint(*self.inner())
    }
}

impl AsKeyRef for CelBool {
    fn as_keyref(&self) -> KeyRef<'_> {
        KeyRef::Bool(*self.inner())
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

impl<'a> Hash for dyn AsKeyRef + 'a {
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
impl<'a, 'v: 'a> Borrow<dyn AsKeyRef + 'a> for Key<'v> {
    fn borrow(&self) -> &(dyn AsKeyRef + 'a) {
        self
    }
}

impl From<bool> for Key<'_> {
    fn from(value: bool) -> Self {
        Key::Bool(value.into())
    }
}

impl From<i64> for Key<'_> {
    fn from(value: i64) -> Self {
        Key::Int(value.into())
    }
}

impl From<String> for Key<'_> {
    fn from(value: String) -> Self {
        Key::String(value.into())
    }
}

/// Borrows the `str`: no copy is made.
impl<'a> From<&'a str> for Key<'a> {
    fn from(value: &'a str) -> Self {
        Key::String(value.into())
    }
}

impl From<u64> for Key<'_> {
    fn from(value: u64) -> Self {
        Key::UInt(value.into())
    }
}

impl<'v> TryFrom<Box<dyn Val + 'v>> for Key<'v> {
    type Error = ExecutionError;

    fn try_from(value: Box<dyn Val + 'v>) -> Result<Self, Self::Error> {
        if let Some(b) = value.downcast_ref::<CelBool>() {
            return Ok(Key::Bool(*b));
        }
        if let Some(i) = value.downcast_ref::<CelInt>() {
            return Ok(Key::Int(*i));
        }
        if let Some(u) = value.downcast_ref::<CelUInt>() {
            return Ok(Key::UInt(*u));
        }
        match super::into_builtin(value) {
            Ok(Builtin::String(s)) => Ok(Key::String(s)),
            Ok(other) => Err(unsupported_key(other.into_boxed().as_ref())),
            Err(value) => Err(unsupported_key(value.as_ref())),
        }
    }
}

impl<'b, 'v> TryFrom<CowVal<'b, 'v>> for Key<'v> {
    type Error = ExecutionError;

    fn try_from(value: CowVal<'b, 'v>) -> Result<Self, Self::Error> {
        match value {
            CowVal::Owned(b) => b.try_into(),
            CowVal::Borrowed(v) => {
                if let Some(b) = v.downcast_ref::<CelBool>() {
                    Ok(Key::Bool(*b))
                } else if let Some(i) = v.downcast_ref::<CelInt>() {
                    Ok(Key::Int(*i))
                } else if let Some(u) = v.downcast_ref::<CelUInt>() {
                    Ok(Key::UInt(*u))
                } else if let Some(s) = v.downcast_ref::<CelString>() {
                    // a cheap clone when the string is itself borrowed
                    Ok(Key::String(s.clone()))
                } else {
                    Err(unsupported_key(v))
                }
            }
        }
    }
}

pub struct MapKeyIterator<'b, 'v> {
    keys: Keys<'b, Key<'v>, Box<dyn Val + 'v>>,
}

impl<'b, 'v> MapKeyIterator<'b, 'v> {
    fn new(keys: Keys<'b, Key<'v>, Box<dyn Val + 'v>>) -> Self {
        Self { keys }
    }
}

impl<'b, 'v: 'w, 'w> traits::Iterator<'b, 'w> for MapKeyIterator<'b, 'v> {
    fn next(&mut self) -> Option<&'b (dyn Val + 'w)> {
        self.keys.next().map(|k| k.inner())
    }
}

pub(crate) fn stdlib(env: &mut crate::Env) {
    env.add_type(crate::common::types::MAP_TYPE)
        .expect("Must be unique");
    env.add_overload(
        "size",
        "size_map",
        vec![super::MAP_TYPE],
        traits::adapter::sizer_size,
    )
    .expect("Must be unique id");
    env.add_member_overload(
        "size",
        "map_size",
        super::MAP_TYPE,
        vec![],
        traits::adapter::sizer_size,
    )
    .expect("Must be unique id");
}
