use crate::common::traits::TraitSet;
use crate::common::value::Val;
use crate::ExecutionError;
use std::borrow::Cow;

pub struct Overload {
    operator: String,
    operand_trait: TraitSet,
    op: Op,
}

// todo, probably don't want 'static here, but... for now
pub(crate) enum Op {
    Unary(UnaryOp<'static>),
    Binary(BinaryOp<'static>),
    Function(Function<'static>),
}

type UnaryOp<'a> = fn(Cow<dyn Val>) -> Result<Cow<'a, dyn Val>, ExecutionError>;
type BinaryOp<'a> = fn(Cow<dyn Val>, Cow<dyn Val>) -> Result<Cow<'a, dyn Val>, ExecutionError>;
type Function<'a> = fn(&[Cow<dyn Val>]) -> Result<Cow<'a, dyn Val>, ExecutionError>;
