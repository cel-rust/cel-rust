use std::sync::Arc;

use cel::common::ast::Expr;
use cel::{Context, Env, Value};

fn eval(source: &str) -> Result<Value, String> {
    let env = Env::stdlib();
    let expr = env
        .parser()
        .enable_optional_syntax(true)
        .parse(source)
        .map_err(|e| e.to_string())?;
    let context = Context::with_env(Arc::new(env));
    Value::resolve(&expr, &context).map_err(|e| e.to_string())
}

#[test]
fn opt_map_and_opt_flat_map_map_the_value_of_an_optional() {
    for (source, want) in [
        ("optional.of(42).optMap(y, y + 1).value()", Value::Int(43)),
        (
            "optional.none().optMap(y, y + 1).hasValue()",
            Value::Bool(false),
        ),
        ("optional.of(0).optMap(y, y).hasValue()", Value::Bool(true)),
        (
            "{'k': {'s': 'v'}}.?k.optFlatMap(k, k.?s).value()",
            Value::from("v"),
        ),
        (
            "{'k': {}}.?k.optFlatMap(k, k.?s).hasValue()",
            Value::Bool(false),
        ),
        ("{}.?k.optFlatMap(k, k.?s).hasValue()", Value::Bool(false)),
        (
            "optional.of(1).optFlatMap(x, optional.ofNonZeroValue(x - 1)).hasValue()",
            Value::Bool(false),
        ),
    ] {
        assert_eq!(eval(source), Ok(want), "{source}");
    }
}

#[test]
fn opt_map_binds_a_variable_named_as_a_namespace() {
    let source = "optional.of(3).optMap(optional, optional * 2).value()";
    assert_eq!(eval(source), Ok(Value::Int(6)));
}

#[test]
fn opt_map_needs_a_simple_name() {
    let err = eval("optional.of(1).optMap(1 + 1, 2)").unwrap_err();
    assert!(err.contains("argument must be a simple name"), "{err}");
}

#[test]
fn opt_map_is_left_as_written_without_the_optional_library() {
    let expr = Env::default().parser().parse("a.optMap(x, x)").unwrap();
    assert!(matches!(expr.expr, Expr::Call(call) if call.func_name == "optMap"));
}
