use crate::common::traits::{Adder, Comparer, Divider, Multiplier, Negator, Subtractor, Zeroer};
use crate::common::types::{CelInt, CelString, CelUInt, Kind, Type};
use crate::common::value::{CowVal, StaticVal, Val};
use crate::{ExecutionError, Value};
use std::any::Any;
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

impl Val for Double {
    fn get_type(&self) -> &Type {
        &super::DOUBLE_TYPE
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

    fn as_divider<'b, 'v>(&'b self) -> Option<&'b (dyn Divider + 'v)>
    where
        Self: 'v,
    {
        Some(self)
    }

    fn as_multiplier<'b, 'v>(&'b self) -> Option<&'b (dyn Multiplier + 'v)>
    where
        Self: 'v,
    {
        Some(self)
    }

    fn as_negator<'b, 'v>(&'b self) -> Option<&'b (dyn Negator + 'v)>
    where
        Self: 'v,
    {
        Some(self)
    }

    fn as_subtractor<'b, 'v>(&'b self) -> Option<&'b (dyn Subtractor + 'v)>
    where
        Self: 'v,
    {
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

    fn clone_as_boxed<'v>(&self) -> Box<dyn Val + 'v>
    where
        Self: 'v,
    {
        Box::new(*self)
    }

    fn as_any(&self) -> Option<&dyn Any> {
        Some(self)
    }
}

impl StaticVal for Double {}

fn unsupported(op: &'static str, lhs: &dyn Val, rhs: &dyn Val) -> ExecutionError {
    ExecutionError::UnsupportedBinaryOperator(
        op,
        lhs.try_into().unwrap_or(Value::Null),
        rhs.try_into().unwrap_or(Value::Null),
    )
}

impl Adder for Double {
    fn add<'b, 'v>(&'b self, rhs: &(dyn Val + 'v)) -> Result<CowVal<'b, 'v>, ExecutionError>
    where
        Self: 'v,
    {
        if let Some(other) = rhs.downcast_ref::<Self>() {
            Ok(CowVal::owned(Double(self.0 + other.0)))
        } else {
            Err(unsupported("add", self, rhs))
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
    fn div<'b, 'v>(&'b self, rhs: &(dyn Val + 'v)) -> Result<CowVal<'b, 'v>, ExecutionError>
    where
        Self: 'v,
    {
        if let Some(rhs) = rhs.downcast_ref::<Double>() {
            Ok(CowVal::owned(Double(self.0 / rhs.0)))
        } else {
            Err(unsupported("div", self, rhs))
        }
    }
}

impl Multiplier for Double {
    fn mul<'b, 'v>(&'b self, rhs: &(dyn Val + 'v)) -> Result<CowVal<'b, 'v>, ExecutionError>
    where
        Self: 'v,
    {
        if let Some(rhs) = rhs.downcast_ref::<Double>() {
            Ok(CowVal::owned(Double(self.0 * rhs.0)))
        } else {
            Err(unsupported("mul", self, rhs))
        }
    }
}

impl Negator for Double {
    fn negate<'v>(&self) -> Result<Box<dyn Val + 'v>, ExecutionError>
    where
        Self: 'v,
    {
        Ok(Box::new(Double(-self.0)))
    }
}

impl Subtractor for Double {
    fn sub<'b, 'v>(&'b self, rhs: &(dyn Val + 'v)) -> Result<CowVal<'b, 'v>, ExecutionError>
    where
        Self: 'v,
    {
        if let Some(rhs) = rhs.downcast_ref::<Double>() {
            Ok(CowVal::owned(Double(self.0 - rhs.0)))
        } else {
            Err(unsupported("sub", self, rhs))
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

impl<'v> TryFrom<Box<dyn Val + 'v>> for f64 {
    type Error = Box<dyn Val + 'v>;

    fn try_from(value: Box<dyn Val + 'v>) -> Result<Self, Self::Error> {
        if let Some(d) = value.downcast_ref::<Double>() {
            return Ok(d.0);
        }
        Err(value)
    }
}

impl<'a, 'v> TryFrom<&'a (dyn Val + 'v)> for &'a f64 {
    type Error = &'a (dyn Val + 'v);

    fn try_from(value: &'a (dyn Val + 'v)) -> Result<Self, Self::Error> {
        if let Some(d) = value.downcast_ref::<Double>() {
            return Ok(&d.0);
        }
        Err(value)
    }
}

fn double<'b, 'v>(mut args: Vec<CowVal<'b, 'v>>) -> Result<CowVal<'b, 'v>, ExecutionError> {
    let arg = args.remove(0);
    if arg.downcast_ref::<Double>().is_some() {
        return Ok(arg);
    }
    let converted: Option<f64> =
        match arg.get_type().kind() {
            Kind::Int => arg.downcast_ref::<CelInt>().map(|i| *i.inner() as f64),
            Kind::UInt => arg.downcast_ref::<CelUInt>().map(|u| *u.inner() as f64),
            Kind::String => {
                match arg.downcast_ref::<CelString>() {
                    None => None,
                    Some(s) => Some(s.inner().parse::<f64>().map_err(|e| {
                        ExecutionError::FunctionError {
                            function: "double".to_owned(),
                            message: format!("string parse error: {e}"),
                        }
                    })?),
                }
            }
            _ => None,
        };

    match converted {
        Some(value) => Ok(CowVal::owned(Double::from(value))),
        None => Err(ExecutionError::FunctionError {
            function: "double".to_owned(),
            message: format!("cannot convert {:?} to double", arg.as_ref()),
        }),
    }
}

pub(crate) fn stdlib(env: &mut crate::Env) {
    env.add_overload(
        "double",
        "double_to_double",
        vec![super::DOUBLE_TYPE],
        double,
    )
    .expect("Must be unique id");
    env.add_overload("double", "int64_to_double", vec![super::INT_TYPE], double)
        .expect("Must be unique id");
    env.add_overload("double", "uint64_to_double", vec![super::UINT_TYPE], double)
        .expect("Must be unique id");
    env.add_overload(
        "double",
        "string_to_double",
        vec![super::STRING_TYPE],
        double,
    )
    .expect("Must be unique id");
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
