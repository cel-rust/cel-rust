use crate::common::traits::{self, Adder, Comparer, Sizer, Zeroer};
use crate::common::types::{CelBool, CelBytes, CelDouble, CelInt, CelUInt, Kind, Type};
#[cfg(feature = "chrono")]
use crate::common::types::{CelDuration, CelTimestamp};
use crate::common::value::{Downcast, Val};
use crate::ExecutionError;
use std::borrow::Cow;
use std::cmp::Ordering;
use std::ops::Deref;
use std::string::String as StdString;

#[derive(Clone, Debug, Default, Eq, Hash, PartialEq, PartialOrd, Ord)]
pub struct String(StdString);

impl String {
    pub fn into_inner(self) -> StdString {
        self.0
    }

    pub fn inner(&self) -> &str {
        &self.0
    }
}

impl Deref for String {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.inner()
    }
}

impl super::CelValType for String {
    fn cel_type() -> &'static Type {
        &super::STRING_TYPE
    }
}

impl Val for String {
    fn get_type(&self) -> &Type {
        &super::STRING_TYPE
    }

    fn as_adder(&self) -> Option<&dyn Adder> {
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
            .downcast_ref::<Self>()
            .is_some_and(|other| self.0 == other.0)
    }

    fn clone_as_boxed(&self) -> Box<dyn Val> {
        Box::new(String(self.0.clone()))
    }
}

impl Adder for String {
    fn add<'a>(&'a self, rhs: &dyn Val) -> Result<Cow<'a, dyn Val>, ExecutionError> {
        if let Some(rhs) = rhs.downcast_ref::<Self>() {
            let mut s = StdString::with_capacity(rhs.0.len() + self.0.len());
            s.push_str(&self.0);
            s.push_str(&rhs.0);
            Ok(Cow::<dyn Val>::Owned(Box::new(Self(s))))
        } else {
            Err(ExecutionError::UnsupportedBinaryOperator(
                "add",
                (self as &dyn Val).try_into()?,
                rhs.try_into()?,
            ))
        }
    }
}

impl Comparer for String {
    fn compare(&self, rhs: &dyn Val) -> Result<Ordering, ExecutionError> {
        if let Some(rhs) = rhs.downcast_ref::<Self>() {
            Ok(self.0.cmp(&rhs.0))
        } else {
            Err(ExecutionError::NoSuchOverload)
        }
    }
}

impl Sizer for String {
    fn size(&self) -> CelInt {
        (self.inner().len() as i64).into()
    }
}

impl Zeroer for String {
    fn is_zero_value(&self) -> bool {
        self.inner().is_empty()
    }
}

impl From<StdString> for String {
    fn from(v: StdString) -> Self {
        Self(v)
    }
}

impl From<String> for StdString {
    fn from(v: String) -> Self {
        v.0
    }
}

impl From<&str> for String {
    fn from(value: &str) -> Self {
        Self(StdString::from(value))
    }
}

impl TryFrom<Box<dyn Val>> for StdString {
    type Error = Box<dyn Val>;

    fn try_from(value: Box<dyn Val>) -> Result<Self, Self::Error> {
        super::cast_boxed::<String>(value).map(|s| s.into_inner())
    }
}

impl<'a> TryFrom<&'a dyn Val> for &'a str {
    type Error = &'a dyn Val;
    fn try_from(value: &'a dyn Val) -> Result<Self, Self::Error> {
        if let Some(s) = value.downcast_ref::<String>() {
            return Ok(s.inner());
        }
        Err(value)
    }
}

fn contains<'a>(
    this: Cow<'a, String>,
    needle: Cow<'a, String>,
) -> Result<Cow<'a, CelBool>, ExecutionError> {
    Ok(Cow::Owned(CelBool::from(this.contains(needle.inner()))))
}

fn ends_with<'a>(
    this: Cow<'a, String>,
    needle: Cow<'a, String>,
) -> Result<Cow<'a, CelBool>, ExecutionError> {
    Ok(Cow::Owned(CelBool::from(this.ends_with(needle.inner()))))
}

fn starts_with<'a>(
    this: Cow<'a, String>,
    needle: Cow<'a, String>,
) -> Result<Cow<'a, CelBool>, ExecutionError> {
    Ok(Cow::Owned(CelBool::from(this.starts_with(needle.inner()))))
}

fn size<'a>(this: Cow<'a, String>) -> Result<Cow<'a, CelInt>, ExecutionError> {
    Ok(Cow::Owned((this.inner().len() as i64).into()))
}

#[cfg(feature = "regex")]
fn matches<'a>(
    this: Cow<'a, String>,
    re: Cow<'a, String>,
) -> Result<Cow<'a, CelBool>, ExecutionError> {
    match regex::Regex::new(re.inner()) {
        Ok(re) => Ok(Cow::Owned(CelBool::from(re.is_match(this.inner())))),
        Err(err) => Err(ExecutionError::FunctionError {
            function: "matches".to_string(),
            message: format!("'{}' not a valid regex:\n{err}", re.inner()),
        }),
    }
}

fn string<'a>(args: Vec<Cow<'a, dyn Val>>) -> Result<Cow<'a, dyn Val>, ExecutionError> {
    let mut args = args;
    let arg = args.remove(0).into_owned();
    let ret: Result<Box<String>, Box<dyn Val>> = match arg.get_type().kind() {
        Kind::String => arg.downcast::<String>(),
        Kind::Int => arg
            .downcast::<CelInt>()
            .map(|arg| Box::new(String::from(arg.to_string()))),
        Kind::UInt => arg
            .downcast::<CelUInt>()
            .map(|arg| Box::new(String::from(arg.to_string()))),
        Kind::Double => arg
            .downcast::<CelDouble>()
            .map(|arg| Box::new(String::from(arg.to_string()))),
        Kind::Bytes => arg.downcast::<CelBytes>().map(|arg| {
            Box::new(String::from(
                StdString::from_utf8_lossy(arg.inner()).as_ref(),
            ))
        }),
        #[cfg(feature = "chrono")]
        Kind::Timestamp => arg
            .downcast::<CelTimestamp>()
            .map(|ts| Box::new(String::from(ts.inner().to_rfc3339()))),
        #[cfg(feature = "chrono")]
        Kind::Duration => arg
            .downcast::<CelDuration>()
            .map(|arg| Box::new(String::from(crate::duration::format_duration(arg.inner())))),
        _ => Err(arg),
    };
    match ret {
        Ok(ret) => Ok(Cow::<dyn Val>::Owned(ret)),
        Err(arg) => Err(ExecutionError::FunctionError {
            function: "string".to_owned(),
            message: format!("cannot convert {arg:?} to string"),
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

    crate::add_member_overload!(env, fn contains: (String, String) -> CelBool);
    crate::add_member_overload!(env, fn ends_with: (String, String) -> CelBool,
        name = "endsWith");
    env.add_overload(
        "size",
        "size_string",
        vec![super::STRING_TYPE],
        traits::adapter::sizer_size,
    )
    .expect("Must be unique id");
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
    use crate::common::value::Val;

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
}
