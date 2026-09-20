use crate::common::traits::{Adder, Comparer, Subtractor, Zeroer};
use crate::common::types::{CelDuration, CelInt, CelString, Type};
use crate::common::value::{CowVal, StaticVal, Val};
use crate::{ExecutionError, Value};
use chrono::{Datelike, Days, Months};
use chrono::{TimeZone, Timelike};
use std::any::Any;
use std::cmp::Ordering;
use std::ops::{Add, Sub};
use std::sync::LazyLock;

#[derive(Clone, Debug, PartialEq)]
pub struct Timestamp(chrono::DateTime<chrono::FixedOffset>);

impl Timestamp {
    pub fn into_inner(self) -> chrono::DateTime<chrono::FixedOffset> {
        self.0
    }

    pub fn inner(&self) -> &chrono::DateTime<chrono::FixedOffset> {
        &self.0
    }
}

impl Val for Timestamp {
    fn get_type(&self) -> &Type {
        <Self as Val>::cel_type()
    }

    fn cel_type() -> &'static Type {
        &super::TIMESTAMP_TYPE
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
        Box::new(Timestamp(self.0))
    }

    fn as_any(&self) -> Option<&dyn Any> {
        Some(self)
    }
}

impl StaticVal for Timestamp {}

/// Timestamp values are limited to the range of values which can be serialized as a string:
/// `["0001-01-01T00:00:00Z", "9999-12-31T23:59:59.999999999Z"]`. Since the max is a smaller
/// and the min is a larger timestamp than what is possible to represent with
/// [`chrono::DateTime`],
/// we need to perform our own spec-compliant overflow checks.
///
/// <https://github.com/google/cel-spec/blob/master/doc/langdef.md#overflow>
static MAX_TIMESTAMP: LazyLock<chrono::DateTime<chrono::FixedOffset>> = LazyLock::new(|| {
    let naive = chrono::NaiveDate::from_ymd_opt(9999, 12, 31)
        .unwrap()
        .and_hms_nano_opt(23, 59, 59, 999_999_999)
        .unwrap();
    chrono::FixedOffset::east_opt(0)
        .unwrap()
        .from_utc_datetime(&naive)
});

static MIN_TIMESTAMP: LazyLock<chrono::DateTime<chrono::FixedOffset>> = LazyLock::new(|| {
    let naive = chrono::NaiveDate::from_ymd_opt(1, 1, 1)
        .unwrap()
        .and_hms_opt(0, 0, 0)
        .unwrap();
    chrono::FixedOffset::east_opt(0)
        .unwrap()
        .from_utc_datetime(&naive)
});

impl Adder for Timestamp {
    fn add<'b, 'v>(&'b self, rhs: &(dyn Val + 'v)) -> Result<CowVal<'b, 'v>, ExecutionError>
    where
        Self: 'v,
    {
        if let Some(rhs) = rhs.downcast_ref::<CelDuration>() {
            let result = self.0.add(*rhs.inner());
            if result > *MAX_TIMESTAMP || result < *MIN_TIMESTAMP {
                return Err(ExecutionError::Overflow(
                    "add",
                    (self as &dyn Val).try_into().unwrap_or(Value::Null),
                    (rhs as &dyn Val).try_into().unwrap_or(Value::Null),
                ));
            }
            Ok(CowVal::owned(Self(result)))
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
            Err(ExecutionError::values_not_comparable(self, rhs))
        }
    }
}

impl Subtractor for Timestamp {
    fn sub<'b, 'v>(&'b self, rhs: &(dyn Val + 'v)) -> Result<CowVal<'b, 'v>, ExecutionError>
    where
        Self: 'v,
    {
        if let Some(rhs) = rhs.downcast_ref::<CelDuration>() {
            let result = self.0.sub(*rhs.inner());
            if result > *MAX_TIMESTAMP || result < *MIN_TIMESTAMP {
                return Err(ExecutionError::Overflow(
                    "sub",
                    (self as &dyn Val).try_into().unwrap_or(Value::Null),
                    (rhs as &dyn Val).try_into().unwrap_or(Value::Null),
                ));
            }
            Ok(CowVal::owned(Self(result)))
        } else if let Some(rhs) = rhs.downcast_ref::<Self>() {
            Ok(CowVal::owned(CelDuration::from(
                self.0.signed_duration_since(rhs.inner()),
            )))
        } else {
            Err(ExecutionError::UnsupportedBinaryOperator(
                "sub",
                (self as &dyn Val).try_into().unwrap_or(Value::Null),
                rhs.try_into().unwrap_or(Value::Null),
            ))
        }
    }
}

impl Zeroer for Timestamp {
    fn is_zero_value(&self) -> bool {
        self.0.timestamp_nanos_opt().is_some_and(|ns| ns == 0)
    }
}

impl From<chrono::DateTime<chrono::FixedOffset>> for Timestamp {
    fn from(system_time: chrono::DateTime<chrono::FixedOffset>) -> Self {
        Self(system_time)
    }
}

impl From<Timestamp> for chrono::DateTime<chrono::FixedOffset> {
    fn from(timestamp: Timestamp) -> Self {
        timestamp.0
    }
}

impl<'v> TryFrom<Box<dyn Val + 'v>> for chrono::DateTime<chrono::FixedOffset> {
    type Error = Box<dyn Val + 'v>;

