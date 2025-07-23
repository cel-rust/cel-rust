use crate::common::reference::Val;
use crate::common::types::Type;
use std::any::Any;

pub struct Null;

impl Val for Null {
    fn get_type(&self) -> Type {
        super::NULL_TYPE
    }

    fn into_inner(self) -> Box<dyn Any> {
        Box::new(None::<()>)
    }
}
