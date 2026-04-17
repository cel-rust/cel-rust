use crate::common::types::CelInt;
use crate::common::value::Val;
use crate::ExecutionError;
use std::any::Any;
use std::borrow::Cow;
use std::cmp::Ordering;
use std::fmt::Debug;

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

pub trait Adder {
    fn add<'a>(&'a self, _rhs: &dyn Val) -> Result<Cow<'a, dyn Val>, ExecutionError>;
}

pub trait Comparer {
    fn compare(&self, _rhs: &dyn Val) -> Result<Ordering, ExecutionError>;
}

pub trait Container {
    fn contains(&self, _value: &dyn Val) -> Result<bool, ExecutionError>;
}

pub trait Divider {
    fn div<'a>(&self, _rhs: &'a dyn Val) -> Result<Cow<'a, dyn Val>, ExecutionError>;
}

pub trait Iterable {
    fn iter<'a>(&'a self) -> Box<dyn Iterator<'a> + 'a>;
}

pub trait Iterator<'a> {
    fn next(&mut self) -> Option<&'a dyn Val>;
}

pub trait Modder {
    fn modulo<'a>(&self, _rhs: &'a dyn Val) -> Result<Cow<'a, dyn Val>, ExecutionError>;
}

pub trait Multiplier {
    fn mul<'a>(&self, _rhs: &'a dyn Val) -> Result<Cow<'a, dyn Val>, ExecutionError>;
}

pub trait Negator {
    fn negate(&self) -> Result<Box<dyn Val>, ExecutionError>;
}

pub trait Sizer {
    fn size(&self) -> CelInt;
}

pub trait Subtractor {
    fn sub<'a>(&'a self, _rhs: &'_ dyn Val) -> Result<Cow<'a, dyn Val>, ExecutionError>;
}

pub trait Zeroer {
    fn is_zero_value(&self) -> bool;
}

pub trait Indexer {
    fn get<'a>(&'a self, _idx: &dyn Val) -> Result<Cow<'a, dyn Val>, ExecutionError>;

    fn steal(self: Box<Self>, _idx: &dyn Val) -> Result<Box<dyn Val>, ExecutionError>;
}

pub trait Lister: Debug + Any {
    fn as_indexer(&self) -> &dyn Indexer;
}

pub(crate) mod adapter {
    use std::borrow::Cow;

    use crate::{common::value::Val, ExecutionError};

    pub fn sizer_size<'a>(args: Vec<Cow<'a, dyn Val>>) -> Result<Cow<'a, dyn Val>, ExecutionError> {
        let target = &args[0];
        match target.as_sizer() {
            None => Err(ExecutionError::UnexpectedType {
                got: target.get_type().name().to_owned(),
                want: "missing trait Sizer".to_owned(),
            }),
            Some(sizer) => Ok(Cow::<dyn Val>::Owned(Box::new(sizer.size()))),
        }
    }
}
