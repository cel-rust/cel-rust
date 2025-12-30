use crate::common::types;
use crate::common::types::{CelBool, CelInt, CelString, CelUInt, Type};
use crate::common::value::Val;
use crate::ExecutionError;
use std::collections::HashMap;
use std::ops::Deref;

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

    fn clone_as_boxed(&self) -> Box<dyn Val> {
        let mut map = HashMap::with_capacity(self.0.len());
        for (k, v) in self.0.iter() {
            map.insert(k.clone(), v.clone_as_boxed());
        }
        Box::new(Self(map))
    }
}

impl From<HashMap<Key, Box<dyn Val>>> for DefaultMap {
    fn from(value: HashMap<Key, Box<dyn Val>>) -> Self {
        Self(value)
    }
}

#[derive(Debug, Eq, PartialEq, Hash, Ord, Clone, PartialOrd)]
pub enum Key {
    Bool(CelBool),
    Int(CelInt),
    String(CelString),
    UInt(CelUInt),
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
                .ok_or(ExecutionError::UnsupportedKeyType(
                    value.as_ref().try_into().expect("Can't convert key!"),
                ))?,
            types::INT_TYPE => value
                .downcast_ref::<CelInt>()
                .copied()
                .map(Key::Int)
                .ok_or(ExecutionError::UnsupportedKeyType(
                    value.as_ref().try_into().expect("Can't convert key!"),
                ))?,
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
                .ok_or(ExecutionError::UnsupportedKeyType(
                    value.as_ref().try_into().expect("Can't convert key!"),
                ))?,
            _ => {
                return Err(ExecutionError::UnsupportedKeyType(
                    value.as_ref().try_into().expect("Can't convert key!"),
                ))
            }
        };
        Ok(key)
    }
}
