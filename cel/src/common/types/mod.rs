//! CEL's types: [`Type`] and [`Kind`], and the Rust types of the built-in
//! values.
//!
//! # Types, kinds and type values
//!
//! A [`Type`] describes a CEL type: its [`Kind`], its name, and, for `list` and
//! `map`, its parameters, the types of the elements. Every value reports its
//! type with [`Val::get_type`], and the type is:
//!
//! * what function overloads are declared for and picked by: an overload
//!   applies to arguments its parameter types [accept](Type::is_assignable);
//! * what `type(x)` evaluates to, as a [`CelType`]: a *type value*, which only
//!   holds the type's name. A type registered with
//!   [`Env::add_type`](crate::Env::add_type) can be named in an expression,
//!   so `type(x) == int` compares two type values;
//! * what the conversion of a value to [`Value`](crate::Value) goes by, through
//!   its kind.
//!
//! The kind is the coarse category, the name tells types of a kind apart, e.g.
//! two struct types, or two of an application's own types.
//!
//! A type doesn't decide which operators its values support: each value does,
//! through its `as_*` accessors, see [`traits`]. The [`TraitSet`] a type
//! carries only describes them.
//!
//! # Built-in types
//!
//! | CEL type | [`Type`] | [`Kind`] | Rust type | Operators |
//! |---|---|---|---|---|
//! | `bool` | [`BOOL_TYPE`] | `Boolean` | [`CelBool`] | `<` |
//! | `int` | [`INT_TYPE`] | `Int` | [`CelInt`] | `+ - * / %`, unary `-`, `<` |
//! | `uint` | [`UINT_TYPE`] | `UInt` | [`CelUInt`] | `+ - * / %`, `<` |
//! | `double` | [`DOUBLE_TYPE`] | `Double` | [`CelDouble`] | `+ - * /`, unary `-`, `<` |
//! | `string` | [`STRING_TYPE`] | `String` | [`CelString`] | `+`, `<` |
//! | `bytes` | [`BYTES_TYPE`] | `Bytes` | [`CelBytes`] | `+`, `<` |
//! | `list` | [`LIST_TYPE`] | `List` | [`CelList`] | `+`, `in`, `[]`, comprehensions |
//! | `map` | [`MAP_TYPE`] | `Map` | [`CelMap`] | `in`, `[]`, `.f`, comprehensions |
//! | `null_type` | [`NULL_TYPE`] | `NullType` | [`CelNull`] | |
//! | `type` | [`TYPE_TYPE`] | `Type` | [`CelType`] | |
//! | `optional_type` | [`OPTIONAL_TYPE`] | `Opaque` | [`CelOptional`] | |
//! | `google.protobuf.Duration` | [`DURATION_TYPE`] | `Duration` | `CelDuration` | `+ -`, `<` |
//! | `google.protobuf.Timestamp` | [`TIMESTAMP_TYPE`] | `Timestamp` | `CelTimestamp` | `+ -`, `<` |
//! | a struct, e.g. `acme.User` | its own | `Struct` | [`CelStruct`] | `.f`, `has(.f)`, `[]` |
//!
//! `<` stands for all four comparisons. All of them support `==` and `!=`.
//! `!`, `&&`, `||` and `? :` take `bool`s. `CelDuration` and `CelTimestamp`
//! need the `chrono` feature.
//!
//! Everything else, `size()`, conversions such as `int(x)`, member functions
//! such as `startsWith` and `hasValue`, is a function overload the standard
//! library declares on [`Env::stdlib`](crate::Env::stdlib), for the types above:
//! the `optional` ones only while optional support is on, see [`CelOptional`].
//!
//! # Example
//!
//! ```
//! use cel::common::types::{CelDouble, CelInt, CelType, Kind, DYN_TYPE, INT_TYPE};
//! use cel::common::value::Val;
//!
//! let one = CelInt::from(1);
//! assert_eq!(one.get_type(), &INT_TYPE);
//! assert_eq!(one.get_type().kind(), Kind::Int);
//! assert_eq!(one.get_type().name(), "int");
//!
//! // what overload resolution checks, argument by argument
//! assert!(INT_TYPE.is_assignable(&one));
//! assert!(!INT_TYPE.is_assignable(&CelDouble::from(1.0)));
//! assert!(DYN_TYPE.is_assignable(&CelDouble::from(1.0)));
//!
//! // `type(1) == int`: two type values, equal by name
//! assert!(CelType::from(one.get_type()).equals(&CelType::new("int")));
//! ```
use crate::common::traits;
use crate::ExecutionError;
use std::borrow::Cow;

