use crate::common::traits;
use crate::common::types::Type;
use crate::common::value::Val;
use crate::ExecutionError;
use std::borrow::Cow;
use std::ops::Deref;
use std::string::String as StdString;

#[derive(Clone, Debug, Default, PartialEq)]
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

impl Val for String {
    fn get_type(&self) -> Type<'_> {
        super::STRING_TYPE
    }

    fn clone_as_boxed(&self) -> Box<dyn Val> {
        Box::new(String(self.0.clone()))
    }

    fn equals(&self, other: &dyn Val) -> bool {
        match other.downcast_ref::<Self>() {
            Some(s) => self.0 == s.0,
            None => false,
        }
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

impl traits::Adder for String {
    fn add<'a>(&self, _rhs: &'a dyn Val) -> Result<Cow<'a, dyn Val>, ExecutionError> {
        todo!("implement Adder for String!")
    }
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
