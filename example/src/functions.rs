#![allow(clippy::too_many_arguments)]
use std::sync::Arc;

use cel::extractors::This;
use cel::{Context, ExecutionError, FunctionContext, Program, ResolveResult, Value};
use chrono::{DateTime, Duration, FixedOffset};

fn main() {
    let program = Program::compile("add(2, 3) == 5 && ''.isEmpty() && fail()").unwrap();
    let mut context = Context::default();

    // Add functions using closures
    context.add_function("add", |a: i64, b: i64| a + b);

    // Add methods to a string type
    context.add_function("isEmpty", is_empty);

    // Use the function context to return error messages
    context.add_function("fail", fail);

    // See all the different value types you can accept in your functions
    context.add_function("primitives", primitives);

    // Run the program
    let result = program.execute(&context);
    assert!(matches!(result, Err(ExecutionError::FunctionError { .. })));
}

/// A method on a string type. When added to the CEL context, this function
/// can be called by running. We use the [`This`] extractor give us a reference
/// to the string that this method was called on.
///
/// ```skip
/// "foo".isEmpty()
/// ```
fn is_empty(This(s): This<Arc<String>>) -> bool {
    s.is_empty()
}

/// A function that gives us access to the [`FunctionContext`]. All functions have
/// can accept this context as their first argument. The context is helpful for
/// several reasons:
/// 1. Creating shadowed variables using cloned contexts (see the `filter` and `map`
///    functions for example).
/// 2. Functions that are fallible can return an error message using ftx.error(...)
///    which attaches some helpful context for the error.
fn fail(ftx: &FunctionContext) -> ResolveResult {
    ftx.error("This function always fails").into()
}

/// A function that illustrates all the different types of values that are supported
/// as arguments to a function, as well as the fact that any of these types can also
/// be returned from a function.
fn primitives(
    _a: i64,
    _b: u64,
    _c: f64,
    _d: bool,
    _e: Arc<String>,
    _f: Arc<Vec<u8>>,
    _g: Duration,
    _h: DateTime<FixedOffset>,
    _i: Arc<Vec<Value>>,
) -> Duration {
    Duration::zero()
}
