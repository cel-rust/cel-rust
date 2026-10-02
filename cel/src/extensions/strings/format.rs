//! `string.format(list)`: printf-style substitution, following the strings
//! extension's spec and cel-go's `ext/formatting_v2.go`.
//!
//! A clause is `%[.precision]conversion`, and `%%` is a literal `%`. Each
//! clause takes the next argument of the list.

use crate::common::traits::{Indexer, Iterable};
use crate::common::types::{
    CelBool, CelBytes, CelDouble, CelInt, CelString, CelType, CelUInt, Kind,
};
#[cfg(feature = "chrono")]
use crate::common::types::{CelDuration, CelTimestamp};
use crate::common::value::{CowVal, Val};
use crate::ExecutionError;

/// The precision of `%f` and `%e` when the clause has none.
const DEFAULT_PRECISION: usize = 6;

/// The largest precision a clause may ask for, cel-go's default: digits are
/// written out, so the precision bounds the size of the result.
const MAX_PRECISION: usize = 100;

pub(super) fn format<'b, 'v>(args: Vec<CowVal<'b, 'v>>) -> Result<CowVal<'b, 'v>, ExecutionError> {
    match args.as_slice() {
        [this, list] => {
            let this = super::string_arg(this.as_ref())?;
            let mut items = super::iterable(list.as_ref())?.iter();
            let mut args = Vec::new();
            while let Some(item) = items.next() {
                args.push(item);
            }
            format_args(this, &args)
                .map(|s| CowVal::owned(CelString::from(s)))
                .map_err(|message| ExecutionError::function_error("format", message))
        }
        _ => Err(ExecutionError::invalid_argument_count(2, args.len())),
    }
}

/// `format` with each clause replaced by its argument, in order. Arguments
/// left over are ignored.
fn format_args(format: &str, args: &[&dyn Val]) -> Result<String, String> {
    let mut out = String::with_capacity(format.len());
    let mut rest = format;
    let mut next_arg = 0;
    while let Some(at) = rest.find('%') {
        out.push_str(&rest[..at]);
        rest = &rest[at + 1..];
        if let Some(after) = rest.strip_prefix('%') {
            out.push('%');
            rest = after;
            continue;
        }
        let arg = args
            .get(next_arg)
            .ok_or_else(|| format!("index {next_arg} out of range"))?;
        if rest.is_empty() {
            return Err("unexpected end of string".to_owned());
        }
        let (clause, after) =
            Clause::parse(rest).map_err(|e| format!("could not parse formatting clause: {e}"))?;
        out.push_str(
            &clause
                .format(*arg)
                .map_err(|e| format!("error during formatting: {e}"))?,
        );
        rest = after;
        next_arg += 1;
    }
    out.push_str(rest);
    Ok(out)
}

/// A clause's conversion, with its precision for those that use it.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Clause {
    String,
    Decimal,
    Fixed(usize),
    Scientific(usize),
    Binary,
    Hex { upper: bool },
    Octal,
}

impl Clause {
    /// The clause at the start of `clause`, its leading `%` stripped, and
    /// what follows it.
    fn parse(clause: &str) -> Result<(Self, &str), String> {
        let (precision, rest) =
            precision(clause).map_err(|e| format!("error while parsing precision: {e}"))?;
        let mut chars = rest.chars();
        let parsed = match chars.next() {
            Some('s') => Self::String,
            Some('d') => Self::Decimal,
            Some('f') => Self::Fixed(precision),
            Some('e') => Self::Scientific(precision),
            Some('b') => Self::Binary,
            Some('x') => Self::Hex { upper: false },
            Some('X') => Self::Hex { upper: true },
            Some('o') => Self::Octal,
            Some(c) => return Err(format!("unrecognized formatting clause \"{c}\"")),
            None => return Err("unexpected end of string".to_owned()),
        };
        Ok((parsed, chars.as_str()))
    }

