use crate::common::traits::{Adder, Comparer, Divider, Modder, Multiplier, Subtractor};
use crate::common::types::{CelDouble, CelInt, Type};
use crate::common::value::Val;
use crate::{ExecutionError, Value};
use std::borrow::Cow;
use std::cmp::Ordering;
use std::ops::Deref;

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, PartialOrd, Ord)]
pub struct UInt(u64);

impl UInt {
    pub fn into_inner(self) -> u64 {
        self.0
    }

    pub fn inner(&self) -> &u64 {
        &self.0
    }
}

impl Deref for UInt {
    type Target = u64;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Val for UInt {
    fn get_type(&self) -> Type<'_> {
        super::UINT_TYPE
    }

    fn as_adder(&self) -> Option<&dyn Adder> {
        Some(self)
    }

    fn as_comparer(&self) -> Option<&dyn Comparer> {
        Some(self)
    }

    fn as_divider(&self) -> Option<&dyn Divider> {
        Some(self)
    }

    fn as_modder(&self) -> Option<&dyn Modder> {
        Some(self)
    }

    fn as_multiplier(&self) -> Option<&dyn Multiplier> {
        Some(self)
    }

    fn as_subtractor(&self) -> Option<&dyn Subtractor> {
        Some(self)
    }

    fn equals(&self, other: &dyn Val) -> bool {
        other
            .downcast_ref::<Self>()
            .is_some_and(|other| self.0 == other.0)
    }

    fn clone_as_boxed(&self) -> Box<dyn Val> {
        Box::new(UInt(self.0))
    }
}

impl Adder for UInt {
    fn add<'a>(&'a self, rhs: &dyn Val) -> Result<Cow<'a, dyn Val>, ExecutionError> {
        if let Some(rhs) = rhs.downcast_ref::<Self>() {
            Ok(Cow::<dyn Val>::Owned(Box::new(UInt(
                self.0.checked_add(rhs.0).ok_or(ExecutionError::Overflow(
                    "add",
                    (self as &dyn Val).try_into().unwrap_or(Value::Null),
                    (rhs as &dyn Val).try_into().unwrap_or(Value::Null),
                ))?,
            ))))
        } else {
            Err(ExecutionError::UnsupportedBinaryOperator(
                "add",
                (self as &dyn Val).try_into().unwrap_or(Value::Null),
                rhs.try_into().unwrap_or(Value::Null),
            ))
        }
    }
}

impl Comparer for UInt {
    fn compare(&self, rhs: &dyn Val) -> Result<Ordering, ExecutionError> {
        if let Some(rhs) = rhs.downcast_ref::<Self>() {
            Ok(self.0.cmp(&rhs.0))
        } else if let Some(rhs) = rhs.downcast_ref::<CelInt>() {
            Ok(self
                .0
                .try_into()
                .map(|a: i64| a.cmp(rhs.inner()))
                // If the u64 doesn't fit into a i64 it must be greater than i64::MAX.
                .unwrap_or(Ordering::Greater))
        } else if let Some(rhs) = rhs.downcast_ref::<CelDouble>() {
            Ok((*self.inner() as f64)
                .partial_cmp(rhs.inner())
                .ok_or(ExecutionError::NoSuchOverload)?)
        } else {
            Err(ExecutionError::NoSuchOverload)
        }
    }
}

impl Divider for UInt {
    fn div<'a>(&self, rhs: &'a dyn Val) -> Result<Cow<'a, dyn Val>, ExecutionError> {
        if let Some(rhs) = rhs.downcast_ref::<Self>() {
            if rhs.0 == 0 {
                return Err(ExecutionError::DivisionByZero(self.0.into()));
            }
            Ok(Cow::<dyn Val>::Owned(Box::new(UInt(
                self.0.checked_div(rhs.0).ok_or(ExecutionError::Overflow(
                    "div",
                    (self as &dyn Val).try_into().unwrap_or(Value::Null),
                    (rhs as &dyn Val).try_into().unwrap_or(Value::Null),
                ))?,
            ))))
        } else {
            Err(ExecutionError::UnsupportedBinaryOperator(
                "div",
                (self as &dyn Val).try_into().unwrap_or(Value::Null),
                rhs.try_into().unwrap_or(Value::Null),
            ))
        }
    }
}

