//! # CEL-Rust
//!
//! A parser and interpreter for the Common Expression Language (CEL) in Rust.
//!
//! ## Optional Features
//!
//! - `structs`: Enables support for custom struct types. This allows you to define
//!   struct definitions using [`StructDef`] and add them to your [`Env`].
//!   Custom structs can then be instantiated and accessed within CEL expressions.
//! - `chrono`: Enables support for `duration` and `timestamp` types using the `chrono` crate.
//! - `regex`: Enables support for regular expressions.
//! - `json`: Enables conversion between CEL values and JSON.
//!
extern crate core;

use std::convert::TryFrom;
use std::sync::Arc;
use thiserror::Error;

mod macros;

mod budget;
pub mod common;
pub mod context;
mod env;
pub mod parser;

pub use budget::ExecutionBudget;
pub use common::ast::IdedExpr;
use common::ast::SelectExpr;
pub use context::Context;
pub use functions::FunctionContext;
pub use objects::{ResolveResult, Value};
use parser::{Expression, ExpressionReferences, Parser};
pub use parser::{ParseError, ParseErrors};
pub mod functions;
mod magic;
pub mod objects;
mod resolvers;

#[cfg(feature = "chrono")]
mod duration;
#[cfg(feature = "chrono")]
pub use ser::{Duration, Timestamp};

pub use env::Env;
#[cfg(feature = "structs")]
pub use env::StructDef;

mod ser;
pub use ser::to_value;
pub use ser::SerializationError;

#[cfg(feature = "json")]
mod json;
#[cfg(feature = "json")]
pub use json::ConvertToJsonError;

use magic::FromContext;

pub mod extractors {
    pub use crate::magic::{Arguments, Identifier, This};

    pub use crate::magic::{IntoFunction, IntoResolveResult};
}

#[derive(Error, Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum ExecutionError {
    #[error("Invalid argument count: expected {expected}, got {actual}")]
    InvalidArgumentCount { expected: usize, actual: usize },
    #[error("Invalid argument type: {:?}", .target)]
    UnsupportedTargetType { target: Value },
    #[error("Method '{method}' not supported on type '{target:?}'")]
    NotSupportedAsMethod { method: String, target: Value },
    /// Indicates that the script attempted to use a value as a key in a map,
    /// but the type of the value was not supported as a key.
    #[error("Unable to use value '{0:?}' as a key")]
    UnsupportedKeyType(Value),
    #[error("Unexpected type: got '{got}', want '{want}'")]
    UnexpectedType { got: String, want: String },
    /// Indicates that the script attempted to reference a key on a type that
    /// was missing the requested key.
    #[error("No such key: {0}")]
    NoSuchKey(Arc<String>),
    /// Indicates that the script used an existing operator or function with
    /// values of one or more types for which no overload was declared.
    #[error("No such overload")]
    NoSuchOverload,
    /// Indicates that the script attempted to reference an undeclared variable
    /// method, or function.
    #[error("Undeclared reference to '{0}'")]
    UndeclaredReference(Arc<String>),
    /// Indicates that a function expected to be called as a method, or to be
    /// called with at least one parameter.
    #[error("Missing argument or target")]
    MissingArgumentOrTarget,
    /// Indicates that a comparison could not be performed.
    #[error("{0:?} can not be compared to {1:?}")]
    ValuesNotComparable(Value, Value),
    #[deprecated]
    #[error("Unsupported unary operator '{0}': {1:?}")]
    UnsupportedUnaryOperator(&'static str, Value),
    /// Indicates that an unsupported binary operator was applied on two values
    /// where it's unsupported, for example list + map.
    #[error("Unsupported binary operator '{0}': {1:?}, {2:?}")]
    UnsupportedBinaryOperator(&'static str, Value, Value),
    #[deprecated]
    #[error("Cannot use value as map index: {0:?}")]
    UnsupportedMapIndex(Value),
    #[deprecated]
    #[error("Cannot use value as list index: {0:?}")]
    UnsupportedListIndex(Value),
    /// Indicates that an unsupported type was used to index a list
    #[error("Cannot use value {0:?} to index {1:?}")]
    UnsupportedIndex(Value, Value),
    #[deprecated]
    #[error("Unsupported function call identifier type: {0:?}")]
    UnsupportedFunctionCallIdentifierType(Expression),
    #[deprecated]
    #[error("Unsupported fields construction: {0:?}")]
    UnsupportedFieldsConstruction(SelectExpr),
    /// Indicates that a function had an error during execution.
    #[error("Error executing function '{function}': {message}")]
    FunctionError { function: String, message: String },
    #[error("Division by zero of {0:?}")]
    DivisionByZero(Value),
    #[error("Remainder by zero of {0:?}")]
    RemainderByZero(Value),
    #[error("Overflow from binary operator '{0}': {1:?}, {2:?}")]
    Overflow(&'static str, Value, Value),
    #[error("Index out of bounds: {0:?}")]
    IndexOutOfBounds(Value),
    /// Indicates that the per-invocation monotonic execution budget expired
    /// before evaluation completed.
    #[error("Execution deadline exceeded")]
    DeadlineExceeded,
    #[error("InternalError: {0:?}")]
    InternalError(String),
}

impl ExecutionError {
    pub fn no_such_key(name: &str) -> Self {
        ExecutionError::NoSuchKey(Arc::new(name.to_string()))
    }

    pub fn undeclared_reference(name: &str) -> Self {
        ExecutionError::UndeclaredReference(Arc::new(name.to_string()))
    }

    pub fn invalid_argument_count(expected: usize, actual: usize) -> Self {
        ExecutionError::InvalidArgumentCount { expected, actual }
    }

    pub fn function_error<E: ToString>(function: &str, error: E) -> Self {
        ExecutionError::FunctionError {
            function: function.to_string(),
            message: error.to_string(),
        }
    }

    pub fn unsupported_target_type(target: Value) -> Self {
        ExecutionError::UnsupportedTargetType { target }
    }

    pub fn not_supported_as_method(method: &str, target: Value) -> Self {
        ExecutionError::NotSupportedAsMethod {
            method: method.to_string(),
            target,
        }
    }

    pub fn unsupported_key_type(value: Value) -> Self {
        ExecutionError::UnsupportedKeyType(value)
    }

    pub fn missing_argument_or_target() -> Self {
        ExecutionError::MissingArgumentOrTarget
    }

    pub fn deadline_exceeded() -> Self {
        ExecutionError::DeadlineExceeded
    }
}

#[derive(Debug)]
pub struct Program {
    expression: Expression,
}

impl Program {
    pub fn compile(source: &str) -> Result<Program, ParseErrors> {
        let parser = Parser::default();
        parser
            .parse(source)
            .map(|expression| Program { expression })
    }

    pub fn execute(&self, context: &Context) -> ResolveResult {
        Value::resolve(&self.expression, context)
    }

    /// Execute this program with a per-invocation [`ExecutionBudget`].
    ///
    /// The budget is applied through a temporary child scope and does not
    /// mutate `context` or this compiled [`Program`]. Concurrent executions of
    /// the same program may use independent budgets.
    pub fn execute_with_budget(
        &self,
        context: &Context<'_>,
        budget: ExecutionBudget,
    ) -> ResolveResult {
        let scoped = context.with_execution_budget(budget);
        Value::resolve(&self.expression, &scoped)
    }

    /// Returns the variables and functions referenced by the CEL program
    ///
    /// # Example
    /// ```rust
    /// # use cel::Program;
    /// let program = Program::compile("size(foo) > 0").unwrap();
    /// let references = program.references();
    ///
    /// assert!(references.has_function("size"));
    /// assert!(references.has_variable("foo"));
    /// ```
    pub fn references(&self) -> ExpressionReferences<'_> {
        self.expression.references()
    }

    /// Returns the contained expression
    pub fn expression(&self) -> &Expression {
        &self.expression
    }
}

impl TryFrom<&str> for Program {
    type Error = ParseErrors;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Program::compile(value)
    }
}

