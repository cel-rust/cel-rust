use crate::common::traits::{Adder, Comparer, Divider, Modder, Multiplier, Subtractor, Zeroer};
use crate::common::types::{CelDouble, CelInt, CelString, Kind, Type};
use crate::common::value::{CowVal, StaticVal, Val};
use crate::{ExecutionError, Value};
use std::any::Any;
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
    fn get_type(&self) -> &Type {
        &super::UINT_TYPE
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

    fn as_modder<'b, 'v>(&'b self) -> Option<&'b (dyn Modder + 'v)>
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

impl StaticVal for UInt {}

fn overflow(op: &'static str, lhs: &dyn Val, rhs: &dyn Val) -> ExecutionError {
    ExecutionError::Overflow(
        op,
        lhs.try_into().unwrap_or(Value::Null),
        rhs.try_into().unwrap_or(Value::Null),
    )
}

fn unsupported(op: &'static str, lhs: &dyn Val, rhs: &dyn Val) -> ExecutionError {
    ExecutionError::UnsupportedBinaryOperator(
        op,
        lhs.try_into().unwrap_or(Value::Null),
        rhs.try_into().unwrap_or(Value::Null),
    )
}

impl Adder for UInt {
    fn add<'b, 'v>(&'b self, rhs: &(dyn Val + 'v)) -> Result<CowVal<'b, 'v>, ExecutionError>
    where
        Self: 'v,
    {
        if let Some(rhs) = rhs.downcast_ref::<Self>() {
            Ok(CowVal::owned(UInt(
                self.0
                    .checked_add(rhs.0)
                    .ok_or_else(|| overflow("add", self, rhs))?,
            )))
        } else {
            Err(unsupported("add", self, rhs))
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
    fn div<'b, 'v>(&'b self, rhs: &(dyn Val + 'v)) -> Result<CowVal<'b, 'v>, ExecutionError>
    where
        Self: 'v,
    {
        if let Some(rhs) = rhs.downcast_ref::<Self>() {
            if rhs.0 == 0 {
                return Err(ExecutionError::DivisionByZero(self.0.into()));
            }
            Ok(CowVal::owned(UInt(
                self.0
                    .checked_div(rhs.0)
                    .ok_or_else(|| overflow("div", self, rhs))?,
            )))
        } else {
            Err(unsupported("div", self, rhs))
        }
    }
}

impl Modder for UInt {
    fn modulo<'b, 'v>(&'b self, rhs: &(dyn Val + 'v)) -> Result<CowVal<'b, 'v>, ExecutionError>
    where
        Self: 'v,
    {
        if let Some(rhs) = rhs.downcast_ref::<Self>() {
            if rhs.0 == 0 {
                return Err(ExecutionError::RemainderByZero(self.0.into()));
            }
            Ok(CowVal::owned(UInt(
                self.0
                    .checked_rem(rhs.0)
                    .ok_or_else(|| overflow("rem", self, rhs))?,
            )))
        } else {
            Err(unsupported("rem", self, rhs))
        }
    }
}

impl Multiplier for UInt {
    fn mul<'b, 'v>(&'b self, rhs: &(dyn Val + 'v)) -> Result<CowVal<'b, 'v>, ExecutionError>
    where
        Self: 'v,
    {
        if let Some(rhs) = rhs.downcast_ref::<Self>() {
            Ok(CowVal::owned(UInt(
                self.0
                    .checked_mul(rhs.0)
                    .ok_or_else(|| overflow("mul", self, rhs))?,
            )))
        } else {
            Err(unsupported("mul", self, rhs))
        }
    }
}

impl Subtractor for UInt {
    fn sub<'b, 'v>(&'b self, rhs: &(dyn Val + 'v)) -> Result<CowVal<'b, 'v>, ExecutionError>
    where
        Self: 'v,
    {
        if let Some(rhs) = rhs.downcast_ref::<Self>() {
            Ok(CowVal::owned(UInt(
                self.0
                    .checked_sub(rhs.0)
                    .ok_or_else(|| overflow("sub", self, rhs))?,
            )))
        } else {
            Err(unsupported("sub", self, rhs))
        }
    }
}

impl Zeroer for UInt {
    fn is_zero_value(&self) -> bool {
        self.0 == 0
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

impl<'v> TryFrom<Box<dyn Val + 'v>> for u64 {
    type Error = Box<dyn Val + 'v>;

    fn try_from(value: Box<dyn Val + 'v>) -> Result<Self, Self::Error> {
        if let Some(u) = value.downcast_ref::<UInt>() {
            return Ok(u.0);
        }
        Err(value)
    }
}

impl<'a, 'v> TryFrom<&'a (dyn Val + 'v)> for &'a u64 {
    type Error = &'a (dyn Val + 'v);
    fn try_from(value: &'a (dyn Val + 'v)) -> Result<Self, Self::Error> {
        if let Some(u) = value.downcast_ref::<UInt>() {
            return Ok(&u.0);
        }
        Err(value)
    }
}

