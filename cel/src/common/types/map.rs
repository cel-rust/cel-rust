use crate::common::traits::{Container, Indexer, Iterable};
use crate::common::types::{CelBool, CelInt, CelString, CelUInt, Type};
use crate::common::value::Val;
use crate::common::{traits, types};
use crate::ExecutionError;
use crate::ExecutionError::NoSuchOverload;
use std::borrow::{Borrow, Cow};
use std::cmp::Ordering;
use std::collections::hash_map::Keys;
use std::collections::HashMap;
use std::hash::Hash;
use std::ops::Deref;
use std::sync::Arc;

#[derive(Debug, Default)]
pub struct DefaultMap(HashMap<Key, Box<dyn Val>>);

impl DefaultMap {
    pub fn into_inner(self) -> HashMap<Key, Box<dyn Val>> {
        self.0
    }

    pub fn inner(&self) -> &HashMap<Key, Box<dyn Val>> {
        &self.0
    }
}

impl Deref for DefaultMap {
    type Target = HashMap<Key, Box<dyn Val>>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Val for DefaultMap {
    fn get_type(&self) -> Type<'_> {
        types::MAP_TYPE
    }

    fn as_container(&self) -> Option<&dyn Container> {
        Some(self)
    }

    fn as_indexer(&self) -> Option<&dyn Indexer> {
        Some(self)
    }

    fn into_indexer(self: Box<Self>) -> Option<Box<dyn Indexer>> {
        Some(self)
    }

    fn as_iterable(&self) -> Option<&dyn Iterable> {
        Some(self)
    }

    fn equals(&self, other: &dyn Val) -> bool {
        other
            .downcast_ref::<Self>()
            .is_some_and(|other| self.0 == other.0)
    }

    fn clone_as_boxed(&self) -> Box<dyn Val> {
        let mut map = HashMap::with_capacity(self.0.len());
        for (k, v) in self.0.iter() {
            map.insert(k.clone(), v.clone_as_boxed());
        }
        Box::new(Self(map))
    }
}

impl Container for DefaultMap {
    fn contains(&self, key: &dyn Val) -> Result<bool, ExecutionError> {
        if let Some(s) = key.downcast_ref::<CelString>() {
            Ok(self.0.contains_key(s as &dyn AsKeyRef))
        } else if let Some(i) = key.downcast_ref::<CelInt>() {
            Ok(self.0.contains_key(i as &dyn AsKeyRef))
        } else if let Some(u) = key.downcast_ref::<CelUInt>() {
            Ok(self.0.contains_key(u as &dyn AsKeyRef))
        } else if let Some(b) = key.downcast_ref::<CelBool>() {
            Ok(self.0.contains_key(b as &dyn AsKeyRef))
        } else {
            let key: Key = key.clone_as_boxed().try_into()?;
            Ok(self.0.contains_key(&key))
        }
    }
}

impl Indexer for DefaultMap {
    fn get<'a>(&'a self, key: &dyn Val) -> Result<Cow<'a, dyn Val>, ExecutionError> {
        let k = if let Some(s) = key.downcast_ref::<CelString>() {
            s as &dyn AsKeyRef
        } else if let Some(i) = key.downcast_ref::<CelInt>() {
            i as &dyn AsKeyRef
        } else if let Some(u) = key.downcast_ref::<CelUInt>() {
            u as &dyn AsKeyRef
        } else if let Some(b) = key.downcast_ref::<CelBool>() {
            b as &dyn AsKeyRef
        } else {
            return Err(NoSuchOverload);
        };

        self.0
            .get(k)
            .map(|v| Cow::Borrowed(v.as_ref()))
            .ok_or_else(|| {
                let key = match key.clone_as_boxed().try_into().unwrap() {
                    Key::Bool(b) => b.into_inner().to_string(),
                    Key::Int(i) => i.into_inner().to_string(),
                    Key::String(s) => s.into_inner(),
                    Key::UInt(u) => u.into_inner().to_string(),
                };
                ExecutionError::NoSuchKey(Arc::new(key))
            })
    }

    fn steal(self: Box<Self>, key: &dyn Val) -> Result<Box<dyn Val>, ExecutionError> {
        let mut map = self;
        let key: Key = key.clone_as_boxed().try_into()?;
        map.0.remove(&key).ok_or_else(|| {
            let key = match key {
                Key::Bool(b) => b.into_inner().to_string(),
                Key::Int(i) => i.into_inner().to_string(),
                Key::String(s) => s.into_inner(),
                Key::UInt(u) => u.into_inner().to_string(),
            };
            ExecutionError::NoSuchKey(Arc::new(key))
        })
    }
}