#[cfg(test)]
mod tests {
    use crate::context::Context;
    use crate::objects::{ResolveResult, Value};
    use crate::{ExecutionError, Program};
    use std::collections::HashMap;
    use std::convert::TryInto;

    /// Tests the provided script and returns the result. An optional context can be provided.
    pub(crate) fn test_script(script: &str, ctx: Option<Context>) -> ResolveResult {
        let program = match Program::compile(script) {
            Ok(p) => p,
            Err(e) => panic!("{}", e),
        };
        program.execute(&ctx.unwrap_or_default())
    }

    #[test]
    fn parse() {
        Program::compile("1 + 1").unwrap();
    }

    #[test]
    fn from_str() {
        let input = "1.1";
        let _p: Program = input.try_into().unwrap();
    }

    #[test]
    fn variables() {
        fn assert_output(script: &str, expected: ResolveResult) {
            let mut ctx = Context::default();
            ctx.add_variable_from_value("foo", HashMap::from([("bar", 1i64)]));
            ctx.add_variable_from_value("arr", vec![1i64, 2, 3]);
            ctx.add_variable_from_value("str", "foobar".to_string());
            assert_eq!(test_script(script, Some(ctx)), expected);
        }

        // Test methods
        assert_output("size([1, 2, 3]) == 3", Ok(true.into()));
        assert_output("size([size([42]), 2, 3]) == 3", Ok(true.into()));
        assert_output("size([]) == 3", Ok(false.into()));

        // Test variable attribute traversals
        assert_output("foo.bar == 1", Ok(true.into()));

        // Test that we can index into an array
        assert_output("arr[0] == 1", Ok(true.into()));

        // Test that we cannot index into a string
        assert_output("str[0]", Err(ExecutionError::NoSuchOverload));
    }

    #[test]
    fn references() {
        let p = Program::compile("[1, 1].map(x, x * 2)").unwrap();
        assert!(p.references().has_variable("x"));
        assert_eq!(p.references().variables().len(), 1);
    }

