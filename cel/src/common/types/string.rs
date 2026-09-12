use crate::common::traits::{self, Adder, Comparer, Sizer, Zeroer};
use crate::common::types::{CelBool, CelBytes, CelDouble, CelInt, CelUInt, Kind, Type};
#[cfg(feature = "chrono")]
use crate::common::types::{CelDuration, CelTimestamp};
use crate::common::value::{Builtin, BuiltinRef, CowVal, Val};
use crate::ExecutionError;
use std::borrow::Cow;
use std::cmp::Ordering;
use std::ops::Deref;
use std::string::String as StdString;

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
            Err(ExecutionError::NoSuchOverload)
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

fn unexpected_type(got: &dyn Val) -> ExecutionError {
    ExecutionError::UnexpectedType {
        got: got.get_type().name().to_string(),
        want: super::STRING_TYPE.name().to_string(),
    }
}

type StringBinaryFn = fn(&str, &str) -> Result<Box<dyn Val>, ExecutionError>;

/// Applies `func` to two string arguments.
fn string_binary_fn<'b, 'v>(
    args: Vec<CowVal<'b, 'v>>,
    func: StringBinaryFn,
) -> Result<CowVal<'b, 'v>, ExecutionError> {
    let target = args[0].as_ref();
    let arg = args[1].as_ref();
    let target = target
        .downcast_ref::<String>()
        .ok_or_else(|| unexpected_type(target))?;
    let arg = arg
        .downcast_ref::<String>()
        .ok_or_else(|| unexpected_type(arg))?;
    Ok(CowVal::Owned(func(target.inner(), arg.inner())?))
}

fn string_contains<'b, 'v>(args: Vec<CowVal<'b, 'v>>) -> Result<CowVal<'b, 'v>, ExecutionError> {
    string_binary_fn(args, |s, needle| {
        Ok(Box::new(CelBool::from(s.contains(needle))))
    })
}

fn ends_with_string<'b, 'v>(args: Vec<CowVal<'b, 'v>>) -> Result<CowVal<'b, 'v>, ExecutionError> {
    string_binary_fn(args, |s, needle| {
        Ok(Box::new(CelBool::from(s.ends_with(needle))))
    })
}

fn starts_with_string<'b, 'v>(args: Vec<CowVal<'b, 'v>>) -> Result<CowVal<'b, 'v>, ExecutionError> {
    string_binary_fn(args, |s, needle| {
        Ok(Box::new(CelBool::from(s.starts_with(needle))))
    })
}

#[cfg(feature = "regex")]
fn matches<'b, 'v>(args: Vec<CowVal<'b, 'v>>) -> Result<CowVal<'b, 'v>, ExecutionError> {
    string_binary_fn(args, |this, regex| match regex::Regex::new(regex) {
        Ok(re) => Ok(Box::new(CelBool::from(re.is_match(this)))),
        Err(err) => Err(ExecutionError::FunctionError {
            function: "matches".to_string(),
            message: format!("'{regex}' not a valid regex:\n{err}"),
        }),
    })
}

fn string<'b, 'v>(mut args: Vec<CowVal<'b, 'v>>) -> Result<CowVal<'b, 'v>, ExecutionError> {
    let arg = args.remove(0);
    if arg.downcast_ref::<String>().is_some() {
        // `string(s)` is the identity: keep the borrow.
        return Ok(arg);
    }
    let converted: Option<StdString> = match arg.get_type().kind() {
        Kind::Int => arg.downcast_ref::<CelInt>().map(|i| i.to_string()),
        Kind::UInt => arg.downcast_ref::<CelUInt>().map(|u| u.to_string()),
        Kind::Double => arg.downcast_ref::<CelDouble>().map(|d| d.to_string()),
        Kind::Bytes => arg
            .downcast_ref::<CelBytes>()
            .map(|b| StdString::from_utf8_lossy(b.inner()).into_owned()),
        #[cfg(feature = "chrono")]
        Kind::Timestamp => arg
            .downcast_ref::<CelTimestamp>()
            .map(|ts| ts.inner().to_rfc3339()),
        #[cfg(feature = "chrono")]
        Kind::Duration => arg
            .downcast_ref::<CelDuration>()
            .map(|d| crate::duration::format_duration(d.inner())),
        _ => None,
    };
    match converted {
        Some(s) => Ok(CowVal::owned(String::from(s))),
        None => Err(ExecutionError::FunctionError {
            function: "string".to_owned(),
            message: format!("cannot convert {:?} to string", arg.as_ref()),
        }),
    }
}

pub(crate) fn stdlib(env: &mut crate::Env) {
    env.add_overload(
        "string",
        "string_to_string",
        vec![super::STRING_TYPE],
        string,
    )
    .expect("Must be unique id");
    env.add_overload("string", "int64_to_string", vec![super::INT_TYPE], string)
        .expect("Must be unique id");
    env.add_overload("string", "uint64_to_string", vec![super::UINT_TYPE], string)
        .expect("Must be unique id");
    env.add_overload(
        "string",
        "double_to_string",
        vec![super::DOUBLE_TYPE],
        string,
    )
    .expect("Must be unique id");
    env.add_overload("string", "bytes_to_string", vec![super::BYTES_TYPE], string)
        .expect("Must be unique id");

    #[cfg(feature = "chrono")]
    {
        env.add_overload(
            "string",
            "timestamp_to_string",
            vec![super::TIMESTAMP_TYPE],
            string,
        )
        .expect("Must be unique id");
        env.add_overload(
            "string",
            "duration_to_string",
            vec![super::DURATION_TYPE],
            string,
        )
        .expect("Must be unique id");
    }

    env.add_member_overload(
        "contains",
        "contains_string",
        super::STRING_TYPE,
        vec![super::STRING_TYPE],
        string_contains,
    )
    .expect("Must be unique id");
    env.add_member_overload(
        "endsWith",
        "ends_with_string",
        super::STRING_TYPE,
        vec![super::STRING_TYPE],
        ends_with_string,
    )
    .expect("Must be unique id");
    env.add_overload(
        "size",
        "size_string",
        vec![super::STRING_TYPE],
        traits::adapter::sizer_size,
    )
    .expect("Must be unique id");
    env.add_member_overload(
        "size",
        "string_size",
        super::STRING_TYPE,
        vec![],
        traits::adapter::sizer_size,
    )
    .expect("Must be unique id");
    env.add_member_overload(
        "startsWith",
        "starts_with_string",
        super::STRING_TYPE,
        vec![super::STRING_TYPE],
        starts_with_string,
    )
    .expect("Must be unique id");
    #[cfg(feature = "regex")]
    env.add_member_overload(
        "matches",
        "matches",
        super::STRING_TYPE,
        vec![super::STRING_TYPE],
        matches,
    )
    .expect("Must be unique id");
}

#[cfg(test)]
mod tests {
    use super::StdString;
    use super::String;
    use crate::common::value::{CowVal, Val};

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
        let out = super::string(vec![arg]).unwrap();
        let s = out.downcast_ref::<String>().unwrap();
        assert!(std::ptr::eq(s.inner(), owned.as_str()));
    }
}
