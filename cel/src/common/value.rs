use crate::common::traits::Adder;
use crate::common::types;
use crate::common::types::Type;
use std::any::Any;
use std::fmt::Debug;
use std::time::{Duration, SystemTime};

#[derive(Clone, Debug, PartialEq)]
pub enum CelVal {
    Unspecified,
    Error,
    Dyn,
    Any,
    Boolean(bool),
    Bytes(Vec<u8>),
    Double(f64),
    Duration(Duration),
    Int(i64),
    List,
    Map,
    Null,
    String(String),
    Timestamp(SystemTime),
    Type,
    UInt(u64),
    Unknown,
}

pub trait Val: Any + Debug {
    fn get_type(&self) -> Type<'_>;

    fn into_inner(self) -> Box<dyn Any>;

    fn as_adder(&self) -> Option<&dyn Adder> {
        None
    }

    fn clone_as_boxed(&self) -> Box<dyn Val>;
}

impl dyn Val {
    pub fn downcast_ref<T: Val>(&self) -> Option<&T> {
        (self as &dyn Any).downcast_ref::<T>()
    }
}

impl Val for CelVal {
    fn get_type(&self) -> Type<'_> {
        match self {
            CelVal::Unspecified => Type::new_unspecified_type("unspecified"),
            CelVal::Error => types::ERROR_TYPE,
            CelVal::Dyn => types::DYN_TYPE,
            CelVal::Any => types::ANY_TYPE,
            CelVal::Boolean(_) => types::BOOL_TYPE,
            CelVal::Bytes(_) => types::BYTES_TYPE,
            CelVal::Double(_) => types::DOUBLE_TYPE,
            CelVal::Duration(_) => types::DURATION_TYPE,
            CelVal::Int(_) => types::INT_TYPE,
            CelVal::List => types::LIST_TYPE,
            CelVal::Map => types::MAP_TYPE,
            CelVal::Null => types::NULL_TYPE,
            CelVal::String(_) => types::STRING_TYPE,
            CelVal::Timestamp(_) => types::TIMESTAMP_TYPE,
            CelVal::Type => types::TYPE_TYPE,
            CelVal::UInt(_) => types::UINT_TYPE,
            CelVal::Unknown => types::UNKNOWN_TYPE,
        }
    }

    fn into_inner(self) -> Box<dyn Any> {
        match self {
            CelVal::Unspecified => todo!(),
            CelVal::Error => todo!(),
            CelVal::Dyn => todo!(),
            CelVal::Any => todo!(),
            CelVal::Boolean(b) => Box::new(b),
            CelVal::Bytes(b) => Box::new(b),
            CelVal::Double(d) => Box::new(d),
            CelVal::Duration(d) => Box::new(d),
            CelVal::Int(i) => Box::new(i),
            CelVal::List => todo!(),
            CelVal::Map => todo!(),
            CelVal::Null => todo!(),
            CelVal::String(s) => Box::new(s),
            CelVal::Timestamp(t) => Box::new(t),
            CelVal::Type => todo!(),
            CelVal::UInt(u) => Box::new(u),
            CelVal::Unknown => todo!(),
        }
    }

    fn clone_as_boxed(&self) -> Box<dyn Val> {
        Box::new(self.clone())
    }
}

impl ToOwned for dyn Val {
    type Owned = Box<dyn Val>;

    fn to_owned(&self) -> Self::Owned {
        self.clone_as_boxed()
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
        let s1 = types::String::new("cel");
        let s2 = types::String::new("cel");
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
            CelVal::Error => todo!(),
            CelVal::Dyn => todo!(),
            CelVal::Any => todo!(),
            CelVal::Boolean(b) => {
                let b: types::Bool = b.into();
                Box::new(b)
            }
            CelVal::Bytes(bytes) => {
                let bytes: types::Bytes = bytes.into();
                Box::new(bytes)
            }
            CelVal::Double(d) => {
                let d: types::Double = d.into();
                Box::new(d)
            }
            CelVal::Duration(d) => {
                let d: types::Duration = d.into();
                Box::new(d)
            }
            CelVal::Int(i) => {
                let i: types::Int = i.into();
                Box::new(i)
            }
            CelVal::List => todo!(),
            CelVal::Map => todo!(),
            CelVal::Null => Box::new(types::Null),
            CelVal::String(s) => {
                let s: types::String = s.into();
                Box::new(s)
            }
            CelVal::Timestamp(t) => {
                let t: types::Timestamp = t.into();
                Box::new(t)
            }
            CelVal::Type => todo!(),
            CelVal::UInt(u) => {
                let u: types::UInt = u.into();
                Box::new(u)
            }
            CelVal::Unknown => todo!(),
        }
    }
}
