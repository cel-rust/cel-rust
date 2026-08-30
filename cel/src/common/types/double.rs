use crate::common::traits::{Adder, Comparer, Divider, Multiplier, Negator, Subtractor, Zeroer};
use crate::common::types::{CelInt, CelString, CelUInt, Type};
use crate::common::value::Val;
use crate::{ExecutionError, Value};
use std::borrow::Cow;
use std::cmp::Ordering;
use std::ops::Deref;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Double(f64);

impl Double {
    pub fn into_inner(self) -> f64 {
        self.0
    }

    pub fn inner(&self) -> &f64 {
        &self.0
    }
}

impl Deref for Double {
    type Target = f64;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl super::CelValType for Double {
    fn cel_type() -> &'static Type {
        &super::DOUBLE_TYPE
    }
}

impl Val for Double {
    fn get_type(&self) -> &Type {
        &super::DOUBLE_TYPE
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

    fn as_multiplier(&self) -> Option<&dyn Multiplier> {
        Some(self)
    }

    fn as_negator(&self) -> Option<&dyn Negator> {
        Some(self)
    }

    fn as_subtractor(&self) -> Option<&dyn Subtractor> {
        Some(self)
    }

    fn as_zeroer(&self) -> Option<&dyn Zeroer> {
        Some(self)
    }

    fn equals(&self, other: &dyn Val) -> bool {
        self.compare(other)
            .map(|r| r == Ordering::Equal)
            .unwrap_or(false)
    }

    fn clone_as_boxed(&self) -> Box<dyn Val> {
        Box::new(Double(self.0))
    }
}

impl Adder for Double {
    fn add<'a>(&'a self, rhs: &dyn Val) -> Result<Cow<'a, dyn Val>, ExecutionError> {
        if let Some(other) = rhs.downcast_ref::<Self>() {
            Ok(Cow::<dyn Val>::Owned(Box::new(Double(self.0 + other.0))))
        } else {
            Err(ExecutionError::UnsupportedBinaryOperator(
                "add",
                (self as &dyn Val).try_into().unwrap_or(Value::Null),
                rhs.try_into().unwrap_or(Value::Null),
            ))
        }
    }
}

impl Comparer for Double {
    fn compare(&self, rhs: &dyn Val) -> Result<Ordering, ExecutionError> {
        if let Some(rhs) = rhs.downcast_ref::<Self>() {
            Ok(self
                .0
                .partial_cmp(&rhs.0)
                .ok_or(ExecutionError::NoSuchOverload)?)
        } else if let Some(rhs) = rhs.downcast_ref::<CelInt>() {
            Ok(self
                .0
                .partial_cmp(&(*rhs.inner() as f64))
                .ok_or(ExecutionError::NoSuchOverload)?)
        } else if let Some(rhs) = rhs.downcast_ref::<CelUInt>() {
            Ok(self
                .0
                .partial_cmp(&(*rhs.inner() as f64))
                .ok_or(ExecutionError::NoSuchOverload)?)
        } else {
            Err(ExecutionError::NoSuchOverload)
        }
    }
}

impl Divider for Double {
    fn div<'a>(&self, rhs: &'a dyn Val) -> Result<Cow<'a, dyn Val>, ExecutionError> {
        if let Some(rhs) = rhs.downcast_ref::<Double>() {
            Ok(Cow::<dyn Val>::Owned(Box::new(Double(self.0 / rhs.0))))
        } else {
            Err(ExecutionError::UnsupportedBinaryOperator(
                "div",
                (self as &dyn Val).try_into().unwrap_or(Value::Null),
                rhs.try_into().unwrap_or(Value::Null),
            ))
        }
    }
}

impl Multiplier for Double {
    fn mul<'a>(&self, rhs: &'a dyn Val) -> Result<Cow<'a, dyn Val>, ExecutionError> {
        if let Some(rhs) = rhs.downcast_ref::<Double>() {
            Ok(Cow::<dyn Val>::Owned(Box::new(Double(self.0 * rhs.0))))
        } else {
            Err(ExecutionError::UnsupportedBinaryOperator(
                "mul",
                (self as &dyn Val).try_into().unwrap_or(Value::Null),
                rhs.try_into().unwrap_or(Value::Null),
            ))
        }
    }
}

