use crate::common::types::Type;
use crate::common::value::Val;
use std::ops::Deref;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Double(f64);

impl Double {
    pub fn into_inner(self) -> f64 {
        self.0
    }

    pub fn inner(&self) -> &f64 {
        &self.0
    }
}

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

impl TryFrom<Box<dyn Val>> for f64 {
    type Error = Box<dyn Val>;

    fn try_from(value: Box<dyn Val>) -> Result<Self, Self::Error> {
        if let Some(d) = value.downcast_ref::<Double>() {
            return Ok(d.0);
        }
        Err(value)
    }
}

impl<'a> TryFrom<&'a dyn Val> for &'a f64 {
    type Error = &'a dyn Val;

    fn try_from(value: &'a dyn Val) -> Result<Self, Self::Error> {
        if let Some(d) = value.downcast_ref::<Double>() {
            return Ok(&d.0);
        }
        Err(value)
    }
}
