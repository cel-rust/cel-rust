use crate::common::decls::Signature;
use crate::common::value::CowVal;
use crate::ExecutionError;

/// `dyn` has no effect at runtime, it only signals to a type checker that its argument
/// should be treated as dynamically typed. Returning the argument unchanged matches the
/// `identity` binding cel-go declares for the same overload.
fn to_dyn<'b, 'v>(mut args: Vec<CowVal<'b, 'v>>) -> Result<CowVal<'b, 'v>, ExecutionError> {
    Ok(args.remove(0))
}

pub(crate) fn stdlib(env: &mut crate::Env) {
    // As the spec's standard definitions declare it, for containers too. The
    // spec's prose on gradual type checking does have a list be `list(dyn)`, but
    // that can't be declared: it would be up to a type checker.
    let signature = Signature::new(vec![super::PARAM_A]).returns(super::DYN_TYPE);
    env.add_overload("dyn", "to_dyn", signature, to_dyn)
        .expect("Must be unique id");
}

#[cfg(test)]
mod tests {
    use crate::common::types::{DYN_TYPE, PARAM_A};
    use crate::{Context, Env, Program};

    fn eval(expr: &str) -> crate::objects::ResolveResult {
        Program::compile(expr).unwrap().execute(&Context::default())
    }

    /// Spec, Standard Definitions: `dyn(A) -> dyn`.
    #[test]
    fn is_declared_as_in_the_spec() {
        let env = Env::stdlib();
        let mut overloads = env.function("dyn").unwrap().overloads();
        assert_eq!(overloads.len(), 1);
        let overload = overloads.next().unwrap();
        assert!(!overload.is_member_function());
        assert_eq!(overload.arg_types().collect::<Vec<_>>(), [&PARAM_A]);
        assert_eq!(overload.result_type(), &DYN_TYPE);
    }

    #[test]
    fn accepts_a_value_of_any_type() {
        for expr in [
            "dyn(null)",
            "dyn(1u)",
            "dyn(1.5)",
            "dyn(true)",
            "dyn(b'a')",
            "dyn([])",
            "dyn({})",
            "dyn(int)",
            "dyn(optional.none())",
            "dyn(dyn(1))",
        ] {
            assert!(eval(expr).is_ok(), "{expr}: {:?}", eval(expr));
        }
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
