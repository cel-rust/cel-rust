use crate::common::types::Type;
use crate::common::value::Val;
use std::any::Any;

pub struct Bytes(Vec<u8>);

impl Val for Bytes {
    fn get_type(&self) -> Type<'_> {
        super::BYTES_TYPE
    }

    fn into_inner(self) -> Box<dyn Any> {
        Box::new(self.0)
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