pub(crate) mod bool;
pub(crate) mod bytes;
pub(crate) mod double;
#[cfg(feature = "chrono")]
pub(crate) mod duration;
pub(crate) mod r#dyn;
pub(crate) mod int;
pub(crate) mod list;
pub(crate) mod map;
pub(crate) mod null;
pub(crate) mod optional;
pub(crate) mod string;
pub(crate) mod r#struct;
#[cfg(feature = "chrono")]
pub(crate) mod timestamp;
pub(crate) mod type_val;
pub(crate) mod uint;

use crate::common::traits::TraitSet;
use crate::common::value::CowVal;
use crate::common::value::{Builtin, BuiltinRef, Val};
pub use bool::Bool as CelBool;
pub use bytes::Bytes as CelBytes;
pub use double::Double as CelDouble;
#[cfg(feature = "chrono")]
pub use duration::Duration as CelDuration;
pub use int::Int as CelInt;
pub use list::DefaultList as CelList;
#[doc(hidden)]
pub use list::MutableList;
pub use map::DefaultMap as CelMap;
pub use map::Key as CelMapKey;
pub use null::Null as CelNull;
pub use optional::Optional as CelOptional;
pub use r#struct::Struct as CelStruct;
pub use string::String as CelString;
#[cfg(feature = "chrono")]
pub use timestamp::Timestamp as CelTimestamp;
pub use type_val::CelType;
pub use uint::UInt as CelUInt;

/// The category of a [`Type`], following cel-go's `types.Kind`.
///
/// Converting a value to a [`Value`](crate::Value) goes by its kind, as do
/// the interpreter's few kind checks, e.g. a field missing from a `map` is
/// reported as a missing key. A few kinds only mirror cel-go: no value of this
/// crate has them.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Kind {
    /// A type of no CEL kind in particular: what
    /// [`Type::new_unspecified_type`] makes, for an application's own types.
    Unspecified,
    /// The type of errors, [`ERROR_TYPE`]. No value has it: errors are
    /// [`ExecutionError`]s.
    Error,
    /// `dyn`, [`DYN_TYPE`]: any type. An overload parameter of this kind
    /// accepts any argument.
    Dyn,
    /// `google.protobuf.Any`, [`ANY_TYPE`]. No value has it.
    Any,
    /// `bool`.
    Boolean,
    /// `bytes`.
    Bytes,
    /// `double`.
    Double,
    /// `google.protobuf.Duration`.
    Duration,
    /// `int`.
    Int,
    /// `list`, which has one parameter, the type of its elements.
    List,
    /// `map`, which has two parameters, the types of its keys and values.
    Map,
    /// `null_type`, the type of `null`.
    NullType,
    /// A type whose values are only known to the functions that handle them,
    /// such as `optional_type`. [`Type::new_opaque_type`] makes one.
    Opaque,
    /// `string`.
    String,
    /// A struct type, e.g. `acme.User`: values made of named fields, see
    /// [`CelStruct`].
    Struct,
    /// `google.protobuf.Timestamp`.
    Timestamp,
    /// `type`, the type of type values such as `int` or `type(x)`, see
    /// [`CelType`].
    Type,
    /// A type parameter, as in a generic function's declaration. No type of
    /// this crate has it.
    TypeParam,
    /// `uint`.
    UInt,
    /// The type of unknown values, [`UNKNOWN_TYPE`], which cel-go uses for
    /// partial evaluation. No value of this crate has it.
    Unknown,
}

/// A CEL type: a [`Kind`], a name, and, for `list` and `map`, parameters.
///
/// Two types are equal when all of those are, along with their [`TraitSet`].
/// The built-in types are constants of this module, e.g. [`INT_TYPE`]; an
/// application makes its own with [`Type::new_unspecified_type`],
/// [`Type::new_opaque_type`] or [`Type::new_struct_type`], and registers them
/// with [`Env::add_type`](crate::Env::add_type) for expressions to name them.
/// See the [module documentation](self).
#[derive(Debug, Eq, PartialEq)]
pub struct Type {
    kind: Kind,
    parameters: Cow<'static, [Cow<'static, Type>]>,
    runtime_type_name: Cow<'static, str>,
    trait_mask: TraitSet,
}

impl ToOwned for Type {
    type Owned = Type;

    fn to_owned(&self) -> Self::Owned {
        Self {
            kind: self.kind,
            parameters: self.parameters.clone(),
            runtime_type_name: self.runtime_type_name.clone(),
            trait_mask: self.trait_mask,
        }
    }
}

