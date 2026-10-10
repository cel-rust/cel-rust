//! The bindings extension library: the `cel.bind` macro, following cel-go's
//! `ext.Bindings()`.

use crate::common::ast::{Expr, IdedExpr};
use crate::parser::{Macro, MacroExprHelper, ParseError};
use crate::{DeclarationError, Env};
use std::mem;

/// Registers the bindings extension's `cel.bind` macro on `env`.
///
/// `cel.bind(var, init, expr)` evaluates `expr` with `var` bound to `init`,
/// which is evaluated once. It is only expanded by [`Env::compile`] and
/// [`Env::parser`], not by [`Program::compile`](crate::Program::compile).
pub fn extension(env: &mut Env) -> Result<(), DeclarationError> {
    env.add_macro(Macro::receiver("bind", 3, bind))
}

/// `cel.bind(var, init, expr)`: `var` bound to `init` in `expr`. Declines any
/// target but `cel`.
fn bind(
    helper: &mut MacroExprHelper<'_>,
    target: &mut Option<IdedExpr>,
    args: &mut Vec<IdedExpr>,
) -> Result<Option<IdedExpr>, ParseError> {
    let on_cel = |t: &IdedExpr| matches!(&t.expr, Expr::Ident(namespace) if namespace == "cel");
    if !target.as_ref().is_some_and(on_cel) {
        return Ok(None);
    }
    let [var, init, expr] =
        <[IdedExpr; 3]>::try_from(mem::take(args)).expect("the macro matched three arguments");
    let Expr::Ident(var) = var.expr else {
        return Err(helper.new_error(
            var.id,
            "cel.bind() variable names must be simple identifiers",
        ));
    };
    Ok(Some(helper.bind(&var, init, expr)))
}

#[cfg(test)]
mod tests {
    use crate::common::ast::Expr;
    use crate::{Context, DeclarationError, Env, Value};
    use std::sync::Arc;

    fn env() -> Env {
        let mut env = Env::stdlib();
        env.add_extension(crate::extensions::bindings)
            .expect("We can't test the extension, if we can't register it");
        env
    }

    fn eval(expr: &str) -> Value {
        let env = env();
        let program = env.compile(expr).expect("This must be valid CEL");
        let mut context = Context::with_env(Arc::new(env));
        context.add_variable_from_value("x", Value::Int(-1));
        program.execute(&context).expect("This must evaluate")
    }

    #[test]
    fn registering_twice_is_an_error() {
        let mut env = env();
        assert_eq!(
            env.add_extension(crate::extensions::bindings),
            Err(DeclarationError::duplicate_macro("bind"))
        );
    }

    #[test]
    fn bind_evaluates_the_expression_with_the_variable_bound() {
        assert_eq!(eval("cel.bind(a, 1, a + 1)"), Value::Int(2));
        assert_eq!(
            eval("cel.bind(a, [1, 2], a.map(e, e * 2))"),
            Value::List(Arc::new(vec![Value::Int(2), Value::Int(4)]))
        );
    }

    #[test]
    fn binds_nest() {
        assert_eq!(
            eval("cel.bind(a, 1, cel.bind(b, a + 1, a + b))"),
            Value::Int(3)
        );
        assert_eq!(eval("cel.bind(a, 1, cel.bind(a, a + 1, a))"), Value::Int(2));
    }

    #[test]
    fn a_bound_variable_shadows_one_of_the_context() {
        assert_eq!(eval("cel.bind(x, 0, x == 0)"), Value::Bool(true));
        assert_eq!(eval("cel.bind(a, 0, x)"), Value::Int(-1));
    }

    #[test]
    fn the_variable_must_be_a_simple_identifier() {
        for expr in ["cel.bind(a.b, 1, 2)", "cel.bind(1, 1, 2)"] {
            let errors = env().compile(expr).unwrap_err().errors;
            let messages: Vec<_> = errors.iter().map(|e| e.msg.as_str()).collect();
            assert_eq!(
                messages,
                ["cel.bind() variable names must be simple identifiers"],
                "{expr}"
            );
        }
    }

    #[test]
    fn bind_is_only_a_macro_on_cel() {
        for expr in ["m.bind(a, 1, a)", "bind(a, 1, a)", "cel.x.bind(a, 1, a)"] {
            let parsed = env().parser().parse(expr).unwrap();
            assert!(
                matches!(&parsed.expr, Expr::Call(call) if call.func_name == "bind"),
                "{expr}: {parsed:?}"
            );
        }
    }
}
