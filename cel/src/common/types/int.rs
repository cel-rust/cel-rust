use crate::common::types::Type;
use crate::common::value::{CelVal, Val};
use crate::common::{traits, types};
use std::any::Any;

#[derive(Clone, Debug)]
pub struct Int(i64);

impl Val for Int {
    fn get_type(&self) -> Type<'_> {
        super::INT_TYPE
    }

    fn into_inner(self) -> Box<dyn Any> {
        Box::new(self.0)
    }

    fn as_adder(&self) -> Option<&dyn traits::Adder> {
        Some(self as &dyn traits::Adder)
    }

    fn clone_as_boxed(&self) -> Box<dyn Val> {
        Box::new(Int(self.0))
    }
}

impl traits::Adder for Int {
    fn add(&self, rhs: &dyn Val) -> Box<dyn Val> {
        if let Some(i) = (rhs as &dyn Any).downcast_ref::<Int>() {
            let t: types::Int = (self.0 + i.0).into();
            Box::new(t)
        } else {
            Box::new(CelVal::Error)
        }
    }
}

impl From<Int> for i64 {
    fn from(value: Int) -> Self {
        value.0
    }
}

impl From<i64> for Int {
    fn from(value: i64) -> Self {
        Self(value)
    }
}