impl Type {
    /// Returns true if the given value can be assigned to this type.
    ///
    /// That is how an overload is picked: it applies when each of its
    /// parameter types accepts the argument in its position. A type accepts a
    /// value whose [type](Val::get_type) equals it, [`DYN_TYPE`] accepts any
    /// value, and an opaque type with parameters accepts the values its first
    /// parameter accepts: [`OPTIONAL_TYPE`], `optional_type(dyn)`, accepts any
    /// value.
    ///
    /// Parameters are compared, not checked element by element: every
    /// [`CelList`] has type [`LIST_TYPE`], `list(dyn)`, so a parameter of type
    /// `list(int)` accepts none of them.
    pub fn is_assignable(&self, val: &dyn Val) -> bool {
        if self == val.get_type() {
            true
        } else {
            match self.kind() {
                Kind::Dyn => true,
                Kind::Opaque => self
                    .parameters
                    .first()
                    .is_some_and(|t| t.is_assignable(val)),
                _ => false,
            }
        }
    }
}

impl Type {
    /// Returns the kind of the type.
    pub fn kind(&self) -> Kind {
        self.kind
    }
}

/// `google.protobuf.Any`. No value has this type.
pub const ANY_TYPE: Type = Type {
    kind: Kind::Any,
    parameters: Cow::Borrowed(&[]),
    runtime_type_name: Cow::Borrowed("google.protobuf.Any"),
    trait_mask: traits::FIELD_TESTER_TYPE | traits::INDEXER_TYPE,
};

/// `bool`, the type of [`CelBool`].
pub const BOOL_TYPE: Type = Type {
    kind: Kind::Boolean,
    parameters: Cow::Borrowed(&[]),
    runtime_type_name: Cow::Borrowed("bool"),
    trait_mask: traits::COMPARER_TYPE | traits::NEGATOR_TYPE,
};

/// `bytes`, the type of [`CelBytes`].
pub const BYTES_TYPE: Type = Type {
    kind: Kind::Bytes,
    parameters: Cow::Borrowed(&[]),
    runtime_type_name: Cow::Borrowed("bytes"),
    trait_mask: traits::ADDER_TYPE | traits::COMPARER_TYPE | traits::SIZER_TYPE,
};

/// `double`, the type of [`CelDouble`].
pub const DOUBLE_TYPE: Type = Type {
    kind: Kind::Double,
    parameters: Cow::Borrowed(&[]),
    runtime_type_name: Cow::Borrowed("double"),
    trait_mask: traits::ADDER_TYPE
        | traits::COMPARER_TYPE
        | traits::DIVIDER_TYPE
        | traits::MULTIPLIER_TYPE
        | traits::NEGATOR_TYPE
        | traits::SUBTRACTOR_TYPE,
};

/// `google.protobuf.Duration`, the type of `CelDuration`.
pub const DURATION_TYPE: Type = Type {
    kind: Kind::Duration,
    parameters: Cow::Borrowed(&[]),
    runtime_type_name: Cow::Borrowed("google.protobuf.Duration"),
    trait_mask: traits::ADDER_TYPE
        | traits::COMPARER_TYPE
        | traits::NEGATOR_TYPE
        | traits::RECEIVER_TYPE
        | traits::SUBTRACTOR_TYPE,
};

/// `dyn`: as an overload's parameter type, it accepts any value. No value of
/// this crate has this type, but an application's may.
pub const DYN_TYPE: Type = {
    let kind = Kind::Dyn;
    Type {
        kind,
        parameters: Cow::Borrowed(&[]),
        runtime_type_name: Cow::Borrowed("dyn"),
        trait_mask: 0,
    }
};

/// `error`. No value has this type: errors are [`ExecutionError`]s.
pub const ERROR_TYPE: Type = Type::simple_type(Kind::Error, "error");

/// `int`, the type of [`CelInt`].
pub const INT_TYPE: Type = Type {
    kind: Kind::Int,
    parameters: Cow::Borrowed(&[]),
    runtime_type_name: Cow::Borrowed("int"),
    trait_mask: traits::ADDER_TYPE
        | traits::COMPARER_TYPE
        | traits::DIVIDER_TYPE
        | traits::MODDER_TYPE
        | traits::MULTIPLIER_TYPE
        | traits::NEGATOR_TYPE
        | traits::SUBTRACTOR_TYPE,
};

