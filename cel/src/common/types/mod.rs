use crate::common::traits;
use std::any::Any;

pub(crate) mod bool;
mod bytes;
mod double;
#[cfg(feature = "chrono")]
mod duration;
mod int;
mod list;
mod map;
mod null;
mod optional;
mod string;
#[cfg(feature = "chrono")]
mod timestamp;
mod uint;

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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Type<'a> {
    kind: Kind,
    parameters: &'a [&'a Type<'a>],
    runtime_type_name: &'a str,
    trait_mask: u16,
}

pub const ANY_TYPE: Type = Type {
    kind: Kind::Any,
    parameters: &[],
    runtime_type_name: "google.protobuf.Any",
    trait_mask: traits::FIELD_TESTER_TYPE | traits::INDEXER_TYPE,
};

pub const BOOL_TYPE: Type = Type {
    kind: Kind::Boolean,
    parameters: &[],
    runtime_type_name: "bool",
    trait_mask: traits::COMPARER_TYPE | traits::NEGATOR_TYPE,
};

pub const BYTES_TYPE: Type = Type {
    kind: Kind::Bytes,
    parameters: &[],
    runtime_type_name: "bytes",
    trait_mask: traits::ADDER_TYPE | traits::COMPARER_TYPE | traits::SIZER_TYPE,
};

pub const DOUBLE_TYPE: Type = Type {
    kind: Kind::Double,
    parameters: &[],
    runtime_type_name: "double",
    trait_mask: traits::ADDER_TYPE
        | traits::COMPARER_TYPE
        | traits::DIVIDER_TYPE
        | traits::MULTIPLIER_TYPE
        | traits::NEGATOR_TYPE
        | traits::SUBTRACTOR_TYPE,
};

pub const DURATION_TYPE: Type = Type {
    kind: Kind::Duration,
    parameters: &[],
    runtime_type_name: "google.protobuf.Duration",
    trait_mask: traits::ADDER_TYPE
        | traits::COMPARER_TYPE
        | traits::NEGATOR_TYPE
        | traits::RECEIVER_TYPE
        | traits::SUBTRACTOR_TYPE,
};

pub const DYN_TYPE: Type = Type::simple_type(Kind::Dyn, "dyn");

pub const ERROR_TYPE: Type = Type::simple_type(Kind::Error, "error");

pub const INT_TYPE: Type = Type {
    kind: Kind::Int,
    parameters: &[],
    runtime_type_name: "int",
    trait_mask: traits::ADDER_TYPE
        | traits::COMPARER_TYPE
        | traits::DIVIDER_TYPE
        | traits::MODDER_TYPE
        | traits::MULTIPLIER_TYPE
        | traits::NEGATOR_TYPE
        | traits::SUBTRACTOR_TYPE,
};

pub const LIST_TYPE: Type = Type::new_list_type(&[&DYN_TYPE]);

pub const MAP_TYPE: Type = Type::new_map_type(&[&DYN_TYPE, &DYN_TYPE]);

pub const NULL_TYPE: Type = Type::simple_type(Kind::NullType, "null_type");

pub const OPTIONAL_TYPE: Type = Type::new_opaque_type("optional_type");

pub const STRING_TYPE: Type = Type {
    kind: Kind::String,
    parameters: &[],
    runtime_type_name: "string",
    trait_mask: traits::ADDER_TYPE
        | traits::COMPARER_TYPE
        | traits::MATCHER_TYPE
        | traits::RECEIVER_TYPE
        | traits::SIZER_TYPE,
};

pub const TIMESTAMP_TYPE: Type = Type {
    kind: Kind::Timestamp,
    parameters: &[],
    runtime_type_name: "google.protobuf.Timestamp",
    trait_mask: traits::ADDER_TYPE
        | traits::COMPARER_TYPE
        | traits::RECEIVER_TYPE
        | traits::SUBTRACTOR_TYPE,
};

pub const TYPE_TYPE: Type = Type::simple_type(Kind::Type, "type");

