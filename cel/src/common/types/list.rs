use crate::common::traits::{Adder, Indexer};
use crate::common::types::Type;
use crate::common::value::Val;
use crate::common::{traits, types};
use std::any::Any;
use std::borrow::Cow;
use std::ops::Deref;

#[derive(Debug)]
pub struct DefaultList(Vec<Box<dyn Val>>);

impl DefaultList {
    pub fn into_inner(self) -> Vec<Box<dyn Val>> {
        self.0
    }

    pub fn inner(&self) -> &[Box<dyn Val>] {
        &self.0
    }
}

impl Deref for DefaultList {
    type Target = [Box<dyn Val>];

    fn deref(&self) -> &Self::Target {
        self.inner()
    }
}

impl Val for DefaultList {
    fn get_type(&self) -> Type<'_> {
        types::LIST_TYPE
    }

    fn into_any(self: Box<Self>) -> Box<dyn Any> {
        todo!()
    }

    fn as_adder(&self) -> Option<&dyn Adder> {
        todo!()
    }

    fn as_indexer(&self) -> Option<&dyn Indexer> {
        Some(self as &dyn traits::Indexer)
    }

    fn clone_as_boxed(&self) -> Box<dyn Val> {
        let mut vec = Vec::with_capacity(self.0.len());
        for i in self.0.iter().map(|i| i.clone_as_boxed()) {
            vec.push(i);
        }
        Box::new(DefaultList(vec))
    }
}

impl Indexer for DefaultList {
    fn get<'a>(&self, _idx: &'a dyn Val) -> Cow<'a, dyn Val> {
        todo!()
    }
}
