use crate::common::traits;
use crate::ExecutionError;
use std::any::Any;
use std::borrow::Cow;

pub(crate) mod bool;
pub(crate) mod bytes;
pub(crate) mod double;
#[cfg(feature = "chrono")]
pub(crate) mod duration;
pub(crate) mod int;
pub(crate) mod list;
pub(crate) mod map;
mod null;
mod optional;
pub(crate) mod string;
#[cfg(feature = "structs")]
pub(crate) mod r#struct;
#[cfg(feature = "chrono")]
pub(crate) mod timestamp;
pub(crate) mod uint;

use crate::common::traits::TraitSet;
use crate::common::value::Val;
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
    pub fn is_assignable(&self, val: &dyn Val) -> bool {
        self == val.get_type()
    }
}

impl Type {
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
    parameters: Cow::Borrowed(&[]),
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
    pub const fn simple_type(kind: Kind, name: &'static str) -> Type {
        Type {
            kind,
            parameters: Cow::Borrowed(&[]),
            runtime_type_name: Cow::Borrowed(name),
            trait_mask: 0,
        }
    }

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

    pub const fn new_unspecified_type(name: &'static str) -> Type {
        Type {
            kind: Kind::Unspecified,
            parameters: Cow::Borrowed(&[]),
            runtime_type_name: Cow::Borrowed(name),
            trait_mask: 0,
        }
    }

    pub fn new_opaque_type(name: &'static str) -> Type {
        Type {
            kind: Kind::Opaque,
            parameters: Cow::Borrowed(&[]),
            runtime_type_name: Cow::Borrowed(name),
            trait_mask: 0,
        }
    }

    pub fn new_opaque(name: String) -> Type {
        Type {
            kind: Kind::Opaque,
            parameters: Cow::Borrowed(&[]),
            runtime_type_name: Cow::Owned(name),
            trait_mask: 0,
        }
    }

    #[cfg(feature = "structs")]
    pub const fn new_struct_type(name: &'static str) -> Type {
        Type {
            kind: Kind::Struct,
            parameters: Cow::Borrowed(&[]),
            runtime_type_name: Cow::Borrowed(name),
            trait_mask: traits::FIELD_TESTER_TYPE | traits::INDEXER_TYPE,
        }
    }

    #[cfg(feature = "structs")]
    pub const fn new_struct(name: String) -> Type {
        Type {
            kind: Kind::Struct,
            parameters: Cow::Borrowed(&[]),
            runtime_type_name: Cow::Owned(name),
            trait_mask: traits::FIELD_TESTER_TYPE | traits::INDEXER_TYPE,
        }
    }

    pub fn name(&self) -> &str {
        &self.runtime_type_name
    }

    pub fn has_trait(&self, t: u16) -> bool {
        self.trait_mask & t == t
    }
}

/// Try to cast a `Box<dyn Val>` to its concrete type `T: Val`
/// Will return `Result::Ok` if the type check succeeded with the actual Box to the
/// `Box<T>`. `Result::Err` with the `Box<dyn Val>` back to the caller should the type check
/// fail.
fn cast_boxed<T: Val>(value: Box<dyn Val>) -> Result<Box<T>, Box<dyn Val>> {
    if <dyn Any>::is::<T>(&*value) {
        let temp_container = &mut Some(value);
        // SAFETY: just checked whether we are pointing to the correct type, and we can rely on
        // that check for memory safety because we have implemented Any for all types; no other
        // impls can exist as they would conflict with our impl.
        let temp_container = unsafe { &mut *(temp_container as *mut _ as *mut Option<Box<T>>) };
        return Ok(temp_container.take().unwrap());
    }
    Err(value)
}

type UnaryFn<A> = fn(&A) -> Result<Box<dyn Val>, ExecutionError>;
type BinaryFn<A, B> = fn(&A, &B) -> Result<Box<dyn Val>, ExecutionError>;

fn unary_fn<'a, A: Val>(
    args: Vec<Cow<'a, dyn Val>>,
    type_a: Type,
    func: UnaryFn<A>,
) -> Result<Cow<'a, dyn Val>, ExecutionError> {
    let arg = &args[0];
    match arg.downcast_ref::<A>() {
        None => Err(ExecutionError::UnexpectedType {
            got: arg.get_type().name().to_string(),
            want: type_a.name().to_string(),
        }),
        Some(arg) => Ok(Cow::<dyn Val>::Owned(func(arg)?)),
    }
}

fn binary_fn<'a, A: Val, B: Val>(
    args: Vec<Cow<'a, dyn Val>>,
    type_a: Type,
    type_b: Type,
    func: BinaryFn<A, B>,
) -> Result<Cow<'a, dyn Val>, ExecutionError> {
    let arg1 = &args[0];
    let arg2 = &args[1];
    match arg1.downcast_ref::<A>() {
        None => Err(ExecutionError::UnexpectedType {
            got: arg1.get_type().name().to_string(),
            want: type_a.name().to_string(),
        }),
        Some(arg1) => match arg2.downcast_ref::<B>() {
            None => Err(ExecutionError::UnexpectedType {
                got: arg2.get_type().name().to_string(),
                want: type_b.name().to_string(),
            }),
            Some(arg2) => Ok(Cow::<dyn Val>::Owned(func(arg1, arg2)?)),
        },
    }
}

fn noop<'a>(args: Vec<Cow<'a, dyn Val>>) -> Result<Cow<'a, dyn Val>, ExecutionError> {
    let mut args = args;
    let ts = args.remove(0);
    Ok(ts)
}
