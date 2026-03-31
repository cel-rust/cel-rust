use crate::common::functions::Function;
use crate::common::types::Type;
use crate::common::value::Val;
use std::collections::btree_map::Entry::{Occupied, Vacant};
use std::collections::BTreeMap;

pub struct FunctionDecl<'a> {
    pub name: String,
    overloads: BTreeMap<String, OverloadDecl<'a>>,
}

impl<'a> FunctionDecl<'a> {
    pub fn new(name: &str) -> FunctionDecl<'a> {
        FunctionDecl {
            name: name.to_string(),
            overloads: BTreeMap::default(),
        }
    }

    pub fn find_overload(&self, member_function: bool, arg_types: &[Type<'a>]) -> Option<Function> {
        for overload in self.overloads.values() {
            if overload.member_function == member_function && overload.arg_types == arg_types {
                return Some(overload.op);
            }
        }
        None
    }

    pub(crate) fn add_overload(
        &mut self,
        id: String,
        member_function: bool,
        arg_types: Vec<Type<'a>>,
        op: Function,
    ) -> Result<(), ()> {
        match self.overloads.entry(id) {
            Vacant(vacant_entry) => {
                let id = vacant_entry.key().clone();
                vacant_entry.insert(OverloadDecl {
                    id,
                    arg_types,
                    member_function,
                    op,
                });
                Ok(())
            }
            Occupied(_) => Err(()),
        }
    }
}

pub struct OverloadDecl<'a> {
    pub id: String,
    arg_types: Vec<Type<'a>>,
    //result_type: &'a Type<'a>,
    member_function: bool,
    //operand_traits: TraitSet,
    op: Function,
}

#[allow(dead_code)]
struct VariableDecl<'a, 'b> {
    name: String,
    var_type: &'a Type<'a>,
    value: &'b dyn Val,
}