impl Modder for UInt {
    fn modulo<'a>(&self, rhs: &'a dyn Val) -> Result<Cow<'a, dyn Val>, ExecutionError> {
        if let Some(rhs) = rhs.downcast_ref::<Self>() {
            if rhs.0 == 0 {
                return Err(ExecutionError::RemainderByZero(self.0.into()));
            }
            Ok(Cow::<dyn Val>::Owned(Box::new(UInt(
                self.0.checked_rem(rhs.0).ok_or(ExecutionError::Overflow(
                    "rem",
                    (self as &dyn Val).try_into().unwrap_or(Value::Null),
                    (rhs as &dyn Val).try_into().unwrap_or(Value::Null),
                ))?,
            ))))
        } else {
            Err(ExecutionError::UnsupportedBinaryOperator(
                "rem",
                (self as &dyn Val).try_into().unwrap_or(Value::Null),
                rhs.try_into().unwrap_or(Value::Null),
            ))
        }
    }
}

impl Multiplier for UInt {
    fn mul<'a>(&self, rhs: &'a dyn Val) -> Result<Cow<'a, dyn Val>, ExecutionError> {
        if let Some(rhs) = rhs.downcast_ref::<Self>() {
            Ok(Cow::<dyn Val>::Owned(Box::new(UInt(
                self.0.checked_mul(rhs.0).ok_or(ExecutionError::Overflow(
                    "mul",
                    (self as &dyn Val).try_into().unwrap_or(Value::Null),
                    (rhs as &dyn Val).try_into().unwrap_or(Value::Null),
                ))?,
            ))))
        } else {
            Err(ExecutionError::UnsupportedBinaryOperator(
                "mul",
                (self as &dyn Val).try_into().unwrap_or(Value::Null),
                rhs.try_into().unwrap_or(Value::Null),
            ))
        }
    }
}

impl Subtractor for UInt {
    fn sub<'a>(&'a self, rhs: &'_ dyn Val) -> Result<Cow<'a, dyn Val>, ExecutionError> {
        if let Some(rhs) = rhs.downcast_ref::<Self>() {
            Ok(Cow::<dyn Val>::Owned(Box::new(UInt(
                self.0.checked_sub(rhs.0).ok_or(ExecutionError::Overflow(
                    "sub",
                    (self as &dyn Val).try_into().unwrap_or(Value::Null),
                    (rhs as &dyn Val).try_into().unwrap_or(Value::Null),
                ))?,
            ))))
        } else {
            Err(ExecutionError::UnsupportedBinaryOperator(
                "sub",
                (self as &dyn Val).try_into().unwrap_or(Value::Null),
                rhs.try_into().unwrap_or(Value::Null),
            ))
        }
    }
}

impl From<UInt> for u64 {
    fn from(value: UInt) -> Self {
        value.0
    }
}

impl From<u64> for UInt {
    fn from(value: u64) -> Self {
        Self(value)
    }
}

impl TryFrom<Box<dyn Val>> for u64 {
    type Error = Box<dyn Val>;

    fn try_from(value: Box<dyn Val>) -> Result<Self, Self::Error> {
        if let Some(u) = value.downcast_ref::<UInt>() {
            return Ok(u.0);
        }
        Err(value)
    }
}

impl<'a> TryFrom<&'a dyn Val> for &'a u64 {
    type Error = &'a dyn Val;
    fn try_from(value: &'a dyn Val) -> Result<Self, Self::Error> {
        if let Some(u) = value.downcast_ref::<UInt>() {
            return Ok(&u.0);
        }
        Err(value)
    }
}
