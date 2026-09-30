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

    /// Formats like cel-go's `Timestamp.ConvertToType(StringType)`: UTC,
    /// always `Z`, and a trimmed fractional part (dropped when it's zero).
    pub(crate) fn to_rfc3339_nano(&self) -> String {
        let utc = self.0.with_timezone(&chrono::Utc);
        let nanos = utc.timestamp_subsec_nanos() % 1_000_000_000;
        let base = utc.format("%Y-%m-%dT%H:%M:%S");
        if nanos == 0 {
            format!("{base}Z")
        } else {
            let frac = format!("{nanos:09}");
            format!("{base}.{}Z", frac.trim_end_matches('0'))
        }
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
            let result = self.0.signed_duration_since(rhs.inner());
            if crate::common::types::duration::out_of_range(&result) {
                return Err(ExecutionError::Overflow(
                    "sub",
                    (self as &dyn Val).try_into().unwrap_or(Value::Null),
                    (rhs as &dyn Val).try_into().unwrap_or(Value::Null),
                ));
            }
            Ok(CowVal::owned(CelDuration::from(result)))
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

fn get_milliseconds(this: &Timestamp) -> CelInt {
    CelInt::from(this.inner().timestamp_subsec_millis() as i64)
}

fn get_seconds(this: &Timestamp) -> CelInt {
    CelInt::from(this.inner().second() as i64)
}

fn get_minutes(this: &Timestamp) -> CelInt {
    CelInt::from(this.inner().minute() as i64)
}

fn get_hours(this: &Timestamp) -> CelInt {
    CelInt::from(this.inner().hour() as i64)
}

fn get_day_of_week(this: &Timestamp) -> CelInt {
    CelInt::from(this.inner().weekday().num_days_from_sunday() as i64)
}

fn get_date(this: &Timestamp) -> CelInt {
    CelInt::from(this.inner().day() as i64)
}

fn get_day_of_month(this: &Timestamp) -> CelInt {
    CelInt::from(this.inner().day0() as i64)
}

fn get_day_of_year(this: &Timestamp) -> CelInt {
    let year = this
        .inner()
        .checked_sub_days(Days::new(this.inner().day0() as u64))
        .unwrap()
        .checked_sub_months(Months::new(this.inner().month0()))
        .unwrap();
    CelInt::from(this.inner().signed_duration_since(year).num_days())
}

fn get_month(this: &Timestamp) -> CelInt {
    CelInt::from(this.inner().month0() as i64)
}

fn get_full_year(this: &Timestamp) -> CelInt {
    CelInt::from(this.inner().year() as i64)
}

/// Mirrors cel-go's `timeZone`: an IANA name, or a `[+-]HH:MM` offset when there is a colon.
fn in_time_zone(this: &Timestamp, tz: &CelString<'_>) -> Result<Timestamp, ExecutionError> {
    let tz = tz.inner();
    let err = |e: &dyn std::fmt::Display| ExecutionError::function_error("timezone", e);
    let dt = this.inner();
    let Some((hours, minutes)) = tz.split_once(':') else {
        let zone = if tz.is_empty() {
            chrono_tz::UTC
        } else {
            tz.parse::<chrono_tz::Tz>()
                .map_err(|_| err(&format!("unknown time zone {tz}")))?
        };
        return Ok(dt.with_timezone(&zone).fixed_offset().into());
    };
    let hr: i32 = hours.parse().map_err(|e| err(&e))?;
    let min: i32 = minutes.parse().map_err(|e| err(&e))?;
    if !(-23..=23).contains(&hr) {
        return Err(err(&format!(
            "timezone offset hours out of range [-23, 23]: {tz}"
        )));
    }
    if !(0..=59).contains(&min) {
        return Err(err(&format!(
            "timezone offset minutes out of range [0, 59]: {tz}"
        )));
    }
    let offset = if hours.starts_with('-') {
        hr * 60 - min
    } else {
        hr * 60 + min
    };
    let offset = chrono::FixedOffset::east_opt(offset * 60).ok_or_else(|| err(&tz))?;
    Ok(dt.with_timezone(&offset).into())
}

macro_rules! with_time_zone {
    ($($name:ident => $get:ident),* $(,)?) => {$(
        fn $name(this: &Timestamp, tz: &CelString<'_>) -> Result<CelInt, ExecutionError> {
            Ok($get(&in_time_zone(this, tz)?))
        }
    )*};
}

with_time_zone! {
    get_full_year_tz => get_full_year,
    get_month_tz => get_month,
    get_day_of_year_tz => get_day_of_year,
    get_day_of_month_tz => get_day_of_month,
    get_date_tz => get_date,
    get_day_of_week_tz => get_day_of_week,
    get_hours_tz => get_hours,
    get_minutes_tz => get_minutes,
    get_seconds_tz => get_seconds,
    get_milliseconds_tz => get_milliseconds,
}

fn timestamp_from_string(this: &CelString<'_>) -> Result<Timestamp, ExecutionError> {
    Ok(Timestamp::from(
        chrono::DateTime::parse_from_rfc3339(this.inner())
            .map_err(|e| ExecutionError::function_error("timestamp", e.to_string().as_str()))?,
    ))
}

fn timestamp_from_timestamp(this: &Timestamp) -> Timestamp {
    this.clone()
}

pub(crate) fn stdlib(env: &mut crate::Env) {
    env.add_type(super::TIMESTAMP_TYPE).expect("Must be unique");
    crate::add_overload!(env, fn timestamp_from_string: (CelString) -> Result<Timestamp>,
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
    crate::add_member_overload!(env, fn get_full_year_tz: (Timestamp, CelString) -> Result<CelInt>,
        name = "getFullYear", id = "timestamp_to_year_with_tz");
    crate::add_member_overload!(env, fn get_month_tz: (Timestamp, CelString) -> Result<CelInt>,
        name = "getMonth", id = "timestamp_to_month_with_tz");
    crate::add_member_overload!(env, fn get_day_of_year_tz: (Timestamp, CelString) -> Result<CelInt>,
        name = "getDayOfYear", id = "timestamp_to_day_of_year_with_tz");
    crate::add_member_overload!(env, fn get_day_of_month_tz: (Timestamp, CelString) -> Result<CelInt>,
        name = "getDayOfMonth", id = "timestamp_to_day_of_month_with_tz");
    crate::add_member_overload!(env, fn get_date_tz: (Timestamp, CelString) -> Result<CelInt>,
        name = "getDate", id = "timestamp_to_day_of_month_1_based_with_tz");
    crate::add_member_overload!(env, fn get_day_of_week_tz: (Timestamp, CelString) -> Result<CelInt>,
        name = "getDayOfWeek", id = "timestamp_to_day_of_week_with_tz");
    crate::add_member_overload!(env, fn get_hours_tz: (Timestamp, CelString) -> Result<CelInt>,
        name = "getHours", id = "timestamp_to_hours_with_tz");
    crate::add_member_overload!(env, fn get_minutes_tz: (Timestamp, CelString) -> Result<CelInt>,
        name = "getMinutes", id = "timestamp_to_minutes_with_tz");
    crate::add_member_overload!(env, fn get_seconds_tz: (Timestamp, CelString) -> Result<CelInt>,
        name = "getSeconds", id = "timestamp_to_seconds_tz");
    crate::add_member_overload!(env, fn get_milliseconds_tz: (Timestamp, CelString) -> Result<CelInt>,
        name = "getMilliseconds", id = "timestamp_to_milliseconds_with_tz");
}
