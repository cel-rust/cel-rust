use crate::common::traits::Adder;
use crate::common::types;
use crate::common::types::Type;
use std::any::Any;
use std::fmt::Debug;
use std::time::SystemTime;

#[derive(Clone, Debug, PartialEq)]
pub enum CelVal {
    Unspecified,
    Error,
    Dyn,
    Any,
    Boolean(types::Bool),
    Bytes(types::Bytes),
    Double(types::Double),
    Duration(types::Duration),
    Int(types::Int),
    List,
    Map,
    Null,
    String(types::String),
    Timestamp(types::Timestamp),
    Type,
    UInt(types::UInt),
    Unknown,
}

pub trait Val: Any + Debug {
    fn get_type(&self) -> Type<'_>;

    fn into_inner(self: Box<Self>) -> Box<dyn Any>;

    fn as_adder(&self) -> Option<&dyn Adder> {
        None
    }
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

    fn into_inner(self: Box<Self>) -> Box<dyn Any> {
        match *self {
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
