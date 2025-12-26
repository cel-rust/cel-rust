use crate::common::types::Type;
use crate::common::value::Val;
use std::any::Any;
use std::ops::Deref;

#[derive(Clone, Debug, PartialEq)]
pub struct Bytes(Vec<u8>);

impl Bytes {
    pub fn into_inner(self) -> Vec<u8> {
        self.0
    }

    pub fn inner(&self) -> &[u8] {
        &self.0
    }
}

impl Deref for Bytes {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        self.inner()
    }
}

impl Val for Bytes {
    fn get_type(&self) -> Type<'_> {
        super::BYTES_TYPE
    }

    fn into_any(self: Box<Self>) -> Box<dyn Any> {
        Box::new(self.0)
    }

    fn eq(&self, other: &dyn Val) -> bool {
        other
            .downcast_ref::<Self>()
            .map_or(false, |a| self.0.eq(&a.0))
    }

    fn clone_as_boxed(&self) -> Box<dyn Val> {
        Box::new(Bytes(self.0.clone()))
    }
}

impl From<Vec<u8>> for Bytes {
    fn from(value: Vec<u8>) -> Self {
        Bytes(value)
    }
}

impl From<Bytes> for Vec<u8> {
    fn from(value: Bytes) -> Self {
        value.0
    }
}
