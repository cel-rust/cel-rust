use crate::common::value::CelVal;

pub type UnaryOp = dyn Fn(&CelVal) -> CelVal;

pub type BinaryOp = dyn Fn(&CelVal, &CelVal) -> CelVal;

pub type FunctionOp = dyn Fn(&[CelVal]) -> CelVal;
