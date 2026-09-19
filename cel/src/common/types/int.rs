use crate::common::traits::Negator;
use crate::common::traits::{self, Comparer};
use crate::common::types::{CelDouble, CelString, CelUInt, Kind, Type};
use crate::common::value::{CowVal, StaticVal, Val};
use crate::ExecutionError;
use std::any::Any;
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
    fn get_type(&self) -> &Type {
        &super::INT_TYPE
    }

    fn as_adder<'b, 'v>(&'b self) -> Option<&'b (dyn traits::Adder + 'v)>
    where
        Self: 'v,
    {
        Some(self)
    }

    fn as_comparer(&self) -> Option<&dyn traits::Comparer> {
        Some(self)
    }

    fn as_divider<'b, 'v>(&'b self) -> Option<&'b (dyn traits::Divider + 'v)>
    where
        Self: 'v,
    {
        Some(self)
    }

    fn as_modder<'b, 'v>(&'b self) -> Option<&'b (dyn traits::Modder + 'v)>
    where
        Self: 'v,
    {
        Some(self)
    }

    fn as_multiplier<'b, 'v>(&'b self) -> Option<&'b (dyn traits::Multiplier + 'v)>
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

    fn as_subtractor<'b, 'v>(&'b self) -> Option<&'b (dyn traits::Subtractor + 'v)>
    where
        Self: 'v,
    {
        Some(self)
    }

    fn as_zeroer(&self) -> Option<&dyn traits::Zeroer> {
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

impl StaticVal for Int {}

impl traits::Adder for Int {
    fn add<'b, 'v>(&'b self, other: &(dyn Val + 'v)) -> Result<CowVal<'b, 'v>, ExecutionError>
    where
        Self: 'v,
    {
        if let Some(i) = other.downcast_ref::<Int>() {
            let t: Self = self
                .0
                .checked_add(i.0)
                .ok_or_else(|| ExecutionError::Overflow("add", self.0.into(), i.0.into()))?
                .into();
            Ok(CowVal::owned(t))
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
    fn div<'b, 'v>(&'b self, rhs: &(dyn Val + 'v)) -> Result<CowVal<'b, 'v>, ExecutionError>
    where
        Self: 'v,
    {
        if let Some(i) = rhs.downcast_ref::<Int>() {
            if i.0 == 0 {
                return Err(ExecutionError::DivisionByZero(self.0.into()));
            }
            let t: Self = (self
                .0
                .checked_div(i.0)
                .ok_or_else(|| ExecutionError::Overflow("div", self.0.into(), i.0.into()))?)
            .into();
            Ok(CowVal::owned(t))
        } else {
            Err(ExecutionError::NoSuchOverload)
        }
    }
}

impl traits::Modder for Int {
    fn modulo<'b, 'v>(&'b self, rhs: &(dyn Val + 'v)) -> Result<CowVal<'b, 'v>, ExecutionError>
    where
        Self: 'v,
    {
        if let Some(i) = rhs.downcast_ref::<Int>() {
            if i.0 == 0 {
                return Err(ExecutionError::RemainderByZero(self.0.into()));
            }
            let t: Self = (self
                .0
                .checked_rem(i.0)
                .ok_or_else(|| ExecutionError::Overflow("rem", self.0.into(), i.0.into()))?)
            .into();
            Ok(CowVal::owned(t))
        } else {
            Err(ExecutionError::NoSuchOverload)
        }
    }
}

impl traits::Multiplier for Int {
    fn mul<'b, 'v>(&'b self, rhs: &(dyn Val + 'v)) -> Result<CowVal<'b, 'v>, ExecutionError>
    where
        Self: 'v,
    {
        if let Some(i) = rhs.downcast_ref::<Int>() {
            let t: Self = (self
                .0
                .checked_mul(i.0)
                .ok_or_else(|| ExecutionError::Overflow("mul", self.0.into(), i.0.into()))?)
            .into();
            Ok(CowVal::owned(t))
        } else {
            Err(ExecutionError::NoSuchOverload)
        }
    }
}

impl Negator for Int {
    fn negate<'v>(&self) -> Result<Box<dyn Val + 'v>, ExecutionError>
    where
        Self: 'v,
    {
        Ok(Box::new(Self::from(self.0.neg())))
    }
}

impl traits::Subtractor for Int {
    fn sub<'b, 'v>(&'b self, rhs: &(dyn Val + 'v)) -> Result<CowVal<'b, 'v>, ExecutionError>
    where
        Self: 'v,
    {
        if let Some(i) = rhs.downcast_ref::<Int>() {
            Ok(CowVal::owned(Self::from(
                self.0
                    .checked_sub(i.0)
                    .ok_or_else(|| ExecutionError::Overflow("sub", self.0.into(), i.0.into()))?,
            )))
        } else {
            Err(ExecutionError::NoSuchOverload)
        }
    }
}

impl traits::Zeroer for Int {
    fn is_zero_value(&self) -> bool {
        self.0 == 0
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

impl<'v> TryFrom<Box<dyn Val + 'v>> for i64 {
    type Error = Box<dyn Val + 'v>;

    fn try_from(value: Box<dyn Val + 'v>) -> Result<Self, Self::Error> {
        if let Some(i) = value.downcast_ref::<Int>() {
            return Ok(i.0);
        }
        Err(value)
    }
}

impl<'a, 'v> TryFrom<&'a (dyn Val + 'v)> for &'a i64 {
    type Error = &'a (dyn Val + 'v);

    fn try_from(value: &'a (dyn Val + 'v)) -> Result<Self, Self::Error> {
        if let Some(i) = value.downcast_ref::<Int>() {
            return Ok(&i.0);
        }
        Err(value)
    }
}

fn int<'b, 'v>(mut args: Vec<CowVal<'b, 'v>>) -> Result<CowVal<'b, 'v>, ExecutionError> {
    let arg = args.remove(0);
    if arg.downcast_ref::<Int>().is_some() {
        return Ok(arg);
    }
    let overflow = || ExecutionError::FunctionError {
        function: "int".to_owned(),
        message: "integer overflow".to_owned(),
    };
    let converted: Option<i64> =
        match arg.get_type().kind() {
            Kind::UInt => match arg.downcast_ref::<CelUInt>() {
                None => None,
                Some(u) => Some(i64::try_from(*u.inner()).map_err(|_| overflow())?),
            },
            Kind::Double => match arg.downcast_ref::<CelDouble>() {
                None => None,
                Some(d) => {
                    let value = *d.inner();
                    // Double to int conversions are limited to (minInt, maxInt) non-inclusive.
                    // 'i64::MAX as f64' rounds up to 2^63, and the largest double below that
                    // is 2^63 - 2^10, so the check also keeps 'value as i64' from saturating.
                    // 'i64::MIN as f64' is exactly -(2^63), so the exclusive lower bound
                    // rejects a double that i64 could actually hold. NaN, -infinity and
                    // infinity will also be rejected.
                    if !(value > (i64::MIN as f64) && value < (i64::MAX as f64)) {
                        return Err(overflow());
                    }
                    Some(value as i64)
                }
            },
            Kind::String => {
                match arg.downcast_ref::<CelString>() {
                    None => None,
                    Some(s) => Some(s.inner().parse::<i64>().map_err(|e| {
                        ExecutionError::FunctionError {
                            function: "int".to_owned(),
                            message: format!("string parse error: {e}"),
                        }
                    })?),
                }
            }
            _ => None,
        };

    match converted {
        Some(value) => Ok(CowVal::owned(Int::from(value))),
        None => Err(ExecutionError::FunctionError {
            function: "int".to_owned(),
            message: format!("cannot convert {:?} to int", arg.as_ref()),
        }),
    }
}

pub(crate) fn stdlib(env: &mut crate::Env) {
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
    use crate::{Context, Program};
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

    #[test]
    fn test_conversion_boundaries() {
        let context = Context::default();

        // int(double) -> int
        // Accepted doubles are those in (-2^63, 2^63) exclusive. The upper bound
        // is 2^63 rather than i64::MAX because f64 cannot hold i64::MAX.
        // The largest double below 2^63 is:
        // 2^63 - 2^10 == 9223372036854774784
        let program = Program::compile("int(9223372036854774784.0)").unwrap();
        let value = program.execute(&context).unwrap();
        assert_eq!(value, 9223372036854774784i64.into());

        // int(double) -> int
        // The smallest double above -2^63 is:
        // -(2^63) + 2^10 == -9223372036854774784
        let program = Program::compile("int(-9223372036854774784.0)").unwrap();
        let value = program.execute(&context).unwrap();
        assert_eq!(value, (-9223372036854774784i64).into());

        // int(uint) -> int
        // i64::MAX == (2^63 - 1) is the largest uint that still fits in an int
        let program = Program::compile("int(9223372036854775807u)").unwrap();
        let value = program.execute(&context).unwrap();
        assert_eq!(value, 9223372036854775807i64.into());
    }

    #[test]
    fn test_conversion_errors() {
        let context = Context::default();

        // int(double) -> int
        // -2^63 is exactly representable as f64 and equals i64::MIN, but the
        // lower bound is exclusive, so it should not convert:
        // -(2^63) == -9223372036854775808
        let program = Program::compile("int(-9223372036854775808.0)").unwrap();
        let result = program.execute(&context);
        assert!(
            result.is_err(),
            "int(-9223372036854775808.0) should return error, got {result:?}"
        );

        // int(double) -> int
        // i64::MAX == 2^63 - 1 == 9223372036854775807 cannot be held by f64,
        // so this literal rounds up to 2^63, which is outside the accepted range.
        let program = Program::compile("int(9223372036854775807.0)").unwrap();
        let result = program.execute(&context);
        assert!(
            result.is_err(),
            "int(9223372036854775807.0) should return error, got {result:?}"
        );

        // int(double) -> int
        let program = Program::compile("int(double('NaN'))").unwrap();
        let result = program.execute(&context);
        assert!(
            result.is_err(),
            "int(double('NaN')) should return error, got {result:?}"
        );

        // int(double) -> int
        let program = Program::compile("int(double('infinity'))").unwrap();
        let result = program.execute(&context);
        assert!(
            result.is_err(),
            "int(double('infinity')) should return error, got {result:?}"
        );

        // int(double) -> int
        let program = Program::compile("int(double('-infinity'))").unwrap();
        let result = program.execute(&context);
        assert!(
            result.is_err(),
            "int(double('-infinity')) should return error, got {result:?}"
        );

        // int(uint) -> int
        // One above the largest uint that fits in an int:
        // (i64::MAX + 1) == 2^63 == 9223372036854775808
        let program = Program::compile("int(9223372036854775808u)").unwrap();
        let result = program.execute(&context);
        assert!(
            result.is_err(),
            "int(9223372036854775808u) should return error, got {result:?}"
        );
    }
}
