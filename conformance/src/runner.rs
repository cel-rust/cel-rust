//! Conformance test runner.
//!
//! This is a simplified version that works with the current cel crate.
//! Type checking is not available, and Struct/Enum types are not supported.
//! Many tests will fail due to missing features.

use cel::common::ast::{CallExpr, Expr, LiteralValue};
use cel::context::Context;
use cel::objects::Value as CelValue;
use cel::parser::{Macro, MacroExprHelper, ParseError};
use cel::{Env, IdedExpr, Value};

use crate::proto::cel::expr::conformance::test::{simple_test::ResultMatcher, SimpleTest};
use crate::textproto::parse_textproto_to_prost;
use crate::value_converter::proto_value_to_cel_value;

pub fn run_test(simple_test_textproto: &str) {
    let test: SimpleTest = parse_textproto_to_prost(
        simple_test_textproto,
        "cel.expr.conformance.test.SimpleTest",
    )
    .expect("Failed to parse SimpleTest textproto");

    // For check_only tests, fail since type checking is not available.
    assert!(
        !test.check_only,
        "Type checking not available (check_only test)"
    );

    let mut env = Env::stdlib();
    env.set_container(&test.container)
        .expect("Invalid container name in conformance test");
    env.add_extension(cel::extensions::bindings)
        .expect("We need that extension to register");
    env.add_extension(cel::extensions::strings)
        .expect("We need that extension to register");
    env.add_extension(cel::extensions::encoders)
        .expect("We need that extension to register");
    env.add_extension(cel::extensions::lists)
        .expect("We need that extension to register");
    env.add_extension(cel::extensions::math)
        .expect("We need that extension to register");
    env.add_extension(cel::extensions::network)
        .expect("We need that extension to register");
    for m in block_test_macros() {
        env.add_macro(m)
            .expect("We need the test macros to register");
    }

    // Parse with the env's parser, so the extensions' macros are expanded,
    // with optional syntax (`.?`, `[?…]`, `{?k: v}`) and backtick-escaped
    // identifiers (`` `foo.bar` ``) enabled. Matches cel-go's own conformance
    // runner (`conformance/conformance_test.go:87,95`).
    let program = env
        .parser()
        .enable_optional_syntax(true)
        .enable_ident_escape_syntax(true)
        .parse(&test.expr)
        .expect("Failed to compile CEL expression");

    // Build context with bindings.
    let mut context = Context::with_env(std::sync::Arc::new(env));

    if !test.bindings.is_empty() {
        for (key, expr_value) in &test.bindings {
            // Extract Value from ExprValue.
            let expr_kind = expr_value.kind.as_ref().expect("Binding kind is missing");
            assert!(
                matches!(
                    expr_kind,
                    crate::proto::cel::expr::expr_value::Kind::Value(_)
                ),
                "Binding '{}' is not a value (error/unknown)",
                key
            );
            let proto_value = match expr_kind {
                crate::proto::cel::expr::expr_value::Kind::Value(v) => v,
                _ => unreachable!(),
            };

            let cel_value =
                proto_value_to_cel_value(proto_value).expect("Failed to convert binding value");
            context.add_variable(key, cel_value);
        }
    }

    let result = Value::resolve(&program, &context);

    // Check the result against the expected result.
    match &test.result_matcher {
        Some(ResultMatcher::Value(expected_value)) => {
            let expected_cel_value =
                proto_value_to_cel_value(expected_value).expect("Failed to convert expected value");
            let actual_value = result.expect("Execution failed for value result matcher");
            assert!(
                values_equal(&actual_value, &expected_cel_value),
                "Expected {:?}, got {:?}",
                expected_cel_value,
                actual_value
            );
        }
        Some(ResultMatcher::EvalError(_)) => {
            // Test expects an error.
            assert!(result.is_err(), "Expected error but got success");
        }
        Some(ResultMatcher::Unknown(_)) => {
            todo!("Unknown result matching not implemented");
        }
        Some(ResultMatcher::AnyEvalErrors(_)) => {
            todo!("Any eval errors matching not implemented");
        }
        Some(ResultMatcher::AnyUnknowns(_)) => {
            todo!("Any unknowns matching not implemented");
        }
        Some(ResultMatcher::TypedResult(_typed_result)) => {
            // TypedResult requires type checking which is not available.
            todo!("TypedResult matching requires type checker (not available)");
        }
        None => {
            // Default to expecting true.
            let value = result.expect("Execution failed for default (true) matcher");
            assert!(
                matches!(value, CelValue::Bool(true)),
                "Expected true, got {:?}",
                value
            );
        }
    }
}

