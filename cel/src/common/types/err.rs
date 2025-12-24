use crate::common::types;
use crate::common::types::Type;
use crate::common::value::Val;
use std::any::Any;
use std::borrow::Cow;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::ops::Deref;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct Err {
    msg: String,
    src: Option<Arc<dyn Error + Sync + Send + 'static>>,
    pub val: Option<Arc<dyn Val>>,
}

impl Err {
    pub fn maybe_no_such_overload(val: &dyn Val) -> Cow<dyn Val> {
        if val.get_type() == types::UNKNOWN_TYPE || val.get_type() == types::ERROR_TYPE {
            Cow::Borrowed(val)
        } else {
            let err: Box<dyn Val> = Box::new(Self::no_such_overload());
            Cow::Owned(err)
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
    fn get_type(&self) -> Type<'_> {
        types::ERROR_TYPE
    }

    fn into_inner(self: Box<Self>) -> Box<dyn Any> {
        self
    }

    fn clone_as_boxed(&self) -> Box<dyn Val> {
        Box::new(self.clone())
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

impl PartialEq for Err {
    fn eq(&self, _other: &Self) -> bool {
        false
    }
}
