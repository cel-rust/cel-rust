use crate::common::traits;
use crate::common::types::Type;
use crate::common::value::Val;
use crate::ExecutionError;
use std::borrow::Cow;
use std::cmp::Ordering;
use std::ops::Deref;
use crate::common::traits::Subtractor;

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

    fn as_adder(&self) -> Option<&dyn traits::Adder> {
        Some(self as &dyn traits::Adder)
    }

    fn as_comparer(&self) -> Option<&dyn traits::Comparer> {
        Some(self as &dyn traits::Comparer)
    }

    fn as_subtractor(&self) -> Option<&dyn Subtractor> {
        Some(self as &dyn Subtractor)
    }

    fn clone_as_boxed(&self) -> Box<dyn Val> {
        Box::new(Int(self.0))
    }
}

impl traits::Adder for Int {
    fn add<'a>(&self, other: &'a dyn Val) -> Result<Cow<'a, dyn Val>, ExecutionError> {
        if let Some(i) = other.downcast_ref::<Int>() {
            let t: Self = (self.0 + i.0).into();
            let b: Box<dyn Val> = Box::new(t);
            Ok(Cow::Owned(b))
        } else {
            Err(ExecutionError::NoSuchOverload)
        }
    }
}

impl traits::Comparer for Int {
    fn compare(&self, rhs: &dyn Val) -> Result<Ordering, ExecutionError> {
        if let Some(i) = rhs.downcast_ref::<Int>() {
            Ok(self.0.cmp(&i.0))
        } else {
            Err(ExecutionError::NoSuchOverload)
        }
    }
}

impl traits::Subtractor for Int {
    fn sub<'a>(&'a self, rhs: &dyn Val) -> Result<Cow<'a, dyn Val>, ExecutionError> {
        if let Some(i) = rhs.downcast_ref::<Int>() {
            Ok(Cow::<dyn Val>::Owned(Box::new(Self::from(self.0 - i.0))))
        } else {
            Err(ExecutionError::NoSuchOverload)
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
