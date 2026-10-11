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
///
/// A `fn` captures nothing: an overload that needs to, say the settings of
/// the library declaring it, is a closure, added with
/// [`Env::add_overload_with`](crate::Env::add_overload_with).
pub type Function = for<'b, 'v> fn(Vec<CowVal<'b, 'v>>) -> Result<CowVal<'b, 'v>, ExecutionError>;

/// A closure implementing an overload, as a [`Function`] does.
pub(crate) type Closure =
    dyn for<'b, 'v> Fn(Vec<CowVal<'b, 'v>>) -> Result<CowVal<'b, 'v>, ExecutionError> + Send + Sync;

/// The implementation of an overload: a [`Function`], or a closure, which
/// holds what it captured.
pub(crate) enum Op {
    Function(Function),
    Closure(Box<Closure>),
}

impl Op {
    #[inline]
    pub(crate) fn call<'b, 'v>(
        &self,
        args: Vec<CowVal<'b, 'v>>,
    ) -> Result<CowVal<'b, 'v>, ExecutionError> {
        match self {
            Op::Function(op) => op(args),
            Op::Closure(op) => op(args),
        }
    }

    /// The function, of an overload implemented by one.
    pub(crate) fn as_function(&self) -> Option<Function> {
        match self {
            Op::Function(op) => Some(*op),
            Op::Closure(_) => None,
        }
    }
}
