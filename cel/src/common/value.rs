use crate::common::traits::{Adder, Comparer, Divider, Indexer, Lister, Multiplier, Subtractor};
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

    fn as_divider(&self) -> Option<&dyn Divider> {
        None
    }

    fn as_indexer(&self) -> Option<&dyn Indexer> {
        None
    }

    fn into_indexer(self: Box<Self>) -> Option<Box<dyn Indexer>> {
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
        let s1 = types::CelString::new("cel");
        let s2 = types::CelString::new("cel");
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
            CelVal::Boolean(b) => {
                let b: types::CelBool = b.into();
                Box::new(b)
            }
            CelVal::Bytes(bytes) => {
                let bytes: types::CelBytes = bytes.into();
                Box::new(bytes)
            }
            CelVal::Double(d) => {
                let d: types::CelDouble = d.into();
                Box::new(d)
            }
            CelVal::Duration(d) => {
                let d: types::CelDuration = d.into();
                Box::new(d)
            }
            CelVal::Int(i) => {
                let i: types::CelInt = i.into();
                Box::new(i)
            }
            CelVal::List(_) => todo!(),
            CelVal::Map => todo!(),
            CelVal::Null => Box::new(types::CelNull),
            CelVal::String(s) => {
                let s: types::CelString = s.into();
                Box::new(s)
            }
            CelVal::Timestamp(t) => {
                let t: types::CelTimestamp = t.into();
                Box::new(t)
            }
            CelVal::Type => todo!(),
            CelVal::UInt(u) => {
                let u: types::CelUInt = u.into();
                Box::new(u)
            }
            CelVal::Unknown(_) => todo!(),
        }
    }
}
