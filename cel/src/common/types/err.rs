use crate::common::types;
use crate::common::types::Type;
use crate::common::value::Val;
use std::any::Any;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::ops::Deref;

#[derive(Debug)]
pub struct Err {
    msg: String,
    src: Option<Box<dyn Error + Sync + Send + 'static>>,
    pub val: Option<Box<dyn Val>>,
}

impl Err {
    pub fn maybe_no_such_overload(val: Box<dyn Val>) -> Box<dyn Val> {
        if val.get_type() == types::UNKNOWN_TYPE || val.get_type() == types::ERROR_TYPE {
            val
        } else {
            Box::new(Self::no_such_overload())
        }
    }

    pub fn no_such_overload() -> Err {
        types::Err {
            msg: "no such overload".to_string(),
            src: None,
            val: None,
        }
    }
}

impl Val for Err {
    fn get_type(&self) -> Type {
        types::ERROR_TYPE
    }

    fn into_inner(self) -> Box<dyn Any> {
        Box::new(self)
    }
}

impl Display for Err {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "cel::Err: \"{}\"", self.msg)
    }
}

impl Error for Err {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.src
            .as_ref()
            .map(|e| e.deref() as &(dyn Error + 'static))
    }
}
