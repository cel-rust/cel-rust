use crate::common::value::Val;
use crate::ExecutionError;
use std::borrow::Cow;

/// `dyn` has no effect at runtime, it only signals to a type checker that its argument
/// should be treated as dynamically typed. Returning the argument unchanged matches the
/// `identity` binding cel-go declares for the same overload.
fn to_dyn<'a>(mut args: Vec<Cow<'a, dyn Val>>) -> Result<Cow<'a, dyn Val>, ExecutionError> {
    Ok(args.remove(0))
}

pub(crate) fn stdlib(env: &mut crate::Env) {
    env.add_overload("dyn", "to_dyn", vec![super::DYN_TYPE], to_dyn)
        .expect("Must be unique id");
}

#[cfg(test)]
mod tests {
    use crate::{Context, Program};

    fn eval(expr: &str) -> crate::objects::ResolveResult {
        Program::compile(expr).unwrap().execute(&Context::default())
    }

    #[test]
    fn returns_the_argument_unchanged() {
        assert_eq!(eval("dyn(1)"), Ok(1.into()));
        assert_eq!(eval("dyn('hello')"), Ok("hello".into()));
        assert_eq!(eval("dyn([1, 2])"), Ok(vec![1, 2].into()));
    }

    #[test]
    fn allows_cross_type_numeric_comparison() {
        // The spec lists `dyn(3.0) == 3` as an equality example.
        assert_eq!(eval("dyn(3.0) == 3"), Ok(true.into()));
        assert_eq!(eval("dyn(1) == 1u"), Ok(true.into()));
        assert_eq!(eval("dyn(1) < 2u"), Ok(true.into()));
    }

    #[test]
    fn does_not_make_unrelated_types_equal() {
        assert_eq!(eval("dyn(1) == 'a'"), Ok(false.into()));
        assert_eq!(eval("dyn(1) == null"), Ok(false.into()));
    }
}
