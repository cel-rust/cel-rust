use crate::common::types::Type;
use crate::common::value::Val;
use std::sync::Arc;

#[derive(Debug)]
pub struct Optional(Option<OptionalInternal>);

#[derive(Debug)]
enum OptionalInternal {
    Box(Box<dyn Val>),
    Arc(Arc<dyn Val>),
}

impl OptionalInternal {
    fn clone_as_boxed(&self) -> Box<dyn Val> {
        match self {
            OptionalInternal::Box(val) => val.clone_as_boxed(),
            OptionalInternal::Arc(val) => val.clone_as_boxed(),
        }
    }
}

impl Val for Optional {
    fn get_type(&self) -> Type<'_> {
        super::OPTIONAL_TYPE
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
        Optional(val.map(OptionalInternal::Box))
    }
}

impl From<Option<Arc<dyn Val>>> for Optional {
    fn from(val: Option<Arc<dyn Val>>) -> Self {
        Optional(val.map(OptionalInternal::Arc))
    }
}

impl From<Optional> for Option<Box<dyn Val>> {
    fn from(val: Optional) -> Option<Box<dyn Val>> {
        val.0.map(|val| val.clone_as_boxed())
    }
}

impl From<Optional> for Option<Arc<dyn Val>> {
    fn from(val: Optional) -> Option<Arc<dyn Val>> {
        val.0.map(|i| match i {
            OptionalInternal::Arc(a) => a,
            OptionalInternal::Box(b) => Arc::from(b),
        })
    }
}