fn uint<'b, 'v>(mut args: Vec<CowVal<'b, 'v>>) -> Result<CowVal<'b, 'v>, ExecutionError> {
    let arg = args.remove(0);
    if arg.downcast_ref::<UInt>().is_some() {
        return Ok(arg);
    }
    let overflow = || ExecutionError::FunctionError {
        function: "uint".to_owned(),
        message: "unsigned integer overflow".to_owned(),
    };
    let converted: Option<u64> =
        match arg.get_type().kind() {
            Kind::Int => match arg.downcast_ref::<CelInt>() {
                None => None,
                Some(i) => Some(u64::try_from(*i.inner()).map_err(|_| overflow())?),
            },
            Kind::Double => match arg.downcast_ref::<CelDouble>() {
                None => None,
                Some(d) => {
                    let value = *d.inner();
                    // Double to uint conversions are limited to [0, maxUint).
                    // 'u64::MAX as f64' rounds up to 2^64 and the largest double below that
                    // is 2^64 - 2^11, so the check also keeps 'value as u64' from saturating.
                    // NaN, -infinity and infinity will also be rejected.
                    if !(value >= 0.0 && value < (u64::MAX as f64)) {
                        return Err(overflow());
                    }
                    Some(value as u64)
                }
            },
            Kind::String => {
                match arg.downcast_ref::<CelString>() {
                    None => None,
                    Some(s) => Some(s.inner().parse::<u64>().map_err(|e| {
                        ExecutionError::FunctionError {
                            function: "uint".to_owned(),
                            message: format!("string parse error: {e}"),
                        }
                    })?),
                }
            }
            _ => None,
        };

    match converted {
        Some(value) => Ok(CowVal::owned(UInt::from(value))),
        None => Err(ExecutionError::FunctionError {
            function: "uint".to_owned(),
            message: format!("cannot convert {:?} to uint", arg.as_ref()),
        }),
    }
}

pub(crate) fn stdlib(env: &mut crate::Env) {
    env.add_overload("uint", "uint64_to_uint64", vec![super::UINT_TYPE], uint)
        .expect("Must be unique id");
    env.add_overload("uint", "int64_to_uint64", vec![super::INT_TYPE], uint)
        .expect("Must be unique id");
    env.add_overload("uint", "double_to_uint64", vec![super::DOUBLE_TYPE], uint)
        .expect("Must be unique id");
    env.add_overload("uint", "string_to_uint64", vec![super::STRING_TYPE], uint)
        .expect("Must be unique id");
}

#[cfg(test)]
mod tests {
    use crate::{
        common::{
            types::{CelDouble, CelInt, CelString, CelUInt},
            value::Val,
        },
        Context, Program,
    };

    #[test]
    fn test_equals() {
        let uint = CelUInt::from(42);
        assert!(uint.equals(&uint));
        assert!(uint.equals(&CelInt::from(42)));
        assert!(!uint.equals(&CelInt::from(-42)));
        assert!(uint.equals(&CelDouble::from(42.0)));
        assert!(!uint.equals(&CelDouble::from(42.2)));
        assert!(!uint.equals(&CelDouble::from(f64::NAN)));
        assert!(!uint.equals(&CelString::from("42")));
    }

    #[test]
    fn test_conversion_boundaries() {
        let context = Context::default();

        // uint(double) -> uint
        let program = Program::compile("uint(0.0)").unwrap();
        let value = program.execute(&context).unwrap();
        assert_eq!(value, 0u64.into());

        // uint(double) -> uint
        // For double upper boundary we cannot test u64::MAX (2^64 - 1) since f64
        // cannot hold that integer value. The closest integer value below is:
        // 2^64 - 2^11 == 18446744073709549568
        let program = Program::compile("uint(18446744073709549568.0)").unwrap();
        let value = program.execute(&context).unwrap();
        assert_eq!(value, 18446744073709549568u64.into());

        // uint(int) -> uint
        let program = Program::compile("uint(0)").unwrap();
        let value = program.execute(&context).unwrap();
        assert_eq!(value, 0u64.into());

        // uint(int) -> uint
        // i64::MAX == 2^63 - 1 == 9223372036854775807
        let program = Program::compile("uint(9223372036854775807)").unwrap();
        let value = program.execute(&context).unwrap();
        assert_eq!(value, 9223372036854775807u64.into());
    }

    #[test]
    fn test_conversion_errors() {
        let context = Context::default();

        let program = Program::compile("uint(-1)").unwrap();
        let result = program.execute(&context);
        assert!(
            result.is_err(),
            "uint(-1) should return error, got {result:?}"
        );

        let program = Program::compile("uint(-1.0)").unwrap();
        let result = program.execute(&context);
        assert!(
            result.is_err(),
            "uint(-1.0) should return error, got {result:?}"
        );

        // (u64::MAX + 1) == 2^64 == 18446744073709551616
        let program = Program::compile("uint(18446744073709551616.0)").unwrap();
        let result = program.execute(&context);
        assert!(
            result.is_err(),
            "uint(18446744073709551616.0) should return error, got {result:?}"
        );

        let program = Program::compile("uint(double('NaN'))").unwrap();
        let result = program.execute(&context);
        assert!(
            result.is_err(),
            "uint(double('NaN')) should return error, got {result:?}"
        );

        let program = Program::compile("uint(double('infinity'))").unwrap();
        let result = program.execute(&context);
        assert!(
            result.is_err(),
            "uint(double('infinity')) should return error, got {result:?}"
        );

        let program = Program::compile("uint(double('-infinity'))").unwrap();
        let result = program.execute(&context);
        assert!(
            result.is_err(),
            "uint(double('-infinity')) should return error, got {result:?}"
        );
    }
}
