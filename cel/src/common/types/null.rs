use crate::common::traits::Zeroer;
use crate::common::types::Type;
use crate::common::value::{StaticVal, Val};
use std::any::Any;

/// CEL's `null`, of type [`NULL_TYPE`](super::NULL_TYPE), `null_type`.
///
/// It is only equal to `null`, and supports no operator.
#[derive(Clone, Copy, Debug, Default)]
pub struct Null;

impl Val for Null {
    fn get_type(&self) -> &Type {
        <Self as Val>::cel_type()
    }

    fn cel_type() -> &'static Type {
        &super::NULL_TYPE
    }

    fn equals(&self, other: &dyn Val) -> bool {
        other.downcast_ref::<Null>().is_some()
    }

    fn as_zeroer(&self) -> Option<&dyn Zeroer> {
        Some(self)
    }

    fn clone_as_boxed<'v>(&self) -> Box<dyn Val + 'v>
    where
        Self: 'v,
    {
        Box::new(Null)
    }

    fn as_any(&self) -> Option<&dyn Any> {
        Some(self)
    }
}

impl StaticVal for Null {}

impl Zeroer for Null {
    fn is_zero_value(&self) -> bool {
        true
    }
}

pub(crate) fn stdlib(env: &mut crate::Env) {
    env.add_type(crate::common::types::NULL_TYPE)
        .expect("Must be unique");
}
