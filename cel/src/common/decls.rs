use std::borrow::Cow;

use crate::common::functions::Function;
use crate::common::types::Type;
use crate::common::value::Val;

pub struct FunctionDecl {
    pub name: String,
    overloads: Vec<OverloadDecl>,
}

impl FunctionDecl {
    pub fn new(name: &str) -> FunctionDecl {
        FunctionDecl {
            name: name.to_string(),
            overloads: Vec::default(),
        }
    }

    pub fn find_overload(&self, member_function: bool, args: &[Cow<dyn Val>]) -> Option<Function> {
        for overload in &self.overloads {
            if overload.member_function == member_function
                && args.len() == overload.arg_types.len()
                && overload
                    .arg_types
                    .iter()
                    .enumerate()
                    .all(|(i, t)| t.is_assignable(args[i].as_ref()))
            {
                return Some(overload.op);
            }
        }
        None
    }

    pub(crate) fn add_overload(
        &mut self,
        id: String,
        member_function: bool,
        arg_types: Vec<Type>,
        op: Function,
    ) -> Result<(), ()> {
        if self.is_present(&id, member_function, &arg_types) {
            return Err(());
        }
        self.overloads.push(OverloadDecl {
            id,
            arg_types,
            member_function,
            op,
        });
        Ok(())
    }

    fn is_present(&self, name: &str, member_function: bool, arg_types: &[Type]) -> bool {
        for overload in &self.overloads {
            if overload.id == name
                || (overload.member_function == member_function && overload.arg_types == arg_types)
            {
                return true;
            }
        }
        false
    }
}

pub struct OverloadDecl {
    pub id: String,
    arg_types: Vec<Type>,
    //result_type: &'a Type<'a>,
    member_function: bool,
    //operand_traits: TraitSet,
    op: Function,
}

#[allow(dead_code)]
struct VariableDecl<'a, 'b> {
    name: String,
    var_type: &'a Type,
    value: &'b dyn Val,
}