/// `list(dyn)`, the type of every [`CelList`], whatever its elements.
pub const LIST_TYPE: Type = {
    Type {
        kind: Kind::List,
        parameters: Cow::Borrowed(&[Cow::Borrowed(&DYN_TYPE)]),
        runtime_type_name: Cow::Borrowed("list"),
        trait_mask: traits::ADDER_TYPE
            | traits::CONTAINER_TYPE
            | traits::INDEXER_TYPE
            | traits::ITERABLE_TYPE
            | traits::SIZER_TYPE,
    }
};

/// `map(dyn, dyn)`, the type of every [`CelMap`], whatever its entries.
pub const MAP_TYPE: Type = {
    Type {
        kind: Kind::Map,
        parameters: Cow::Borrowed(&[Cow::Borrowed(&DYN_TYPE), Cow::Borrowed(&DYN_TYPE)]),
        runtime_type_name: Cow::Borrowed("map"),
        trait_mask: traits::CONTAINER_TYPE
            | traits::INDEXER_TYPE
            | traits::ITERABLE_TYPE
            | traits::SIZER_TYPE,
    }
};

/// `null_type`, the type of [`CelNull`], i.e. of `null`.
pub const NULL_TYPE: Type = {
    let kind = Kind::NullType;
    Type {
        kind,
        parameters: Cow::Borrowed(&[]),
        runtime_type_name: Cow::Borrowed("null_type"),
        trait_mask: 0,
    }
};

/// `optional_type(dyn)`, the type of every [`CelOptional`], whatever its
/// value. As an overload's parameter type, it accepts any value, see
/// [`Type::is_assignable`].
pub const OPTIONAL_TYPE: Type = Type {
    kind: Kind::Opaque,
    parameters: Cow::Borrowed(&[Cow::Borrowed(&DYN_TYPE)]),
    runtime_type_name: Cow::Borrowed("optional_type"),
    trait_mask: 0,
};

/// `string`, the type of [`CelString`].
pub const STRING_TYPE: Type = Type {
    kind: Kind::String,
    parameters: Cow::Borrowed(&[]),
    runtime_type_name: Cow::Borrowed("string"),
    trait_mask: traits::ADDER_TYPE
        | traits::COMPARER_TYPE
        | traits::MATCHER_TYPE
        | traits::RECEIVER_TYPE
        | traits::SIZER_TYPE,
};

/// `google.protobuf.Timestamp`, the type of `CelTimestamp`.
pub const TIMESTAMP_TYPE: Type = Type {
    kind: Kind::Timestamp,
    parameters: Cow::Borrowed(&[]),
    runtime_type_name: Cow::Borrowed("google.protobuf.Timestamp"),
    trait_mask: traits::ADDER_TYPE
        | traits::COMPARER_TYPE
        | traits::RECEIVER_TYPE
        | traits::SUBTRACTOR_TYPE,
};

/// `type`, the type of type values, [`CelType`]: of `int`, or of `type(x)`.
pub const TYPE_TYPE: Type = Type::simple_type(Kind::Type, "type");

/// `uint`, the type of [`CelUInt`].
pub const UINT_TYPE: Type = Type {
    kind: Kind::UInt,
    parameters: Cow::Borrowed(&[]),
    runtime_type_name: Cow::Borrowed("uint"),
    trait_mask: traits::ADDER_TYPE
        | traits::COMPARER_TYPE
        | traits::DIVIDER_TYPE
        | traits::MODDER_TYPE
        | traits::MULTIPLIER_TYPE
        | traits::SUBTRACTOR_TYPE,
};

/// `unknown`, which cel-go uses for partial evaluation. No value of this
/// crate has this type.
pub const UNKNOWN_TYPE: Type = Type::simple_type(Kind::Unknown, "unknown");

impl Type {
    /// Creates a new simple type with the given kind and name.
    ///
    /// It has no parameters and an empty [`TraitSet`].
    pub const fn simple_type(kind: Kind, name: &'static str) -> Type {
        Type {
            kind,
            parameters: Cow::Borrowed(&[]),
            runtime_type_name: Cow::Borrowed(name),
            trait_mask: 0,
        }
    }

    /// Creates a new list type with the given element type.
    ///
    /// No [`CelList`] has this type unless the element type is [`DYN_TYPE`]:
    /// see [`Type::is_assignable`].
    pub fn new_list_type(param: &'static [Cow<Type>; 1]) -> Type {
        Type {
            kind: Kind::List,
            parameters: Cow::Borrowed(param),
            runtime_type_name: Cow::Borrowed("list"),
            trait_mask: traits::ADDER_TYPE
                | traits::CONTAINER_TYPE
                | traits::INDEXER_TYPE
                | traits::ITERABLE_TYPE
                | traits::SIZER_TYPE,
        }
    }

