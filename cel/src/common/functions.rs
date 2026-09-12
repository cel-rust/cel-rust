use crate::common::traits::TraitSet;
use crate::common::value::CowVal;
use crate::ExecutionError;

#[allow(dead_code)]
pub struct Overload {
    operator: String,
    operand_trait: TraitSet,
    op: Function,
}

/// A function overload. It receives its arguments as [`CowVal`]s bounded by
/// the caller's `'b` borrow and `'v` value lifetime, and may hand one of
/// them back unchanged.
pub type Function = for<'b, 'v> fn(Vec<CowVal<'b, 'v>>) -> Result<CowVal<'b, 'v>, ExecutionError>;
