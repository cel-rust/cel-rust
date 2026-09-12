use crate::common::traits::Zeroer;
use crate::common::types::Type;
use crate::common::value::{StaticVal, Val};
use std::any::Any;

#[derive(Clone, Copy, Debug, Default)]
pub struct Null;

impl Val for Null {
    fn get_type(&self) -> &Type {
        &super::NULL_TYPE
    }

    fn equals(&self, other: &dyn Val) -> bool {
        other.downcast_ref::<Null>().is_some()
    }

    fn as_zeroer(&self) -> Option<&dyn Zeroer> {
        Some(self)
    }

    fn clone_as_boxed<'v>(&self) -> Box<dyn Val + 'v>
    where
        Self: 'v,
    {
        Box::new(Null)
    }

    fn as_any(&self) -> Option<&dyn Any> {
        Some(self)
    }
}

impl StaticVal for Null {}

impl Zeroer for Null {
    fn is_zero_value(&self) -> bool {
        true
    }
}
