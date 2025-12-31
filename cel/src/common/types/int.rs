use crate::common::traits;
use crate::common::types::Type;
use crate::common::value::Val;
use crate::ExecutionError;
use std::borrow::Cow;
use std::cmp::Ordering;
use std::ops::Deref;

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, PartialOrd, Ord)]
pub struct Int(i64);

impl Int {
    pub fn into_inner(self) -> i64 {
        self.0
    }

    pub fn inner(&self) -> &i64 {
        &self.0
    }
}

impl Deref for Int {
    type Target = i64;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Val for Int {
    fn get_type(&self) -> Type<'_> {
        super::INT_TYPE
    }

    fn as_adder(&self) -> Option<&dyn traits::Adder> {
        Some(self)
    }

    fn as_comparer(&self) -> Option<&dyn traits::Comparer> {
        Some(self)
    }

    fn as_divider(&self) -> Option<&dyn traits::Divider> {
        Some(self)
    }

    fn as_modder(&self) -> Option<&dyn traits::Modder> {
        Some(self)
    }

    fn as_multiplier(&self) -> Option<&dyn traits::Multiplier> {
        Some(self)
    }

    fn as_subtractor(&self) -> Option<&dyn traits::Subtractor> {
        Some(self)
    }

    fn equals(&self, other: &dyn Val) -> bool {
        other
            .downcast_ref::<Self>()
            .is_some_and(|other| self.0 == other.0)
    }

    fn clone_as_boxed(&self) -> Box<dyn Val> {
        Box::new(Int(self.0))
    }
}

impl traits::Adder for Int {
    fn add<'a>(&'a self, other: &dyn Val) -> Result<Cow<'a, dyn Val>, ExecutionError> {
        if let Some(i) = other.downcast_ref::<Int>() {
            let t: Self = self
                .0
                .checked_add(i.0)
                .ok_or(ExecutionError::Overflow("add", self.0.into(), i.0.into()))?
                .into();
            let b: Box<dyn Val> = Box::new(t);
            Ok(Cow::Owned(b))
        } else {
            Err(ExecutionError::NoSuchOverload)
        }
    }
}

impl traits::Comparer for Int {
    fn compare(&self, rhs: &dyn Val) -> Result<Ordering, ExecutionError> {
        if let Some(i) = rhs.downcast_ref::<Int>() {
            Ok(self.0.cmp(&i.0))
        } else {
            Err(ExecutionError::NoSuchOverload)
        }
    }
}

impl traits::Divider for Int {
    fn div<'a>(&self, rhs: &'a dyn Val) -> Result<Cow<'a, dyn Val>, ExecutionError> {
        if let Some(i) = rhs.downcast_ref::<Int>() {
            if i.0 == 0 {
                return Err(ExecutionError::DivisionByZero(self.0.into()));
            }
            let t: Self = (self.0.checked_div(i.0).ok_or(ExecutionError::Overflow(
                "div",
                self.0.into(),
                i.0.into(),
            ))?)
            .into();
            let b: Box<dyn Val> = Box::new(t);
            Ok(Cow::Owned(b))
        } else {
            Err(ExecutionError::NoSuchOverload)
        }
    }
}

impl traits::Modder for Int {
    fn modulo<'a>(&self, rhs: &'a dyn Val) -> Result<Cow<'a, dyn Val>, ExecutionError> {
        if let Some(i) = rhs.downcast_ref::<Int>() {
            if i.0 == 0 {
                return Err(ExecutionError::RemainderByZero(self.0.into()));
            }
            let t: Self = (self.0.checked_rem(i.0).ok_or(ExecutionError::Overflow(
                "rem",
                self.0.into(),
                i.0.into(),
            ))?)
            .into();
            let b: Box<dyn Val> = Box::new(t);
            Ok(Cow::Owned(b))
        } else {
            Err(ExecutionError::NoSuchOverload)
        }
    }
}

impl traits::Multiplier for Int {
    fn mul<'a>(&self, rhs: &'a dyn Val) -> Result<Cow<'a, dyn Val>, ExecutionError> {
        if let Some(i) = rhs.downcast_ref::<Int>() {
            let t: Self = (self.0.checked_mul(i.0).ok_or(ExecutionError::Overflow(
                "mul",
                self.0.into(),
                i.0.into(),
            ))?)
            .into();
            let b: Box<dyn Val> = Box::new(t);
            Ok(Cow::Owned(b))
        } else {
            Err(ExecutionError::NoSuchOverload)
        }
    }
}

impl traits::Subtractor for Int {
    fn sub<'a>(&'a self, rhs: &dyn Val) -> Result<Cow<'a, dyn Val>, ExecutionError> {
        if let Some(i) = rhs.downcast_ref::<Int>() {
            Ok(Cow::<dyn Val>::Owned(Box::new(Self::from(
                self.0.checked_sub(i.0).ok_or(ExecutionError::Overflow(
                    "sub",
                    self.0.into(),
                    i.0.into(),
                ))?,
            ))))
        } else {
            Err(ExecutionError::NoSuchOverload)
        }
    }
}

impl From<Int> for i64 {
    fn from(value: Int) -> Self {
        value.0
    }
}

impl From<i64> for Int {
    fn from(value: i64) -> Self {
        Self(value)
    }
}

impl TryFrom<Box<dyn Val>> for i64 {
    type Error = Box<dyn Val>;

    fn try_from(value: Box<dyn Val>) -> Result<Self, Self::Error> {
        if let Some(i) = value.downcast_ref::<Int>() {
            return Ok(i.0);
        }
        Err(value)
    }
}

impl<'a> TryFrom<&'a dyn Val> for &'a i64 {
    type Error = &'a dyn Val;

    fn try_from(value: &'a dyn Val) -> Result<Self, Self::Error> {
        if let Some(i) = value.downcast_ref::<Int>() {
            return Ok(&i.0);
        }
        Err(value)
    }
}

#[cfg(test)]
mod tests {
    use crate::common::traits::Comparer;
    use crate::common::types::CelInt;
    use std::cmp::Ordering::{Equal, Greater, Less};

    #[test]
    fn test_compare() {
        let one = CelInt::from(1);
        let two = CelInt::from(2);
        assert_eq!(one.compare(&two), Ok(Less));
        assert_eq!(two.compare(&one), Ok(Greater));
        assert_eq!(two.compare(&two), Ok(Equal));
    }
}
