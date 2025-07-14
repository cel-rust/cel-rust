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
    Opaque,
    String(String),
    Object,
    Timestamp(SystemTime),
    Type,
    UInt(u64),
    Unknown,
}
