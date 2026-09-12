use crate::common::traits;
#[cfg(feature = "chrono")]
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
mod null;
pub(crate) mod optional;
pub(crate) mod string;
#[cfg(feature = "structs")]
pub(crate) mod r#struct;
#[cfg(feature = "chrono")]
pub(crate) mod timestamp;
pub(crate) mod type_val;
pub(crate) mod uint;

use crate::common::traits::TraitSet;
use crate::common::value::{Builtin, BuiltinRef, Val};
#[cfg(feature = "chrono")]
use crate::common::value::{CowVal, StaticVal};
pub use bool::Bool as CelBool;
pub use bytes::Bytes as CelBytes;
pub use double::Double as CelDouble;
#[cfg(feature = "chrono")]
pub use duration::Duration as CelDuration;
pub use int::Int as CelInt;
pub use list::DefaultList as CelList;
pub use map::DefaultMap as CelMap;
pub use map::Key as CelMapKey;
pub use null::Null as CelNull;
pub use optional::Optional as CelOptional;
#[cfg(feature = "structs")]
pub use r#struct::Struct as CelStruct;
pub use string::String as CelString;
#[cfg(feature = "chrono")]
pub use timestamp::Timestamp as CelTimestamp;
pub use type_val::CelType;
pub use uint::UInt as CelUInt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Kind {
    Unspecified,
    Error,
    Dyn,
    Any,
    Boolean,
    Bytes,
    Double,
    Duration,
    Int,
    List,
    Map,
    NullType,
    Opaque,
    String,
    Struct,
    Timestamp,
    Type,
    TypeParam,
    UInt,
    Unknown,
}

/// Represents a CEL type.
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

pub const ANY_TYPE: Type = Type {
    kind: Kind::Any,
    parameters: Cow::Borrowed(&[]),
    runtime_type_name: Cow::Borrowed("google.protobuf.Any"),
    trait_mask: traits::FIELD_TESTER_TYPE | traits::INDEXER_TYPE,
};

pub const BOOL_TYPE: Type = Type {
    kind: Kind::Boolean,
    parameters: Cow::Borrowed(&[]),
    runtime_type_name: Cow::Borrowed("bool"),
    trait_mask: traits::COMPARER_TYPE | traits::NEGATOR_TYPE,
};

pub const BYTES_TYPE: Type = Type {
    kind: Kind::Bytes,
    parameters: Cow::Borrowed(&[]),
    runtime_type_name: Cow::Borrowed("bytes"),
    trait_mask: traits::ADDER_TYPE | traits::COMPARER_TYPE | traits::SIZER_TYPE,
};

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

pub const DYN_TYPE: Type = {
    let kind = Kind::Dyn;
    Type {
        kind,
        parameters: Cow::Borrowed(&[]),
        runtime_type_name: Cow::Borrowed("dyn"),
        trait_mask: 0,
    }
};

pub const ERROR_TYPE: Type = Type::simple_type(Kind::Error, "error");

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

pub const NULL_TYPE: Type = {
    let kind = Kind::NullType;
    Type {
        kind,
        parameters: Cow::Borrowed(&[]),
        runtime_type_name: Cow::Borrowed("null_type"),
        trait_mask: 0,
    }
};

pub const OPTIONAL_TYPE: Type = Type {
    kind: Kind::Opaque,
    parameters: Cow::Borrowed(&[Cow::Borrowed(&DYN_TYPE)]),
    runtime_type_name: Cow::Borrowed("optional_type"),
    trait_mask: 0,
};

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

pub const TIMESTAMP_TYPE: Type = Type {
    kind: Kind::Timestamp,
    parameters: Cow::Borrowed(&[]),
    runtime_type_name: Cow::Borrowed("google.protobuf.Timestamp"),
    trait_mask: traits::ADDER_TYPE
        | traits::COMPARER_TYPE
        | traits::RECEIVER_TYPE
        | traits::SUBTRACTOR_TYPE,
};

