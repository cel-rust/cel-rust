use crate::parser::Expression;
use crate::{FunctionContext, ResolveResult, Value};

/// Resolver knows how to resolve a [`Value`] from a [`FunctionContext`].
/// At their core, resolvers are responsible for taking Expressions and
/// turned them into values, but this trait allows us to abstract away
/// some of the complexity surrounding how the expression is obtained in
/// the first place.
pub trait Resolver {
    fn resolve(&self, ctx: &FunctionContext) -> ResolveResult;
}

impl Resolver for Expression {
    fn resolve(&self, ctx: &FunctionContext) -> ResolveResult {
        Value::resolve(self, ctx.ptx)
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
            args.push(Value::try_from(arg.as_ref())?);
        }
        Ok(Value::List(args.into()))
    }
}
