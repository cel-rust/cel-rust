use crate::common::types::Type;
use crate::common::value::Val;
use std::any::Any;

#[derive(Debug)]
pub struct Optional(Option<Box<dyn Val>>);

const OPTIONAL_TYPE: Type = Type::new_opaque_type("optional_type");

impl Val for Optional {
    fn get_type(&self) -> Type<'_> {
        OPTIONAL_TYPE
    }

    fn into_inner(self: Box<Self>) -> Box<dyn Any> {
        match self.0 {
            None => Box::new(None::<()>),
            Some(v) => Box::new(Some(v)),
        }
    }

    fn clone_as_boxed(&self) -> Box<dyn Val> {
        match &self.0 {
            None => Box::new(Optional(None)),
            Some(val) => val.clone_as_boxed(),
        }
    }
}

impl From<Option<Box<dyn Val>>> for Optional {
    fn from(val: Option<Box<dyn Val>>) -> Self {
        Optional(val)
    }
}

impl From<Optional> for Option<Box<dyn Val>> {
    fn from(val: Optional) -> Option<Box<dyn Val>> {
        val.0
    }
}
