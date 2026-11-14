use crate::common::decls::Signature;
use crate::common::traits::{Container, Indexer, Iterable, Sizer, Zeroer};
use crate::common::types::{
    CelBool, CelDouble, CelInt, CelString, CelUInt, Type, INT_TYPE, MAP_OF_K_V,
};
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
///
/// Of type [`MAP_TYPE`](super::MAP_TYPE), `map(dyn, dyn)`, whatever its
/// entries. Keys are [`CelMapKey`](super::CelMapKey)s: `bool`, `int`, `uint`
/// or `string` values; values can be of any type. It supports `in` (a key),
/// `[k]` and `.f` (the value at a key, `f` being a string key), `size()`, and
/// the comprehensions, which iterate over its keys in no particular order.
/// Two maps are equal when they have the same keys, with equal values.
///
/// Numeric keys are looked up by numeric value, as the spec requires:
/// `{1u: 'one'}[1]` and `{1: 'one'}[1.0]` both find `'one'`.
#[derive(Debug, Default)]
pub struct DefaultMap<'v>(HashMap<Key<'v>, Box<dyn Val + 'v>>);

impl<'v> DefaultMap<'v> {
    /// The entries.
    pub fn into_inner(self) -> HashMap<Key<'v>, Box<dyn Val + 'v>> {
        self.0
    }

