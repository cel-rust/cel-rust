use crate::common::reference::Val;
use crate::common::types::Type;
use std::any::Any;

pub struct UInt(u64);

impl Val for UInt {
    fn get_type(&self) -> Type {
        super::UINT_TYPE
    }

    fn into_inner(self) -> Box<dyn Any> {
        Box::new(self.0)
    }
}

impl From<UInt> for u64 {
    fn from(value: UInt) -> Self {
        value.0
    }
}

impl From<u64> for UInt {
    fn from(value: u64) -> Self {
        Self(value)
    }
}
