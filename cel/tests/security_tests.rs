//! Security-focused tests for CEL-rust library
//!
//! These tests verify that stack overflow vulnerabilities are properly handled
//! by returning errors instead of crashing the process.

use cel::Program;

// =============================================================================
// STACK OVERFLOW PREVENTION TESTS
// =============================================================================

/// Deeply nested parentheses (95 levels) should be rejected
#[test]
fn test_deeply_nested_parentheses_95() {
    let open = "(".repeat(95);
    let close = ")".repeat(95);
    let script = format!("{}1{}", open, close);
    let result = Program::compile(&script);
    assert!(result.is_err(), "Should reject 95 levels of nesting");
}

/// Deeply nested parentheses (96 levels) should be rejected
#[test]
fn test_deeply_nested_parentheses_96() {
    let open = "(".repeat(96);
    let close = ")".repeat(96);
    let script = format!("{}1{}", open, close);
    let result = Program::compile(&script);
    assert!(result.is_err(), "Should reject 96 levels of nesting");
}

/// Deeply nested parentheses (97 levels) should be rejected
#[test]
fn test_deeply_nested_parentheses_97() {
    let open = "(".repeat(97);
    let close = ")".repeat(97);
    let script = format!("{}1{}", open, close);
    let result = Program::compile(&script);
    assert!(result.is_err(), "Should reject 97 levels of nesting");
}

/// Deeply nested ternary operators (50 levels) should be rejected
#[test]
fn test_deeply_nested_ternary() {
    let mut script = String::from("1");
    for _ in 0..50 {
        script = format!("true ? ({}) : 0", script);
    }
    let result = Program::compile(&script);
    assert!(
        result.is_err(),
        "Should reject 50 levels of ternary nesting"
    );
}

/// Large string concatenation chain (100 operations) should be handled safely
/// Either rejected at parse time or limited at evaluation time
#[test]
fn test_large_string_concatenation() {
    let script = vec!["\"a\""; 100].join(" + ");
    match Program::compile(&script) {
        Ok(program) => {
            // If parsing succeeds, evaluation should either succeed or hit depth limit
            let result = program.execute(&cel::Context::default());
            // Either outcome is acceptable - no crash is the key requirement
            match result {
                Ok(cel::Value::String(s)) => {
                    assert_eq!(s.len(), 100, "Should concatenate 100 'a' characters");
                }
                Err(cel::ExecutionError::EvaluationDepthExceeded(_)) => {
                    // This is also acceptable - depth limit hit
                }
                Err(e) => panic!("Unexpected error: {:?}", e),
                Ok(v) => panic!("Unexpected value type: {:?}", v),
            }
        }
        Err(_) => {
            // Parse rejection is acceptable
        }
    }
}

/// Verify that reasonable nesting still works
#[test]
fn test_reasonable_nesting_works() {
    // 10 levels of nesting should work fine
    let open = "(".repeat(10);
    let close = ")".repeat(10);
    let script = format!("{}1{}", open, close);
    let program = Program::compile(&script).expect("10 levels should parse");
    let result = program.execute(&cel::Context::default());
    assert!(result.is_ok(), "10 levels should evaluate");
}

/// Verify that reasonable operator chains still work
#[test]
fn test_reasonable_operators_work() {
    // 20 operators should work fine
    let script = vec!["1"; 20].join(" + ");
    let program = Program::compile(&script).expect("20 operators should parse");
    let result = program.execute(&cel::Context::default());
    assert!(result.is_ok(), "20 operators should evaluate");
}
