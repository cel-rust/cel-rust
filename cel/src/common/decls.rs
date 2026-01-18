use crate::common::functions::{Op, Overload};
use crate::common::traits::TraitSet;
use crate::common::types::Type;
use crate::common::value::Val;
use std::collections::BTreeMap;

pub struct FunctionDecl<'a> {
    name: String,
    overloads: BTreeMap<String, OverloadDecl<'a>>,
    singleton: Overload,
}

struct OverloadDecl<'a> {
    id: String,
    arg_types: Vec<&'a Type<'a>>,
    result_type: &'a Type<'a>,
    member_function: bool,
    operand_traits: TraitSet,
    op: Op,
}

struct VariableDecl<'a, 'b> {
    name: String,
    var_type: &'a Type<'a>,
    value: &'b dyn Val,
}
