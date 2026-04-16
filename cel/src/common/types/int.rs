use crate::common::traits::Negator;
use crate::common::traits::{self, Comparer};
use crate::common::types::{CelDouble, CelString, CelUInt, Type};
use crate::common::value::{Downcast, Val};
use crate::ExecutionError;
use std::borrow::Cow;
use std::cmp::Ordering;
use std::ops::{Deref, Neg};

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
    fn get_type<'a>(&self) -> &Type<'a> {
        &super::INT_TYPE
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

    fn as_negator(&self) -> Option<&dyn Negator> {
        Some(self)
    }

    fn as_subtractor(&self) -> Option<&dyn traits::Subtractor> {
        Some(self)
    }

    fn equals(&self, other: &dyn Val) -> bool {
        self.compare(other)
            .map(|r| r == Ordering::Equal)
            .unwrap_or(false)
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
                .ok_or_else(|| ExecutionError::Overflow("add", self.0.into(), i.0.into()))?
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
        if let Some(i) = rhs.downcast_ref::<Self>() {
            Ok(self.0.cmp(&i.0))
        } else if let Some(u) = rhs.downcast_ref::<CelUInt>() {
            Ok((*self.inner())
                .try_into()
                .map(|a: u64| a.cmp(u.inner()))
                // If the i64 doesn't fit into a u64 it must be less than 0.
                .unwrap_or(Ordering::Less))
        } else if let Some(d) = rhs.downcast_ref::<CelDouble>() {
            Ok((*self.inner() as f64)
                .partial_cmp(d.inner())
                .ok_or(ExecutionError::NoSuchOverload)?)
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
            let t: Self = (self
                .0
                .checked_div(i.0)
                .ok_or_else(|| ExecutionError::Overflow("div", self.0.into(), i.0.into()))?)
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
            let t: Self = (self
                .0
                .checked_rem(i.0)
                .ok_or_else(|| ExecutionError::Overflow("rem", self.0.into(), i.0.into()))?)
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
            let t: Self = (self
                .0
                .checked_mul(i.0)
                .ok_or_else(|| ExecutionError::Overflow("mul", self.0.into(), i.0.into()))?)
            .into();
            let b: Box<dyn Val> = Box::new(t);
            Ok(Cow::Owned(b))
        } else {
            Err(ExecutionError::NoSuchOverload)
        }
    }
}

impl Negator for Int {
    fn negate(&self) -> Result<Box<dyn Val>, ExecutionError> {
        Ok(Box::new(Self::from(self.0.neg())))
    }
}

impl traits::Subtractor for Int {
    fn sub<'a>(&'a self, rhs: &dyn Val) -> Result<Cow<'a, dyn Val>, ExecutionError> {
        if let Some(i) = rhs.downcast_ref::<Int>() {
            Ok(Cow::<dyn Val>::Owned(Box::new(Self::from(
                self.0
                    .checked_sub(i.0)
                    .ok_or_else(|| ExecutionError::Overflow("sub", self.0.into(), i.0.into()))?,
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

fn int<'a>(args: Vec<Cow<'a, dyn Val>>) -> Result<Cow<'a, dyn Val>, ExecutionError> {
    let mut args = args;
    let arg = args.remove(0).into_owned();
    let ret: Result<Box<Int>, Box<dyn Val>> = match *arg.get_type() {
        super::INT_TYPE => arg.downcast::<Int>(),
        super::UINT_TYPE => arg
            .downcast::<CelUInt>()
            .map(|arg| Box::new(Int::from(*arg.inner() as i64))),
        super::DOUBLE_TYPE => arg
            .downcast::<CelDouble>()
            .map(|arg| Box::new(Int::from(*arg.inner() as i64))),
        super::STRING_TYPE => match arg.downcast::<CelString>() {
            Err(arg) => Err(arg),
            Ok(arg) => match arg.inner().parse::<i64>() {
                Ok(arg) => Ok(Box::new(Int::from(arg))),
                Err(e) => {
                    return Err(ExecutionError::FunctionError {
                        function: "int".to_owned(),
                        message: format!("string parse error: {e}"),
                    })
                }
            },
        },
        _ => Err(arg),
    };

    match ret {
        Ok(ret) => Ok(Cow::<dyn Val>::Owned(ret)),
        Err(arg) => Err(ExecutionError::FunctionError {
            function: "double".to_owned(),
            message: format!("cannot convert {arg:?} to double"),
        }),
    }
}

pub(crate) fn stdlib(env: &mut crate::Env<'_>) {
    env.add_overload("int", "int64_to_int64", vec![super::INT_TYPE], int)
        .expect("Must be unique id");
    env.add_overload("int", "uint64_to_int64", vec![super::UINT_TYPE], int)
        .expect("Must be unique id");
    env.add_overload("int", "double_to_int64", vec![super::DOUBLE_TYPE], int)
        .expect("Must be unique id");
    env.add_overload("int", "string_to_int64", vec![super::STRING_TYPE], int)
        .expect("Must be unique id");
}

#[cfg(test)]
mod tests {
    use crate::common::traits::Comparer;
    use crate::common::types::{CelDouble, CelInt, CelString, CelUInt};
    use crate::common::value::Val;
    use std::cmp::Ordering::{Equal, Greater, Less};

    #[test]
    fn test_compare() {
        let one = CelInt::from(1);
        let two = CelInt::from(2);
        assert_eq!(one.compare(&two), Ok(Less));
        assert_eq!(two.compare(&one), Ok(Greater));
        assert_eq!(two.compare(&two), Ok(Equal));
    }

    #[test]
    fn test_equals() {
        let int = CelInt::from(42);
        let neg = CelInt::from(-42);
        assert!(int.equals(&int));
        assert!(int.equals(&CelUInt::from(42u64)));
        assert!(!neg.equals(&CelUInt::from(42u64)));
        assert!(int.equals(&CelDouble::from(42.0)));
        assert!(neg.equals(&CelDouble::from(-42.0)));
        assert!(!int.equals(&CelDouble::from(f64::NAN)));
        assert!(!neg.equals(&CelDouble::from(f64::NAN)));
        assert!(!int.equals(&CelString::from("42")));
    }
}