pub const UINT_TYPE: Type = Type {
    kind: Kind::UInt,
    parameters: &[],
    runtime_type_name: "uint",
    trait_mask: traits::ADDER_TYPE
        | traits::COMPARER_TYPE
        | traits::DIVIDER_TYPE
        | traits::MODDER_TYPE
        | traits::MULTIPLIER_TYPE
        | traits::SUBTRACTOR_TYPE,
};

pub const UNKNOWN_TYPE: Type = Type::simple_type(Kind::Unknown, "unknown");

impl<'a> Type<'a> {
    pub const fn simple_type(kind: Kind, name: &str) -> Type<'_> {
        Type {
            kind,
            parameters: &[],
            runtime_type_name: name,
            trait_mask: 0,
        }
    }

    pub const fn new_list_type<'b>(param: &'b [&'b Type<'b>; 1]) -> Type<'b> {
        Type {
            kind: Kind::List,
            parameters: param,
            runtime_type_name: "list",
            trait_mask: traits::ADDER_TYPE
                | traits::CONTAINER_TYPE
                | traits::INDEXER_TYPE
                | traits::ITERABLE_TYPE
                | traits::SIZER_TYPE,
        }
    }

    pub const fn new_map_type<'b>(param: &'b [&'b Type<'b>; 2]) -> Type<'b> {
        Type {
            kind: Kind::Map,
            parameters: param,
            runtime_type_name: "map",
            trait_mask: traits::CONTAINER_TYPE
                | traits::INDEXER_TYPE
                | traits::ITERABLE_TYPE
                | traits::SIZER_TYPE,
        }
    }

    pub const fn new_unspecified_type(name: &str) -> Type<'_> {
        Type {
            kind: Kind::Unspecified,
            parameters: &[],
            runtime_type_name: name,
            trait_mask: 0,
        }
    }

    pub const fn new_opaque_type(name: &str) -> Type<'_> {
        Type {
            kind: Kind::Opaque,
            parameters: &[],
            runtime_type_name: name,
            trait_mask: 0,
        }
    }

    pub fn name(&self) -> &'a str {
        self.runtime_type_name
    }

    pub fn has_trait(&self, t: u16) -> bool {
        self.trait_mask & t == t
    }
}

/// Extends the lifetime of a `*const T` into a caller-chosen `&'a T`.
///
/// Lifetime-laundry primitive: the caller picks `'a`, which may be much
/// longer than any borrow the raw pointer was derived from (including
/// `'static`). Use only when a documented project-local invariant bounds
/// every read of the returned reference to the real liveness of the
/// pointee; the caller's SAFETY comment must name that invariant. The
/// only current user is `<BorrowedVal<'a, String> as From<&'a str>>` in
/// `string.rs`, relying on the Safety invariant on `String`'s field and
/// on `BorrowedVal::phantom`. Prefer safe borrow-based APIs whenever
/// possible.
///
/// # Safety
///
/// The caller must ensure that, for the entire caller-chosen lifetime
/// `'a`:
///
/// 1. `s` is non-null.
/// 2. `s` is properly aligned for `T`.
/// 3. `s` carries correct metadata for `T` (e.g. the length for `str`
///    and slices), and the whole `size_of_val` byte range starting at
///    `s` lies in one live allocation and holds a valid, initialized
///    `T`.
/// 4. The allocation backing `s` remains live: no deallocation,
///    reallocation, or repurposing occurs.
/// 5. No other access path mutates the pointee (except through
///    `UnsafeCell` inside `T`), and no `&mut` reference to the pointee
///    is alive.
///
/// The chosen `'a` is not tied to any source borrow by the compiler.
/// Callers must audit every downstream use site, including drops, to
/// confirm the returned reference is neither read nor dropped as part of
/// a value after the duration for which conditions 1–5 hold.
///
/// If these preconditions hold, the returned `&'a T` is a valid shared
/// reference to the pointee for all of `'a`.
unsafe fn leak_ref<'a, T: ?Sized>(s: *const T) -> &'a T {
    // SAFETY:
    // Operation: creating a shared reference from a raw pointer via `&*s`.
    // Contract (AXIOM: Rust Reference, reference validity and place
    // expressions): `s` must be dereferenceable, i.e. non-null, aligned
    // for `T`, with correct metadata and its full `size_of_val` range in
    // one live allocation; the pointee must be a valid initialized `T`;
    // and no conflicting `&mut` alias may exist for the reference's
    // lifetime `'a`.
    // Evidence: each of these obligations is exactly one of
    // PRECONDITIONS 1–5 in this function's `# Safety` section, which the
    // caller has discharged. Nothing in this body mutates state or runs
    // intervening code that could invalidate them.
    &*s
}

