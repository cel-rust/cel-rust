// Resolver knows how to resolve a [`Value`] from a [`FunctionContext`].
// At their core, resolvers are responsible for taking Expressions and
// turned them into values, but this trait allows us to abstract away
// some of the complexity surrounding how the expression is obtained in
// the first place.
//
// For example, the [`Argument`] resolver takes an index and resolves the
// corresponding argument from the [`FunctionContext`]. Resolver makes it
// easy to (1) get the expression for a specific argument index, (2)
// return an error if the argument is missing, and (3) resolve the expression
// into a value.
// pub trait Resolver {
//     fn resolve<'a, 'vars: 'a>(self, ctx: &FunctionContext<'vars>) -> ResolveResult<'a>;
// }

// Argument is a [`Resolver`] that resolves to the nth argument.
//
// pub(crate) struct Argument(pub usize);
//
// impl Resolver for Argument {
//     fn resolve<'a, 'vars: 'a>(self, ctx: &FunctionContext<'vars>) -> ResolveResult<'a> {
//         let index = self.0;
//         let arg = ctx
//             .args
//             .get(index)
//             .ok_or(ExecutionError::invalid_argument_count(
//                 index + 1,
//                 ctx.args.len(),
//             ))?;
//         Value::resolve(arg, ctx.ptx, ctx.variables.clone())
//     }
// }

// A resolver for all arguments passed to a function. Each argument will be
// resolved and then returned as a [`Value::List`]
//
//
// pub(crate) struct AllArguments;
//
// impl Resolver for AllArguments {
//     fn resolve<'a, 'vars: 'a>(self, ctx: &FunctionContext<'vars>) -> ResolveResult<'a> {
//         let mut args = Vec::with_capacity(ctx.args.len());
//         for arg in ctx.args.iter() {
//             args.push(Value::resolve(arg, ctx.ptx, ctx.variables.clone())?);
//         }
//         Ok(Value::List(ListValue::PartiallyOwned(args.into())))
//     }
// }
