use crate::common::types::Type;
use crate::common::value::Val;
use std::any::Any;

pub struct Null;

impl Val for Null {
    fn get_type(&self) -> Type<'_> {
        super::NULL_TYPE
    }

    fn into_inner(self) -> Box<dyn Any> {
        Box::new(None::<()>)
    }

    fn clone_as_boxed(&self) -> Box<dyn Val> {
        Box::new(Null)
    }
}
