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

pub trait Val {
    fn get_type(&self) -> Type;

    fn into_inner(self) -> Box<dyn Any>;
}
