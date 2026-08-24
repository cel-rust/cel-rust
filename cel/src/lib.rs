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
use std::fmt;
use std::sync::Arc;
use thiserror::Error;

mod macros;

pub mod common;
pub mod context;
mod env;
pub mod parser;

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

/// Details about an operator or function call for which no overload matched.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct OverloadError {
    function: Option<Arc<String>>,
    argument_types: Arc<[String]>,
    member_function: bool,
}

impl OverloadError {
    fn new(function: &str, argument_types: Vec<String>, member_function: bool) -> Self {
        Self {
            function: Some(Arc::new(function.to_owned())),
            argument_types: argument_types.into(),
            member_function,
        }
    }

    /// Returns the operator or function name, when it is known.
    pub fn function(&self) -> Option<&str> {
        self.function.as_deref().map(String::as_str)
    }

    /// Returns the runtime types of the arguments supplied to the call.
    ///
    /// For a member function, the first type is the receiver type.
    pub fn argument_types(&self) -> &[String] {
        &self.argument_types
    }

    /// Returns whether the overload was invoked using member-call syntax.
    pub fn is_member_function(&self) -> bool {
        self.member_function
    }
}

impl fmt::Display for OverloadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Some(function) = &self.function else {
            return f.write_str("No such overload");
        };

        let signature = if self.member_function && !self.argument_types.is_empty() {
            format!(
                "{}.({})",
                self.argument_types[0],
                self.argument_types[1..].join(", ")
            )
        } else {
            format!("({})", self.argument_types.join(", "))
        };
        write!(
            f,
            "found no matching overload for '{function}' applied to '{signature}'"
        )
    }
}

impl std::error::Error for OverloadError {}

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
    #[error("{0}")]
    NoSuchOverload(OverloadError),
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
    #[error("InternalError: {0:?}")]
    InternalError(String),
}

impl ExecutionError {
    /// Creates an error for a global function or operator with no matching overload.
    pub fn no_such_overload(function: &str, argument_types: Vec<String>) -> Self {
        ExecutionError::NoSuchOverload(OverloadError::new(function, argument_types, false))
    }

    /// Creates an error for a member function with no matching overload.
    ///
    /// The receiver type must be the first entry in `argument_types`.
    pub fn no_such_member_overload(function: &str, argument_types: Vec<String>) -> Self {
        ExecutionError::NoSuchOverload(OverloadError::new(function, argument_types, true))
    }

    pub(crate) fn unresolved_overload() -> Self {
        ExecutionError::NoSuchOverload(OverloadError::default())
    }

    pub(crate) fn overload_for_values<'a>(
        function: &str,
        arguments: impl IntoIterator<Item = &'a dyn common::value::Val>,
        member_function: bool,
    ) -> Self {
        let argument_types = arguments
            .into_iter()
            .map(|argument| argument.get_type().name().to_owned())
            .collect();
        let overload = OverloadError::new(function, argument_types, member_function);
        ExecutionError::NoSuchOverload(overload)
    }

    pub(crate) fn with_overload_context(self, context: ExecutionError) -> Self {
        match self {
            ExecutionError::NoSuchOverload(_) => context,
            error => error,
        }
    }

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
        assert_output("str[0]", Err(ExecutionError::unresolved_overload()));
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
    fn no_such_overload_reports_the_call_and_runtime_types() {
        let tests = [
            (
                "1 > \"1\"",
                "found no matching overload for '_>_' applied to '(int, string)'",
            ),
            (
                "size(1, 2)",
                "found no matching overload for 'size' applied to '(int, int)'",
            ),
            (
                "\"foobar\".contains(1)",
                "found no matching overload for 'contains' applied to 'string.(int)'",
            ),
        ];

        for (script, expected) in tests {
            let error = test_script(script, None).expect_err(script);
            assert_eq!(error.to_string(), expected, "{script}");
        }

        assert_eq!(
            test_script("missing(1)", None),
            Err(ExecutionError::undeclared_reference("missing"))
        );
        assert_eq!(
            test_script("\"foobar\".missing(1)", None),
            Err(ExecutionError::undeclared_reference("missing"))
        );

        let error = test_script("\"foobar\".contains(1)", None).unwrap_err();
        let ExecutionError::NoSuchOverload(overload) = error else {
            panic!("expected a no-such-overload error");
        };
        assert_eq!(overload.function(), Some("contains"));
        assert_eq!(overload.argument_types(), ["string", "int"]);
        assert!(overload.is_member_function());
    }
}
