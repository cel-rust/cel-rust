use super::Result;
use crate::magic::This;
use crate::{ExecutionError, Value};
use chrono::{Datelike, Days, Months, Timelike};
use std::sync::Arc;

/// Duration parses the provided argument into a [`Value::Duration`] value.
///
/// The argument must be string, and must be in the format of a duration. See
/// the [`parse_duration`] documentation for more information on the supported
/// formats.
///
/// # Examples
/// - `1h` parses as 1 hour
/// - `1.5h` parses as 1 hour and 30 minutes
/// - `1h30m` parses as 1 hour and 30 minutes
/// - `1h30m1s` parses as 1 hour, 30 minutes, and 1 second
/// - `1ms` parses as 1 millisecond
/// - `1.5ms` parses as 1 millisecond and 500 microseconds
/// - `1ns` parses as 1 nanosecond
/// - `1.5ns` parses as 1 nanosecond (sub-nanosecond durations not supported)
pub fn duration(value: Arc<String>) -> crate::functions::Result<Value> {
    Ok(Value::Duration(_duration(value.as_str())?))
}

/// Timestamp parses the provided argument into a [`Value::Timestamp`] value.
/// The
pub fn timestamp(value: Arc<String>) -> Result<Value> {
    Ok(Value::Timestamp(
        chrono::DateTime::parse_from_rfc3339(value.as_str())
            .map_err(|e| ExecutionError::function_error("timestamp", e.to_string().as_str()))?,
    ))
}

/// A wrapper around [`parse_duration`] that converts errors into [`ExecutionError`].
/// and only returns the duration, rather than returning the remaining input.
fn _duration(i: &str) -> Result<chrono::Duration> {
    let (_, duration) = crate::duration::parse_duration(i)
        .map_err(|e| ExecutionError::function_error("duration", e.to_string()))?;
    Ok(duration)
}

fn _timestamp(i: &str) -> Result<chrono::DateTime<chrono::FixedOffset>> {
    chrono::DateTime::parse_from_rfc3339(i)
        .map_err(|e| ExecutionError::function_error("timestamp", e.to_string()))
}

pub fn timestamp_year(This(this): This<chrono::DateTime<chrono::FixedOffset>>) -> Result<Value> {
    Ok(this.year().into())
}

pub fn timestamp_month(This(this): This<chrono::DateTime<chrono::FixedOffset>>) -> Result<Value> {
    Ok((this.month0() as i32).into())
}

pub fn timestamp_year_day(
    This(this): This<chrono::DateTime<chrono::FixedOffset>>,
) -> Result<Value> {
    let year = this
        .checked_sub_days(Days::new(this.day0() as u64))
        .unwrap()
        .checked_sub_months(Months::new(this.month0()))
        .unwrap();
    Ok(this.signed_duration_since(year).num_days().into())
}

pub fn timestamp_month_day(
    This(this): This<chrono::DateTime<chrono::FixedOffset>>,
) -> Result<Value> {
    Ok((this.day0() as i32).into())
}

pub fn timestamp_date(This(this): This<chrono::DateTime<chrono::FixedOffset>>) -> Result<Value> {
    Ok((this.day() as i32).into())
}

pub fn timestamp_weekday(This(this): This<chrono::DateTime<chrono::FixedOffset>>) -> Result<Value> {
    Ok((this.weekday().num_days_from_sunday() as i32).into())
}

pub fn get_hours(This(this): This<Value>) -> Result<Value> {
    Ok(match this {
        Value::Timestamp(ts) => (ts.hour() as i32).into(),
        Value::Duration(d) => (d.num_hours() as i32).into(),
        _ => {
            return Err(ExecutionError::function_error(
                "getHours",
                "expected timestamp or duration",
            ))
        }
    })
}

pub fn get_minutes(This(this): This<Value>) -> Result<Value> {
    Ok(match this {
        Value::Timestamp(ts) => (ts.minute() as i32).into(),
        Value::Duration(d) => (d.num_minutes() as i32).into(),
        _ => {
            return Err(ExecutionError::function_error(
                "getMinutes",
                "expected timestamp or duration",
            ))
        }
    })
}

pub fn get_seconds(This(this): This<Value>) -> Result<Value> {
    Ok(match this {
        Value::Timestamp(ts) => (ts.second() as i32).into(),
        Value::Duration(d) => (d.num_seconds() as i32).into(),
        _ => {
            return Err(ExecutionError::function_error(
                "getSeconds",
                "expected timestamp or duration",
            ))
        }
    })
}

pub fn get_milliseconds(This(this): This<Value>) -> Result<Value> {
    Ok(match this {
        Value::Timestamp(ts) => (ts.timestamp_subsec_millis() as i32).into(),
        Value::Duration(d) => (d.num_milliseconds() as i32).into(),
        _ => {
            return Err(ExecutionError::function_error(
                "getMilliseconds",
                "expected timestamp or duration",
            ))
        }
    })
}
