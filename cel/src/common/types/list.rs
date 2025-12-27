use crate::common::traits::{Adder, Indexer};
use crate::common::types::{CelErr, CelInt, CelUInt, Type};
use crate::common::value::Val;
use crate::common::{traits, types};
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

    fn as_adder(&self) -> Option<&dyn Adder> {
        todo!()
    }

    fn as_indexer(&self) -> Option<&dyn Indexer> {
        Some(self as &dyn Indexer)
    }

    fn into_indexer(self: Box<Self>) -> Option<Box<dyn Indexer>> {
        Some(self)
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
    fn get<'a>(&'a self, idx: &dyn Val) -> Cow<'a, dyn Val> {
        match idx.get_type() {
            types::INT_TYPE => {
                let idx: i64 = idx.downcast_ref::<CelInt>().unwrap().inner().clone();
                Cow::Borrowed(self.0.get(idx as usize).unwrap().as_ref())
            }
            types::UINT_TYPE => {
                let idx: u64 = idx.downcast_ref::<CelUInt>().unwrap().inner().clone();
                Cow::Borrowed(self.0.get(idx as usize).unwrap().as_ref())
            }
            _ => Cow::<dyn Val>::Owned(Box::new(CelErr::no_such_overload())),
        }
    }

    fn steal(self: Box<Self>, idx: &dyn Val) -> Box<dyn Val> {
        let mut list = self;
        match idx.get_type() {
            types::INT_TYPE => {
                let idx: i64 = idx.downcast_ref::<CelInt>().unwrap().inner().clone();
                list.0.remove(idx as usize)
            }
            types::UINT_TYPE => {
                let idx: u64 = idx.downcast_ref::<CelUInt>().unwrap().inner().clone();
                list.0.remove(idx as usize)
            }
            _ => Box::new(CelErr::no_such_overload()),
        }
    }
}
