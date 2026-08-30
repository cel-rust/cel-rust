use crate::common::traits::{Adder, Comparer, Sizer, Zeroer};
use crate::common::types::{CelBool, CelBytes, CelDouble, CelInt, CelUInt, Type};
#[cfg(feature = "chrono")]
use crate::common::types::{CelDuration, CelTimestamp};
use crate::common::value::{Builtin, BuiltinRef, CowVal, Val};
use crate::ExecutionError;
use std::borrow::Cow;
use std::cmp::Ordering;
use std::ops::Deref;
use std::string::String as StdString;
use std::sync::Arc;

/// A CEL string. Owns its bytes, or borrows them for `'a`.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq, PartialOrd, Ord)]
pub struct String<'a>(Cow<'a, str>);

impl<'a> String<'a> {
    /// The string, copied out if it was borrowed.
    pub fn into_inner(self) -> StdString {
        self.0.into_owned()
    }

    pub fn inner(&self) -> &str {
        &self.0
    }

    /// Copies the bytes out if they were borrowed, so the result owns them.
    pub fn into_static(self) -> String<'static> {
        String(Cow::Owned(self.0.into_owned()))
    }

    /// The bytes, with the lifetime of the borrow itself, if they are borrowed
    /// rather than owned.
    ///
    /// Unlike [`inner`](String::inner), whose result is bounded by the borrow
    /// of `self`, this lets a slice of the bytes outlive the `String` that
    /// handed it out: a value read out of a borrowed container can be
    /// re-borrowed, without a copy, for as long as the container's own data.
    pub fn as_borrowed(&self) -> Option<&'a str> {
        match &self.0 {
            Cow::Borrowed(s) => Some(s),
            Cow::Owned(_) => None,
        }
    }

    pub(crate) fn into_cow(self) -> Cow<'a, str> {
        self.0
    }
}

impl Deref for String<'_> {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.inner()
    }
}

impl<'a> Val for String<'a> {
    fn get_type(&self) -> &Type {
        <Self as Val>::cel_type()
    }

    fn cel_type() -> &'static Type {
        &super::STRING_TYPE
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

    fn as_sizer(&self) -> Option<&dyn Sizer> {
        Some(self)
    }

    fn as_zeroer(&self) -> Option<&dyn Zeroer> {
        Some(self)
    }

    fn equals(&self, other: &dyn Val) -> bool {
        other
            .downcast_ref::<String>()
            .is_some_and(|other| self.0 == other.0)
    }

    fn clone_as_boxed<'v>(&self) -> Box<dyn Val + 'v>
    where
        Self: 'v,
    {
        Box::new(self.clone())
    }

    fn as_builtin<'b, 'v>(&'b self) -> BuiltinRef<'b, 'v>
    where
        Self: 'v,
    {
        BuiltinRef::String(self)
    }

    fn into_builtin<'v>(self: Box<Self>) -> Option<Builtin<'v>>
    where
        Self: 'v,
    {
        Some(Builtin::String(*self))
    }
}

impl<'a> Adder for String<'a> {
    fn add<'b, 'v>(&'b self, rhs: &(dyn Val + 'v)) -> Result<CowVal<'b, 'v>, ExecutionError>
    where
        Self: 'v,
    {
        if let Some(rhs) = rhs.downcast_ref::<String>() {
            let mut s = StdString::with_capacity(rhs.0.len() + self.0.len());
            s.push_str(&self.0);
            s.push_str(&rhs.0);
            Ok(CowVal::owned(String::from(s)))
        } else {
            Err(ExecutionError::UnsupportedBinaryOperator(
                "add",
                (self as &dyn Val).try_into()?,
                rhs.try_into()?,
            ))
        }
    }
}

impl Comparer for String<'_> {
    fn compare(&self, rhs: &dyn Val) -> Result<Ordering, ExecutionError> {
        if let Some(rhs) = rhs.downcast_ref::<String>() {
            Ok(self.0.cmp(&rhs.0))
        } else {
            Err(ExecutionError::values_not_comparable(self, rhs))
        }
    }
}

impl Sizer for String<'_> {
    fn size(&self) -> CelInt {
        (self.inner().len() as i64).into()
    }
}

impl Zeroer for String<'_> {
    fn is_zero_value(&self) -> bool {
        self.inner().is_empty()
    }
}

impl From<StdString> for String<'_> {
    fn from(v: StdString) -> Self {
        Self(Cow::Owned(v))
    }
}

impl From<String<'_>> for StdString {
    fn from(v: String<'_>) -> Self {
        v.into_inner()
    }
}

