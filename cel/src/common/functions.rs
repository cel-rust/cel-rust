use crate::common::traits::TraitSet;
use crate::common::value::Val;
use crate::ExecutionError;
use std::borrow::Cow;

#[allow(dead_code)]
pub struct Overload {
    operator: String,
    operand_trait: TraitSet,
    op: Function,
}

pub type Function = for<'a> fn(Vec<Cow<'a, dyn Val>>) -> Result<Cow<'a, dyn Val>, ExecutionError>;
