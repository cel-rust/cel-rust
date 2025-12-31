use crate::common::traits::{Adder, Comparer, Subtractor};
use crate::common::types::{CelDuration, Type};
use crate::common::value::Val;
use crate::{ExecutionError, Value};
use std::borrow::Cow;
use std::cmp::Ordering;
use std::ops::{Add, Sub};
use std::sync::LazyLock;
use std::time::{Duration, SystemTime};

#[derive(Clone, Debug, PartialEq)]
pub struct Timestamp(SystemTime);

impl Timestamp {
    pub fn into_inner(self) -> SystemTime {
        self.0
    }

    pub fn inner(&self) -> &SystemTime {
        &self.0
    }
}

impl Val for Timestamp {
    fn get_type(&self) -> Type<'_> {
        super::TIMESTAMP_TYPE
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
        Box::new(Timestamp(self.0))
    }
}

static MAX_TIMESTAMP: LazyLock<SystemTime> =
    LazyLock::new(|| SystemTime::UNIX_EPOCH + Duration::from_secs(253402300799));

#[cfg(feature = "chrono")]
static MIN_TIMESTAMP: LazyLock<SystemTime> =
    LazyLock::new(|| SystemTime::UNIX_EPOCH - Duration::from_secs(62135596800));

impl Adder for Timestamp {
    fn add<'a>(&'a self, rhs: &dyn Val) -> Result<Cow<'a, dyn Val>, ExecutionError> {
        if let Some(rhs) = rhs.downcast_ref::<CelDuration>() {
            let result = self.0.add(*rhs.inner());
            if result > *MAX_TIMESTAMP || result < *MIN_TIMESTAMP {
                return Err(ExecutionError::Overflow(
                    "add",
                    (self as &dyn Val).try_into().unwrap_or(Value::Null),
                    (rhs as &dyn Val).try_into().unwrap_or(Value::Null),
                ));
            }
            Ok(Cow::<dyn Val>::Owned(Box::new(Self(result))))
        } else {
            Err(ExecutionError::UnsupportedBinaryOperator(
                "add",
                (self as &dyn Val).try_into().unwrap_or(Value::Null),
                rhs.try_into().unwrap_or(Value::Null),
            ))
        }
    }
}

impl Comparer for Timestamp {
    fn compare(&self, rhs: &dyn Val) -> Result<Ordering, ExecutionError> {
        if let Some(rhs) = rhs.downcast_ref::<Self>() {
            Ok(self.0.cmp(&rhs.0))
        } else {
            Err(ExecutionError::NoSuchOverload)
        }
    }
}

impl Subtractor for Timestamp {
    fn sub<'a>(&'a self, rhs: &'_ dyn Val) -> Result<Cow<'a, dyn Val>, ExecutionError> {
        if let Some(rhs) = rhs.downcast_ref::<CelDuration>() {
            let result = self.0.sub(*rhs.inner());
            if result > *MAX_TIMESTAMP || result < *MIN_TIMESTAMP {
                return Err(ExecutionError::Overflow(
                    "sub",
                    (self as &dyn Val).try_into().unwrap_or(Value::Null),
                    (rhs as &dyn Val).try_into().unwrap_or(Value::Null),
                ));
            }
            Ok(Cow::<dyn Val>::Owned(Box::new(Self(result))))
        } else if let Some(rhs) = rhs.downcast_ref::<Self>() {
            Ok(Cow::<dyn Val>::Owned(Box::new(CelDuration::from(
                self.0
                    .duration_since(*rhs.inner())
                    .map_err(|_| ExecutionError::Overflow("sub", Value::Null, Value::Null))?,
            ))))
        } else {
            Err(ExecutionError::UnsupportedBinaryOperator(
                "sub",
                (self as &dyn Val).try_into().unwrap_or(Value::Null),
                rhs.try_into().unwrap_or(Value::Null),
            ))
        }
    }
}

impl From<SystemTime> for Timestamp {
    fn from(system_time: SystemTime) -> Self {
        Self(system_time)
    }
}

impl From<Timestamp> for SystemTime {
    fn from(timestamp: Timestamp) -> Self {
        timestamp.0
    }
}

impl TryFrom<Box<dyn Val>> for SystemTime {
    type Error = Box<dyn Val>;

    fn try_from(value: Box<dyn Val>) -> Result<Self, Self::Error> {
        if let Some(ts) = value.downcast_ref::<Timestamp>() {
            return Ok(ts.0);
        }
        Err(value)
    }
}

impl<'a> TryFrom<&'a dyn Val> for &'a SystemTime {
    type Error = &'a dyn Val;

    fn try_from(value: &'a dyn Val) -> Result<Self, Self::Error> {
        if let Some(ts) = value.downcast_ref::<Timestamp>() {
            return Ok(&ts.0);
        }
        Err(value)
    }
}