    #[test]
    fn test_execution_errors() {
        let tests = vec![
            (
                "no such key",
                "foo.baz.bar == 1",
                ExecutionError::no_such_key("baz"),
            ),
            (
                "undeclared reference",
                "missing == 1",
                ExecutionError::undeclared_reference("missing"),
            ),
            (
                "undeclared method",
                "1.missing()",
                ExecutionError::undeclared_reference("missing"),
            ),
            (
                "undeclared function",
                "missing(1)",
                ExecutionError::undeclared_reference("missing"),
            ),
            (
                "unsupported key type",
                "{null: true}",
                ExecutionError::unsupported_key_type(Value::Null),
            ),
        ];

        for (name, script, error) in tests {
            let mut ctx = Context::default();
            ctx.add_variable_from_value("foo", HashMap::from([("bar", 1)]));
            let res = test_script(script, Some(ctx));
            assert_eq!(res, error.into(), "{name}");
        }
    }

    #[test]
    fn execution_budget_unlimited_preserves_behavior() {
        let program = Program::compile("[1, 2, 3].map(x, x * 2)").unwrap();
        let ctx = Context::default();
        assert_eq!(
            program.execute(&ctx).unwrap(),
            Value::List(vec![2i64.into(), 4i64.into(), 6i64.into()].into())
        );
        assert_eq!(
            program
                .execute_with_budget(&ctx, crate::ExecutionBudget::unlimited())
                .unwrap(),
            Value::List(vec![2i64.into(), 4i64.into(), 6i64.into()].into())
        );
    }

    #[test]
    fn execution_budget_zero_is_already_expired() {
        let program = Program::compile("1 + 1").unwrap();
        let ctx = Context::default();
        let err = program
            .execute_with_budget(
                &ctx,
                crate::ExecutionBudget::with_timeout(std::time::Duration::ZERO),
            )
            .unwrap_err();
        assert_eq!(err, ExecutionError::DeadlineExceeded);
    }

    #[test]
    fn execution_budget_interrupts_comprehension() {
        let program = Program::compile("items.map(x, x + 1)").unwrap();
        let mut ctx = Context::default();
        ctx.add_variable_from_value("items", vec![1i64; 100_000]);

        let started = std::time::Instant::now();
        let err = program
            .execute_with_budget(
                &ctx,
                crate::ExecutionBudget::with_timeout(std::time::Duration::from_millis(1)),
            )
            .unwrap_err();
        let elapsed = started.elapsed();

        assert_eq!(err, ExecutionError::DeadlineExceeded);
        assert!(
            elapsed < std::time::Duration::from_secs(1),
            "expensive comprehension should return promptly, took {elapsed:?}"
        );
    }

    #[test]
    fn execution_budget_is_per_invocation() {
        let program = Program::compile("items.map(x, x + 1)").unwrap();
        let mut ctx = Context::default();
        ctx.add_variable_from_value("items", vec![1i64; 10_000]);

        let err = program
            .execute_with_budget(
                &ctx,
                crate::ExecutionBudget::with_timeout(std::time::Duration::ZERO),
            )
            .unwrap_err();
        assert_eq!(err, ExecutionError::DeadlineExceeded);

        // A later unlimited execution must still succeed; timeouts do not poison the program.
        let value = program.execute(&ctx).unwrap();
        assert!(matches!(value, Value::List(_)));
    }

    #[test]
    fn execution_budget_does_not_mutate_shared_context() {
        let program = Program::compile("1 + 1").unwrap();
        let ctx = Context::default();
        assert!(ctx.execution_budget().is_unlimited());

        let _ = program
            .execute_with_budget(
                &ctx,
                crate::ExecutionBudget::with_timeout(std::time::Duration::ZERO),
            )
            .unwrap_err();

        assert!(ctx.execution_budget().is_unlimited());
        assert_eq!(program.execute(&ctx).unwrap(), Value::Int(2));
    }

    #[test]
    fn execution_budget_concurrent_timeout_and_success() {
        use std::sync::Arc;
        use std::thread;

        let program = Arc::new(Program::compile("items.map(x, x + 1)").unwrap());

        let timeout_program = Arc::clone(&program);
        let timeout_thread = thread::spawn(move || {
            let mut ctx = Context::default();
            ctx.add_variable_from_value("items", vec![1i64; 10_000]);
            timeout_program.execute_with_budget(
                &ctx,
                crate::ExecutionBudget::with_timeout(std::time::Duration::ZERO),
            )
        });

        let success_program = Arc::clone(&program);
        let success_thread = thread::spawn(move || {
            let mut ctx = Context::default();
            ctx.add_variable_from_value("items", vec![1i64; 32]);
            success_program.execute(&ctx)
        });

        assert_eq!(
            timeout_thread.join().unwrap().unwrap_err(),
            ExecutionError::DeadlineExceeded
        );
        assert!(matches!(
            success_thread.join().unwrap().unwrap(),
            Value::List(_)
        ));
    }
}
