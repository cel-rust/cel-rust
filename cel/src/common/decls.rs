use crate::common::functions::Function;
use crate::common::types::Type;
use crate::common::value::Val;

pub struct FunctionDecl<'a> {
    pub name: String,
    overloads: Vec<OverloadDecl<'a>>,
}

impl<'a> FunctionDecl<'a> {
    pub fn new(name: &str) -> FunctionDecl<'a> {
        FunctionDecl {
            name: name.to_string(),
            overloads: Vec::default(),
        }
    }

    pub fn find_overload(&self, member_function: bool, arg_types: &[Type<'a>]) -> Option<Function> {
        for overload in &self.overloads {
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

    fn is_present(&self, name: &str, member_function: bool, arg_types: &[Type<'a>]) -> bool {
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
