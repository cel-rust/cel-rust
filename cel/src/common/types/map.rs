use crate::common::traits::{Container, Indexer, Iterable, Sizer, Zeroer};
use crate::common::types::{CelBool, CelInt, CelString, CelUInt, Type};
use crate::common::value::{Builtin, BuiltinRef, CowVal, Val};
use crate::common::{traits, types};
use crate::ExecutionError;
use crate::ExecutionError::NoSuchOverload;
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
        other.downcast_ref::<DefaultMap>().is_some_and(|other| {
            self.0.len() == other.0.len()
                && self.0.iter().all(|(k, v)| {
                    other
                        .0
                        .get(k as &dyn AsKeyRef)
                        .is_some_and(|ov| v.equals(ov.as_ref()))
                })
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

fn unsupported_key(key: &dyn Val) -> ExecutionError {
    ExecutionError::UnsupportedKeyType(key.try_into().unwrap_or(crate::Value::Null))
}

fn no_such_key(key: &dyn AsKeyRef) -> ExecutionError {
    let key = match key.as_keyref() {
        KeyRef::Bool(b) => b.to_string(),
        KeyRef::Int(i) => i.to_string(),
        KeyRef::String(s) => s.to_string(),
        KeyRef::Uint(u) => u.to_string(),
    };
    ExecutionError::NoSuchKey(Arc::new(key))
}

impl Container for DefaultMap<'_> {
    fn contains(&self, key: &dyn Val) -> Result<bool, ExecutionError> {
        match key_ref(key) {
            Some(k) => Ok(self.0.contains_key(k)),
            None => Err(unsupported_key(key)),
        }
    }
}

impl<'v> Indexer for DefaultMap<'v> {
    fn get<'b, 'w>(&'b self, key: &dyn Val) -> Result<CowVal<'b, 'w>, ExecutionError>
    where
        Self: 'w,
    {
        let k = key_ref(key).ok_or(NoSuchOverload)?;
        self.0
            .get(k)
            .map(|v| CowVal::Borrowed(v.as_ref()))
            .ok_or_else(|| no_such_key(k))
    }

    fn steal<'w>(self: Box<Self>, key: &dyn Val) -> Result<Box<dyn Val + 'w>, ExecutionError>
    where
        Self: 'w,
    {
        let mut map = self;
        let k = key_ref(key).ok_or_else(|| unsupported_key(key))?;
        map.0
            .remove(k)
            .map(|v| v as Box<dyn Val + 'w>)
            .ok_or_else(|| no_such_key(k))
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