    fn format(self, val: &dyn Val) -> Result<String, String> {
        let arg = Arg::of(val);
        let formatted = match self {
            Self::String => return string(val),
            Self::Decimal => match arg {
                Arg::Int(i) => Some(i.to_string()),
                Arg::UInt(u) => Some(u.to_string()),
                Arg::Double(d) => Some(double(d)),
                _ => None,
            },
            Self::Fixed(precision) => as_double(&arg)
                .map(|d| non_finite(d).map_or_else(|| format!("{d:.precision$}"), str::to_owned)),
            Self::Scientific(precision) => as_double(&arg)
                .map(|d| non_finite(d).map_or_else(|| scientific(d, precision), str::to_owned)),
            Self::Binary => match arg {
                Arg::Bool(b) => Some(u8::from(b).to_string()),
                Arg::Int(i) => Some(format!("{}{:b}", sign(i), i.unsigned_abs())),
                Arg::UInt(u) => Some(format!("{u:b}")),
                _ => None,
            },
            Self::Hex { upper } => match arg {
                Arg::Int(i) if upper => Some(format!("{}{:X}", sign(i), i.unsigned_abs())),
                Arg::Int(i) => Some(format!("{}{:x}", sign(i), i.unsigned_abs())),
                Arg::UInt(u) if upper => Some(format!("{u:X}")),
                Arg::UInt(u) => Some(format!("{u:x}")),
                Arg::String(s) => Some(hex(s.as_bytes(), upper)),
                Arg::Bytes(b) => Some(hex(b, upper)),
                _ => None,
            },
            Self::Octal => match arg {
                Arg::Int(i) => Some(format!("{}{:o}", sign(i), i.unsigned_abs())),
                Arg::UInt(u) => Some(format!("{u:o}")),
                _ => None,
            },
        };
        formatted.ok_or_else(|| self.type_error(val))
    }

    fn type_error(self, val: &dyn Val) -> String {
        let accepted = match self {
            Self::String => return string_type_error(val),
            Self::Decimal => "decimal clause can only be used on ints, uints, and doubles",
            Self::Fixed(_) => "fixed-point clause can only be used on ints, uints, and doubles",
            Self::Scientific(_) => "scientific clause can only be used on ints, uints, and doubles",
            Self::Binary => "only ints, uints, and bools can be formatted as binary",
            Self::Hex { .. } => "only ints, uints, bytes, and strings can be formatted as hex",
            Self::Octal => "octal clause can only be used on ints and uints",
        };
        format!("{accepted}, was given {}", val.get_type().name())
    }
}

/// The precision at the start of `clause`, or the default when there is
/// none, and what follows it.
fn precision(clause: &str) -> Result<(usize, &str), String> {
    let Some(rest) = clause.strip_prefix('.') else {
        return Ok((DEFAULT_PRECISION, clause));
    };
    let end = rest
        .find(|c: char| !c.is_ascii_digit())
        .ok_or("could not find end of precision specifier")?;
    let precision: usize = rest[..end]
        .parse()
        .map_err(|e| format!("error while converting precision to integer: {e}"))?;
    if precision > MAX_PRECISION {
        return Err(format!(
            "precision {precision} exceeds maximum allowed precision {MAX_PRECISION}"
        ));
    }
    Ok((precision, &rest[end..]))
}