    /// The entries, borrowed from `self`.
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
pub(super) fn double_to_int_lossless(d: f64) -> Option<i64> {
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

fn duplicate_key(key: &dyn Val) -> ExecutionError {
    ExecutionError::DuplicateKey(key.try_into().unwrap_or(crate::Value::Null))
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
        // As in cel-go, a value that can't be a key is not in the map, rather than an error.
        Ok(self.find(key).is_some())
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

/// A map under construction, inserted into in place.
///
/// `MutableMap` is an internal helper used by the comprehension evaluator
/// (see [`crate::objects::Value::resolve_val`]) to build up the accumulator of
/// comprehensions that insert into a map on every iteration, as
/// `@result = cel.@mapInsert(@result, k, v)` does, avoiding the quadratic
/// copy-on-insert cost a [`DefaultMap`] would incur.
///
/// Like [`MutableList`](super::MutableList), it is deliberately not a
/// [`Val`]: the evaluator owns it for the duration of the loop and never binds
/// it to a variable, so no expression can observe it half-built. When the loop
/// completes, the evaluator converts it into a [`DefaultMap`] via
/// [`MutableMap::to_immutable`].
#[derive(Debug, Default)]
#[doc(hidden)]
pub struct MutableMap<'v>(HashMap<Key<'v>, Box<dyn Val + 'v>>);

impl<'v> MutableMap<'v> {
    pub fn with_capacity(cap: usize) -> Self {
        Self(HashMap::with_capacity(cap))
    }

    /// Inserts `value` at `key`.
    ///
    /// # Errors
    ///
    /// Fails with [`ExecutionError::UnsupportedKeyType`] when `key` isn't a
    /// `bool`, `int`, `uint` or `string`, and with
    /// [`ExecutionError::DuplicateKey`] when the map already has `key`, or the
    /// same number as the other integer type: `1` and `1u` are the same key.
    pub fn insert(
        &mut self,
        key: CowVal<'_, 'v>,
        value: CowVal<'_, 'v>,
    ) -> Result<(), ExecutionError> {
        let key = Key::try_from(key)?;
        if has_key(&self.0, &key) {
            return Err(duplicate_key(key.inner()));
        }
        self.0.insert(key, value.into_owned());
        Ok(())
    }

    /// Inserts a copy of every entry of `entries`, which must be a map, each
    /// as [`insert`](Self::insert) does.
    ///
    /// # Errors
    ///
    /// Fails with [`ExecutionError::UnexpectedType`] when `entries` isn't an
    /// iterable and indexable map, and as [`insert`](Self::insert) does for an
    /// entry whose key the map already has. The entries before it stay
    /// inserted.
    pub fn extend(&mut self, entries: &(dyn Val + 'v)) -> Result<(), ExecutionError> {
        let not_a_map = || ExecutionError::UnexpectedType {
            got: entries.get_type().name().to_owned(),
            want: "map".to_owned(),
        };
        if entries.get_type().kind() != types::Kind::Map {
            return Err(not_a_map());
        }
        let (Some(keys), Some(map)) = (entries.as_iterable(), entries.as_indexer()) else {
            return Err(not_a_map());
        };
        let mut keys = keys.iter();
        while let Some(key) = keys.next() {
            self.insert(CowVal::Borrowed(key), map.get(key)?)?;
        }
        Ok(())
    }

    /// Converts the mutable map into an immutable [`DefaultMap`], reusing the
    /// backing storage.
    pub fn to_immutable(self) -> DefaultMap<'v> {
        DefaultMap(self.0)
    }
}

/// A map key. A string key may borrow its bytes for `'v`.
///
/// CEL only allows these four types as map keys; a map literal with a key of
/// another type is an error.
#[derive(Debug, Eq, Clone)]
pub enum Key<'v> {
    /// A `bool` key.
    Bool(CelBool),
    /// An `int` key.
    Int(CelInt),
    /// A `string` key.
    String(CelString<'v>),
    /// A `uint` key.
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
    /// The key as a value, e.g. to iterate over a map's keys.
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
        Signature::new(vec![MAP_OF_K_V]).returns(INT_TYPE),
        traits::adapter::sizer_size,
    )
    .expect("Must be unique id");
    env.add_member_overload(
        "size",
        "map_size",
        MAP_OF_K_V,
        Signature::new(vec![]).returns(INT_TYPE),
        traits::adapter::sizer_size,
    )
    .expect("Must be unique id");
}

#[cfg(test)]
mod tests {
    use super::{DefaultMap, Key, MutableMap};
    use crate::common::types::{self, CelDouble, CelInt, CelList, CelString, CelUInt, Type};
    use crate::common::value::{CowVal, Val};
    use crate::{Context, Env, ExecutionError, Value};
    use std::borrow::Cow;
    use std::collections::HashMap;
    use std::sync::Arc;

    /// As the spec's standard definitions declare it: `(map(A, B)) -> int`.
    #[test]
    fn size_is_declared_generic() {
        static OF_K_V: [Cow<'static, Type>; 2] = [
            Cow::Borrowed(&types::PARAM_K),
            Cow::Borrowed(&types::PARAM_V),
        ];
        assert_eq!(types::MAP_OF_K_V, Type::new_map_type(&OF_K_V));

        let env = Env::stdlib();
        for (id, member) in [("size_map", false), ("map_size", true)] {
            let overload = env
                .function("size")
                .unwrap()
                .overloads()
                .find(|overload| overload.id == id)
                .unwrap();
            assert_eq!(overload.is_member_function(), member, "{id}");
            assert_eq!(
                overload.arg_types().collect::<Vec<_>>(),
                [&types::MAP_OF_K_V],
                "{id}"
            );
            assert_eq!(overload.result_type(), &types::INT_TYPE, "{id}");
        }
    }

    /// Parameters are erased at runtime: a map's type doesn't depend on its
    /// entries, and is the type expressions name.
    #[test]
    fn values_have_the_erased_type() {
        assert_eq!(DefaultMap::default().get_type(), &types::MAP_TYPE);
        let entries: HashMap<Key, Box<dyn Val>> =
            HashMap::from([(Key::from("a"), Box::new(CelInt::from(1)) as Box<dyn Val>)]);
        assert_eq!(DefaultMap::from(entries).get_type(), &types::MAP_TYPE);

        let env = Arc::new(Env::stdlib());
        let eval = |source: &str| {
            let program = env.compile(source).unwrap();
            program.execute(&Context::with_env(Arc::clone(&env)))
        };
        assert_eq!(eval("type({}) == map"), Ok(Value::Bool(true)));
        assert_eq!(eval("type({'a': 1}) == map"), Ok(Value::Bool(true)));
        assert_eq!(eval("size({'a': 1, 'b': 2})"), Ok(Value::Int(2)));
        assert_eq!(eval("{'a': 1, 'b': 2}.size()"), Ok(Value::Int(2)));
    }

    fn int(i: i64) -> CowVal<'static, 'static> {
        CowVal::owned(CelInt::from(i))
    }

    /// `{key: value}`, a map to extend a [`MutableMap`] with.
    fn entry(key: Key<'static>, value: i64) -> DefaultMap<'static> {
        DefaultMap::from(HashMap::from([(
            key,
            Box::new(CelInt::from(value)) as Box<dyn Val>,
        )]))
    }

    #[test]
    fn mutable_map_inserts_in_place() {
        let mut m = MutableMap::with_capacity(2);
        m.insert(int(1), CowVal::owned(CelString::from("one")))
            .unwrap();
        m.insert(CowVal::owned(CelString::from("two")), int(2))
            .unwrap();

        let imm = m.to_immutable();
        assert_eq!(imm.inner().len(), 2);
        let one = imm.inner()[&Key::from(1i64)].downcast_ref::<CelString>();
        assert_eq!(one.map(|s| s.inner()), Some("one"));
        let two = imm.inner()[&Key::from("two")].downcast_ref::<CelInt>();
        assert_eq!(two.map(|i| *i.inner()), Some(2));
    }

    /// Keys compare as numbers, as in a map literal: `1u` is the key `1`.
    #[test]
    fn mutable_map_insert_rejects_a_key_it_has() {
        let mut m = MutableMap::with_capacity(1);
        m.insert(int(1), int(10)).unwrap();

        assert_eq!(
            m.insert(int(1), int(20)),
            Err(ExecutionError::DuplicateKey(Value::Int(1)))
        );
        assert_eq!(
            m.insert(CowVal::owned(CelUInt::from(1u64)), int(20)),
            Err(ExecutionError::DuplicateKey(Value::UInt(1)))
        );
        let imm = m.to_immutable();
        assert_eq!(imm.inner().len(), 1);
        let kept = imm.inner()[&Key::from(1i64)].downcast_ref::<CelInt>();
        assert_eq!(kept.map(|i| *i.inner()), Some(10));
    }

    #[test]
    fn mutable_map_insert_requires_a_key_type() {
        let mut m = MutableMap::with_capacity(0);
        assert_eq!(
            m.insert(CowVal::owned(CelDouble::from(1.0)), int(1)),
            Err(ExecutionError::UnsupportedKeyType(Value::Float(1.0)))
        );
    }

    #[test]
    fn mutable_map_extends_with_the_entries_of_a_map() {
        let mut m = MutableMap::with_capacity(0);
        m.extend(&entry(Key::from(1i64), 10)).unwrap();
        m.extend(&entry(Key::from("a"), 20)).unwrap();

        assert_eq!(
            m.extend(&entry(Key::from(1u64), 30)),
            Err(ExecutionError::DuplicateKey(Value::UInt(1)))
        );
        assert_eq!(m.to_immutable().inner().len(), 2);
    }

    #[test]
    fn mutable_map_extend_requires_a_map() {
        let mut m = MutableMap::with_capacity(0);
        let list = CelList::from(vec![Box::new(CelInt::from(1i64)) as Box<dyn Val>]);
        assert_eq!(
            m.extend(&list),
            Err(ExecutionError::UnexpectedType {
                got: "list".to_owned(),
                want: "map".to_owned(),
            })
        );
    }
}
