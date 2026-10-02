//! The math extension library: functions in the `math` namespace, following
//! cel-go's `ext.Math()`.
//!
//! The `math.greatest` and `math.least` macros are not provided yet: they
//! take any number of arguments.

use crate::common::types::{CelBool, CelDouble, CelInt, CelUInt};
use crate::{DeclarationError, Env, ExecutionError};

/// Registers the math extension's overloads on `env`.
pub fn extension(env: &mut Env) -> Result<(), DeclarationError> {
    crate::add_overload!(env, fn ceil: (CelDouble) -> CelDouble, name = "math.ceil")?;
    crate::add_overload!(env, fn floor: (CelDouble) -> CelDouble, name = "math.floor")?;
    crate::add_overload!(env, fn round: (CelDouble) -> CelDouble, name = "math.round")?;
    crate::add_overload!(env, fn trunc: (CelDouble) -> CelDouble, name = "math.trunc")?;
    crate::add_overload!(env, fn is_inf: (CelDouble) -> CelBool, name = "math.isInf")?;
    crate::add_overload!(env, fn is_nan: (CelDouble) -> CelBool, name = "math.isNaN")?;
    crate::add_overload!(env, fn is_finite: (CelDouble) -> CelBool, name = "math.isFinite")?;
    crate::add_overload!(env, fn abs_double: (CelDouble) -> CelDouble, name = "math.abs")?;
    crate::add_overload!(env, fn abs_int: (CelInt) -> Result<CelInt>, name = "math.abs")?;
    crate::add_overload!(env, fn abs_uint: (CelUInt) -> CelUInt, name = "math.abs")?;
    crate::add_overload!(env, fn sign_double: (CelDouble) -> CelDouble, name = "math.sign")?;
    crate::add_overload!(env, fn sign_int: (CelInt) -> CelInt, name = "math.sign")?;
    crate::add_overload!(env, fn sign_uint: (CelUInt) -> CelUInt, name = "math.sign")?;
    crate::add_overload!(env, fn bit_and_int: (CelInt, CelInt) -> CelInt, name = "math.bitAnd")?;
    crate::add_overload!(env, fn bit_and_uint: (CelUInt, CelUInt) -> CelUInt, name = "math.bitAnd")?;
    crate::add_overload!(env, fn bit_or_int: (CelInt, CelInt) -> CelInt, name = "math.bitOr")?;
    crate::add_overload!(env, fn bit_or_uint: (CelUInt, CelUInt) -> CelUInt, name = "math.bitOr")?;
    crate::add_overload!(env, fn bit_xor_int: (CelInt, CelInt) -> CelInt, name = "math.bitXor")?;
    crate::add_overload!(env, fn bit_xor_uint: (CelUInt, CelUInt) -> CelUInt, name = "math.bitXor")?;
    crate::add_overload!(env, fn bit_not_int: (CelInt) -> CelInt, name = "math.bitNot")?;
    crate::add_overload!(env, fn bit_not_uint: (CelUInt) -> CelUInt, name = "math.bitNot")?;
    crate::add_overload!(env, fn bit_shift_left_int: (CelInt, CelInt) -> Result<CelInt>,
        name = "math.bitShiftLeft")?;
    crate::add_overload!(env, fn bit_shift_left_uint: (CelUInt, CelInt) -> Result<CelUInt>,
        name = "math.bitShiftLeft")?;
    crate::add_overload!(env, fn bit_shift_right_int: (CelInt, CelInt) -> Result<CelInt>,
        name = "math.bitShiftRight")?;
    crate::add_overload!(env, fn bit_shift_right_uint: (CelUInt, CelInt) -> Result<CelUInt>,
        name = "math.bitShiftRight")?;
    crate::add_overload!(env, fn sqrt_double: (CelDouble) -> CelDouble, name = "math.sqrt")?;
    crate::add_overload!(env, fn sqrt_int: (CelInt) -> CelDouble, name = "math.sqrt")?;
    crate::add_overload!(env, fn sqrt_uint: (CelUInt) -> CelDouble, name = "math.sqrt")?;
    Ok(())
}

fn ceil(x: &CelDouble) -> CelDouble {
    x.inner().ceil().into()
}

fn floor(x: &CelDouble) -> CelDouble {
    x.inner().floor().into()
}

/// Rounds half away from zero.
fn round(x: &CelDouble) -> CelDouble {
    x.inner().round().into()
}

fn trunc(x: &CelDouble) -> CelDouble {
    x.inner().trunc().into()
}

fn is_inf(x: &CelDouble) -> CelBool {
    x.inner().is_infinite().into()
}

fn is_nan(x: &CelDouble) -> CelBool {
    x.inner().is_nan().into()
}

fn is_finite(x: &CelDouble) -> CelBool {
    x.inner().is_finite().into()
}

fn abs_double(x: &CelDouble) -> CelDouble {
    x.inner().abs().into()
}

/// Fails for `i64::MIN`, whose absolute value is not an `int`.
fn abs_int(x: &CelInt) -> Result<CelInt, ExecutionError> {
    x.inner()
        .checked_abs()
        .map(CelInt::from)
        .ok_or_else(|| ExecutionError::function_error("math.abs", "integer overflow"))
}

