use chrono::Duration;
use nom::branch::alt;
use nom::bytes::complete::tag;
use nom::character::complete::char;
use nom::combinator::{map, opt};
use nom::multi::many1;
use nom::number::complete::double;
use nom::IResult;

/// Parses a duration string into a [`Duration`]. Duration strings support the
/// following grammar:
///
/// DurationString -> Sign? Number Unit String?
/// Sign           -> '-'
/// Number         -> Digit+ ('.' Digit+)?
/// Digit          -> '0' | '1' | '2' | '3' | '4' | '5' | '6' | '7' | '8' | '9'
/// Unit           -> 'h' | 'm' | 's' | 'ms' | 'us' | 'ns'
/// String         -> DurationString
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
pub fn parse_duration(i: &str) -> IResult<&str, Duration> {
    let (i, neg) = opt(parse_negative)(i)?;
    if i == "0" {
        return Ok((i, Duration::zero()));
    }
    let (i, components) = many1(parse_number_unit)(i)?;
    let mut duration = Duration::zero();
    for component in components {
        duration = match duration.checked_add(&component) {
            Some(duration) => duration,
            None => return Err(too_large(i)),
        };
    }
    Ok((i, duration * if neg.is_some() { -1 } else { 1 }))
}

fn too_large(i: &str) -> nom::Err<nom::error::Error<&str>> {
    nom::Err::Failure(nom::error::Error::new(i, nom::error::ErrorKind::TooLarge))
}

enum Unit {
    Nanosecond,
    Microsecond,
    Millisecond,
    Second,
    Minute,
    Hour,
}

impl Unit {
    fn nanos(&self) -> i64 {
        match self {
            Unit::Nanosecond => 1,
            Unit::Microsecond => 1_000,
            Unit::Millisecond => 1_000_000,
            Unit::Second => 1_000_000_000,
            Unit::Minute => 60 * 1_000_000_000,
            Unit::Hour => 60 * 60 * 1_000_000_000,
        }
    }
}

fn parse_number_unit(i: &str) -> IResult<&str, Duration> {
    let (i, num) = double(i)?;
    let (i, unit) = parse_unit(i)?;
    match to_duration(num, unit) {
        Some(duration) => Ok((i, duration)),
        None => Err(too_large(i)),
    }
}

fn parse_negative(i: &str) -> IResult<&str, ()> {
    let (i, _): (&str, char) = char('-')(i)?;
    Ok((i, ()))
}

fn parse_unit(i: &str) -> IResult<&str, Unit> {
    alt((
        map(tag("ms"), |_| Unit::Millisecond),
        map(tag("us"), |_| Unit::Microsecond),
        map(tag("ns"), |_| Unit::Nanosecond),
        map(char('h'), |_| Unit::Hour),
        map(char('m'), |_| Unit::Minute),
        map(char('s'), |_| Unit::Second),
    ))(i)
}

/// Components past the `i64` nanosecond range are clamped to `Duration::MAX`/`MIN`
/// so the caller reports them as out of range.
fn to_duration(num: f64, unit: Unit) -> Option<Duration> {
    let nanos = num * unit.nanos() as f64;
    if nanos.is_nan() {
        return None;
    }
    if nanos.abs() > i64::MAX as f64 {
        return Some(if nanos > 0.0 {
            Duration::MAX
        } else {
            Duration::MIN
        });
    }
    Some(Duration::nanoseconds(nanos as i64))
}

/// Formats a [`Duration`] as CEL's `string()` does: whole seconds, plus a
/// trimmed fractional part when non-zero, followed by `s` (e.g. `"1.5s"`).
pub fn format_duration_seconds(d: &Duration) -> String {
    let secs = d.num_seconds();
    let nanos = d.subsec_nanos();
    let sign = if secs < 0 || nanos < 0 { "-" } else { "" };
    let abs_secs = secs.unsigned_abs();
    let abs_nanos = nanos.unsigned_abs();
    if abs_nanos == 0 {
        format!("{sign}{abs_secs}s")
    } else {
        let frac = format!("{abs_nanos:09}");
        format!("{sign}{abs_secs}.{}s", frac.trim_end_matches('0'))
    }
}

#[cfg(test)]
mod tests {
    use crate::duration::{format_duration_seconds, parse_duration};
    use chrono::Duration;

    fn assert_duration(input: &str, expected: Duration) {
        let (_, duration) = parse_duration(input).unwrap();
        assert_eq!(duration, expected, "{input}");
    }

    fn assert_print_duration_seconds(input: Duration, expected: &str) {
        let actual = format_duration_seconds(&input);
        assert_eq!(actual, expected, "{input}");
    }

    macro_rules! assert_durations {
        ($($str:expr => $duration:expr),*$(,)?) => {
            #[test]
            fn test_durations() {
                $(
                    assert_duration($str, $duration);
                )*
            }
        };
    }

    macro_rules! assert_duration_format_seconds {
        ($($duration:expr => $str:expr),*$(,)?) => {
            #[test]
            fn test_format_duration_seconds() {
                $(
                    assert_print_duration_seconds($duration, $str);
                )*
            }
        };
    }

    assert_durations! {
        "1s" => Duration::seconds(1),
        "-1s" => Duration::seconds(-1),
        "1.1s" => Duration::seconds(1) + Duration::milliseconds(100),
        "1.5m" => Duration::minutes(1) + Duration::seconds(30),
        "1m1s" => Duration::minutes(1) + Duration::seconds(1),
        "1h1m1s" => Duration::hours(1) + Duration::minutes(1) + Duration::seconds(1),
        "1ms" => Duration::milliseconds(1),
        "1us" => Duration::microseconds(1),
        "1ns" => Duration::nanoseconds(1),
        "1.1ns" => Duration::nanoseconds(1),
        "1.123us" => Duration::microseconds(1) + Duration::nanoseconds(123),
        "0s" => Duration::zero(),
        "0h0m0s" => Duration::zero(),
        "0h0m1s" => Duration::seconds(1),
        "0" => Duration::zero(),
        "-0" => Duration::zero(),
        "9223372036854775807ns" => Duration::nanoseconds(i64::MAX),
        "-9223372036854775807ns" => Duration::nanoseconds(-i64::MAX),
    }

    assert_duration_format_seconds! {
        Duration::zero() => "0s",
        Duration::seconds(1_000_000) => "1000000s",
        Duration::seconds(1) + Duration::milliseconds(500) => "1.5s",
        Duration::seconds(-1) => "-1s",
        Duration::nanoseconds(1) => "0.000000001s",
        Duration::seconds(-1) - Duration::milliseconds(500) => "-1.5s",
        Duration::milliseconds(-500) => "-0.5s",
        Duration::nanoseconds(i64::MAX) => "9223372036.854775807s",
        Duration::nanoseconds(i64::MIN) => "-9223372036.854775808s",
        // exceeds num_nanoseconds()'s i64 range; format_duration_seconds
        // doesn't go through it, so it still formats exactly.
        Duration::seconds(10_000_000_000_000) + Duration::milliseconds(500)
            => "10000000000000.5s",
    }
}
