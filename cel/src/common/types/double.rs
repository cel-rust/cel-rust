use crate::common::types::Type;
use crate::common::value::Val;
use std::any::Any;
use std::ops::Deref;

#[derive(Clone, Debug, PartialEq)]
pub struct Double(f64);

impl Deref for Double {
    type Target = f64;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Val for Double {
    fn get_type(&self) -> Type<'_> {
        super::DOUBLE_TYPE
    }

    fn into_inner(self: Box<Self>) -> Box<dyn Any> {
        Box::new(self.0)
    }

    fn clone_as_boxed(&self) -> Box<dyn Val> {
        Box::new(Double(self.0))
    }
}

impl From<Double> for f64 {
    fn from(value: Double) -> Self {
        value.0
    }
}

impl From<f64> for Double {
    fn from(value: f64) -> Self {
        Self(value)
    }
}
