use crate::common::types::duration::{MAX_NANOS, MIN_NANOS};
use chrono::Duration;
use nom::branch::alt;
use nom::bytes::complete::tag;
use nom::character::complete::char;
use nom::combinator::{map, opt};
use nom::multi::many1;
use nom::number::complete::recognize_float_or_exceptions;
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
    /// The unit in nanoseconds, as `(factor, power)`: `factor * 10^power`.
    fn nanos(&self) -> (u8, i64) {
        match self {
            Unit::Nanosecond => (1, 0),
            Unit::Microsecond => (1, 3),
            Unit::Millisecond => (1, 6),
            Unit::Second => (1, 9),
            Unit::Minute => (6, 10),
            Unit::Hour => (36, 11),
        }
    }
}

/// A number and its unit. The number is any float `nom` recognizes, as
/// `nom::number::complete::double` does, so the syntax accepted is that of
/// `double`, but its value is converted exactly.
fn parse_number_unit(i: &str) -> IResult<&str, Duration> {
    let (i, number) = recognize_float_or_exceptions(i)?;
    let (i, unit) = parse_unit(i)?;
    match to_duration(number, unit) {
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

/// `number` of `unit`, truncated toward zero to whole nanoseconds; `None` for
/// `nan`. A component past the range of a duration, `inf` included, is
/// clamped to `Duration::MAX`/`MIN` so the caller reports it as out of range.
fn to_duration(number: &str, unit: Unit) -> Option<Duration> {
    let (negative, magnitude) = match number.as_bytes().first() {
        Some(b'-') => (true, &number[1..]),
        Some(b'+') => (false, &number[1..]),
        _ => (false, number),
    };
    if magnitude.eq_ignore_ascii_case("nan") {
        return None;
    }
    let is_infinite =
        magnitude.eq_ignore_ascii_case("inf") || magnitude.eq_ignore_ascii_case("infinity");
    let nanos = if is_infinite {
        None
    } else {
        whole_nanos(magnitude, unit)
    };
    let past = if negative {
        Duration::MIN
    } else {
        Duration::MAX
    };
    let Some(nanos) = nanos.map(|n| if negative { -n } else { n }) else {
        return Some(past);
    };
    // A leading `-` negates the components after they're summed, so a
    // component as long as the most negative duration isn't past it.
    let longest = MAX_NANOS.max(-MIN_NANOS);
    if !(-longest..=longest).contains(&nanos) {
        return Some(past);
    }
    Some(
        Duration::seconds((nanos / 1_000_000_000) as i64)
            + Duration::nanoseconds((nanos % 1_000_000_000) as i64),
    )
}

/// The whole nanoseconds in `magnitude` of `unit`, truncated toward zero, or
/// `None` past what an `i128` holds. `magnitude` is an unsigned decimal, with
/// an optional fraction and exponent: `12`, `1.5`, `.5`, `5.`, `1e-3`.
fn whole_nanos(magnitude: &str, unit: Unit) -> Option<i128> {
    let (mantissa, exponent) = match magnitude.find(['e', 'E']) {
        Some(at) => (&magnitude[..at], &magnitude[at + 1..]),
        None => (magnitude, ""),
    };
    let (integer, fraction) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    // `number = digits * 10^-fraction.len() * 10^exponent`, and
    // `unit = factor * 10^power` nanoseconds.
    let mut digits: Vec<u8> = integer
        .bytes()
        .chain(fraction.bytes())
        .map(|b| b - b'0')
        .collect();
    if digits.iter().all(|&d| d == 0) {
        return Some(0);
    }
    let exponent = match exponent.parse::<i64>() {
        Ok(exponent) => exponent,
        Err(_) if exponent.is_empty() => 0,
        // Too many digits for an `i64`: far past any range, or far below a
        // nanosecond.
        Err(_) if exponent.starts_with('-') => return Some(0),
        Err(_) => return None,
    };
    let (factor, power) = unit.nanos();
    // `digits * factor`, as decimal digits.
    let mut carry = 0u32;
    for digit in digits.iter_mut().rev() {
        let product = u32::from(*digit) * u32::from(factor) + carry;
        *digit = (product % 10) as u8;
        carry = product / 10;
    }
    while carry > 0 {
        digits.insert(0, (carry % 10) as u8);
        carry /= 10;
    }
    // Then shifted by `10^shift`: the digits before the decimal point.
    let shift = exponent
        .saturating_add(power)
        .saturating_sub(fraction.len() as i64);
    let whole = (digits.len() as i64).saturating_add(shift).max(0);
    // Past the digits, each `0` multiplies a non-zero `nanos` by ten, so this
    // overflows, and ends, within 39 of them.
    let mut nanos = 0i128;
    let digits = digits.iter().copied().chain(std::iter::repeat(0));
    for digit in digits.take(usize::try_from(whole).unwrap_or(usize::MAX)) {
        nanos = nanos.checked_mul(10)?.checked_add(i128::from(digit))?;
    }
    Some(nanos)
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

    /// What `parse_duration` accepts through the float syntax it parses each
    /// number with, beyond its documented grammar: kept as is.
    #[test]
    fn test_durations_keep_their_float_syntax() {
        for (input, expected) in [
            ("1e3s", Duration::seconds(1000)),
            ("1E-3s", Duration::milliseconds(1)),
            ("1h-30m", Duration::minutes(30)),
            ("1h+30m", Duration::minutes(90)),
            ("--5s", Duration::seconds(5)),
            ("-+5s", Duration::seconds(-5)),
            ("+5s", Duration::seconds(5)),
            (".5s", Duration::milliseconds(500)),
            ("5.s", Duration::seconds(5)),
            ("1.5ns", Duration::nanoseconds(1)),
            ("-1.5ns", Duration::nanoseconds(-1)),
            ("1e-400s", Duration::zero()),
            ("0e999999999999999999999s", Duration::zero()),
            // Past the range of a duration, for the caller to report.
            ("infs", Duration::MAX),
            ("-INFs", Duration::MIN),
            ("1e400s", Duration::MAX),
            ("9223372037s", Duration::MAX),
        ] {
            assert_duration(input, expected);
        }
    }

    /// The errors `parse_duration` fails with for numbers it can't parse, or
    /// for a component past the range of a duration followed by another.
    #[test]
    fn test_durations_fail_as_they_did() {
        use nom::error::ErrorKind;

        for (input, kind) in [
            ("nans", ErrorKind::TooLarge),
            ("9223372037s1ns", ErrorKind::TooLarge),
            ("1es", ErrorKind::Float),
        ] {
            match parse_duration(input) {
                Err(nom::Err::Failure(e)) => assert_eq!(e.code, kind, "{input}"),
                other => panic!("{input}: {other:?}"),
            }
        }
        // `inf` is a number, so `inityh` is left to parse as a unit.
        match parse_duration("infinityh") {
            Err(nom::Err::Error(e)) => {
                assert_eq!((e.input, e.code), ("inityh", ErrorKind::Char))
            }
            other => panic!("infinityh: {other:?}"),
        }
    }

    /// Each component is converted exactly, not through an `f64` product
    /// truncated to whole nanoseconds.
    #[test]
    fn test_durations_are_exact() {
        for (input, nanos) in [
            ("4.02us", 4_020),
            ("8.049us", 8_049),
            ("66628.12us", 66_628_120),
            ("0.000004020s", 4_020),
            ("8738239041.3us", 8_738_239_041_300),
            ("-9223372036854775808ns", i64::MIN),
        ] {
            assert_duration(input, Duration::nanoseconds(nanos));
        }
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
