use crate::common::types::Type;
use crate::common::value::Val;
use std::any::Any;
use std::ops::Deref;

#[derive(Clone, Debug, PartialEq)]
pub struct UInt(u64);

impl UInt {
    pub fn into_inner(self) -> u64 {
        self.0
    }

    pub fn inner(&self) -> &u64 {
        &self.0
    }
}

impl Deref for UInt {
    type Target = u64;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Val for UInt {
    fn get_type(&self) -> Type<'_> {
        super::UINT_TYPE
    }

    fn into_any(self: Box<Self>) -> Box<dyn Any> {
        Box::new(self.0)
    }

    fn clone_as_boxed(&self) -> Box<dyn Val> {
        Box::new(UInt(self.0))
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
