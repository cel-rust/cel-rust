use crate::common::traits::{Adder, Comparer, Subtractor};
use crate::common::types::Type;
use crate::common::value::Val;
use crate::{ExecutionError, Value};
use std::borrow::Cow;
use std::ops::Deref;
use std::time::Duration as StdDuration;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Duration(StdDuration);

impl Duration {
    pub fn into_inner(self) -> StdDuration {
        self.0
    }

    pub fn inner(&self) -> &StdDuration {
        &self.0
    }
}

impl Deref for Duration {
    type Target = StdDuration;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Val for Duration {
    fn get_type(&self) -> Type<'_> {
        super::DURATION_TYPE
    }

    fn as_adder(&self) -> Option<&dyn Adder> {
        Some(self)
    }

    fn as_comparer(&self) -> Option<&dyn Comparer> {
        Some(self)
    }

    fn as_subtractor(&self) -> Option<&dyn Subtractor> {
        Some(self)
    }

    fn equals(&self, other: &dyn Val) -> bool {
        other
            .downcast_ref::<Self>()
            .is_some_and(|other| self.0 == other.0)
    }

    fn clone_as_boxed(&self) -> Box<dyn Val> {
        Box::new(Duration(self.0))
    }
}

impl Adder for Duration {
    fn add<'a>(&'a self, rhs: &dyn Val) -> Result<Cow<'a, dyn Val>, crate::ExecutionError> {
        if let Some(rhs) = rhs.downcast_ref::<Duration>() {
            Ok(Cow::<dyn Val>::Owned(Box::new(Duration(
                // todo report the proper values in the error
                self.0
                    .checked_add(rhs.0)
                    .ok_or_else(|| ExecutionError::Overflow("add", Value::Null, Value::Null))?,
            ))))
        } else {
            Err(crate::ExecutionError::UnsupportedBinaryOperator(
                "add",
                (self as &dyn Val).try_into().unwrap_or(Value::Null),
                rhs.try_into().unwrap_or(Value::Null),
            ))
        }
    }
}

impl Comparer for Duration {
    fn compare(&self, rhs: &dyn Val) -> Result<std::cmp::Ordering, ExecutionError> {
        if let Some(rhs) = rhs.downcast_ref::<Duration>() {
            Ok(self.0.cmp(&rhs.0))
        } else {
            Err(ExecutionError::NoSuchOverload)
        }
    }
}

impl Subtractor for Duration {
    fn sub<'a>(&'a self, rhs: &'_ dyn Val) -> Result<Cow<'a, dyn Val>, ExecutionError> {
        if let Some(rhs) = rhs.downcast_ref::<Duration>() {
            Ok(Cow::<dyn Val>::Owned(Box::new(Duration(
                // todo report the proper values in the error
                self.0
                    .checked_sub(rhs.0)
                    .ok_or_else(|| ExecutionError::Overflow("add", Value::Null, Value::Null))?,
            ))))
        } else {
            Err(ExecutionError::NoSuchOverload)
        }
    }
}

impl From<StdDuration> for Duration {
    fn from(duration: StdDuration) -> Self {
        Self(duration)
    }
}

impl From<Duration> for StdDuration {
    fn from(duration: Duration) -> Self {
        duration.0
    }
}

impl TryFrom<Box<dyn Val>> for StdDuration {
    type Error = Box<dyn Val>;

    fn try_from(value: Box<dyn Val>) -> Result<Self, Self::Error> {
        if let Some(d) = value.downcast_ref::<Duration>() {
            return Ok(d.0);
        }
        Err(value)
    }
}

impl<'a> TryFrom<&'a dyn Val> for &'a StdDuration {
    type Error = &'a dyn Val;
    fn try_from(value: &'a dyn Val) -> Result<Self, Self::Error> {
        if let Some(d) = value.downcast_ref::<Duration>() {
            return Ok(&d.0);
        }
        Err(value)
    }
}
