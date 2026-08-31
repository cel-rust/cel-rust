use crate::common::traits::{Adder, Comparer, Divider, Modder, Multiplier, Subtractor, Zeroer};
use crate::common::types::{CelDouble, CelInt, CelString, Kind, Type};
use crate::common::value::{Downcast, Val};
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
    fn get_type(&self) -> &Type {
        &super::UINT_TYPE
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

    fn as_zeroer(&self) -> Option<&dyn Zeroer> {
        Some(self)
    }

    fn equals(&self, other: &dyn Val) -> bool {
        self.compare(other)
            .map(|r| r == Ordering::Equal)
            .unwrap_or(false)
    }

    fn clone_as_boxed(&self) -> Box<dyn Val> {
        Box::new(UInt(self.0))
    }
}

impl Adder for UInt {
    fn add<'a>(&'a self, rhs: &dyn Val) -> Result<Cow<'a, dyn Val>, ExecutionError> {
        if let Some(rhs) = rhs.downcast_ref::<Self>() {
            Ok(Cow::<dyn Val>::Owned(Box::new(UInt(
                self.0.checked_add(rhs.0).ok_or_else(|| {
                    ExecutionError::Overflow(
                        "add",
                        (self as &dyn Val).try_into().unwrap_or(Value::Null),
                        (rhs as &dyn Val).try_into().unwrap_or(Value::Null),
                    )
                })?,
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
                self.0.checked_div(rhs.0).ok_or_else(|| {
                    ExecutionError::Overflow(
                        "div",
                        (self as &dyn Val).try_into().unwrap_or(Value::Null),
                        (rhs as &dyn Val).try_into().unwrap_or(Value::Null),
                    )
                })?,
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
                self.0.checked_rem(rhs.0).ok_or_else(|| {
                    ExecutionError::Overflow(
                        "rem",
                        (self as &dyn Val).try_into().unwrap_or(Value::Null),
                        (rhs as &dyn Val).try_into().unwrap_or(Value::Null),
                    )
                })?,
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
                self.0.checked_mul(rhs.0).ok_or_else(|| {
                    ExecutionError::Overflow(
                        "mul",
                        (self as &dyn Val).try_into().unwrap_or(Value::Null),
                        (rhs as &dyn Val).try_into().unwrap_or(Value::Null),
                    )
                })?,
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
                self.0.checked_sub(rhs.0).ok_or_else(|| {
                    ExecutionError::Overflow(
                        "sub",
                        (self as &dyn Val).try_into().unwrap_or(Value::Null),
                        (rhs as &dyn Val).try_into().unwrap_or(Value::Null),
                    )
                })?,
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

fn uint<'a>(args: Vec<Cow<'a, dyn Val>>) -> Result<Cow<'a, dyn Val>, ExecutionError> {
    let mut args = args;
    let arg = args.remove(0).into_owned();
    let ret: Result<Box<UInt>, Box<dyn Val>> = match arg.get_type().kind() {
        Kind::UInt => arg.downcast::<UInt>(),
        Kind::Int => match arg.downcast::<CelInt>() {
            Err(arg) => Err(arg),
            Ok(arg) => match u64::try_from(*arg.inner()) {
                Ok(value) => Ok(Box::new(UInt::from(value))),
                Err(_) => {
                    return Err(ExecutionError::FunctionError {
                        function: "uint".to_owned(),
                        message: "unsigned integer overflow".to_owned(),
                    });
                }
            },
        },
        Kind::Double => match arg.downcast::<CelDouble>() {
            Err(arg) => Err(arg),
            Ok(arg) => {
                let value = *arg.inner();
                // Double to uint conversions are limited to [0, maxUint).
                // 'u64::MAX as f64' rounds up to 2^64 and the largest double below that
                // is 2^64 - 2^11, so the check also keeps 'value as u64' from saturating.
                // NaN, -infinity and infinity will also be rejected.
                if !(value >= 0.0 && value < (u64::MAX as f64)) {
                    return Err(ExecutionError::FunctionError {
                        function: "uint".to_owned(),
                        message: "unsigned integer overflow".to_owned(),
                    });
                }

                Ok(Box::new(UInt::from(value as u64)))
            }
        },
        Kind::String => match arg.downcast::<CelString>() {
            Err(arg) => Err(arg),
            Ok(arg) => match arg.inner().parse::<u64>() {
                Ok(arg) => Ok(Box::new(UInt::from(arg))),
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
