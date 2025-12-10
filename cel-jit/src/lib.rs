//! # cel-jit: JIT Compilation Backend for CEL
//!
//! This crate provides an alternative execution backend for CEL (Common Expression
//! Language) expressions that compiles expressions to native code using Cranelift JIT.
//!
//! ## Features
//!
//! - **JIT Compilation**: Expressions are compiled to native machine code for fast execution
//! - **Inline Optimizations**: Small integers and common operations are handled inline
//! - **Compatible API**: Works with the same `Context` as the interpreted `cel` crate
//! - **Supported Operations**:
//!   - Arithmetic: `+`, `-`, `*`, `/`, `%`
//!   - Comparison: `==`, `!=`, `<`, `<=`, `>`, `>=`
//!   - Logical: `&&`, `||`, `!`
//!   - Ternary: `condition ? then : else`
//!   - Collections: lists `[]`, maps `{}`, indexing `[n]`, membership `in`
//!   - Comprehensions: `all`, `exists`, `exists_one`, `map`, `filter`
//!   - Built-in functions: `size()`, `contains()`, `startsWith()`, `endsWith()`,
//!     `string()`, `int()`, `uint()`, `double()`, `bytes()`, `type()`
//!
//! ## Performance
//!
//! The JIT-compiled backend provides significant speedups for many expressions:
//! - Simple arithmetic: ~3x faster than interpreter
//! - Comparisons: ~5x faster than interpreter
//! - Conditional expressions: ~1.6x faster than interpreter
//!
//! ## Custom Functions
//!
//! Custom host functions registered via `Context::add_function()` are supported:
//!
//! ```rust,no_run
//! use cel_jit::CompiledProgram;
//! use cel::Context;
//!
//! let mut ctx = Context::default();
//! ctx.add_function("add", |a: i64, b: i64| a + b);
//!
//! let program = CompiledProgram::compile("add(2, 3)").unwrap();
//! let result = program.execute(&ctx).unwrap();
//! assert_eq!(result, cel::Value::Int(5));
//! ```
//!
//! ## Feature Flags
//!
//! This crate supports the same feature flags as the `cel` crate:
//!
//! - `regex` (default): Enables `matches()` function for regex matching
//! - `chrono` (default): Enables timestamp and duration types
//! - `json`: Enables JSON-related functionality
//!
//! ## Example
//!
//! ```rust,no_run
//! use cel_jit::CompiledProgram;
//! use cel::Context;
//!
//! // Compile a CEL expression
//! let program = CompiledProgram::compile("x + y > 10").unwrap();
//!
//! // Execute with a context
//! let mut ctx = Context::default();
//! ctx.add_variable_from_value("x", 5i64);
//! ctx.add_variable_from_value("y", 10i64);
//!
//! let result = program.execute(&ctx).unwrap();
//! assert_eq!(result, cel::Value::Bool(true));
//! ```
//!
//! ## Convenience Function
//!
//! For one-off evaluations, use the [`eval`] function:
//!
//! ```rust,no_run
//! use cel_jit::eval;
//! use cel::Context;
//!
//! let ctx = Context::default();
//! let result = eval("1 + 2 * 3", &ctx).unwrap();
//! assert_eq!(result, cel::Value::Int(7));
//! ```

pub mod compiler;
pub mod error;
pub mod runtime;

pub use compiler::Compiler;
pub use error::CompileError;
pub use runtime::{BoxedValue, RuntimeContext};

use cel::{Context, Program, ResolveResult};
use compiler::lowering::LoweringData;
use compiler::CompiledFn;

/// A compiled CEL program that can be executed efficiently.
///
/// Unlike `cel::Program` which uses a tree-walking interpreter,
/// `CompiledProgram` compiles the expression to native code for
/// potentially better performance on complex expressions.
pub struct CompiledProgram {
    /// The compiled function.
    func: CompiledFn,
    /// The compiler instance (kept alive to preserve JIT memory).
    _compiler: Compiler,
    /// String constants and other data that must outlive the compiled code.
    _lowering_data: LoweringData,
}

impl CompiledProgram {
    /// Compile a CEL source string to native code.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use cel_jit::CompiledProgram;
    ///
    /// let program = CompiledProgram::compile("x + y > 10").unwrap();
    /// ```
    pub fn compile(source: &str) -> Result<Self, CompileError> {
        let cel_program = Program::compile(source)?;
        Self::from_program(&cel_program)
    }

