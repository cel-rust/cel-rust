use crate::common::traits::{Adder, Comparer, Subtractor, Zeroer};
use crate::common::types::{CelInt, Type};
use crate::common::value::{CowVal, StaticVal, Val};
use crate::{ExecutionError, Value};
use std::any::Any;
use std::ops::Deref;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Duration(chrono::Duration);

impl Duration {
    pub fn into_inner(self) -> chrono::Duration {
        self.0
    }

    pub fn inner(&self) -> &chrono::Duration {
        &self.0
    }
}

impl Deref for Duration {
    type Target = chrono::Duration;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Val for Duration {
    fn get_type(&self) -> &Type {
        &super::DURATION_TYPE
    }

    fn as_adder<'b, 'v>(&'b self) -> Option<&'b (dyn Adder + 'v)>
    where
        Self: 'v,
    {
        Some(self)
    }

    fn as_comparer(&self) -> Option<&dyn Comparer> {
        Some(self)
    }

    fn as_subtractor<'b, 'v>(&'b self) -> Option<&'b (dyn Subtractor + 'v)>
    where
        Self: 'v,
    {
        Some(self)
    }

    fn as_zeroer(&self) -> Option<&dyn Zeroer> {
        Some(self)
    }

    fn equals(&self, other: &dyn Val) -> bool {
        other
            .downcast_ref::<Self>()
            .is_some_and(|other| self.0 == other.0)
    }

    fn clone_as_boxed<'v>(&self) -> Box<dyn Val + 'v>
    where
        Self: 'v,
    {
        Box::new(*self)
    }

    fn as_any(&self) -> Option<&dyn Any> {
        Some(self)
    }
}

impl StaticVal for Duration {}

impl Adder for Duration {
    fn add<'b, 'v>(&'b self, rhs: &(dyn Val + 'v)) -> Result<CowVal<'b, 'v>, ExecutionError>
    where
        Self: 'v,
    {
        if let Some(rhs) = rhs.downcast_ref::<Duration>() {
            Ok(CowVal::owned(Duration(
                // todo report the proper values in the error
                self.0
                    .checked_add(&rhs.0)
                    .ok_or_else(|| ExecutionError::Overflow("add", Value::Null, Value::Null))?,
            )))
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
    fn sub<'b, 'v>(&'b self, rhs: &(dyn Val + 'v)) -> Result<CowVal<'b, 'v>, ExecutionError>
    where
        Self: 'v,
    {
        if let Some(rhs) = rhs.downcast_ref::<Duration>() {
            Ok(CowVal::owned(Duration(
                // todo report the proper values in the error
                self.0
                    .checked_sub(&rhs.0)
                    .ok_or_else(|| ExecutionError::Overflow("add", Value::Null, Value::Null))?,
            )))
        } else {
            Err(ExecutionError::NoSuchOverload)
        }
    }
}

impl Zeroer for Duration {
    fn is_zero_value(&self) -> bool {
        self.0.is_zero()
    }
}

impl From<chrono::Duration> for Duration {
    fn from(duration: chrono::Duration) -> Self {
        Self(duration)
    }
}

impl From<Duration> for chrono::Duration {
    fn from(duration: Duration) -> Self {
        duration.0
    }
}

impl<'v> TryFrom<Box<dyn Val + 'v>> for chrono::Duration {
    type Error = Box<dyn Val + 'v>;

    fn try_from(value: Box<dyn Val + 'v>) -> Result<Self, Self::Error> {
        if let Some(d) = value.downcast_ref::<Duration>() {
            return Ok(d.0);
        }
        Err(value)
    }
}

impl<'a, 'v> TryFrom<&'a (dyn Val + 'v)> for &'a chrono::Duration {
    type Error = &'a (dyn Val + 'v);
    fn try_from(value: &'a (dyn Val + 'v)) -> Result<Self, Self::Error> {
        if let Some(d) = value.downcast_ref::<Duration>() {
            return Ok(&d.0);
        }
        Err(value)
    }
}

fn millis<'b, 'v>(args: Vec<CowVal<'b, 'v>>) -> Result<CowVal<'b, 'v>, ExecutionError> {
    super::unary_fn(args, super::DURATION_TYPE, |ts: &Duration| {
        Ok(Box::new(CelInt::from(ts.inner().num_milliseconds())))
    })
}

fn seconds<'b, 'v>(args: Vec<CowVal<'b, 'v>>) -> Result<CowVal<'b, 'v>, ExecutionError> {
    super::unary_fn(args, super::DURATION_TYPE, |ts: &Duration| {
        Ok(Box::new(CelInt::from(ts.inner().num_seconds())))
    })
}

fn minutes<'b, 'v>(args: Vec<CowVal<'b, 'v>>) -> Result<CowVal<'b, 'v>, ExecutionError> {
    super::unary_fn(args, super::DURATION_TYPE, |ts: &Duration| {
        Ok(Box::new(CelInt::from(ts.inner().num_minutes())))
    })
}

fn hours<'b, 'v>(args: Vec<CowVal<'b, 'v>>) -> Result<CowVal<'b, 'v>, ExecutionError> {
    super::unary_fn(args, super::DURATION_TYPE, |ts: &Duration| {
        Ok(Box::new(CelInt::from(ts.inner().num_hours())))
    })
}

fn duration<'b, 'v>(args: Vec<CowVal<'b, 'v>>) -> Result<CowVal<'b, 'v>, ExecutionError> {
    super::string_fn(args, |value: &str| {
        let (_, duration) = crate::duration::parse_duration(value)
            .map_err(|e| ExecutionError::function_error("duration", e.to_string()))?;
        Ok(Box::new(Duration::from(duration)))
    })
}

pub(crate) fn stdlib(env: &mut crate::Env) {
    env.add_overload(
        "duration",
        "string_to_duration",
        vec![super::STRING_TYPE],
        duration,
    )
    .expect("Must be unique");
    env.add_overload(
        "duration",
        "duration_to_duration",
        vec![super::DURATION_TYPE],
        super::noop,
    )
    .expect("Must be unique");
    env.add_member_overload(
        "getHours",
        "duration_to_hours",
        super::DURATION_TYPE,
        Vec::default(),
        hours,
    )
    .expect("Must be unique");
    env.add_member_overload(
        "getMinutes",
        "duration_to_minutes",
        super::DURATION_TYPE,
        Vec::default(),
        minutes,
    )
    .expect("Must be unique");
    env.add_member_overload(
        "getSeconds",
        "duration_to_seconds",
        super::DURATION_TYPE,
        Vec::default(),
        seconds,
    )
    .expect("Must be unique");
    env.add_member_overload(
        "getMilliseconds",
        "duration_to_millis",
        super::DURATION_TYPE,
        Vec::default(),
        millis,
    )
    .expect("Must be unique");
}
