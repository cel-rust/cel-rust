use crate::common::traits::Zeroer;
use crate::common::types::Type;
use crate::common::value::Val;

#[derive(Clone, Debug, Default)]
pub struct Null;

impl Val for Null {
    fn get_type(&self) -> &Type {
        &super::NULL_TYPE
    }

    fn equals(&self, other: &dyn Val) -> bool {
        other.downcast_ref::<Null>().is_some()
    }

    fn as_zeroer(&self) -> Option<&dyn Zeroer> {
        Some(self)
    }

    fn clone_as_boxed(&self) -> Box<dyn Val> {
        Box::new(Null)
    }
}

impl Zeroer for Null {
    fn is_zero_value(&self) -> bool {
        true
    }
}
