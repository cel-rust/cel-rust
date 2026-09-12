use crate::common::types::CelInt;
use crate::common::value::{CowVal, Val};
use crate::ExecutionError;
use std::cmp::Ordering;

pub type TraitSet = u16;

/// ADDER_TYPE types provide a '+' operator overload.
pub const ADDER_TYPE: TraitSet = 1;

/// COMPARER_TYPE types support ordering comparisons '<', '<=', '>', '>='.
pub const COMPARER_TYPE: TraitSet = ADDER_TYPE << 1;

/// CONTAINER_TYPE types support 'in' operations.
pub const CONTAINER_TYPE: TraitSet = COMPARER_TYPE << 1;

/// DIVIDER_TYPE types support '/' operations.
pub const DIVIDER_TYPE: TraitSet = CONTAINER_TYPE << 1;

/// FIELD_TESTER_TYPE types support the detection of field value presence.
pub const FIELD_TESTER_TYPE: TraitSet = DIVIDER_TYPE << 1;

/// INDEXER_TYPE types support index access with dynamic values.
pub const INDEXER_TYPE: TraitSet = FIELD_TESTER_TYPE << 1;

/// ITERABLE_TYPE types can be iterated over in comprehensions.
pub const ITERABLE_TYPE: TraitSet = INDEXER_TYPE << 1;

/// ITERATOR_TYPE types support iterator semantics.
pub const ITERATOR_TYPE: TraitSet = ITERABLE_TYPE << 1;

/// MATCHER_TYPE types support pattern matching via 'matches' method.
pub const MATCHER_TYPE: TraitSet = ITERATOR_TYPE << 1;

/// MODDER_TYPE types support modulus operations '%'
pub const MODDER_TYPE: TraitSet = MATCHER_TYPE << 1;

/// MULTIPLIER_TYPE types support '*' operations.
pub const MULTIPLIER_TYPE: TraitSet = MODDER_TYPE << 1;

/// NEGATOR_TYPE types support either negation via '!' or '-'
pub const NEGATOR_TYPE: TraitSet = MULTIPLIER_TYPE << 1;

/// RECEIVER_TYPE types support dynamic dispatch to instance methods.
pub const RECEIVER_TYPE: TraitSet = NEGATOR_TYPE << 1;

/// SIZER_TYPE types support the size() method.
pub const SIZER_TYPE: TraitSet = RECEIVER_TYPE << 1;

/// SUBTRACTOR_TYPE types support '-' operations.
pub const SUBTRACTOR_TYPE: TraitSet = SIZER_TYPE << 1;

/// FOLDABLE_TYPE types support comprehensions v2 macros which iterate over (key, value) pairs.
pub const FOLDABLE_TYPE: TraitSet = SUBTRACTOR_TYPE << 1;

// Operator traits produce values bounded by a caller-chosen `'v` that `Self`
// outlives, so a borrowing operand yields a result borrowing the same data
// rather than a `'static` copy.

pub trait Adder {
    fn add<'b, 'v>(&'b self, _rhs: &(dyn Val + 'v)) -> Result<CowVal<'b, 'v>, ExecutionError>
    where
        Self: 'v;
}

pub trait Comparer {
    fn compare(&self, _rhs: &dyn Val) -> Result<Ordering, ExecutionError>;
}

pub trait Container {
    fn contains(&self, _value: &dyn Val) -> Result<bool, ExecutionError>;
}

pub trait Divider {
    fn div<'b, 'v>(&'b self, _rhs: &(dyn Val + 'v)) -> Result<CowVal<'b, 'v>, ExecutionError>
    where
        Self: 'v;
}

pub trait Iterable {
    fn iter<'b, 'v>(&'b self) -> Box<dyn Iterator<'b, 'v> + 'b>
    where
        Self: 'v;
}

pub trait Iterator<'b, 'v> {
    fn next(&mut self) -> Option<&'b (dyn Val + 'v)>;
}

pub trait Modder {
    fn modulo<'b, 'v>(&'b self, _rhs: &(dyn Val + 'v)) -> Result<CowVal<'b, 'v>, ExecutionError>
    where
        Self: 'v;
}

pub trait Multiplier {
    fn mul<'b, 'v>(&'b self, _rhs: &(dyn Val + 'v)) -> Result<CowVal<'b, 'v>, ExecutionError>
    where
        Self: 'v;
}

pub trait Negator {
    fn negate<'v>(&self) -> Result<Box<dyn Val + 'v>, ExecutionError>
    where
        Self: 'v;
}

pub trait Sizer {
    fn size(&self) -> CelInt;
}

pub trait Subtractor {
    fn sub<'b, 'v>(&'b self, _rhs: &(dyn Val + 'v)) -> Result<CowVal<'b, 'v>, ExecutionError>
    where
        Self: 'v;
}

pub trait Zeroer {
    fn is_zero_value(&self) -> bool;
}

pub trait Indexer {
    fn get<'b, 'v>(&'b self, _idx: &dyn Val) -> Result<CowVal<'b, 'v>, ExecutionError>
    where
        Self: 'v;

    fn steal<'v>(self: Box<Self>, _idx: &dyn Val) -> Result<Box<dyn Val + 'v>, ExecutionError>
    where
        Self: 'v;
}

pub(crate) mod adapter {
    use crate::{common::value::CowVal, ExecutionError};

    pub fn sizer_size<'b, 'v>(args: Vec<CowVal<'b, 'v>>) -> Result<CowVal<'b, 'v>, ExecutionError> {
        let target = &args[0];
        match target.as_sizer() {
            None => Err(ExecutionError::UnexpectedType {
                got: target.get_type().name().to_owned(),
                want: "missing trait Sizer".to_owned(),
            }),
            Some(sizer) => Ok(CowVal::owned(sizer.size())),
        }
    }
}
