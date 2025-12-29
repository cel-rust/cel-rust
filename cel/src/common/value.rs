use crate::common::traits::{
    Adder, Comparer, Container, Divider, Indexer, Lister, Modder, Multiplier, Subtractor,
};
use crate::common::types;
use crate::common::types::Type;
use std::any::Any;
use std::fmt::Debug;

#[derive(Debug)]
pub enum CelVal {
    Unspecified,
    Error(types::CelErr),
    Dyn,
    Any,
    Boolean(types::CelBool),
    Bytes(types::CelBytes),
    Double(types::CelDouble),
    Duration(types::CelDuration),
    Int(types::CelInt),
    List(Box<dyn Lister>),
    Map,
    Null,
    String(types::CelString),
    Timestamp(types::CelTimestamp),
    Type,
    UInt(types::CelUInt),
    Unknown(Box<dyn Val>),
}

pub trait Val: Any + Debug {
    fn get_type(&self) -> Type<'_>;

    fn as_adder(&self) -> Option<&dyn Adder> {
        None
    }

    fn as_comparer(&self) -> Option<&dyn Comparer> {
        None
    }

    fn as_container(&self) -> Option<&dyn Container> {
        None
    }

    fn as_divider(&self) -> Option<&dyn Divider> {
        None
    }

    fn as_indexer(&self) -> Option<&dyn Indexer> {
        None
    }

    fn into_indexer(self: Box<Self>) -> Option<Box<dyn Indexer>> {
        None
    }

    fn as_modder(&self) -> Option<&dyn Modder> {
        None
    }

    fn as_multiplier(&self) -> Option<&dyn Multiplier> {
        None
    }

    fn as_subtractor(&self) -> Option<&dyn Subtractor> {
        None
    }

    fn equals(&self, _other: &dyn Val) -> bool {
        false
    }

    fn clone_as_boxed(&self) -> Box<dyn Val>;
}

impl dyn Val {
    pub fn downcast_ref<T: Val>(&self) -> Option<&T> {
        <dyn Any>::downcast_ref::<T>(self)
    }
}

impl ToOwned for dyn Val {
    type Owned = Box<dyn Val>;

    fn to_owned(&self) -> Self::Owned {
        self.clone_as_boxed()
    }
}

impl PartialEq for dyn Val {
    fn eq(&self, other: &Self) -> bool {
        self.equals(other)
    }
}

#[cfg(test)]
mod test {
    use crate::common::types;
    use crate::common::value::Val;
    use std::borrow::Cow;

    fn test(val: &dyn Val) -> bool {
        val.get_type() == types::STRING_TYPE
    }

    #[test]
    fn test_cow() {
        let s1 = types::CelString::from("cel");
        let s2 = types::CelString::from("cel");
        let b: Box<dyn Val> = Box::new(s1);
        let cow: Cow<dyn Val> = Cow::Owned(b);
        let borrowed: Cow<dyn Val> = Cow::Borrowed(&s2);
        assert!(test(borrowed.as_ref()));
        assert!(test(cow.as_ref()));
        assert!(test(borrowed.to_owned().as_ref()));
    }
}

impl CelVal {
    pub fn into_val(self) -> Box<dyn Val> {
        match self {
            CelVal::Unspecified => todo!(),
            CelVal::Error(_err) => todo!(),
            CelVal::Dyn => todo!(),
            CelVal::Any => todo!(),
            CelVal::Boolean(b) => Box::new(b),
            CelVal::Bytes(bytes) => Box::new(bytes),
            CelVal::Double(d) => Box::new(d),
            CelVal::Duration(d) => Box::new(d),
            CelVal::Int(i) => Box::new(i),
            CelVal::List(_) => todo!(),
            CelVal::Map => todo!(),
            CelVal::Null => Box::new(types::CelNull),
            CelVal::String(s) => Box::new(s),
            CelVal::Timestamp(t) => Box::new(t),
            CelVal::Type => todo!(),
            CelVal::UInt(u) => Box::new(u),
            CelVal::Unknown(_) => todo!(),
        }
    }
}