/// Borrows the `str`: no copy is made.
impl<'a> From<&'a str> for String<'a> {
    fn from(value: &'a str) -> Self {
        Self(Cow::Borrowed(value))
    }
}

impl From<Arc<StdString>> for String<'_> {
    fn from(v: Arc<StdString>) -> Self {
        match Arc::try_unwrap(v) {
            Ok(s) => Self(Cow::Owned(s)),
            Err(v) => Self(Cow::Owned((*v).clone())),
        }
    }
}

impl<'a> From<Cow<'a, str>> for String<'a> {
    fn from(value: Cow<'a, str>) -> Self {
        Self(value)
    }
}

impl<'v> TryFrom<Box<dyn Val + 'v>> for StdString {
    type Error = Box<dyn Val + 'v>;

    fn try_from(value: Box<dyn Val + 'v>) -> Result<Self, Self::Error> {
        take_string(CowVal::Owned(value))
            .map(String::into_inner)
            .map_err(|v| v.into_owned())
    }
}

impl<'a, 'v> TryFrom<&'a (dyn Val + 'v)> for &'a str {
    type Error = &'a (dyn Val + 'v);
    fn try_from(value: &'a (dyn Val + 'v)) -> Result<Self, Self::Error> {
        if let Some(s) = value.downcast_ref::<String>() {
            return Ok(s.inner());
        }
        Err(value)
    }
}

/// Takes the string out of `arg`: a move for an owned box, a cheap clone of
/// the `Cow` for a borrowed one. Hands `arg` back when it is not a string.
pub(crate) fn take_string<'b, 'v>(arg: CowVal<'b, 'v>) -> Result<String<'v>, CowVal<'b, 'v>> {
    match arg {
        CowVal::Borrowed(v) => v
            .downcast_ref::<String>()
            .cloned()
            .ok_or(CowVal::Borrowed(v)),
        CowVal::Owned(b) => match super::into_builtin(b) {
            Ok(Builtin::String(s)) => Ok(s),
            Ok(other) => Err(CowVal::Owned(other.into_boxed())),
            Err(b) => Err(CowVal::Owned(b)),
        },
    }
}

fn contains<'b, 'v>(
    this: &String<'_>,
    needle: &String<'_>,
) -> Result<CowVal<'b, 'v>, ExecutionError> {
    Ok(CowVal::owned(CelBool::from(this.contains(needle.inner()))))
}

fn ends_with<'b, 'v>(
    this: &String<'_>,
    needle: &String<'_>,
) -> Result<CowVal<'b, 'v>, ExecutionError> {
    Ok(CowVal::owned(CelBool::from(this.ends_with(needle.inner()))))
}

fn starts_with<'b, 'v>(
    this: &String<'_>,
    needle: &String<'_>,
) -> Result<CowVal<'b, 'v>, ExecutionError> {
    Ok(CowVal::owned(CelBool::from(
        this.starts_with(needle.inner()),
    )))
}

fn size<'b, 'v>(this: &String<'_>) -> Result<CowVal<'b, 'v>, ExecutionError> {
    Ok(CowVal::owned(CelInt::from(this.inner().len() as i64)))
}

#[cfg(feature = "regex")]
fn matches<'b, 'v>(this: &String<'_>, re: &String<'_>) -> Result<CowVal<'b, 'v>, ExecutionError> {
    match regex::Regex::new(re.inner()) {
        Ok(compiled) => Ok(CowVal::owned(CelBool::from(
            compiled.is_match(this.inner()),
        ))),
        Err(err) => Err(ExecutionError::FunctionError {
            function: "matches".to_string(),
            message: format!("'{}' not a valid regex:\n{err}", re.inner()),
        }),
    }
}

fn string_from_int<'b, 'v>(this: &CelInt) -> Result<CowVal<'b, 'v>, ExecutionError> {
    Ok(CowVal::owned(String::from(this.to_string())))
}

fn string_from_uint<'b, 'v>(this: &CelUInt) -> Result<CowVal<'b, 'v>, ExecutionError> {
    Ok(CowVal::owned(String::from(this.to_string())))
}

fn string_from_double<'b, 'v>(this: &CelDouble) -> Result<CowVal<'b, 'v>, ExecutionError> {
    Ok(CowVal::owned(String::from(this.to_string())))
}

fn string_from_bytes<'b, 'v>(this: &CelBytes<'_>) -> Result<CowVal<'b, 'v>, ExecutionError> {
    Ok(CowVal::owned(String::from(
        StdString::from_utf8_lossy(this.inner()).into_owned(),
    )))
}