/// An argument, as the clauses tell them apart.
enum Arg<'b, 'v> {
    Bool(bool),
    Int(i64),
    UInt(u64),
    Double(f64),
    String(&'b str),
    Bytes(&'b [u8]),
    #[cfg(feature = "chrono")]
    Duration(&'b CelDuration),
    #[cfg(feature = "chrono")]
    Timestamp(&'b CelTimestamp),
    Null,
    Type(&'b str),
    List(&'b (dyn Iterable + 'v)),
    Map(&'b (dyn Iterable + 'v), &'b (dyn Indexer + 'v)),
    Other,
}

impl<'b, 'v> Arg<'b, 'v> {
    fn of(val: &'b (dyn Val + 'v)) -> Self {
        let arg = match val.get_type().kind() {
            Kind::Boolean => val
                .downcast_ref::<CelBool>()
                .map(|b| Self::Bool(*b.inner())),
            Kind::Int => val.downcast_ref::<CelInt>().map(|i| Self::Int(*i.inner())),
            Kind::UInt => val
                .downcast_ref::<CelUInt>()
                .map(|u| Self::UInt(*u.inner())),
            Kind::Double => val
                .downcast_ref::<CelDouble>()
                .map(|d| Self::Double(*d.inner())),
            Kind::String => val.downcast_ref::<CelString>().map(|s| Self::String(s)),
            Kind::Bytes => val.downcast_ref::<CelBytes>().map(|b| Self::Bytes(b)),
            #[cfg(feature = "chrono")]
            Kind::Duration => val.downcast_ref::<CelDuration>().map(Self::Duration),
            #[cfg(feature = "chrono")]
            Kind::Timestamp => val.downcast_ref::<CelTimestamp>().map(Self::Timestamp),
            Kind::NullType => Some(Self::Null),
            Kind::Type => val.downcast_ref::<CelType>().map(|t| Self::Type(t.name())),
            Kind::List => val.as_iterable().map(Self::List),
            Kind::Map => val
                .as_iterable()
                .zip(val.as_indexer())
                .map(|(keys, values)| Self::Map(keys, values)),
            _ => None,
        };
        arg.unwrap_or(Self::Other)
    }
}

/// `%s`: any value but a message or an opaque one, lists and maps included
/// as long as what they hold is.
fn string(val: &dyn Val) -> Result<String, String> {
    Ok(match Arg::of(val) {
        Arg::Bool(b) => b.to_string(),
        Arg::Int(i) => i.to_string(),
        Arg::UInt(u) => u.to_string(),
        Arg::Double(d) => double(d),
        Arg::String(s) => s.to_owned(),
        Arg::Bytes(b) => lossy_utf8(b),
        #[cfg(feature = "chrono")]
        Arg::Duration(d) => format!("{}s", double(seconds(d))),
        #[cfg(feature = "chrono")]
        Arg::Timestamp(t) => t.to_rfc3339_nano(),
        Arg::Null => "null".to_owned(),
        Arg::Type(name) => name.to_owned(),
        Arg::List(list) => {
            let mut items = list.iter();
            let mut formatted = Vec::new();
            while let Some(item) = items.next() {
                formatted.push(string(item)?);
            }
            format!("[{}]", formatted.join(", "))
        }
        Arg::Map(keys, values) => {
            let mut keys = keys.iter();
            let mut entries = Vec::new();
            while let Some(key) = keys.next() {
                let value = values.get(key).map_err(|e| e.to_string())?;
                entries.push((string(key)?, string(value.as_ref())?));
            }
            // By the formatted keys, as the spec has it, then values: keys of
            // different types can format alike.
            entries.sort();
            let entries: Vec<String> = entries
                .into_iter()
                .map(|(key, value)| format!("{key}: {value}"))
                .collect();
            format!("{{{}}}", entries.join(", "))
        }
        Arg::Other => return Err(string_type_error(val)),
    })
}

fn string_type_error(val: &dyn Val) -> String {
    format!(
        "string clause can only be used on strings, bools, bytes, ints, doubles, maps, lists, \
         types, durations, and timestamps, was given {}",
        val.get_type().name()
    )
}

/// The double that `%f` and `%e` format, from any number.
fn as_double(arg: &Arg) -> Option<f64> {
    match *arg {
        Arg::Int(i) => Some(i as f64),
        Arg::UInt(u) => Some(u as f64),
        Arg::Double(d) => Some(d),
        _ => None,
    }
}

/// How every clause spells a double without digits.
fn non_finite(d: f64) -> Option<&'static str> {
    if d.is_nan() {
        Some("NaN")
    } else if d.is_infinite() {
        Some(if d > 0.0 { "Infinity" } else { "-Infinity" })
    } else {
        None
    }
}

