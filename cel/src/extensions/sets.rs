//! The sets extension library: `sets.contains`, `sets.equivalent` and
//! `sets.intersects`, which treat lists as sets, following cel-go's
//! `ext.Sets()`.

use super::iterable;
use crate::common::types::{CelBool, LIST_TYPE};
use crate::common::value::{CowVal, Val};
use crate::{DeclarationError, Env, ExecutionError, Library};

/// Adds the sets extension library to `env`: [`Sets`].
pub fn extension(env: &mut Env) -> Result<(), DeclarationError> {
    env.add_library(Sets::default())
}

/// The sets extension library, `cel.lib.ext.sets`: registers its overloads on
/// the [`Env`] it is added to.
///
/// The lists compare as sets: neither the order of their elements nor
/// duplicates matter. Elements compare with CEL's `==`, so `1`, `1u` and
/// `1.0` are one element, but `[1, 2]` and `[2, 1]` are two.
#[derive(Clone, Debug, Default)]
#[non_exhaustive]
pub struct Sets {}

impl Library for Sets {
    fn name(&self) -> Option<&str> {
        Some("cel.lib.ext.sets")
    }

    fn register(&self, env: &mut Env) -> Result<(), DeclarationError> {
        // Registered by hand, as the macro would downcast the arguments to
        // `CelList`s: these take any `list`, like the stdlib's list overloads.
        env.add_overload(
            "sets.contains",
            "sets.contains(list,list)",
            vec![LIST_TYPE, LIST_TYPE],
            contains,
        )?;
        env.add_overload(
            "sets.equivalent",
            "sets.equivalent(list,list)",
            vec![LIST_TYPE, LIST_TYPE],
            equivalent,
        )?;
        env.add_overload(
            "sets.intersects",
            "sets.intersects(list,list)",
            vec![LIST_TYPE, LIST_TYPE],
            intersects,
        )?;
        Ok(())
    }
}

/// Whether every element of the second list is in the first.
fn contains<'b, 'v>(args: Vec<CowVal<'b, 'v>>) -> Result<CowVal<'b, 'v>, ExecutionError> {
    match args.as_slice() {
        [list, sublist] => to_bool(is_subset(sublist.as_ref(), list.as_ref())),
        _ => Err(ExecutionError::invalid_argument_count(2, args.len())),
    }
}

/// Whether each list contains every element of the other.
fn equivalent<'b, 'v>(args: Vec<CowVal<'b, 'v>>) -> Result<CowVal<'b, 'v>, ExecutionError> {
    match args.as_slice() {
        [lhs, rhs] => {
            let (lhs, rhs) = (lhs.as_ref(), rhs.as_ref());
            to_bool(Ok(is_subset(lhs, rhs)? && is_subset(rhs, lhs)?))
        }
        _ => Err(ExecutionError::invalid_argument_count(2, args.len())),
    }
}

/// Whether the lists share at least one element.
fn intersects<'b, 'v>(args: Vec<CowVal<'b, 'v>>) -> Result<CowVal<'b, 'v>, ExecutionError> {
    match args.as_slice() {
        [lhs, rhs] => {
            let mut items = iterable(lhs.as_ref())?.iter();
            while let Some(item) = items.next() {
                if has(rhs.as_ref(), item)? {
                    return to_bool(Ok(true));
                }
            }
            to_bool(Ok(false))
        }
        _ => Err(ExecutionError::invalid_argument_count(2, args.len())),
    }
}

fn to_bool<'b, 'v>(result: Result<bool, ExecutionError>) -> Result<CowVal<'b, 'v>, ExecutionError> {
    Ok(CowVal::owned(CelBool::from(result?)))
}

/// Whether every element of `sublist` is in `list`.
fn is_subset(sublist: &dyn Val, list: &dyn Val) -> Result<bool, ExecutionError> {
    let mut items = iterable(sublist)?.iter();
    while let Some(item) = items.next() {
        if !has(list, item)? {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Whether `list` has an element equal to `value`.
fn has(list: &dyn Val, value: &dyn Val) -> Result<bool, ExecutionError> {
    let mut items = iterable(list)?.iter();
    while let Some(item) = items.next() {
        if value.equals(item) {
            return Ok(true);
        }
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    use crate::common::types::{CelInt, CelUInt};
    use crate::extensions::tests::OtherList;
    use crate::{Context, Env, Value};
    use std::sync::Arc;

    /// The overloads take any `list`, not just a `CelList`.
    #[test]
    fn takes_lists_that_are_not_cel_lists() {
        let mut env = Env::stdlib();
        assert_eq!(env.add_extension(crate::extensions::sets), Ok(()));
        assert_eq!(env.add_extension(crate::extensions::sets), Ok(()));
        let mut ctx = Context::with_env(Arc::new(env));
        ctx.add_variable_as_val(
            "ints",
            Box::new(OtherList(vec![CelInt::from(1), CelInt::from(2)])),
        );
        ctx.add_variable_as_val("uints", Box::new(OtherList(vec![CelUInt::from(2)])));
        for (expr, expected) in [
            ("sets.contains(ints, uints)", true),
            ("sets.contains(uints, ints)", false),
            ("sets.equivalent(ints, [2.0, 1, 1u])", true),
            ("sets.equivalent(uints, ints)", false),
            ("sets.intersects(uints, ints)", true),
            ("sets.intersects(ints, [3])", false),
        ] {
            let program = ctx.env().compile(expr).expect("This must be valid CEL");
            assert_eq!(program.execute(&ctx), Ok(Value::Bool(expected)), "{expr}");
        }
    }
}
