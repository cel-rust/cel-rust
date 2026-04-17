use std::{borrow::Cow, collections::BTreeMap, ops::Deref, sync::Arc};

use crate::{
    common::{
        traits::{Indexer, Zeroer},
        types::{CelString, Type},
        value::Val,
    },
    ExecutionError,
};

#[derive(Debug, Eq, PartialEq)]
pub struct Struct {
    r#type: Type,
    entries: BTreeMap<String, Arc<dyn Val>>,
}

impl Struct {
    pub fn new(name: String) -> Self {
        Self {
            r#type: Type::new_struct(name),
            entries: BTreeMap::default(),
        }
    }

    pub fn name(&self) -> &str {
        self.r#type.name()
    }

    pub fn field_value(&self, name: &str) -> Option<&dyn Val> {
        self.entries.get(name).map(Deref::deref)
    }

    pub fn add_field_value(&mut self, name: String, value: Cow<dyn Val>) {
        self.entries.insert(name, Arc::from(value.into_owned()));
    }

    pub fn field_values(&self) -> BTreeMap<String, Arc<dyn Val>> {
        self.entries.clone()
    }
}

impl Val for Struct {
    fn get_type(&self) -> &Type {
        &self.r#type
    }

    fn clone_as_boxed(&self) -> Box<dyn Val> {
        Box::new(Self {
            r#type: Type::new_struct(self.name().to_owned()),
            entries: self
                .entries
                .iter()
                .map(|(k, v)| (k.clone(), Arc::from(v.clone_as_boxed())))
                .collect(),
        })
    }

    fn as_indexer(&self) -> Option<&dyn crate::common::traits::Indexer> {
        Some(self)
    }

    fn into_indexer(self: Box<Self>) -> Option<Box<dyn crate::common::traits::Indexer>> {
        Some(self)
    }

    fn as_zeroer(&self) -> Option<&dyn Zeroer> {
        Some(self)
    }

    fn equals(&self, other: &dyn Val) -> bool {
        other
            .downcast_ref::<Struct>()
            .is_some_and(|other| self == other)
    }
}

impl Indexer for Struct {
    fn get<'a>(&'a self, idx: &dyn Val) -> Result<Cow<'a, dyn Val>, crate::ExecutionError> {
        if let Some(field) = idx.downcast_ref::<CelString>() {
            self.field_value(field.inner())
                .map(Cow::Borrowed)
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

    fn steal(self: Box<Self>, idx: &dyn Val) -> Result<Box<dyn Val>, crate::ExecutionError> {
        self.get(idx).map(Cow::into_owned)
    }
}

impl Zeroer for Struct {
    fn is_zero_value(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use crate::common::{
        types::{CelBool, CelStruct},
        value::Val,
    };

    #[test]
    fn equality() {
        let mut s1 = CelStruct::new("foo".to_owned());
        s1.add_field_value(
            "bar".to_owned(),
            Cow::<dyn Val>::Owned(Box::new(CelBool::from(true))),
        );
        let mut s2 = CelStruct::new("foo".to_owned());
        assert_ne!(s1, s2);
        s2.add_field_value(
            "bar".to_owned(),
            Cow::<dyn Val>::Owned(Box::new(CelBool::from(true))),
        );
        assert_eq!(s1, s2);
        s2.add_field_value(
            "bar".to_owned(),
            Cow::<dyn Val>::Owned(Box::new(CelBool::from(false))),
        );
        assert_ne!(s1, s2);
    }
}
