//! Conformance test runner.
//!
//! This is a simplified version that works with the current cel crate.
//! Type checking is not available, and Struct/Enum types are not supported.
//! Many tests will fail due to missing features.

use cel::context::Context;
use cel::objects::Value as CelValue;
use cel::{Env, Program};

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

    let program = Program::compile(&test.expr).expect("Failed to compile CEL expression");

    // Build context with bindings.
    let env = Env::stdlib();
    let mut context = Context::with_env(std::sync::Arc::new(env));

    // Add container if specified (simplified - just note it).
    if !test.container.is_empty() {
        // Container support requires features not available in current cel crate.
        // Tests using containers will likely fail.
    }

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

    let result = program.execute(&context);

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
