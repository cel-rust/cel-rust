use crate::common::types::Type;
use crate::common::value::Val;
use crate::common::{traits, types};
use std::any::Any;
use std::borrow::Cow;
use std::ops::Deref;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Int(i64);

impl Int {
    pub fn into_inner(self) -> i64 {
        self.0
    }

    pub fn inner(&self) -> &i64 {
        &self.0
    }
}

impl Deref for Int {
    type Target = i64;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Val for Int {
    fn get_type(&self) -> Type<'_> {
        super::INT_TYPE
    }

    fn into_any(self: Box<Self>) -> Box<dyn Any> {
        Box::new(self.0)
    }

    fn as_adder(&self) -> Option<&dyn traits::Adder> {
        Some(self as &dyn traits::Adder)
    }

    fn clone_as_boxed(&self) -> Box<dyn Val> {
        Box::new(Int(self.0))
    }
}

impl traits::Adder for Int {
    fn add<'a>(&self, other: &'a dyn Val) -> Cow<'a, dyn Val> {
        if let Some(i) = other.downcast_ref::<Int>() {
            let t: types::Int = (self.0 + i.0).into();
            let b: Box<dyn Val> = Box::new(t);
            Cow::Owned(b)
        } else {
            types::Err::maybe_no_such_overload(other)
        }
    }
}

impl From<Int> for i64 {
    fn from(value: Int) -> Self {
        value.0
    }
}

impl From<i64> for Int {
    fn from(value: i64) -> Self {
        Self(value)
    }
}

impl Default for Int {
    fn default() -> Self {
        Self(i64::default())
    }
}
