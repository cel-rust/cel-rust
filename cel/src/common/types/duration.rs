use crate::common::traits::{Adder, Comparer, Subtractor, Zeroer};
use crate::common::types::{CelInt, CelString, CelTimestamp, Type};
use crate::common::value::{CowVal, StaticVal, Val};
use crate::{ExecutionError, Value};
use std::any::Any;
use std::ops::Deref;

/// CEL durations are limited to what fits in an `i64` count of nanoseconds,
/// the same range as cel-go's `time.Duration`.
pub(crate) fn out_of_range(d: &chrono::Duration) -> bool {
    d.num_nanoseconds().is_none()
}

/// A CEL `google.protobuf.Duration`, of type
/// [`DURATION_TYPE`](super::DURATION_TYPE): a [`chrono::Duration`].
///
/// It adds to and subtracts from another `duration`, and adds to a
/// `timestamp`. A `duration` must fit in an `i64` count of nanoseconds, as in
/// cel-go, so an operation whose result doesn't fails. It only compares, and
/// is only equal, to another `duration`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Duration(chrono::Duration);

impl Duration {
    /// The [`chrono::Duration`].
    pub fn into_inner(self) -> chrono::Duration {
        self.0
    }

    /// A reference to the [`chrono::Duration`].
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
        <Self as Val>::cel_type()
    }

    fn cel_type() -> &'static Type {
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
            let overflow = || {
                ExecutionError::Overflow(
                    "add",
                    (self as &dyn Val).try_into().unwrap_or(Value::Null),
                    (rhs as &dyn Val).try_into().unwrap_or(Value::Null),
                )
            };
            let result = self.0.checked_add(&rhs.0).ok_or_else(overflow)?;
            if out_of_range(&result) {
                return Err(overflow());
            }
            Ok(CowVal::owned(Duration(result)))
        } else if let Some(rhs) = rhs.downcast_ref::<CelTimestamp>() {
            // duration + timestamp commutes; reuse the timestamp range checks,
            // but report the operands in the order they were written.
            rhs.add(self)
                .map(|ts| CowVal::Owned(ts.into_owned()))
                .map_err(|e| match e {
                    ExecutionError::Overflow(op, ts, d) => ExecutionError::Overflow(op, d, ts),
                    e => e,
                })
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
            Err(ExecutionError::values_not_comparable(self, rhs))
        }
    }
}

impl Subtractor for Duration {
    fn sub<'b, 'v>(&'b self, rhs: &(dyn Val + 'v)) -> Result<CowVal<'b, 'v>, ExecutionError>
    where
        Self: 'v,
    {
        if let Some(rhs) = rhs.downcast_ref::<Duration>() {
            let overflow = || {
                ExecutionError::Overflow(
                    "sub",
                    (self as &dyn Val).try_into().unwrap_or(Value::Null),
                    (rhs as &dyn Val).try_into().unwrap_or(Value::Null),
                )
            };
            let result = self.0.checked_sub(&rhs.0).ok_or_else(overflow)?;
            if out_of_range(&result) {
                return Err(overflow());
            }
            Ok(CowVal::owned(Duration(result)))
        } else {
            Err(ExecutionError::unsupported_binary_operator(
                "sub", self, rhs,
            ))
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

fn get_milliseconds(this: &Duration) -> CelInt {
    CelInt::from(this.inner().num_milliseconds())
}

fn get_seconds(this: &Duration) -> CelInt {
    CelInt::from(this.inner().num_seconds())
}

fn get_minutes(this: &Duration) -> CelInt {
    CelInt::from(this.inner().num_minutes())
}

fn get_hours(this: &Duration) -> CelInt {
    CelInt::from(this.inner().num_hours())
}

fn duration_from_string(this: &CelString<'_>) -> Result<Duration, ExecutionError> {
    let (_, d) = crate::duration::parse_duration(this.inner())
        .map_err(|e| ExecutionError::function_error("duration", e.to_string()))?;
    if out_of_range(&d) {
        return Err(ExecutionError::function_error(
            "duration",
            "range error parsing duration",
        ));
    }
    Ok(Duration::from(d))
}

fn duration_from_duration(this: &Duration) -> Duration {
    *this
}

pub(crate) fn stdlib(env: &mut crate::Env) {
    env.add_type(super::DURATION_TYPE).expect("Must be unique");
    crate::add_overload!(env, fn duration_from_string: (CelString) -> Result<Duration>,
        name = "duration", id = "string_to_duration")
    .expect("Must be unique id");
    crate::add_overload!(env, fn duration_from_duration: (Duration) -> Duration,
        name = "duration", id = "duration_to_duration")
    .expect("Must be unique id");
    crate::add_member_overload!(env, fn get_hours: (Duration) -> CelInt, id = "duration_to_hours")
        .expect("Must be unique id");
    crate::add_member_overload!(env, fn get_minutes: (Duration) -> CelInt,
        id = "duration_to_minutes")
    .expect("Must be unique id");
    crate::add_member_overload!(env, fn get_seconds: (Duration) -> CelInt,
        id = "duration_to_seconds")
    .expect("Must be unique id");
    crate::add_member_overload!(env, fn get_milliseconds: (Duration) -> CelInt,
        id = "duration_to_millis")
    .expect("Must be unique id");
}