impl Iterable for DefaultMap {
    fn iter<'a>(&'a self) -> Box<dyn super::traits::Iterator<'a> + 'a> {
        Box::new(MapKeyIterator::new(self.0.keys()))
    }
}

impl From<HashMap<Key, Box<dyn Val>>> for DefaultMap {
    fn from(value: HashMap<Key, Box<dyn Val>>) -> Self {
        Self(value)
    }
}

#[derive(Debug, Eq, Clone)]
pub enum Key {
    Bool(CelBool),
    Int(CelInt),
    String(CelString),
    UInt(CelUInt),
}

impl Hash for Key {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.as_keyref().hash(state);
    }
}

impl PartialEq for Key {
    fn eq(&self, other: &Self) -> bool {
        self.as_keyref() == other.as_keyref()
    }
}

impl PartialOrd for Key {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Key {
    fn cmp(&self, other: &Self) -> Ordering {
        self.as_keyref().cmp(&other.as_keyref())
    }
}

impl Key {
    pub fn inner(&self) -> &dyn Val {
        match self {
            Key::Bool(b) => b,
            Key::Int(i) => i,
            Key::String(s) => s,
            Key::UInt(u) => u,
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

impl AsKeyRef for Key {
    fn as_keyref(&self) -> KeyRef<'_> {
        match self {
            Key::Int(i) => KeyRef::Int(*i.inner()),
            Key::UInt(u) => KeyRef::Uint(*u.inner()),
            Key::Bool(b) => KeyRef::Bool(*b.inner()),
            Key::String(s) => KeyRef::String(s.inner()),
        }
    }
}

impl AsKeyRef for CelString {
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
impl<'a> Borrow<dyn AsKeyRef + 'a> for Key {
    fn borrow(&self) -> &(dyn AsKeyRef + 'a) {
        self
    }
}

impl From<bool> for Key {
    fn from(value: bool) -> Self {
        Key::Bool(value.into())
    }
}

impl From<i64> for Key {
    fn from(value: i64) -> Self {
        Key::Int(value.into())
    }
}

impl From<String> for Key {
    fn from(value: String) -> Self {
        Key::String(value.into())
    }
}

impl From<&str> for Key {
    fn from(value: &str) -> Self {
        Key::String(value.into())
    }
}

impl From<u64> for Key {
    fn from(value: u64) -> Self {
        Key::UInt(value.into())
    }
}

impl TryFrom<Box<dyn Val>> for Key {
    type Error = ExecutionError;

    fn try_from(value: Box<dyn Val>) -> Result<Self, Self::Error> {
        let key = match value.get_type() {
            types::BOOL_TYPE => value
                .downcast_ref::<CelBool>()
                .copied()
                .map(Key::Bool)
                .ok_or_else(|| {
                    ExecutionError::UnsupportedKeyType(
                        value.as_ref().try_into().expect("Can't convert key!"),
                    )
                })?,
            types::INT_TYPE => value
                .downcast_ref::<CelInt>()
                .copied()
                .map(Key::Int)
                .ok_or_else(|| {
                    ExecutionError::UnsupportedKeyType(
                        value.as_ref().try_into().expect("Can't convert key!"),
                    )
                })?,
            types::STRING_TYPE => {
                let s = super::cast_boxed::<CelString>(value).map_err(|v| {
                    ExecutionError::UnsupportedKeyType(
                        v.as_ref().try_into().expect("Can't convert key!"),
                    )
                })?;
                Key::String(s.into_inner().into())
            }
            types::UINT_TYPE => value
                .downcast_ref::<CelUInt>()
                .copied()
                .map(Key::UInt)
                .ok_or_else(|| {
                    ExecutionError::UnsupportedKeyType(
                        value.as_ref().try_into().expect("Can't convert key!"),
                    )
                })?,
            _ => {
                return Err(ExecutionError::UnsupportedKeyType(
                    value.as_ref().try_into().expect("Can't convert key!"),
                ))
            }
        };
        Ok(key)
    }
}

pub struct MapKeyIterator<'a> {
    keys: Keys<'a, Key, Box<dyn Val>>,
}

impl<'a> MapKeyIterator<'a> {
    fn new(keys: Keys<'a, Key, Box<dyn Val>>) -> Self {
        Self { keys }
    }
}

impl<'a> traits::Iterator<'a> for MapKeyIterator<'a> {
    fn next(&mut self) -> Option<&'a dyn Val> {
        self.keys.next().map(|k| k.inner())
    }
}