    fn try_from(value: Box<dyn Val + 'v>) -> Result<Self, Self::Error> {
        if let Some(ts) = value.downcast_ref::<Timestamp>() {
            return Ok(ts.0);
        }
        Err(value)
    }
}

impl<'a, 'v> TryFrom<&'a (dyn Val + 'v)> for &'a chrono::DateTime<chrono::FixedOffset> {
    type Error = &'a (dyn Val + 'v);

    fn try_from(value: &'a (dyn Val + 'v)) -> Result<Self, Self::Error> {
        if let Some(ts) = value.downcast_ref::<Timestamp>() {
            return Ok(&ts.0);
        }
        Err(value)
    }
}

fn get_milliseconds<'b, 'v>(this: &Timestamp) -> Result<CowVal<'b, 'v>, ExecutionError> {
    Ok(CowVal::owned(CelInt::from(
        this.inner().timestamp_subsec_millis() as i64,
    )))
}

fn get_seconds<'b, 'v>(this: &Timestamp) -> Result<CowVal<'b, 'v>, ExecutionError> {
    Ok(CowVal::owned(CelInt::from(this.inner().second() as i64)))
}

fn get_minutes<'b, 'v>(this: &Timestamp) -> Result<CowVal<'b, 'v>, ExecutionError> {
    Ok(CowVal::owned(CelInt::from(this.inner().minute() as i64)))
}

fn get_hours<'b, 'v>(this: &Timestamp) -> Result<CowVal<'b, 'v>, ExecutionError> {
    Ok(CowVal::owned(CelInt::from(this.inner().hour() as i64)))
}

fn get_day_of_week<'b, 'v>(this: &Timestamp) -> Result<CowVal<'b, 'v>, ExecutionError> {
    Ok(CowVal::owned(CelInt::from(
        this.inner().weekday().num_days_from_sunday() as i64,
    )))
}

fn get_date<'b, 'v>(this: &Timestamp) -> Result<CowVal<'b, 'v>, ExecutionError> {
    Ok(CowVal::owned(CelInt::from(this.inner().day() as i64)))
}

fn get_day_of_month<'b, 'v>(this: &Timestamp) -> Result<CowVal<'b, 'v>, ExecutionError> {
    Ok(CowVal::owned(CelInt::from(this.inner().day0() as i64)))
}

fn get_day_of_year<'b, 'v>(this: &Timestamp) -> Result<CowVal<'b, 'v>, ExecutionError> {
    let year = this
        .inner()
        .checked_sub_days(Days::new(this.inner().day0() as u64))
        .unwrap()
        .checked_sub_months(Months::new(this.inner().month0()))
        .unwrap();
    Ok(CowVal::owned(CelInt::from(
        this.inner().signed_duration_since(year).num_days(),
    )))
}

fn get_month<'b, 'v>(this: &Timestamp) -> Result<CowVal<'b, 'v>, ExecutionError> {
    Ok(CowVal::owned(CelInt::from(this.inner().month0() as i64)))
}

fn get_full_year<'b, 'v>(this: &Timestamp) -> Result<CowVal<'b, 'v>, ExecutionError> {
    Ok(CowVal::owned(CelInt::from(this.inner().year() as i64)))
}

fn timestamp_from_string<'b, 'v>(this: &CelString<'_>) -> Result<CowVal<'b, 'v>, ExecutionError> {
    Ok(CowVal::owned(Timestamp::from(
        chrono::DateTime::parse_from_rfc3339(this.inner())
            .map_err(|e| ExecutionError::function_error("timestamp", e.to_string().as_str()))?,
    )))
}

fn timestamp_from_timestamp<'b, 'v>(this: &Timestamp) -> Result<CowVal<'b, 'v>, ExecutionError> {
    Ok(CowVal::owned(this.clone()))
}

pub(crate) fn stdlib(env: &mut crate::Env) {
    crate::add_overload!(env, fn timestamp_from_string: (CelString) -> Timestamp,
        name = "timestamp", id = "string_to_timestamp");
    crate::add_overload!(env, fn timestamp_from_timestamp: (Timestamp) -> Timestamp,
        name = "timestamp", id = "timestamp_to_timestamp");
    crate::add_member_overload!(env, fn get_full_year: (Timestamp) -> CelInt,
        id = "timestamp_to_year");
    crate::add_member_overload!(env, fn get_month: (Timestamp) -> CelInt,
        id = "timestamp_to_month");
    crate::add_member_overload!(env, fn get_day_of_year: (Timestamp) -> CelInt,
        id = "timestamp_to_day_of_year");
    crate::add_member_overload!(env, fn get_day_of_month: (Timestamp) -> CelInt,
        id = "timestamp_to_day_of_month");
    crate::add_member_overload!(env, fn get_date: (Timestamp) -> CelInt,
        id = "timestamp_to_day_of_month_1_based");
    crate::add_member_overload!(env, fn get_day_of_week: (Timestamp) -> CelInt,
        id = "timestamp_to_day_of_week");
    crate::add_member_overload!(env, fn get_hours: (Timestamp) -> CelInt,
        id = "timestamp_to_hours");
    crate::add_member_overload!(env, fn get_minutes: (Timestamp) -> CelInt,
        id = "timestamp_to_minutes");
    crate::add_member_overload!(env, fn get_seconds: (Timestamp) -> CelInt,
        id = "timestamp_to_seconds");
    crate::add_member_overload!(env, fn get_milliseconds: (Timestamp) -> CelInt,
        id = "timestamp_to_millis");
}
