//! Function declarations: the overloads of a function, and how a call picks
//! one.
//!
//! An [`Env`](crate::Env) declares each function, `size` say, as a
//! [`FunctionDecl`] holding its overloads: one per signature, e.g.
//! `size(string)` and `size(list)`. A call picks the overload by the runtime
//! [types](crate::common::value::Val::get_type) of its arguments, see
//! [`FunctionDecl::find_overload`].
//!
//! Each overload also declares what it returns, see [`Signature`], which no
//! part of evaluation looks at.
use crate::common::functions::{Function, Op};
use crate::common::types::{Type, DYN_TYPE};
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

    /// Finds the overload a call with `args` resolves to: the one that is a
    /// member overload if `member_function` is, takes as many arguments, and
    /// whose parameter types each [accept](Type::is_assignable) the argument in
    /// its position. At most one does, as overloads
    /// [can't overlap](DeclarationError::OverlappingOverload).
    ///
    /// For a member call, `x.f(y)`, `args` starts with the target `x`.
    ///
    /// `None` as well when that overload is implemented by a closure, added
    /// with [`Env::add_overload_with`](crate::Env::add_overload_with): it has
    /// no [`Function`] to return.
    pub fn find_overload(
        &self,
        member_function: bool,
        args: &[CowVal<'_, '_>],
    ) -> Option<Function> {
        self.find_op(member_function, args)?.as_function()
    }

    /// The implementation of the overload a call with `args` resolves to,
    /// see [`find_overload`](Self::find_overload).
    pub(crate) fn find_op(&self, member_function: bool, args: &[CowVal<'_, '_>]) -> Option<&Op> {
        for overload in &self.overloads {
            if overload.member_function == member_function
                && args.len() == overload.arg_types.len()
                && overload
                    .arg_types
                    .iter()
                    .enumerate()
                    .all(|(i, t)| t.is_assignable(args[i].as_ref()))
            {
                return Some(&overload.op);
            }
        }
        None
    }

    /// The function's overloads, in the order they were declared.
    pub fn overloads(&self) -> impl ExactSizeIterator<Item = &OverloadDecl> {
        self.overloads.iter()
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
        signature: Signature,
        op: Op,
    ) -> Result<(), DeclarationError> {
        let Signature {
            args: arg_types,
            result: result_type,
        } = signature;
        if self.is_present(&id, member_function, &arg_types) {
            return Err(DeclarationError::duplicate_overload(&self.name, &id));
        }
        if let Some(overlapping) = self.find_overlapping(member_function, &arg_types) {
            return Err(DeclarationError::overlapping_overload(
                &self.name,
                &id,
                &overlapping.id,
            ));
        }
        self.overloads.push(OverloadDecl {
            id,
            arg_types,
            result_type,
            member_function,
            op,
        });
        Ok(())
    }

    fn is_present(&self, name: &str, member_function: bool, arg_types: &[Type]) -> bool {
        for overload in &self.overloads {
            if overload.id == name
                || overload.compares(member_function, arg_types, Type::is_equivalent)
            {
                return true;
            }
        }
        false
    }

    /// The overload a call could resolve to, as well as one with that signature.
    fn find_overlapping(&self, member_function: bool, arg_types: &[Type]) -> Option<&OverloadDecl> {
        self.overloads
            .iter()
            .find(|overload| overload.compares(member_function, arg_types, Type::overlaps))
    }
}

/// What an overload takes and returns, for
/// [`Env::add_overload`](crate::Env::add_overload) and the like.
///
/// The argument types are what picks the overload. The result type only
/// declares what the overload returns: nothing checks that the implementation
/// does, and overloads can't differ by it alone. It is `dyn` unless declared
/// with [`returns`](Signature::returns), which a [`Vec`] of argument types
/// converts into.
///
/// ```
/// use cel::common::decls::Signature;
/// use cel::common::types::{Type, DYN_TYPE};
/// use cel::common::value::CowVal;
/// use cel::{Env, ExecutionError};
///
/// fn identity<'b, 'v>(mut args: Vec<CowVal<'b, 'v>>) -> Result<CowVal<'b, 'v>, ExecutionError> {
///     Ok(args.remove(0))
/// }
///
/// const A: Type = Type::new_type_param("A");
///
/// let mut env = Env::default();
/// env.add_overload("same", "same_a", Signature::new(vec![A]).returns(A), identity)
///     .unwrap();
/// env.add_overload("other", "other_a", vec![A], identity).unwrap();
///
/// let result_of = |name| {
///     let overload = env.function(name).unwrap().overloads().next().unwrap();
///     overload.result_type().name().to_owned()
/// };
/// assert_eq!(result_of("same"), "A");
/// assert_eq!(result_of("other"), DYN_TYPE.name());
/// ```
#[derive(Debug, Eq, PartialEq)]
pub struct Signature {
    args: Vec<Type>,
    result: Type,
}

impl Signature {
    /// An overload taking arguments of these types, excluding the receiver of a
    /// member overload, and returning `dyn`.
    pub fn new(args: Vec<Type>) -> Signature {
        Signature {
            args,
            result: DYN_TYPE,
        }
    }

    /// Declares the overload to return `result`.
    pub fn returns(mut self, result: Type) -> Signature {
        self.result = result;
        self
    }

    /// The signature of a member overload called on a `receiver`.
    pub(crate) fn with_receiver(mut self, receiver: Type) -> Signature {
        self.args.insert(0, receiver);
        self
    }
}

impl From<Vec<Type>> for Signature {
    fn from(args: Vec<Type>) -> Signature {
        Signature::new(args)
    }
}

/// One overload of a [`FunctionDecl`]: its parameter types, what it returns, and
/// its implementation.
pub struct OverloadDecl {
    /// The overload's identifier, unique among the overloads of its
    /// function, e.g. `size_string`.
    pub id: String,
    arg_types: Vec<Type>,
    result_type: Type,
    member_function: bool,
    //operand_traits: TraitSet,
    op: Op,
}

impl OverloadDecl {
    /// The types of the arguments, in order. For a member overload, the first
    /// one is the type of the receiver.
    pub fn arg_types(&self) -> impl ExactSizeIterator<Item = &Type> {
        self.arg_types.iter()
    }

    /// The type the overload is declared to return, see [`Signature`].
    pub fn result_type(&self) -> &Type {
        &self.result_type
    }

    /// Whether the overload is called on a receiver, as in `x.f(y)`.
    pub fn is_member_function(&self) -> bool {
        self.member_function
    }

    /// Whether the overload is of that call style, takes as many arguments, and
    /// `compare` holds between its type and `arg_types`' in each position.
    fn compares(
        &self,
        member_function: bool,
        arg_types: &[Type],
        compare: fn(&Type, &Type) -> bool,
    ) -> bool {
        self.member_function == member_function
            && self.arg_types.len() == arg_types.len()
            && self
                .arg_types
                .iter()
                .zip(arg_types)
                .all(|(declared, arg)| compare(declared, arg))
    }
}

#[allow(dead_code)]
struct VariableDecl<'a, 'b> {
    name: String,
    var_type: &'a Type,
    value: &'b dyn Val,
}
