use std::any::Any;
use std::ops::Deref;
use crate::common::traits::{Adder, Indexer};
use crate::common::{traits, types};
use crate::common::types::Type;
use crate::common::value::Val;

#[derive(Debug)]
pub struct DefaultList(Vec<Box<dyn Val>>);

impl Deref for DefaultList {
    type Target = [Box<dyn Val>];

    fn deref(&self) -> &Self::Target {
        self.0.deref()
    }
}

impl Val for DefaultList {
    fn get_type(&self) -> Type<'_> {
        types::LIST_TYPE
    }

    fn into_inner(self: Box<Self>) -> Box<dyn Any> {
        todo!()
    }

    fn as_adder(&self) -> Option<&dyn Adder> {
        todo!()
    }

    fn as_indexer(&self) -> Option<&dyn Indexer> {
        Some(self as &dyn traits::Indexer)
    }
}

impl Indexer for DefaultList {
    fn get(&self, idx: Box<dyn Val>) -> Box<dyn Val> {
        todo!()
    }
}