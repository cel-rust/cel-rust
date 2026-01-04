use crate::parser::Expression;
use crate::{ExecutionError, FunctionContext, ResolveResult, Value};

/// Resolver knows how to resolve a [`Value`] from a [`FunctionContext`].
/// At their core, resolvers are responsible for taking Expressions and
/// turned them into values, but this trait allows us to abstract away
/// some of the complexity surrounding how the expression is obtained in
/// the first place.
///
/// For example, the [`Argument`] resolver takes an index and resolves the
/// corresponding argument from the [`FunctionContext`]. Resolver makes it
/// easy to (1) get the expression for a specific argument index, (2)
/// return an error if the argument is missing, and (3) resolve the expression
/// into a value.
pub trait Resolver {
    fn resolve(&self, ctx: &FunctionContext) -> ResolveResult;
}

impl Resolver for Expression {
    fn resolve(&self, ctx: &FunctionContext) -> ResolveResult {
        Value::resolve(self, ctx.ptx)
    }
}

/// Argument is a [`Resolver`] that resolves to the nth argument.
///
/// # Example
/// ```skip
/// let arg0 = ftx.resolve(Argument(0))?;
/// ```
pub(crate) struct Argument(pub usize);

impl Resolver for Argument {
    fn resolve(&self, ctx: &FunctionContext) -> ResolveResult {
        let index = self.0;
        let arg = ctx
            .args
            .get(index)
            .ok_or_else(|| ExecutionError::invalid_argument_count(index + 1, ctx.args.len()))?;
        Value::resolve(arg, ctx.ptx)
    }
}

/// A resolver for all arguments passed to a function. Each argument will be
/// resolved and then returned as a [`Value::List`]
///
/// # Example
/// ```skip
/// let args = ftx.resolve(AllArguments)?;
/// ```
pub(crate) struct AllArguments;

impl Resolver for AllArguments {
    fn resolve(&self, ctx: &FunctionContext) -> ResolveResult {
        let mut args = Vec::with_capacity(ctx.args.len());
        for arg in ctx.args.iter() {
            args.push(Value::resolve(arg, ctx.ptx)?);
        }
        Ok(Value::List(args.into()))
    }
}