#[cfg(feature = "chrono")]
fn string_from_timestamp<'b, 'v>(this: &CelTimestamp) -> Result<CowVal<'b, 'v>, ExecutionError> {
    Ok(CowVal::owned(String::from(this.inner().to_rfc3339())))
}

#[cfg(feature = "chrono")]
fn string_from_duration<'b, 'v>(this: &CelDuration) -> Result<CowVal<'b, 'v>, ExecutionError> {
    Ok(CowVal::owned(String::from(
        crate::duration::format_duration(this.inner()),
    )))
}

pub(crate) fn stdlib(env: &mut crate::Env) {
    // Hand-written: `string(s)` is the identity and keeps the borrow.
    env.add_overload(
        "string",
        "string_to_string",
        vec![super::STRING_TYPE],
        super::noop,
    )
    .expect("Must be unique id");
    crate::add_overload!(env, fn string_from_int: (CelInt) -> String,
        name = "string", id = "int64_to_string");
    crate::add_overload!(env, fn string_from_uint: (CelUInt) -> String,
        name = "string", id = "uint64_to_string");
    crate::add_overload!(env, fn string_from_double: (CelDouble) -> String,
        name = "string", id = "double_to_string");
    crate::add_overload!(env, fn string_from_bytes: (CelBytes) -> String,
        name = "string", id = "bytes_to_string");

    #[cfg(feature = "chrono")]
    {
        crate::add_overload!(env, fn string_from_timestamp: (CelTimestamp) -> String,
            name = "string", id = "timestamp_to_string");
        crate::add_overload!(env, fn string_from_duration: (CelDuration) -> String,
            name = "string", id = "duration_to_string");
    }

    crate::add_member_overload!(env, fn contains: (String, String) -> CelBool);
    crate::add_member_overload!(env, fn ends_with: (String, String) -> CelBool,
        name = "endsWith");
    crate::add_overload!(env, fn size: (String) -> CelInt,
        name = "size", id = "size_string");
    crate::add_member_overload!(env, fn size: (String) -> CelInt,
        id = "string_size");
    crate::add_member_overload!(env, fn starts_with: (String, String) -> CelBool,
        name = "startsWith");
    #[cfg(feature = "regex")]
    crate::add_member_overload!(env, fn matches: (String, String) -> CelBool);
}

#[cfg(test)]
mod tests {
    use super::StdString;
    use super::String;
    use crate::common::value::{CowVal, Val};

    #[test]
    fn as_borrowed_outlives_the_string() {
        let owned = StdString::from("/v1/users");
        let stripped = {
            let s = String::from(owned.as_str());
            // `s` is dropped at the end of this block; the slice is not tied to it
            s.as_borrowed().and_then(|s| s.strip_prefix("/v1"))
        };
        assert_eq!(stripped, Some("/users"));
        assert!(std::ptr::eq(
            stripped.unwrap().as_ptr(),
            owned[3..].as_ptr()
        ));
        assert_eq!(String::from(StdString::from("owned")).as_borrowed(), None);
    }

    #[test]
    fn test_try_into_string() {
        let str: Box<dyn Val> = Box::new(String::from("cel-rust"));
        assert_eq!(Ok(StdString::from("cel-rust")), str.try_into())
    }

    #[test]
    fn test_try_into_str() {
        let str: Box<dyn Val> = Box::new(String::from("cel-rust"));
        assert_eq!(Ok("cel-rust"), str.as_ref().try_into())
    }

    #[test]
    fn from_str_borrows() {
        let owned = StdString::from("cel-rust");
        let s = String::from(owned.as_str());
        assert!(std::ptr::eq(s.inner(), owned.as_str()));
        let boxed: Box<dyn Val + '_> = s.clone_as_boxed();
        let back = boxed.downcast_ref::<String>().unwrap();
        assert!(std::ptr::eq(back.inner(), owned.as_str()));
        assert_eq!(s.into_static().inner(), "cel-rust");
    }

    #[test]
    fn string_of_string_is_identity() {
        let owned = StdString::from("cel-rust");
        let arg: CowVal<'_, '_> = CowVal::owned(String::from(owned.as_str()));
        // `string_to_string` is registered as `super::noop`, not via the
        // overload macro, precisely so the borrow survives.
        let out = crate::common::types::noop(vec![arg]).unwrap();
        let s = out.downcast_ref::<String>().unwrap();
        assert!(std::ptr::eq(s.inner(), owned.as_str()));
    }
}
