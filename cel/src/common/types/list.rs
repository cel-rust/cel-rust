use crate::common::traits::{Adder, Indexer};
use crate::common::types::{CelErr, CelInt, CelUInt, Type};
use crate::common::value::Val;
use crate::common::{traits, types};
use crate::ExecutionError;
use crate::ExecutionError::NoSuchOverload;
use std::borrow::Cow;
use std::ops::Deref;

#[derive(Debug)]
pub struct DefaultList(Vec<Box<dyn Val>>);

impl DefaultList {
    pub fn new(items: Vec<Box<dyn Val>>) -> Self {
        Self(items)
    }

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
    fn get<'a>(&'a self, idx: &dyn Val) -> Result<Cow<'a, dyn Val>, ExecutionError> {
        match idx.get_type() {
            types::INT_TYPE => {
                let idx: i64 = idx
                    .downcast_ref::<CelInt>()
                    .expect("We need an Indexer!")
                    .inner()
                    .clone();
                Ok(Cow::Borrowed(
                    self.0
                        .get(idx as usize)
                        .ok_or(ExecutionError::IndexOutOfBounds(idx.into()))?
                        .as_ref(),
                ))
            }
            types::UINT_TYPE => {
                let idx: u64 = idx
                    .downcast_ref::<CelUInt>()
                    .expect("We need an Indexer!")
                    .inner()
                    .clone();
                Ok(Cow::Borrowed(
                    self.0
                        .get(idx as usize)
                        .ok_or(ExecutionError::IndexOutOfBounds(idx.into()))?
                        .as_ref(),
                ))
            }
            _ => Err(ExecutionError::UnexpectedType {
                got: idx.get_type().runtime_type_name.to_string(),
                want: "(INT|UINT)".to_string(),
            }),
        }
    }

    fn steal(self: Box<Self>, idx: &dyn Val) -> Result<Box<dyn Val>, ExecutionError> {
        let mut list = self;
        match idx.get_type() {
            types::INT_TYPE => {
                let idx: i64 = idx.downcast_ref::<CelInt>().unwrap().inner().clone();
                if idx < 0 || idx as usize >= list.0.len() {
                    return Err(ExecutionError::IndexOutOfBounds(idx.into()));
                }
                Ok(list.0.remove(idx as usize))
            }
            types::UINT_TYPE => {
                let idx: u64 = idx.downcast_ref::<CelUInt>().unwrap().inner().clone();
                if idx as usize >= list.0.len() {
                    return Err(ExecutionError::IndexOutOfBounds(idx.into()));
                }
                Ok(list.0.swap_remove(idx as usize))
            }
            _ => Err(ExecutionError::UnexpectedType {
                got: idx.get_type().runtime_type_name.to_string(),
                want: "(INT|UINT)".to_string(),
            }),
        }
    }
}