impl Negator for Double {
    fn negate(&self) -> Result<Box<dyn Val>, ExecutionError> {
        Ok(Box::new(Double(-self.0)))
    }
}

impl Subtractor for Double {
    fn sub<'a>(&'a self, rhs: &'_ dyn Val) -> Result<Cow<'a, dyn Val>, ExecutionError> {
        if let Some(rhs) = rhs.downcast_ref::<Double>() {
            Ok(Cow::<dyn Val>::Owned(Box::new(Double(self.0 - rhs.0))))
        } else {
            Err(ExecutionError::UnsupportedBinaryOperator(
                "sub",
                (self as &dyn Val).try_into().unwrap_or(Value::Null),
                rhs.try_into().unwrap_or(Value::Null),
            ))
        }
    }
}

impl Zeroer for Double {
    fn is_zero_value(&self) -> bool {
        self.0 == 0.0
    }
}

impl From<Double> for f64 {
    fn from(value: Double) -> Self {
        value.0
    }
}

impl From<f64> for Double {
    fn from(value: f64) -> Self {
        Self(value)
    }
}

impl TryFrom<Box<dyn Val>> for f64 {
    type Error = Box<dyn Val>;

    fn try_from(value: Box<dyn Val>) -> Result<Self, Self::Error> {
        if let Some(d) = value.downcast_ref::<Double>() {
            return Ok(d.0);
        }
        Err(value)
    }
}

impl<'a> TryFrom<&'a dyn Val> for &'a f64 {
    type Error = &'a dyn Val;

    fn try_from(value: &'a dyn Val) -> Result<Self, Self::Error> {
        if let Some(d) = value.downcast_ref::<Double>() {
            return Ok(&d.0);
        }
        Err(value)
    }
}

fn double_from_double<'a>(this: Cow<'a, Double>) -> Result<Cow<'a, Double>, ExecutionError> {
    Ok(this)
}

fn double_from_int<'a>(this: Cow<'a, CelInt>) -> Result<Cow<'a, Double>, ExecutionError> {
    Ok(Cow::Owned(Double::from(*this.inner() as f64)))
}

fn double_from_uint<'a>(this: Cow<'a, CelUInt>) -> Result<Cow<'a, Double>, ExecutionError> {
    Ok(Cow::Owned(Double::from(*this.inner() as f64)))
}

fn double_from_string<'a>(this: Cow<'a, CelString>) -> Result<Cow<'a, Double>, ExecutionError> {
    this.inner()
        .parse::<f64>()
        .map(|v| Cow::Owned(Double::from(v)))
        .map_err(|e| ExecutionError::FunctionError {
            function: "double".to_owned(),
            message: format!("string parse error: {e}"),
        })
}

pub(crate) fn stdlib(env: &mut crate::Env) {
    crate::add_overload!(env, fn double_from_double: (Double) -> Double,
        name = "double", id = "double_to_double");
    crate::add_overload!(env, fn double_from_int: (CelInt) -> Double,
        name = "double", id = "int64_to_double");
    crate::add_overload!(env, fn double_from_uint: (CelUInt) -> Double,
        name = "double", id = "uint64_to_double");
    crate::add_overload!(env, fn double_from_string: (CelString) -> Double,
        name = "double", id = "string_to_double");
}

#[cfg(test)]
mod tests {
    use crate::common::types::{CelDouble, CelInt, CelString, CelUInt};
    use crate::common::value::Val;

    #[test]
    fn test_equals() {
        let double = CelDouble::from(42.2);
        let round = CelDouble::from(42.0);
        assert!(double.equals(&double));
        assert!(!double.equals(&round));
        assert!(!double.equals(&CelInt::from(42)));
        assert!(round.equals(&CelInt::from(42)));
        assert!(!double.equals(&CelUInt::from(42)));
        assert!(round.equals(&CelUInt::from(42)));
        assert!(!double.equals(&CelString::from("42.2")));
        assert!(!round.equals(&CelString::from("42")));
        assert!(!round.equals(&CelDouble::from(f64::NAN)));
    }
}
