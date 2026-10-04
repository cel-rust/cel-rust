//! The signature of a function overload.
use crate::common::traits::TraitSet;
use crate::common::value::CowVal;
use crate::ExecutionError;

/// Not used: overloads are declared with
/// [`Env::add_overload`](crate::Env::add_overload), and operators dispatch
/// through the traits of [`traits`](crate::common::traits).
#[allow(dead_code)]
pub struct Overload {
    operator: String,
    operand_trait: TraitSet,
    op: Function,
}

/// A function overload. It receives its arguments as [`CowVal`]s bounded by
/// the caller's `'b` borrow and `'v` value lifetime, and may hand one of
/// them back unchanged.
///
/// A member overload, `x.f(y)`, receives its target `x` as its first
/// argument. The arguments are of the types the overload was declared for,
/// see [`Env::add_overload`](crate::Env::add_overload); the
/// [`add_overload!`](crate::add_overload) macros write the downcasts for a
/// plain Rust `fn`.
pub type Function = for<'b, 'v> fn(Vec<CowVal<'b, 'v>>) -> Result<CowVal<'b, 'v>, ExecutionError>;