/// `d` in the fewest digits that read back as `d`, without an exponent.
fn double(d: f64) -> String {
    non_finite(d).map_or_else(|| d.to_string(), str::to_owned)
}

/// `d` as `[-]d.ddde±dd`, `precision` digits after the point and at least
/// two in the exponent, where Rust writes `[-]d.ddde[-]d`.
fn scientific(d: f64, precision: usize) -> String {
    let formatted = format!("{d:.precision$e}");
    let (mantissa, exponent) = formatted
        .split_once('e')
        .expect("Rust's LowerExp always writes an exponent");
    let exponent: i32 = exponent
        .parse()
        .expect("Rust's LowerExp writes the exponent as an integer");
    let sign = if exponent < 0 { '-' } else { '+' };
    format!("{mantissa}e{sign}{:02}", exponent.unsigned_abs())
}

/// The `-` before the digits of a negative `i`: Go's integer formatting,
/// where Rust's would write the two's complement.
fn sign(i: i64) -> &'static str {
    if i < 0 {
        "-"
    } else {
        ""
    }
}

/// Each byte as two hex digits.
fn hex(bytes: &[u8], upper: bool) -> String {
    bytes
        .iter()
        .map(|b| {
            if upper {
                format!("{b:02X}")
            } else {
                format!("{b:02x}")
            }
        })
        .collect()
}

/// `bytes` as a string, as the spec has it: a run of invalid UTF-8
/// sequences becomes a single U+FFFD.
fn lossy_utf8(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len());
    for (i, chunk) in bytes.utf8_chunks().enumerate() {
        out.push_str(chunk.valid());
        // Every chunk but the first follows an invalid sequence, which this
        // one's continues when nothing valid is between them.
        if !chunk.invalid().is_empty() && (i == 0 || !chunk.valid().is_empty()) {
            out.push(char::REPLACEMENT_CHARACTER);
        }
    }
    out
}

/// `d` in seconds, as Go's `Duration.Seconds()` computes them.
#[cfg(feature = "chrono")]
fn seconds(d: &CelDuration) -> f64 {
    d.num_seconds() as f64 + f64::from(d.subsec_nanos()) / 1e9
}

#[cfg(test)]
mod tests {
    use super::{lossy_utf8, Clause};

    #[test]
    fn clauses_parse_their_precision() {
        assert_eq!(Clause::parse("s"), Ok((Clause::String, "")));
        assert_eq!(Clause::parse("f rest"), Ok((Clause::Fixed(6), " rest")));
        assert_eq!(Clause::parse(".2e"), Ok((Clause::Scientific(2), "")));
        assert_eq!(Clause::parse(".100f"), Ok((Clause::Fixed(100), "")));
        assert_eq!(
            Clause::parse(".101f"),
            Err(
                "error while parsing precision: precision 101 exceeds maximum allowed precision 100"
                    .to_owned()
            )
        );
        assert_eq!(
            Clause::parse(".3"),
            Err(
                "error while parsing precision: could not find end of precision specifier"
                    .to_owned()
            )
        );
        assert_eq!(
            Clause::parse("é"),
            Err("unrecognized formatting clause \"é\"".to_owned())
        );
    }

    #[test]
    fn invalid_utf8_runs_become_one_replacement_character() {
        assert_eq!(lossy_utf8(b"xyz"), "xyz");
        assert_eq!(lossy_utf8(b"\xff\xfe"), "\u{fffd}");
        assert_eq!(lossy_utf8(b"a\xff\xfeb\xffc"), "a\u{fffd}b\u{fffd}c");
        assert_eq!(lossy_utf8("\u{fffd}".as_bytes()), "\u{fffd}");
    }
}