fn values_equal(a: &CelValue, b: &CelValue) -> bool {
    use CelValue::*;
    match (a, b) {
        (Null, Null) => true,
        (Bool(a), Bool(b)) => a == b,
        (Int(a), Int(b)) => a == b,
        (UInt(a), UInt(b)) => a == b,
        (Float(a), Float(b)) => {
            // Handle NaN specially
            if a.is_nan() && b.is_nan() {
                true
            } else {
                a == b
            }
        }
        (String(a), String(b)) => a == b,
        (Bytes(a), Bytes(b)) => a == b,
        (List(a), List(b)) => {
            if a.len() != b.len() {
                return false;
            }
            a.iter().zip(b.iter()).all(|(a, b)| values_equal(a, b))
        }
        (Map(a), Map(b)) => {
            if a.map.len() != b.map.len() {
                return false;
            }
            for (key, a_val) in a.map.iter() {
                match b.map.get(key) {
                    Some(b_val) => {
                        if !values_equal(a_val, b_val) {
                            return false;
                        }
                    }
                    None => return false,
                }
            }
            true
        }
        (Timestamp(a), Timestamp(b)) => a == b,
        (Duration(a), Duration(b)) => a == b,
        _ => false,
    }
}

/// The test-only `cel.block`, `cel.index`, `cel.iterVar` and `cel.accuVar`
/// macros of `block_ext`, as in cel-go's conformance runner
/// (`conformance/conformance_test.go`): they spell the internal `cel.@block`
/// call and the `@indexN`, `@it:N:M` and `@ac:N:M` names it binds.
fn block_test_macros() -> Vec<Macro> {
    vec![
        Macro::receiver("block", 2, |helper, target, args| {
            if !on_cel(target) {
                return Ok(None);
            }
            if !matches!(args[0].expr, Expr::List(_)) {
                return Err(helper.new_error(
                    args[0].id,
                    "cel.block requires the first arg to be a list literal",
                ));
            }
            let args = std::mem::take(args);
            Ok(Some(helper.next_expr(Expr::Call(CallExpr {
                func_name: "cel.@block".to_owned(),
                target: None,
                args,
            }))))
        }),
        Macro::receiver("index", 1, |helper, target, args| {
            if !on_cel(target) {
                return Ok(None);
            }
            let n = int_arg(&args[0], helper)?;
            Ok(Some(helper.next_expr(Expr::Ident(format!("@index{n}")))))
        }),
        Macro::receiver("iterVar", 2, |helper, target, args| {
            if !on_cel(target) {
                return Ok(None);
            }
            let (n, m) = (int_arg(&args[0], helper)?, int_arg(&args[1], helper)?);
            Ok(Some(helper.next_expr(Expr::Ident(format!("@it:{n}:{m}")))))
        }),
        Macro::receiver("accuVar", 2, |helper, target, args| {
            if !on_cel(target) {
                return Ok(None);
            }
            let (n, m) = (int_arg(&args[0], helper)?, int_arg(&args[1], helper)?);
            Ok(Some(helper.next_expr(Expr::Ident(format!("@ac:{n}:{m}")))))
        }),
    ]
}

fn on_cel(target: &Option<IdedExpr>) -> bool {
    matches!(target.as_ref().map(|t| &t.expr), Some(Expr::Ident(namespace)) if namespace == "cel")
}

fn int_arg(arg: &IdedExpr, helper: &MacroExprHelper<'_>) -> Result<i64, ParseError> {
    match &arg.expr {
        Expr::Literal(LiteralValue::Int(n)) if *n.inner() >= 0 => Ok(*n.inner()),
        _ => Err(helper.new_error(arg.id, "expected a non-negative int literal")),
    }
}
