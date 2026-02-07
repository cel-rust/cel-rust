use crate::common::traits::{Adder, Comparer};
use crate::common::types::Type;
use crate::common::value::{BorrowedVal, Val};
use crate::ExecutionError;
use std::borrow::Cow;
use std::cmp::Ordering;
use std::ops::Deref;
use std::string::String as StdString;

#[derive(Debug, Default, Eq, Hash, PartialEq, PartialOrd, Ord)]
pub struct String(Cow<'static, str>);

impl String {
    pub fn into_inner(self) -> StdString {
        self.clone().into()
    }

    pub fn inner(&self) -> &str {
        self.0.as_ref()
    }
}

impl Clone for String {
    // SAFETY: this explicitly allocates a new `String` with the same contents as the original on the heap
    // as we can't _ever_ reuse the `&'static str`, or the `Cow::Borrowed`.
    fn clone(&self) -> Self {
        Self(Cow::Owned(StdString::from(self.inner())))
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

    fn as_adder(&self) -> Option<&dyn Adder> {
        Some(self)
    }

    fn as_comparer(&self) -> Option<&dyn Comparer> {
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
            Ok(Cow::<dyn Val>::Owned(Box::new(Self(s.to_owned().into()))))
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

impl From<StdString> for String {
    fn from(v: StdString) -> Self {
        Self(Cow::Owned(v))
    }
}

impl From<String> for StdString {
    fn from(v: String) -> Self {
        v.0.into_owned()
    }
}

impl From<&str> for String {
    fn from(value: &str) -> Self {
        Self(StdString::from(value).to_owned().into())
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

impl<'a> From<&'a str> for BorrowedVal<'a, String> {
    fn from(value: &'a str) -> Self {
        // SAFETY: BorrowedVal is retying the `'a` lifetime from the `&'a str`
        // now, the `String`'s borrowed `leaked' reference also need to _never_
        // escape the `String` by other means neither!
        unsafe {
            let leaked: &'static str = super::leak_ref(value);
            let val = String(Cow::Borrowed(leaked));
            BorrowedVal::new(val)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::StdString;
    use super::String;
    use crate::common::value::{BorrowedVal, Val};

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
    fn test_str() {
        let string = StdString::from("cel-rust");
        let val = {
            let s = string.as_ref();
            BorrowedVal::from(s)
        };
        let r = val.inner();
        assert_eq!(string.as_str(), r.inner());
        assert!(std::ptr::eq(string.as_str(), r.inner()));
        assert!(std::ptr::eq(string.as_str(), r.inner()));
    }

    #[test]
    fn test_clone() {
        let s = {
            let string = StdString::from("cel-rust");
            let val = {
                let s = string.as_ref();
                BorrowedVal::from(s)
            };
            val.inner().clone()
        };
        assert_eq!(s, String::from("cel-rust"));
    }
}
