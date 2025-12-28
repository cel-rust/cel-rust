use crate::common::traits;
use crate::common::types::Type;
use crate::common::value::Val;
use crate::ExecutionError;
use std::borrow::Cow;
use std::ops::Deref;
use std::string::String as StdString;

#[derive(Clone, Debug, PartialEq)]
pub struct String(StdString);

impl String {
    pub fn into_inner(self) -> StdString {
        self.0
    }

    pub fn inner(&self) -> &str {
        &self.0
    }
}

impl Deref for String {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.inner()
    }
}

impl Val for String {
    fn get_type(&self) -> Type<'_> {
        super::STRING_TYPE
    }

    fn clone_as_boxed(&self) -> Box<dyn Val> {
        Box::new(String(self.0.clone()))
    }

    fn equals(&self, other: &dyn Val) -> bool {
        match other.downcast_ref::<Self>() {
            Some(s) => self.0 == s.0,
            None => false,
        }
    }
}

impl From<StdString> for String {
    fn from(v: StdString) -> Self {
        Self(v)
    }
}

impl From<String> for StdString {
    fn from(v: String) -> Self {
        v.0
    }
}

impl From<&str> for String {
    fn from(value: &str) -> Self {
        Self(StdString::from(value))
    }
}

impl traits::Adder for String {
    fn add<'a>(&self, _rhs: &'a dyn Val) -> Result<Cow<'a, dyn Val>, ExecutionError> {
        todo!("implement Adder for String!")
    }
}

impl String {
    pub fn new(str: &str) -> Self {
        Self(str.into())
    }
}