/// Try to cast a `Box<dyn Val>` to its concrete type `T: Val`.
///
/// Returns `Ok(Box<T>)` if the underlying concrete type is exactly `T`,
/// otherwise returns `Err(Box<dyn Val>)` with the original box unchanged.
fn cast_boxed<T: Val>(value: Box<dyn Val>) -> Result<Box<T>, Box<dyn Val>> {
    if <dyn Any>::is::<T>(&*value) {
        // Mirror std's `Box<dyn Any>::downcast` pattern: consume the
        // source `Box` via `Box::into_raw`, thin the resulting fat
        // pointer to `*mut T`, and reconstitute a `Box<T>`.
        let raw: *mut dyn Val = Box::into_raw(value);
        // SAFETY:
        // Operation: `Box::from_raw(raw as *mut T)`.
        // Contract from `Box::from_raw` (AXIOM: std docs): the pointer
        // must have been produced by a prior `Box::into_raw` for a value
        // whose concrete type has the same layout and alignment as `T`,
        // and be usable with the global allocator; the resulting `Box`
        // takes exclusive ownership and will free the allocation with
        // `T`'s layout on drop.
        // Evidence:
        //   - POSTCONDITION of `Box::into_raw(value)` (AXIOM: std docs):
        //     `raw` points to the heap allocation that `value` owned,
        //     ownership has been surrendered, and no other Box currently
        //     owns the allocation.
        //   - `T: Val` (TYPE FACT) and `Val: Any` (declared on the trait
        //     in `common::value::Val`), so `<dyn Any>::is::<T>(&*value)`
        //     called above compares `TypeId::of::<T>()` against the
        //     concrete type behind the trait object. When it returns
        //     true, AXIOM (std docs for `Any::is`) states that the
        //     concrete type is exactly `T`.
        //   - Therefore the heap allocation was constructed for `T` and
        //     has `T`'s layout and alignment; `raw as *mut T` is a
        //     non-null pointer to an aligned, initialized `T` in that
        //     allocation.
        //   - Between the type-id check and this `Box::from_raw`, no
        //     code observes, aliases, or frees the allocation.
        // Postcondition: the returned `Box<T>` is the unique owner of
        // the heap allocation and will free it with `T`'s layout on
        // drop.
        return Ok(unsafe { Box::from_raw(raw as *mut T) });
    }
    Err(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parameterized_type() {
        let param = Type {
            kind: Kind::Unspecified,
            parameters: &[],
            runtime_type_name: "",
            trait_mask: 0,
        };

        let t = std::string::String::from("List");
        let parameterized_list = Type {
            kind: Kind::List,
            parameters: &[&param],
            runtime_type_name: &t,
            trait_mask: 0,
        };
        assert_eq!(&param, parameterized_list.parameters[0]);

        let params = [&param];
        let list2 = Type::new_list_type(&params);
        assert_eq!(&param, list2.parameters[0]);
        assert_eq!(1, list2.parameters.len());

        let params = [&param, &param];
        let map = Type::new_map_type(&params);
        assert_eq!(&param, map.parameters[0]);
        assert_eq!(&param, map.parameters[1]);
        assert_eq!(2, map.parameters.len());
    }
}
