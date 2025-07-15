use crate::common::types;
use crate::common::types::Type;
use std::time::{Duration, SystemTime};

#[derive(Clone, Debug, PartialEq)]
pub enum Val {
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

impl Val {
    pub fn get_type(&self) -> Type {
        match self {
            Val::Unspecified => types::UNKNOWN_TYPE,
            Val::Error => types::ERROR_TYPE,
            Val::Dyn => types::DYN_TYPE,
            Val::Any => types::ANY_TYPE,
            Val::Boolean(_) => types::BOOL_TYPE,
            Val::Bytes(_) => types::BYTES_TYPE,
            Val::Double(_) => types::DOUBLE_TYPE,
            Val::Duration(_) => types::DURATION_TYPE,
            Val::Int(_) => types::INT_TYPE,
            Val::List => types::LIST_TYPE,
            Val::Map => types::MAP_TYPE,
            Val::Null => types::NULL_TYPE,
            Val::String(_) => types::STRING_TYPE,
            Val::Timestamp(_) => types::TIMESTAMP_TYPE,
            Val::Type => types::TYPE_TYPE,
            Val::UInt(_) => types::UINT_TYPE,
            Val::Unknown => types::UNKNOWN_TYPE,
        }
    }
}