fn abs_uint(x: &CelUInt) -> CelUInt {
    *x
}

/// `-1.0`, `0.0` or `1.0`, or NaN for NaN: both zeros are `0.0`, where
/// `f64::signum` would give them a sign.
fn sign_double(x: &CelDouble) -> CelDouble {
    let x = *x.inner();
    if x > 0.0 {
        1.0.into()
    } else if x < 0.0 {
        (-1.0).into()
    } else if x == 0.0 {
        0.0.into()
    } else {
        x.into()
    }
}

fn sign_int(x: &CelInt) -> CelInt {
    x.inner().signum().into()
}

fn sign_uint(x: &CelUInt) -> CelUInt {
    u64::from(*x.inner() != 0).into()
}

fn bit_and_int(l: &CelInt, r: &CelInt) -> CelInt {
    (l.inner() & r.inner()).into()
}

fn bit_and_uint(l: &CelUInt, r: &CelUInt) -> CelUInt {
    (l.inner() & r.inner()).into()
}

fn bit_or_int(l: &CelInt, r: &CelInt) -> CelInt {
    (l.inner() | r.inner()).into()
}

fn bit_or_uint(l: &CelUInt, r: &CelUInt) -> CelUInt {
    (l.inner() | r.inner()).into()
}

fn bit_xor_int(l: &CelInt, r: &CelInt) -> CelInt {
    (l.inner() ^ r.inner()).into()
}

fn bit_xor_uint(l: &CelUInt, r: &CelUInt) -> CelUInt {
    (l.inner() ^ r.inner()).into()
}

fn bit_not_int(x: &CelInt) -> CelInt {
    (!x.inner()).into()
}

fn bit_not_uint(x: &CelUInt) -> CelUInt {
    (!x.inner()).into()
}

/// The bits of a shift by `bits`, `None` when they shift all 64 bits out.
/// Fails when negative.
fn shift(function: &str, bits: &CelInt) -> Result<Option<u32>, ExecutionError> {
    let bits = *bits.inner();
    if bits < 0 {
        return Err(ExecutionError::function_error(
            function,
            format!("negative offset: {bits}"),
        ));
    }
    Ok(u32::try_from(bits).ok().filter(|&bits| bits < u64::BITS))
}

fn bit_shift_left_int(x: &CelInt, bits: &CelInt) -> Result<CelInt, ExecutionError> {
    let shifted = shift("math.bitShiftLeft", bits)?.map_or(0, |bits| x.inner() << bits);
    Ok(shifted.into())
}

fn bit_shift_left_uint(x: &CelUInt, bits: &CelInt) -> Result<CelUInt, ExecutionError> {
    let shifted = shift("math.bitShiftLeft", bits)?.map_or(0, |bits| x.inner() << bits);
    Ok(shifted.into())
}

/// Fills the vacated bits with zeros: the sign is not extended.
fn bit_shift_right_int(x: &CelInt, bits: &CelInt) -> Result<CelInt, ExecutionError> {
    let shifted = shift("math.bitShiftRight", bits)?.map_or(0, |bits| (*x.inner() as u64) >> bits);
    Ok((shifted as i64).into())
}

fn bit_shift_right_uint(x: &CelUInt, bits: &CelInt) -> Result<CelUInt, ExecutionError> {
    let shifted = shift("math.bitShiftRight", bits)?.map_or(0, |bits| x.inner() >> bits);
    Ok(shifted.into())
}

/// NaN for a negative `x`.
fn sqrt_double(x: &CelDouble) -> CelDouble {
    x.inner().sqrt().into()
}

fn sqrt_int(x: &CelInt) -> CelDouble {
    (*x.inner() as f64).sqrt().into()
}

fn sqrt_uint(x: &CelUInt) -> CelDouble {
    (*x.inner() as f64).sqrt().into()
}

#[cfg(test)]
mod tests {
    use crate::{Context, DeclarationError, Env, ExecutionError, Value};
    use std::sync::Arc;

    #[test]
    fn registering_twice_is_an_error() {
        let mut env = Env::stdlib();
        assert_eq!(env.add_extension(crate::extensions::math), Ok(()));
        assert_eq!(
            env.add_extension(crate::extensions::math),
            Err(DeclarationError::duplicate_overload(
                "math.ceil",
                "math.ceil(double)"
            ))
        );
    }

    fn eval(expr: &str) -> Result<Value, ExecutionError> {
        let mut env = Env::stdlib();
        env.add_extension(crate::extensions::math)
            .expect("We can't test the extension, if we can't register it");
        let program = env.compile(expr).expect("This must be valid CEL");
        program.execute(&Context::with_env(Arc::new(env)))
    }

    fn assert_eval(expr: &str, expected: impl Into<Value>) {
        assert_eq!(eval(expr), Ok(expected.into()), "{expr}");
    }