    /// Compile from an existing parsed CEL Program.
    ///
    /// This is useful when you want to parse once and have both
    /// interpreted and compiled versions available.
    pub fn from_program(program: &Program) -> Result<Self, CompileError> {
        let mut compiler = Compiler::new()?;
        let (func, lowering_data) = compiler.compile_program(program)?;

        Ok(CompiledProgram {
            func,
            _compiler: compiler,
            _lowering_data: lowering_data,
        })
    }

    /// Execute the compiled program with the given context.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use cel_jit::CompiledProgram;
    /// use cel::Context;
    ///
    /// let program = CompiledProgram::compile("x * 2").unwrap();
    ///
    /// let mut ctx = Context::default();
    /// ctx.add_variable_from_value("x", 21i64);
    ///
    /// let result = program.execute(&ctx).unwrap();
    /// assert_eq!(result, cel::Value::Int(42));
    /// ```
    pub fn execute(&self, context: &Context) -> ResolveResult {
        let mut runtime_ctx = RuntimeContext::new(context);

        // Call the compiled function
        let result = unsafe { (self.func)(&mut runtime_ctx) };

        // Check for error
        if result.error != 0 {
            return Err(runtime_ctx
                .take_error()
                .unwrap_or_else(|| cel::ExecutionError::FunctionError {
                    function: "compiled".to_string(),
                    message: "Unknown error".to_string(),
                }));
        }

        // Unbox the result, consuming and freeing the BoxedValue
        let boxed = unsafe { BoxedValue::from_raw(result.value) };
        Ok(unsafe { boxed.into_value() })
    }
}

