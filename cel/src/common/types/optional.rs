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

impl Optional {
    pub fn none() -> Self {
        Optional(None)
    }

    pub fn of(val: Box<dyn Val>) -> Self {
        Optional(Some(OptionalInternal::Box(val)))
    }

    pub fn map(&self, f: impl FnOnce(&dyn Val) -> Box<dyn Val>) -> Self {
        self.0
            .as_ref()
            .map(|val| {
                let m = match val {
                    OptionalInternal::Box(b) => f(b.as_ref()),
                    OptionalInternal::Arc(a) => f(a.as_ref()),
                };
                Optional(Some(OptionalInternal::Box(m)))
            })
            .unwrap_or(Optional(None))
    }

    pub fn option(&self) -> Option<&dyn Val> {
        self.0.as_ref().map(|val| match val {
            OptionalInternal::Box(b) => b.as_ref(),
            OptionalInternal::Arc(a) => a.as_ref(),
        })
    }

    pub fn inner(&self) -> Option<&dyn Val> {
        self.0.as_ref().map(|val| match val {
            OptionalInternal::Box(b) => b.as_ref(),
            OptionalInternal::Arc(a) => a.as_ref(),
        })
    }
}

impl From<Option<Box<dyn Val>>> for Optional {
    fn from(val: Option<Box<dyn Val>>) -> Self {
        Optional(val.map(OptionalInternal::Box))
    }
}

impl From<Box<dyn Val>> for Optional {
    fn from(val: Box<dyn Val>) -> Self {
        Optional(Some(OptionalInternal::Box(val)))
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