pub const TYPE_TYPE: Type = Type::simple_type(Kind::Type, "type");

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

pub const UNKNOWN_TYPE: Type = Type::simple_type(Kind::Unknown, "unknown");

impl Type {
    /// Creates a new simple type with the given kind and name.
    pub const fn simple_type(kind: Kind, name: &'static str) -> Type {
        Type {
            kind,
            parameters: Cow::Borrowed(&[]),
            runtime_type_name: Cow::Borrowed(name),
            trait_mask: 0,
        }
    }

    /// Creates a new list type with the given element type.
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

    /// Creates a new unspecified type with the given name.
    pub const fn new_unspecified_type(name: &'static str) -> Type {
        Type {
            kind: Kind::Unspecified,
            parameters: Cow::Borrowed(&[]),
            runtime_type_name: Cow::Borrowed(name),
            trait_mask: 0,
        }
    }

    /// Creates a new opaque type with the given name.
    pub fn new_opaque_type<S: Into<Cow<'static, str>>>(name: S) -> Type {
        Type {
            kind: Kind::Opaque,
            parameters: Cow::Borrowed(&[]),
            runtime_type_name: name.into(),
            trait_mask: 0,
        }
    }

    /// Creates a new struct type with the given name.
    #[cfg(feature = "structs")]
    pub const fn new_struct_type(name: &'static str) -> Type {
        Type {
            kind: Kind::Struct,
            parameters: Cow::Borrowed(&[]),
            runtime_type_name: Cow::Borrowed(name),
            trait_mask: traits::FIELD_TESTER_TYPE | traits::INDEXER_TYPE,
        }
    }

    /// Creates a new struct type with the given owned name.
    #[cfg(feature = "structs")]
    pub const fn new_struct(name: String) -> Type {
        Type {
            kind: Kind::Struct,
            parameters: Cow::Borrowed(&[]),
            runtime_type_name: Cow::Owned(name),
            trait_mask: traits::FIELD_TESTER_TYPE | traits::INDEXER_TYPE,
        }
    }

    /// Returns the name of the type.
    pub fn name(&self) -> &str {
        &self.runtime_type_name
    }

    /// Returns true if the type has the given trait.
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
            #[cfg(feature = "structs")]
            Builtin::Struct(s) => Box::new(s),
        }
    }
}

#[cfg(feature = "chrono")]
type UnaryFn<A> = fn(&A) -> Result<Box<dyn Val>, ExecutionError>;

/// Applies `func` to the single `'static` argument of type `A`.
#[cfg(feature = "chrono")]
fn unary_fn<'b, 'v, A: StaticVal>(
    args: Vec<CowVal<'b, 'v>>,
    type_a: Type,
    func: UnaryFn<A>,
) -> Result<CowVal<'b, 'v>, ExecutionError> {
    let arg = &args[0];
    match arg.downcast_ref::<A>() {
        None => Err(ExecutionError::UnexpectedType {
            got: arg.get_type().name().to_string(),
            want: type_a.name().to_string(),
        }),
        Some(arg) => Ok(CowVal::Owned(func(arg)?)),
    }
}

/// Applies `func` to the single string argument.
#[cfg(feature = "chrono")]
fn string_fn<'b, 'v>(
    args: Vec<CowVal<'b, 'v>>,
    func: fn(&str) -> Result<Box<dyn Val>, ExecutionError>,
) -> Result<CowVal<'b, 'v>, ExecutionError> {
    let arg = &args[0];
    match arg.downcast_ref::<CelString>() {
        None => Err(ExecutionError::UnexpectedType {
            got: arg.get_type().name().to_string(),
            want: STRING_TYPE.name().to_string(),
        }),
        Some(arg) => Ok(CowVal::Owned(func(arg.inner())?)),
    }
}

#[cfg(feature = "chrono")]
fn noop<'b, 'v>(mut args: Vec<CowVal<'b, 'v>>) -> Result<CowVal<'b, 'v>, ExecutionError> {
    Ok(args.remove(0))
}
