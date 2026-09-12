use std::{collections::BTreeMap, ops::Deref, sync::Arc};

use crate::{
    common::{
        traits::{Indexer, Zeroer},
        types::{CelString, Type},
        value::{Builtin, BuiltinRef, CowVal, Val},
    },
    ExecutionError,
};

/// A CEL struct value.
///
/// A struct has a type and a set of field values, which may borrow data
/// for `'v`.
#[derive(Debug, Eq, PartialEq)]
pub struct Struct<'v> {
    r#type: Type,
    entries: BTreeMap<String, Arc<dyn Val + 'v>>,
}

impl<'v> Struct<'v> {
    /// Creates a new struct with the given name and no fields.
    pub fn new(name: String) -> Self {
        Self {
            r#type: Type::new_struct(name),
            entries: BTreeMap::default(),
        }
    }

    /// Returns the name of the struct type.
    pub fn name(&self) -> &str {
        self.r#type.name()
    }

    /// Returns the value of the field with the given name, if it exists.
    pub fn field_value<'b>(&'b self, name: &str) -> Option<&'b (dyn Val + 'v)> {
        self.entries.get(name).map(Deref::deref)
    }

    /// Adds a field value to the struct.
    pub fn add_field_value(&mut self, name: String, value: CowVal<'_, 'v>) {
        self.entries.insert(name, Arc::from(value.into_owned()));
    }

    /// Returns a map of all field values in the struct.
    pub fn field_values(&self) -> BTreeMap<String, Arc<dyn Val + 'v>> {
        self.entries.clone()
    }

    /// Copies the struct so that it owns all of its data, converting each
    /// field through [`Value`](crate::Value).
    ///
    /// Fails when a field has no `Value` representation.
    pub fn to_static(&self) -> Result<Struct<'static>, ExecutionError> {
        let mut s = Struct::new(self.name().to_owned());
        for (k, v) in &self.entries {
            let value: crate::Value = v.as_ref().try_into()?;
            let boxed: Box<dyn Val> = value.try_into()?;
            s.entries.insert(k.clone(), Arc::from(boxed));
        }
        Ok(s)
    }
}

impl<'v> Clone for Struct<'v> {
    fn clone(&self) -> Self {
        Self {
            r#type: Type::new_struct(self.name().to_owned()),
            entries: self
                .entries
                .iter()
                .map(|(k, v)| (k.clone(), Arc::from(v.clone_as_boxed())))
                .collect(),
        }
    }
}

impl<'v> Val for Struct<'v> {
    fn get_type(&self) -> &Type {
        &self.r#type
    }

    fn clone_as_boxed<'w>(&self) -> Box<dyn Val + 'w>
    where
        Self: 'w,
    {
        Box::new(self.clone())
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

    fn as_zeroer(&self) -> Option<&dyn Zeroer> {
        Some(self)
    }

    fn equals(&self, other: &dyn Val) -> bool {
        other.downcast_ref::<Struct>().is_some_and(|other| {
            self.r#type == other.r#type
                && self.entries.len() == other.entries.len()
                && self
                    .entries
                    .iter()
                    .all(|(k, v)| other.field_value(k).is_some_and(|o| v.equals(o)))
        })
    }

    fn as_builtin<'b, 'w>(&'b self) -> BuiltinRef<'b, 'w>
    where
        Self: 'w,
    {
        BuiltinRef::Struct(self)
    }

    fn into_builtin<'w>(self: Box<Self>) -> Option<Builtin<'w>>
    where
        Self: 'w,
    {
        Some(Builtin::Struct(*self))
    }
}

impl<'v> Indexer for Struct<'v> {
    fn get<'b, 'w>(&'b self, idx: &dyn Val) -> Result<CowVal<'b, 'w>, crate::ExecutionError>
    where
        Self: 'w,
    {
        if let Some(field) = idx.downcast_ref::<CelString>() {
            self.field_value(field.inner())
                .map(CowVal::Borrowed)
                .ok_or(ExecutionError::NoSuchKey(Arc::new(String::from(
                    field.inner(),
                ))))
        } else {
            Err(ExecutionError::UnsupportedIndex(
                idx.try_into()?,
                (self as &dyn Val).try_into()?,
            ))
        }
    }

    fn steal<'w>(self: Box<Self>, idx: &dyn Val) -> Result<Box<dyn Val + 'w>, crate::ExecutionError>
    where
        Self: 'w,
    {
        let field = idx.downcast_ref::<CelString>().ok_or_else(|| {
            ExecutionError::UnsupportedIndex(
                idx.try_into().unwrap_or(crate::Value::Null),
                (&*self as &dyn Val)
                    .try_into()
                    .unwrap_or(crate::Value::Null),
            )
        })?;
        let mut this = self;
        let value = this
            .entries
            .remove(field.inner())
            .ok_or(ExecutionError::NoSuchKey(Arc::new(String::from(
                field.inner(),
            ))))?;
        // an unsized `Arc<dyn Val>` cannot be unwrapped: clone the field out
        Ok(value.clone_as_boxed())
    }
}

impl Zeroer for Struct<'_> {
    fn is_zero_value(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use crate::common::{
        types::{CelBool, CelStruct},
        value::CowVal,
    };

    #[test]
    fn equality() {
        let mut s1 = CelStruct::new("foo".to_owned());
        s1.add_field_value("bar".to_owned(), CowVal::owned(CelBool::from(true)));
        let mut s2 = CelStruct::new("foo".to_owned());
        assert_ne!(s1, s2);
        s2.add_field_value("bar".to_owned(), CowVal::owned(CelBool::from(true)));
        assert_eq!(s1, s2);
        s2.add_field_value("bar".to_owned(), CowVal::owned(CelBool::from(false)));
        assert_ne!(s1, s2);
    }
}