/// Execute a CEL expression with JIT compilation.
///
/// This is a convenience function that compiles and executes in one step.
/// For repeated executions, prefer creating a `CompiledProgram` and reusing it.
///
/// # Example
///
/// ```rust,no_run
/// use cel_jit::eval;
/// use cel::Context;
///
/// let ctx = Context::default();
/// let result = eval("1 + 2", &ctx).unwrap();
/// assert_eq!(result, cel::Value::Int(3));
/// ```
pub fn eval(source: &str, context: &Context) -> ResolveResult {
    let program = CompiledProgram::compile(source).map_err(|e| {
        cel::ExecutionError::FunctionError {
            function: "compile".to_string(),
            message: e.to_string(),
        }
    })?;
    program.execute(context)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cel::Value;

    #[test]
    fn test_simple_arithmetic() {
        let ctx = Context::default();

        let program = CompiledProgram::compile("1 + 2").unwrap();
        let result = program.execute(&ctx).unwrap();
        assert_eq!(result, Value::Int(3));
    }

    #[test]
    fn test_multiplication() {
        let ctx = Context::default();

        let program = CompiledProgram::compile("3 * 4").unwrap();
        let result = program.execute(&ctx).unwrap();
        assert_eq!(result, Value::Int(12));
    }

    #[test]
    fn test_complex_expression() {
        let ctx = Context::default();

        let program = CompiledProgram::compile("1 + 2 * 3").unwrap();
        let result = program.execute(&ctx).unwrap();
        assert_eq!(result, Value::Int(7));
    }

    #[test]
    fn test_comparison() {
        let ctx = Context::default();

        let program = CompiledProgram::compile("10 > 5").unwrap();
        let result = program.execute(&ctx).unwrap();
        assert_eq!(result, Value::Bool(true));

        let program = CompiledProgram::compile("3 < 2").unwrap();
        let result = program.execute(&ctx).unwrap();
        assert_eq!(result, Value::Bool(false));
    }

    #[test]
    fn test_logical_and() {
        let ctx = Context::default();

        let program = CompiledProgram::compile("true && true").unwrap();
        let result = program.execute(&ctx).unwrap();
        assert_eq!(result, Value::Bool(true));

        let program = CompiledProgram::compile("true && false").unwrap();
        let result = program.execute(&ctx).unwrap();
        assert_eq!(result, Value::Bool(false));
    }

    #[test]
    fn test_logical_or() {
        let ctx = Context::default();

        let program = CompiledProgram::compile("false || true").unwrap();
        let result = program.execute(&ctx).unwrap();
        assert_eq!(result, Value::Bool(true));

        let program = CompiledProgram::compile("false || false").unwrap();
        let result = program.execute(&ctx).unwrap();
        assert_eq!(result, Value::Bool(false));
    }

    #[test]
    fn test_conditional() {
        let ctx = Context::default();

        let program = CompiledProgram::compile("true ? 1 : 2").unwrap();
        let result = program.execute(&ctx).unwrap();
        assert_eq!(result, Value::Int(1));

        let program = CompiledProgram::compile("false ? 1 : 2").unwrap();
        let result = program.execute(&ctx).unwrap();
        assert_eq!(result, Value::Int(2));
    }

    #[test]
    fn test_variable() {
        let mut ctx = Context::default();
        ctx.add_variable_from_value("x", 42i64);

        let program = CompiledProgram::compile("x").unwrap();
        let result = program.execute(&ctx).unwrap();
        assert_eq!(result, Value::Int(42));
    }

    #[test]
    fn test_variable_arithmetic() {
        let mut ctx = Context::default();
        ctx.add_variable_from_value("x", 10i64);
        ctx.add_variable_from_value("y", 20i64);

        let program = CompiledProgram::compile("x + y").unwrap();
        let result = program.execute(&ctx).unwrap();
        assert_eq!(result, Value::Int(30));
    }

    #[test]
    fn test_boolean_literal() {
        let ctx = Context::default();

        let program = CompiledProgram::compile("true").unwrap();
        let result = program.execute(&ctx).unwrap();
        assert_eq!(result, Value::Bool(true));

        let program = CompiledProgram::compile("false").unwrap();
        let result = program.execute(&ctx).unwrap();
        assert_eq!(result, Value::Bool(false));
    }

    #[test]
    fn test_null_literal() {
        let ctx = Context::default();

        let program = CompiledProgram::compile("null").unwrap();
        let result = program.execute(&ctx).unwrap();
        assert_eq!(result, Value::Null);
    }

    #[test]
    fn test_equality() {
        let ctx = Context::default();

        let program = CompiledProgram::compile("1 == 1").unwrap();
        let result = program.execute(&ctx).unwrap();
        assert_eq!(result, Value::Bool(true));

        let program = CompiledProgram::compile("1 == 2").unwrap();
        let result = program.execute(&ctx).unwrap();
        assert_eq!(result, Value::Bool(false));

        let program = CompiledProgram::compile("1 != 2").unwrap();
        let result = program.execute(&ctx).unwrap();
        assert_eq!(result, Value::Bool(true));
    }

    #[test]
    fn test_negation() {
        let ctx = Context::default();

        let program = CompiledProgram::compile("-5").unwrap();
        let result = program.execute(&ctx).unwrap();
        assert_eq!(result, Value::Int(-5));
    }

    #[test]
    fn test_logical_not() {
        let ctx = Context::default();

        let program = CompiledProgram::compile("!true").unwrap();
        let result = program.execute(&ctx).unwrap();
        assert_eq!(result, Value::Bool(false));

        let program = CompiledProgram::compile("!false").unwrap();
        let result = program.execute(&ctx).unwrap();
        assert_eq!(result, Value::Bool(true));
    }

    #[test]
    fn test_member_access() {
        let mut ctx = Context::default();
        ctx.add_variable_from_value("obj", std::collections::HashMap::from([("field", 42i64)]));

        let program = CompiledProgram::compile("obj.field").unwrap();
        let result = program.execute(&ctx).unwrap();
        assert_eq!(result, Value::Int(42));
    }

    #[test]
    fn test_list_literal() {
        let ctx = Context::default();

        let program = CompiledProgram::compile("[1, 2, 3]").unwrap();
        let result = program.execute(&ctx).unwrap();

        if let Value::List(list) = result {
            assert_eq!(list.len(), 3);
            assert_eq!(list[0], Value::Int(1));
            assert_eq!(list[1], Value::Int(2));
            assert_eq!(list[2], Value::Int(3));
        } else {
            panic!("Expected list, got {:?}", result);
        }
    }

    #[test]
    fn test_list_addition() {
        let ctx = Context::default();

        // Test list concatenation
        let program = CompiledProgram::compile("[1, 2, 3] + [4, 5, 6]").unwrap();
        let result = program.execute(&ctx).unwrap();

        if let Value::List(list) = result {
            assert_eq!(list.len(), 6);
            assert_eq!(list[0], Value::Int(1));
            assert_eq!(list[1], Value::Int(2));
            assert_eq!(list[2], Value::Int(3));
            assert_eq!(list[3], Value::Int(4));
            assert_eq!(list[4], Value::Int(5));
            assert_eq!(list[5], Value::Int(6));
        } else {
            panic!("Expected list, got {:?}", result);
        }

        // Test that multiple executions work without memory issues
        for _ in 0..1000 {
            let _ = program.execute(&ctx);
        }
    }

    #[test]
    fn test_eval_convenience() {
        let ctx = Context::default();
        let result = eval("2 + 2", &ctx).unwrap();
        assert_eq!(result, Value::Int(4));
    }

    #[test]
    fn test_size() {
        let ctx = Context::default();

        // String size
        let program = CompiledProgram::compile("size('hello')").unwrap();
        let result = program.execute(&ctx).unwrap();
        assert_eq!(result, Value::Int(5));

        // List size
        let program = CompiledProgram::compile("[1, 2, 3].size()").unwrap();
        let result = program.execute(&ctx).unwrap();
        assert_eq!(result, Value::Int(3));
    }

    #[test]
    fn test_contains() {
        let ctx = Context::default();

        // String contains
        let program = CompiledProgram::compile("'hello world'.contains('world')").unwrap();
        let result = program.execute(&ctx).unwrap();
        assert_eq!(result, Value::Bool(true));

        let program = CompiledProgram::compile("'hello'.contains('xyz')").unwrap();
        let result = program.execute(&ctx).unwrap();
        assert_eq!(result, Value::Bool(false));

        // List contains
        let program = CompiledProgram::compile("[1, 2, 3].contains(2)").unwrap();
        let result = program.execute(&ctx).unwrap();
        assert_eq!(result, Value::Bool(true));
    }

    #[test]
    fn test_starts_with() {
        let ctx = Context::default();

        let program = CompiledProgram::compile("'hello world'.startsWith('hello')").unwrap();
        let result = program.execute(&ctx).unwrap();
        assert_eq!(result, Value::Bool(true));

        let program = CompiledProgram::compile("'hello'.startsWith('world')").unwrap();
        let result = program.execute(&ctx).unwrap();
        assert_eq!(result, Value::Bool(false));
    }

    #[test]
    fn test_ends_with() {
        let ctx = Context::default();

        let program = CompiledProgram::compile("'hello world'.endsWith('world')").unwrap();
        let result = program.execute(&ctx).unwrap();
        assert_eq!(result, Value::Bool(true));

        let program = CompiledProgram::compile("'hello'.endsWith('xyz')").unwrap();
        let result = program.execute(&ctx).unwrap();
        assert_eq!(result, Value::Bool(false));
    }

    #[test]
    fn test_type_conversion() {
        let ctx = Context::default();

        // int() conversion
        let program = CompiledProgram::compile("int(3.14)").unwrap();
        let result = program.execute(&ctx).unwrap();
        assert_eq!(result, Value::Int(3));

        // double() conversion
        let program = CompiledProgram::compile("double(42)").unwrap();
        let result = program.execute(&ctx).unwrap();
        assert_eq!(result, Value::Float(42.0));

        // string() conversion
        let program = CompiledProgram::compile("string(123)").unwrap();
        let result = program.execute(&ctx).unwrap();
        assert_eq!(result, Value::String(std::sync::Arc::new("123".to_string())));
    }

    #[test]
    fn test_type_function() {
        let ctx = Context::default();

        let program = CompiledProgram::compile("type(42)").unwrap();
        let result = program.execute(&ctx).unwrap();
        assert_eq!(result, Value::String(std::sync::Arc::new("int".to_string())));

        let program = CompiledProgram::compile("type('hello')").unwrap();
        let result = program.execute(&ctx).unwrap();
        assert_eq!(result, Value::String(std::sync::Arc::new("string".to_string())));

        let program = CompiledProgram::compile("type(true)").unwrap();
        let result = program.execute(&ctx).unwrap();
        assert_eq!(result, Value::String(std::sync::Arc::new("bool".to_string())));
    }

    #[test]
    fn test_in_operator() {
        let ctx = Context::default();

        // In list
        let program = CompiledProgram::compile("2 in [1, 2, 3]").unwrap();
        let result = program.execute(&ctx).unwrap();
        assert_eq!(result, Value::Bool(true));

        let program = CompiledProgram::compile("5 in [1, 2, 3]").unwrap();
        let result = program.execute(&ctx).unwrap();
        assert_eq!(result, Value::Bool(false));

        // In map (checks keys)
        let program = CompiledProgram::compile("'a' in {'a': 1, 'b': 2}").unwrap();
        let result = program.execute(&ctx).unwrap();
        assert_eq!(result, Value::Bool(true));
    }

    #[test]
    fn test_index_operator() {
        let ctx = Context::default();

        // List index
        let program = CompiledProgram::compile("[10, 20, 30][1]").unwrap();
        let result = program.execute(&ctx).unwrap();
        assert_eq!(result, Value::Int(20));

        // Map index
        let program = CompiledProgram::compile("{'a': 100, 'b': 200}['a']").unwrap();
        let result = program.execute(&ctx).unwrap();
        assert_eq!(result, Value::Int(100));
    }

    #[test]
    fn test_string_literal() {
        let ctx = Context::default();

        let program = CompiledProgram::compile("'hello'").unwrap();
        let result = program.execute(&ctx).unwrap();
        assert_eq!(result, Value::String(std::sync::Arc::new("hello".to_string())));
    }

    #[test]
    fn test_float_literal() {
        let ctx = Context::default();

        let program = CompiledProgram::compile("3.14").unwrap();
        let result = program.execute(&ctx).unwrap();
        assert_eq!(result, Value::Float(3.14));
    }

    #[test]
    fn test_map_literal() {
        let ctx = Context::default();

        let program = CompiledProgram::compile("{'a': 1, 'b': 2}").unwrap();
        let result = program.execute(&ctx).unwrap();

        if let Value::Map(map) = result {
            assert_eq!(map.map.len(), 2);
        } else {
            panic!("Expected map, got {:?}", result);
        }
    }

    #[test]
    fn test_nested_member_access() {
        let mut ctx = Context::default();
        ctx.add_variable_from_value(
            "obj",
            std::collections::HashMap::from([
                ("nested", std::collections::HashMap::from([("value", 42i64)])),
            ]),
        );

        let program = CompiledProgram::compile("obj.nested.value").unwrap();
        let result = program.execute(&ctx).unwrap();
        assert_eq!(result, Value::Int(42));
    }

    #[test]
    fn test_complex_boolean_logic() {
        let ctx = Context::default();

        // Complex AND/OR
        let program = CompiledProgram::compile("(true && false) || (true && true)").unwrap();
        let result = program.execute(&ctx).unwrap();
        assert_eq!(result, Value::Bool(true));

        // Negation with comparison
        let program = CompiledProgram::compile("!(5 > 10)").unwrap();
        let result = program.execute(&ctx).unwrap();
        assert_eq!(result, Value::Bool(true));
    }

    #[test]
    fn test_all_comprehension() {
        let ctx = Context::default();

        // All elements greater than 0
        let program = CompiledProgram::compile("[1, 2, 3].all(x, x > 0)").unwrap();
        let result = program.execute(&ctx).unwrap();
        assert_eq!(result, Value::Bool(true));

        // Not all elements greater than 2
        let program = CompiledProgram::compile("[1, 2, 3].all(x, x > 2)").unwrap();
        let result = program.execute(&ctx).unwrap();
        assert_eq!(result, Value::Bool(false));
    }

    #[test]
    fn test_exists_comprehension() {
        let ctx = Context::default();

        // Some element equals 2
        let program = CompiledProgram::compile("[1, 2, 3].exists(x, x == 2)").unwrap();
        let result = program.execute(&ctx).unwrap();
        assert_eq!(result, Value::Bool(true));

        // No element equals 10
        let program = CompiledProgram::compile("[1, 2, 3].exists(x, x == 10)").unwrap();
        let result = program.execute(&ctx).unwrap();
        assert_eq!(result, Value::Bool(false));
    }

    #[test]
    fn test_exists_one_comprehension() {
        let ctx = Context::default();

        // Exactly one element equals 2
        let program = CompiledProgram::compile("[1, 2, 3].exists_one(x, x == 2)").unwrap();
        let result = program.execute(&ctx).unwrap();
        assert_eq!(result, Value::Bool(true));

        // More than one element greater than 1
        let program = CompiledProgram::compile("[1, 2, 3].exists_one(x, x > 1)").unwrap();
        let result = program.execute(&ctx).unwrap();
        assert_eq!(result, Value::Bool(false));
    }

    #[test]
    fn test_map_comprehension() {
        let ctx = Context::default();

        // Map over list, doubling each element
        let program = CompiledProgram::compile("[1, 2, 3].map(x, x * 2)").unwrap();
        let result = program.execute(&ctx).unwrap();

        if let Value::List(list) = result {
            assert_eq!(list.len(), 3);
            assert_eq!(list[0], Value::Int(2));
            assert_eq!(list[1], Value::Int(4));
            assert_eq!(list[2], Value::Int(6));
        } else {
            panic!("Expected list, got {:?}", result);
        }
    }

    #[test]
    fn test_filter_comprehension() {
        let ctx = Context::default();

        // Filter list, keeping only elements > 1
        let program = CompiledProgram::compile("[1, 2, 3].filter(x, x > 1)").unwrap();
        let result = program.execute(&ctx).unwrap();

        if let Value::List(list) = result {
            assert_eq!(list.len(), 2);
            assert_eq!(list[0], Value::Int(2));
            assert_eq!(list[1], Value::Int(3));
        } else {
            panic!("Expected list, got {:?}", result);
        }
    }

    #[test]
    fn test_max_min() {
        let ctx = Context::default();

        // max of variadic args
        let program = CompiledProgram::compile("max(1, 5, 3)").unwrap();
        let result = program.execute(&ctx).unwrap();
        assert_eq!(result, Value::Int(5));

        // min of variadic args
        let program = CompiledProgram::compile("min(1, 5, 3)").unwrap();
        let result = program.execute(&ctx).unwrap();
        assert_eq!(result, Value::Int(1));
    }

    #[test]
    fn test_has_macro() {
        let ctx = Context::default();

        // has() returns true when field exists
        let program = CompiledProgram::compile("has({'a': 1, 'b': 2}.a)").unwrap();
        let result = program.execute(&ctx).unwrap();
        assert_eq!(result, Value::Bool(true));

        // has() returns false when field doesn't exist
        let program = CompiledProgram::compile("has({'a': 1, 'b': 2}.c)").unwrap();
        let result = program.execute(&ctx).unwrap();
        assert_eq!(result, Value::Bool(false));

        // has() returns false for non-map types
        let program = CompiledProgram::compile("has([1, 2, 3].x)").unwrap();
        let result = program.execute(&ctx).unwrap();
        assert_eq!(result, Value::Bool(false));
    }

    #[test]
    fn test_custom_function_basic() {
        let mut ctx = Context::default();
        ctx.add_function("add", |a: i64, b: i64| a + b);

        let program = CompiledProgram::compile("add(2, 3)").unwrap();
        let result = program.execute(&ctx).unwrap();
        assert_eq!(result, Value::Int(5));
    }

    #[test]
    fn test_custom_function_in_expression() {
        let mut ctx = Context::default();
        ctx.add_function("timesTwo", |x: i64| -> i64 { x * 2 });

        let program = CompiledProgram::compile("timesTwo(5) + 1").unwrap();
        let result = program.execute(&ctx).unwrap();
        assert_eq!(result, Value::Int(11));
    }

    #[test]
    fn test_custom_function_comparison() {
        let mut ctx = Context::default();
        ctx.add_function("add", |a: i64, b: i64| a + b);

        let program = CompiledProgram::compile("add(2, 3) == 5").unwrap();
        let result = program.execute(&ctx).unwrap();
        assert_eq!(result, Value::Bool(true));
    }

    #[cfg(feature = "regex")]
    #[test]
    fn test_matches_regex() {
        let ctx = Context::default();

        // Match lowercase letters
        let program = CompiledProgram::compile(r#""abc".matches("^[a-z]*$")"#).unwrap();
        let result = program.execute(&ctx).unwrap();
        assert_eq!(result, Value::Bool(true));

        // Doesn't match uppercase
        let program = CompiledProgram::compile(r#""ABC".matches("^[a-z]*$")"#).unwrap();
        let result = program.execute(&ctx).unwrap();
        assert_eq!(result, Value::Bool(false));

        // Match email pattern
        let program = CompiledProgram::compile(r#""test@example.com".matches("^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\\.[a-zA-Z]{2,}$")"#).unwrap();
        let result = program.execute(&ctx).unwrap();
        assert_eq!(result, Value::Bool(true));
    }

    #[cfg(feature = "chrono")]
    #[test]
    fn test_timestamp_duration() {
        let ctx = Context::default();

        // Create a timestamp
        let program = CompiledProgram::compile(r#"timestamp("2023-01-15T10:30:00Z")"#).unwrap();
        let result = program.execute(&ctx).unwrap();
        assert!(matches!(result, Value::Timestamp(_)));

        // Create a duration
        let program = CompiledProgram::compile(r#"duration("1h30m")"#).unwrap();
        let result = program.execute(&ctx).unwrap();
        assert!(matches!(result, Value::Duration(_)));
    }
}
