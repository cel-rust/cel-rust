//! Function declarations: the overloads of a function, and how a call picks
//! one.
//!
//! An [`Env`](crate::Env) declares each function, `size` say, as a
//! [`FunctionDecl`] holding its overloads: one per signature, e.g.
//! `size(string)` and `size(list)`. A call picks the overload by the runtime
//! [types](crate::common::value::Val::get_type) of its arguments, see
//! [`FunctionDecl::find_overload`].
use crate::common::functions::Function;
use crate::common::types::Type;
use crate::common::value::{CowVal, Val};
use crate::DeclarationError;

/// A function and its overloads, as declared with
/// [`Env::add_overload`](crate::Env::add_overload) and
/// [`Env::add_member_overload`](crate::Env::add_member_overload).
pub struct FunctionDecl {
    /// The function's name, as called in expressions: `size`, `_==_`,
    /// `lowerAscii`.
    pub name: String,
    overloads: Vec<OverloadDecl>,
}

impl FunctionDecl {
    /// A function of that name, with no overloads.
    pub fn new(name: &str) -> FunctionDecl {
        FunctionDecl {
            name: name.to_string(),
            overloads: Vec::default(),
        }
    }

    /// Finds the overload a call with `args` resolves to: the first one, in
    /// the order they were declared, that is a member overload if
    /// `member_function` is, takes as many arguments, and whose parameter
    /// types each [accept](Type::is_assignable) the argument in its position.
    ///
    /// For a member call, `x.f(y)`, `args` starts with the target `x`.
    pub fn find_overload(
        &self,
        member_function: bool,
        args: &[CowVal<'_, '_>],
    ) -> Option<Function> {
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

    pub(crate) fn has_overload(&self, member_function: bool) -> bool {
        self.overloads
            .iter()
            .any(|overload| overload.member_function == member_function)
    }

    pub(crate) fn add_overload(
        &mut self,
        id: String,
        member_function: bool,
        arg_types: Vec<Type>,
        op: Function,
    ) -> Result<(), DeclarationError> {
        if self.is_present(&id, member_function, &arg_types) {
            return Err(DeclarationError::duplicate_overload(&self.name, &id));
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

/// One overload of a [`FunctionDecl`]: its parameter types, and its
/// implementation.
pub struct OverloadDecl {
    /// The overload's identifier, unique among the overloads of its
    /// function, e.g. `size_string`.
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