    fn assert_error(expr: &str, message: &str) {
        match eval(expr) {
            Err(ExecutionError::FunctionError { message: m, .. }) => {
                assert_eq!(m, message, "{expr}")
            }
            other => panic!("{expr}: expected a function error, got {other:?}"),
        }
    }

    fn assert_no_overload(expr: &str) {
        match eval(expr) {
            Err(ExecutionError::NoSuchOverload { .. }) => {}
            other => panic!("{expr}: expected no such overload, got {other:?}"),
        }
    }

    #[test]
    fn rounding() {
        assert_eval("math.ceil(-1.2)", -1.0);
        assert_eval("math.ceil(1.2)", 2.0);
        assert_eval("math.floor(-1.2)", -2.0);
        assert_eval("math.floor(1.2)", 1.0);
        assert_eval("math.round(-1.5)", -2.0);
        assert_eval("math.round(1.5)", 2.0);
        assert_eval("math.round(2.5)", 3.0);
        assert_eval("math.round(-1.4)", -1.0);
        assert_eval("math.trunc(-1.7)", -1.0);
        assert_eval("math.trunc(1.7)", 1.0);
        assert_eval("math.isNaN(math.round(0.0/0.0))", true);
        assert_no_overload("math.ceil(1)");
    }

    #[test]
    fn floating_point_helpers() {
        assert_eval("math.isNaN(0.0/0.0)", true);
        assert_eval("math.isNaN(1.0/0.0)", false);
        assert_eval("math.isInf(-1.0/0.0)", true);
        assert_eval("math.isInf(0.0/0.0)", false);
        assert_eval("math.isFinite(1.0/1.5)", true);
        assert_eval("math.isFinite(0.0/0.0)", false);
        assert_eval("math.isFinite(1.0/0.0)", false);
        assert_no_overload("math.isNaN(dyn(true))");
    }

    #[test]
    fn abs() {
        assert_eval("math.abs(1u)", 1u64);
        assert_eval("math.abs(-11)", 11);
        assert_eval("math.abs(9223372036854775807)", i64::MAX);
        assert_eval("math.abs(-11.5)", 11.5);
        assert_error("math.abs(-9223372036854775808)", "integer overflow");
    }

    #[test]
    fn sign() {
        assert_eval("math.sign(100u)", 1u64);
        assert_eval("math.sign(0u)", 0u64);
        assert_eval("math.sign(-11)", -1);
        assert_eval("math.sign(0)", 0);
        assert_eval("math.sign(100.5)", 1.0);
        assert_eval("math.sign(-32.0)", -1.0);
        assert_eval("math.sign(-0.0)", 0.0);
        assert_eval("math.isNaN(math.sign(0.0/0.0))", true);
        assert_no_overload("math.sign(dyn(true))");
    }

    #[test]
    fn bitwise() {
        assert_eval("math.bitAnd(1, -1)", 1);
        assert_eval("math.bitAnd(1u, 3u)", 1u64);
        assert_eval("math.bitOr(4, -2)", -2);
        assert_eval("math.bitOr(1u, 4u)", 5u64);
        assert_eval("math.bitXor(4, -2)", -6);
        assert_eval("math.bitXor(1u, 3u)", 2u64);
        assert_eval("math.bitNot(-1)", 0);
        assert_eval("math.bitNot(0u)", u64::MAX);
        assert_no_overload("math.bitAnd(1, 2u)");
    }

    #[test]
    fn bit_shifts() {
        assert_eval("math.bitShiftLeft(-1, 2)", -4);
        assert_eval("math.bitShiftLeft(1, 63)", i64::MIN);
        assert_eval("math.bitShiftLeft(1, 64)", 0);
        assert_eval("math.bitShiftLeft(1u, 2)", 4u64);
        assert_eval("math.bitShiftLeft(1u, 200)", 0u64);
        assert_eval("math.bitShiftRight(-1024, 3)", 2305843009213693824i64);
        assert_eval("math.bitShiftRight(-1024, 64)", 0);
        assert_eval("math.bitShiftRight(1024u, 2)", 256u64);
        assert_eval("math.bitShiftRight(1024u, 9223372036854775807)", 0u64);
        assert_error("math.bitShiftLeft(1u, -1)", "negative offset: -1");
        assert_error("math.bitShiftRight(1, -2)", "negative offset: -2");
        assert_no_overload("math.bitShiftLeft(1, 2u)");
    }

    #[test]
    fn sqrt() {
        assert_eval("math.sqrt(81)", 9.0);
        assert_eval("math.sqrt(81u)", 9.0);
        assert_eval("math.sqrt(985.25)", 31.388692231439016);
        assert_eval("math.isNaN(math.sqrt(-15))", true);
    }

    #[test]
    fn a_variable_named_math_does_not_shadow_the_namespace() {
        let mut env = Env::stdlib();
        env.add_extension(crate::extensions::math).unwrap();
        let program = env.compile("math.abs(-1)").unwrap();
        let mut context = Context::with_env(Arc::new(env));
        context.add_variable_from_value("math", Value::Int(0));
        assert_eq!(program.execute(&context), Ok(Value::Int(1)));
    }
}
