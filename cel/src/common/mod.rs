//! The expression tree, [`ast`], and CEL's type system: how values, types,
//! operators and functions fit together.
//!
//! # Values
//!
//! Every value an expression handles is a [`dyn Val`](value::Val). The
//! built-in CEL types are Rust types in [`types`], such as
//! [`CelInt`](types::CelInt) for `int` or [`CelList`](types::CelList) for
//! `list`, and an application can make any type of its own a CEL value by
//! implementing [`Val`](value::Val) for it. Values may borrow the data they
//! stand for rather than copy it, which [`value`] explains.
//!
//! # Types
//!
//! Each value reports its CEL [`Type`](types::Type): a
//! [`Kind`](types::Kind), such as `Int` or `Struct`, and a name, such as
//! `int` or `acme.User`. `type(x)` evaluates to it, as a type value, and
//! function overloads are declared for, and picked by, types. See [`types`].
//!
//! # Operators
//!
//! Operators are not functions: the interpreter evaluates `a + b` by asking
//! `a` for its [`Adder`](traits::Adder), with [`Val::as_adder`](value::Val::as_adder),
//! and handing it `b`. Each operator has such a capability, one of the traits
//! of [`traits`], and a value that doesn't hand it out doesn't support the
//! operator. Equality is [`Val::equals`](value::Val::equals). Field selection,
//! `a.f`, is an operator too: it looks `"f"` up with the value's
//! [`Indexer`](traits::Indexer).
//!
//! # Macros
//!
//! Some calls are never evaluated as calls: they are macros, rewritten into
//! other expressions when compiling. `has(a.f)` becomes a field test, and
//! `a.all(x, ..)`, `a.exists(x, ..)`, `a.map(x, ..)` and the like become
//! comprehensions. An environment expands the macros of
//! [`Env::stdlib`](crate::Env::stdlib), and those added with
//! [`Env::add_macro`](crate::Env::add_macro), by extensions included, e.g. the
//! lists extension's `sortBy`; see [`Macro`](crate::parser::Macro).
//!
//! # Functions
//!
//! Everything else, `size(x)`, `x.startsWith(y)`, `int(x)`, is a function,
//! and is looked up by name when called:
//!
//! 1. among the overloads declared on the [`Env`](crate::Env), such as those
//!    of [`Env::stdlib`](crate::Env::stdlib). Each overload is a
//!    [`Function`](functions::Function) declared for argument types, and the
//!    first whose types accept the arguments is called, see
//!    [`FunctionDecl::find_overload`](decls::FunctionDecl::find_overload);
//! 2. then among the functions added to the [`Context`](crate::Context) with
//!    [`Context::add_function`](crate::Context::add_function), which aren't
//!    declared for types and are called with any arguments.
//!
//! A call that targets a value, `x.f(y)`, looks for member overloads, with
//! `x` as their first argument.
//!
//! # Adding a type
//!
//! An application type becomes a CEL value by implementing
//! [`Val`](value::Val), which the example there shows: its CEL type, how to
//! copy it, equality, and the capabilities it has, say an
//! [`Indexer`](traits::Indexer) to give it fields. Bound to a variable with
//! [`Context::add_variable_as_val`](crate::Context::add_variable_as_val), it
//! can then be used in expressions. Registering its type with
//! [`Env::add_type`](crate::Env::add_type) lets expressions name the type,
//! and declaring overloads for it, with
//! [`add_overload!`](crate::add_overload), gives it functions.
/// The expression tree: what the parser produces, and the interpreter evaluates.
pub mod ast;
pub mod decls;
pub mod functions;
pub mod traits;
pub mod types;
pub mod value;
