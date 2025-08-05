use crate::common::functions;
use crate::common::types::Type;
use std::collections::HashMap;

#[allow(dead_code)]
pub struct FunctionDecl<'a> {
    name: String,
    overloads: HashMap<String, OverloadDecl<'a>>,
}

#[allow(dead_code)]
pub struct OverloadDecl<'a> {
    id: String,
    arg_types: Vec<Type<'a>>,
    result_type: Type<'a>,
    member_function: bool,
    // non_strict: bool,
    // operand_trait: u16,
    unary_op: Option<Box<functions::UnaryOp>>,
    binary_op: Option<Box<functions::BinaryOp>>,
    function_op: Option<Box<functions::FunctionOp>>,
}