    /// Creates a new map type with the given key and value types.
    ///
    /// No [`CelMap`] has this type unless both are [`DYN_TYPE`]: see
    /// [`Type::is_assignable`].
    pub fn new_map_type(param: &'static [Cow<Type>; 2]) -> Type {
        Type {
            kind: Kind::Map,
            parameters: Cow::Borrowed(param),
            runtime_type_name: Cow::Borrowed("map"),
            trait_mask: traits::CONTAINER_TYPE
                | traits::INDEXER_TYPE
                | traits::ITERABLE_TYPE
                | traits::SIZER_TYPE,
        }
    }

    /// Creates a new unspecified type with the given name, of
    /// [`Kind::Unspecified`]: the type for an application's own values,
    /// as in the example of [`Val`].
    pub const fn new_unspecified_type(name: &'static str) -> Type {
        Type {
            kind: Kind::Unspecified,
            parameters: Cow::Borrowed(&[]),
            runtime_type_name: Cow::Borrowed(name),
            trait_mask: 0,
        }
    }

    /// Creates a new opaque type with the given name, of [`Kind::Opaque`] and
    /// with no parameters.
    ///
    /// Like an unspecified type, it suits an application's own values, which
    /// only its own functions handle; the name can be computed at runtime.
    pub fn new_opaque_type<S: Into<Cow<'static, str>>>(name: S) -> Type {
        Type {
            kind: Kind::Opaque,
            parameters: Cow::Borrowed(&[]),
            runtime_type_name: name.into(),
            trait_mask: 0,
        }
    }

    /// Creates a new struct type with the given name, of [`Kind::Struct`].
    ///
    /// It is the type of the [`CelStruct`]s of that name.
    pub const fn new_struct_type(name: &'static str) -> Type {
        Type {
            kind: Kind::Struct,
            parameters: Cow::Borrowed(&[]),
            runtime_type_name: Cow::Borrowed(name),
            trait_mask: traits::FIELD_TESTER_TYPE | traits::INDEXER_TYPE,
        }
    }

    /// Creates a new struct type with the given owned name, see
    /// [`Type::new_struct_type`].
    pub const fn new_struct(name: String) -> Type {
        Type {
            kind: Kind::Struct,
            parameters: Cow::Borrowed(&[]),
            runtime_type_name: Cow::Owned(name),
            trait_mask: traits::FIELD_TESTER_TYPE | traits::INDEXER_TYPE,
        }
    }

    /// Returns the name of the type, as `type(x)` reports it: `int`, `list`,
    /// `acme.User`. Parameters are not part of it.
    pub fn name(&self) -> &str {
        &self.runtime_type_name
    }

    /// Returns true if the type has the given trait.
    ///
    /// `t` is one of the `*_TYPE` constants of [`traits`], or several or-ed
    /// together. The set only describes the type, mirroring cel-go: the
    /// interpreter doesn't consult it, and a value can support an operator
    /// its type's set lacks, or the reverse. Ask the value's `as_*` accessors
    /// instead, e.g. [`Val::as_adder`].
    pub fn has_trait(&self, t: u16) -> bool {
        self.trait_mask & t == t
    }
}

/// Moves a built-in value out of its box without copying it.
///
/// Hands the box back untouched when the value is not one of the built-in
/// types that [`Val::into_builtin`] covers.
pub(crate) fn into_builtin<'v>(value: Box<dyn Val + 'v>) -> Result<Builtin<'v>, Box<dyn Val + 'v>> {
    if matches!(value.as_builtin(), BuiltinRef::Other) {
        return Err(value);
    }
    // `as_builtin` and `into_builtin` are implemented together on every
    // built-in type, so a value that answered `as_builtin` answers here.
    Ok(value
        .into_builtin()
        .expect("`as_builtin` and `into_builtin` must agree"))
}

impl<'v> Builtin<'v> {
    /// Boxes the value back up.
    pub(crate) fn into_boxed(self) -> Box<dyn Val + 'v> {
        match self {
            Builtin::String(s) => Box::new(s),
            Builtin::Bytes(b) => Box::new(b),
            Builtin::List(l) => Box::new(l),
            Builtin::Map(m) => Box::new(m),
            Builtin::Optional(o) => Box::new(o),
            Builtin::Struct(s) => Box::new(s),
        }
    }
}

/// The identity overload: hands the argument straight back, keeping a borrow
/// borrowed rather than cloning the value out of it.
fn noop<'b, 'v>(mut args: Vec<CowVal<'b, 'v>>) -> Result<CowVal<'b, 'v>, ExecutionError> {
    Ok(args.remove(0))
}
