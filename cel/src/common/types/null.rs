use crate::common::types::Type;
use crate::common::value::Val;

#[derive(Clone, Debug)]
pub struct Null;

impl Val for Null {
    fn get_type(&self) -> Type<'_> {
        super::NULL_TYPE
    }

    fn clone_as_boxed(&self) -> Box<dyn Val> {
        Box::new(Null)
    }
}
