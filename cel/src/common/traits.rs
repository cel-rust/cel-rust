//! The capabilities a value can have: one trait per operator, or group of
//! operators.
//!
//! The interpreter evaluates an operator by asking its operand for the
//! matching capability, through one of [`Val`]'s `as_*` accessors: `a + b`
//! calls [`Val::as_adder`] on `a` and, if it returns an [`Adder`], hands `b`
//! to [`Adder::add`]. A value supports an operator by returning `Some(self)`
//! from its accessor and implementing the trait; the default `None` means it
//! doesn't, and the expression fails. Nothing else decides what a value can
//! do: not its [`Type`](crate::common::types::Type), and not the
//! [`TraitSet`] constants below.
//!
//! | Expression | Called on | Accessor | Trait method |
//! |---|---|---|---|
//! | `a + b` | `a` | [`Val::as_adder`] | [`Adder::add`] |
//! | `a - b` | `a` | [`Val::as_subtractor`] | [`Subtractor::sub`] |
//! | `a * b` | `a` | [`Val::as_multiplier`] | [`Multiplier::mul`] |
//! | `a / b` | `a` | [`Val::as_divider`] | [`Divider::div`] |
//! | `a % b` | `a` | [`Val::as_modder`] | [`Modder::modulo`] |
//! | `-a` | `a` | [`Val::as_negator`] | [`Negator::negate`] |
//! | `a < b`, `a <= b`, `a > b`, `a >= b` | `a` | [`Val::as_comparer`] | [`Comparer::compare`] |
//! | `a in b` | `b` | [`Val::as_container`] | [`Container::contains`] |
//! | `a[i]`, `a.f`, `has(a.f)`, `a.?f`, `a[?i]` | `a` | [`Val::as_indexer`], [`Val::into_indexer`] | [`Indexer::get`], [`Indexer::steal`] |
//! | `a.all(x, ..)`, `a.exists(x, ..)`, `a.map(x, ..)`, ... | `a` | [`Val::as_iterable`] | [`Iterable::iter`] |
//! | `size(a)`, `a.size()`, for a `list` or `map` | `a` | [`Val::as_sizer`] | [`Sizer::size`] |
//! | `a == b`, `a != b` | `a` | | [`Val::equals`] |
//!
//! `!a`, `a && b`, `a || b` and `a ? b : c` take `bool` operands and involve no
//! trait. Every other function, `x.startsWith(y)` say, is an overload declared
//! on the [`Env`](crate::Env) for given argument types, not a capability; see
//! [`decls`](crate::common::decls). [`Zeroer`] serves no operator, but the
//! function `optional.ofNonZeroValue`.
//!
//! # Mixing types
//!
//! Each implementation decides which right-hand operands it accepts. The
//! built-in types follow the CEL spec: arithmetic only combines values of the
//! same type (`1 + 1.0` is an error), with the exceptions of `timestamp` and
//! `duration`, while comparisons and equality treat `int`, `uint` and `double`
//! as numbers (`1 < 1.5`, `1 == 1u` and `1 == 1.0` all hold).
//!
//! # Errors
//!
//! An operand of a type the implementation doesn't support should fail with
//! [`ExecutionError::ValuesNotComparable`], [`ExecutionError::UnexpectedType`]
//! or [`ExecutionError::NoSuchOverload`]. For a comparison, `in`, indexing,
//! field selection and `-`, the interpreter reports any of those as a
//! [`NoSuchOverload`](ExecutionError::NoSuchOverload) that names the operator
//! and the operands' types, as cel-go does. The errors of the arithmetic
//! operators reach the caller unchanged: the built-in types fail with
//! [`ExecutionError::UnsupportedBinaryOperator`].
//!
//! # Lifetimes
//!
//! A value may borrow data for `'v`, see [`value`](crate::common::value). The
//! traits that produce a value take `&'b self` and return a
//! [`CowVal<'b, 'v>`](CowVal), for the `'v` of the operand: a result may borrow
//! the operand (`'b`), or the data the operand borrows (`'v`), rather than
//! copying either. [`Indexer::get`] on a list hands out a borrow of the element,
//! for instance.
//!
//! # Example
//!
//! The interpreter's calls, made by hand:
//!
//! ```
//! use cel::common::types::{CelDouble, CelInt};
//! use cel::common::value::Val;
//! use std::cmp::Ordering;
//!
//! let one = CelInt::from(1);
//!
//! // `1 + 2`
//! let sum = one.as_adder().unwrap().add(&CelInt::from(2)).unwrap();
//! assert_eq!(sum.downcast_ref::<CelInt>(), Some(&CelInt::from(3)));
//!
//! // `1 + 1.0`: arithmetic doesn't mix numeric types...
//! assert!(one.as_adder().unwrap().add(&CelDouble::from(1.0)).is_err());
//!
//! // `1 < 1.5`: ...but comparisons do
//! let ordering = one.as_comparer().unwrap().compare(&CelDouble::from(1.5));
//! assert_eq!(ordering, Ok(Ordering::Less));
//!
//! // `-1u`: `uint` has no `Negator`
//! assert!(cel::common::types::CelUInt::from(1).as_negator().is_none());
//! ```
use crate::common::types::CelInt;
use crate::common::value::{CowVal, Val};
use crate::ExecutionError;
use std::cmp::Ordering;

/// A bit set of the `*_TYPE` constants of this module, as carried by a
/// [`Type`](crate::common::types::Type), see
/// [`Type::has_trait`](crate::common::types::Type::has_trait).
///
/// The set mirrors the traits cel-go assigns its types. It describes a type;
/// it doesn't enable anything: the interpreter never consults it, and asks the
/// value's `as_*` accessors instead. Some constants, such as
/// [`MATCHER_TYPE`] and [`RECEIVER_TYPE`], have no trait in this module.
pub type TraitSet = u16;

/// Types that support `+`, see [`Adder`].
pub const ADDER_TYPE: TraitSet = 1;

/// Types that support the ordering comparisons `<`, `<=`, `>` and `>=`, see
/// [`Comparer`].
pub const COMPARER_TYPE: TraitSet = ADDER_TYPE << 1;

/// Types that support `in`, see [`Container`].
pub const CONTAINER_TYPE: TraitSet = COMPARER_TYPE << 1;

/// Types that support `/`, see [`Divider`].
pub const DIVIDER_TYPE: TraitSet = CONTAINER_TYPE << 1;

/// Types whose fields can be tested for presence with `has(x.f)`. In this
/// crate, that is part of [`Indexer`].
pub const FIELD_TESTER_TYPE: TraitSet = DIVIDER_TYPE << 1;

/// Types that support indexing and field selection, see [`Indexer`].
pub const INDEXER_TYPE: TraitSet = FIELD_TESTER_TYPE << 1;

/// Types that comprehensions can iterate over, see [`Iterable`].
pub const ITERABLE_TYPE: TraitSet = INDEXER_TYPE << 1;

/// Iterators, see [`Iterator`]. No CEL value is one.
pub const ITERATOR_TYPE: TraitSet = ITERABLE_TYPE << 1;

/// Types that support pattern matching with `matches`. In this crate,
/// `matches` is a function overload, not a trait.
pub const MATCHER_TYPE: TraitSet = ITERATOR_TYPE << 1;

/// Types that support `%`, see [`Modder`].
pub const MODDER_TYPE: TraitSet = MATCHER_TYPE << 1;

/// Types that support `*`, see [`Multiplier`].
pub const MULTIPLIER_TYPE: TraitSet = MODDER_TYPE << 1;

/// Types that support negation, see [`Negator`]. In this crate, only unary
/// `-` is a trait: `!` takes a `bool`.
pub const NEGATOR_TYPE: TraitSet = MULTIPLIER_TYPE << 1;

/// Types with member functions, e.g. `timestamp`'s `getHours()`. In this
/// crate, member functions are function overloads, not a trait.
pub const RECEIVER_TYPE: TraitSet = NEGATOR_TYPE << 1;

/// Types that support `size()`, see [`Sizer`].
pub const SIZER_TYPE: TraitSet = RECEIVER_TYPE << 1;

/// Types that support binary `-`, see [`Subtractor`].
pub const SUBTRACTOR_TYPE: TraitSet = SIZER_TYPE << 1;

/// Types that the two-variable comprehension macros of comprehensions v2 can
/// iterate over, as (key, value) pairs. No trait in this crate.
pub const FOLDABLE_TYPE: TraitSet = SUBTRACTOR_TYPE << 1;

// Operator traits produce values bounded by a caller-chosen `'v` that `Self`
// outlives, so a borrowing operand yields a result borrowing the same data
// rather than a `'static` copy.

/// `self + rhs`: obtained with [`Val::as_adder`] on the left-hand operand.
///
/// The built-in types add numbers of the same type (failing on overflow),
/// concatenate strings, bytes and lists, and add a `duration` to a `timestamp`
/// or to another `duration`.
pub trait Adder {
    /// Returns `self + rhs`.
    ///
    /// # Errors
    ///
    /// Fails when `rhs` can't be added to `self`, e.g. with
    /// [`ExecutionError::UnsupportedBinaryOperator`], or when the sum
    /// overflows, with [`ExecutionError::Overflow`].
    fn add<'b, 'v>(&'b self, _rhs: &(dyn Val + 'v)) -> Result<CowVal<'b, 'v>, ExecutionError>
    where
        Self: 'v;
}

/// The ordering of `self` and `rhs`, for `<`, `<=`, `>` and `>=`: obtained with
/// [`Val::as_comparer`] on the left-hand operand.
///
/// Equality doesn't go through `Comparer`, but through [`Val::equals`]; the
/// numeric types implement the latter with the former, so that `1 == 1.0`.
///
/// The lists extension's `sort()` and `sortBy()` order values with it too: a
/// list sorts when its elements, or their keys, are all of one type, and
/// implement `Comparer`.
pub trait Comparer {
    /// Returns how `self` orders against `rhs`.
    ///
    /// # Errors
    ///
    /// Fails when the two can't be ordered: `rhs` is of a type `self` doesn't
    /// compare with, or either is a `NaN` double. The built-in types fail with
    /// [`ExecutionError::ValuesNotComparable`], which the interpreter reports as
    /// a [`NoSuchOverload`](ExecutionError::NoSuchOverload) for the operator.
    fn compare(&self, _rhs: &dyn Val) -> Result<Ordering, ExecutionError>;
}

/// `value in self`: obtained with [`Val::as_container`] on the **right-hand**
/// operand.
///
/// A `list` contains the values one of its elements [equals](Val::equals); a
/// `map` contains its keys.
pub trait Container {
    /// Returns whether `value` is in `self`.
    ///
    /// # Errors
    ///
    /// Fails when `value` can't be looked up in `self`, such as a `list` used
    /// as a map key.
    fn contains(&self, _value: &dyn Val) -> Result<bool, ExecutionError>;
}

/// `self / rhs`: obtained with [`Val::as_divider`] on the left-hand operand.
pub trait Divider {
    /// Returns `self / rhs`.
    ///
    /// # Errors
    ///
    /// Fails when `rhs` can't divide `self`, e.g. with
    /// [`ExecutionError::UnsupportedBinaryOperator`], or when the integer
    /// division is by zero or overflows.
    fn div<'b, 'v>(&'b self, _rhs: &(dyn Val + 'v)) -> Result<CowVal<'b, 'v>, ExecutionError>
    where
        Self: 'v;
}

/// The values comprehensions iterate over: obtained with
/// [`Val::as_iterable`].
///
/// The macros `all`, `exists`, `exists_one` (or `existsOne`), `map` and
/// `filter` expand to comprehensions, as do the macros added with
/// [`Env::add_macro`](crate::Env::add_macro) that build one, e.g. the lists
/// extension's `sortBy`. A `list` yields its elements, in order; a `map`
/// yields its keys, in no particular order. The interpreter clones each
/// element into the loop variable.
///
/// The `+` of a `list` also takes any `Iterable` on its right. And functions
/// that take a `list` reach its elements through it: those of the lists
/// extension, `join` and `format` of the strings extension, and
/// `math.greatest` and `math.least` given a list. So any value whose
/// [type](Val::get_type) is `list` gets them by implementing `Iterable`.
pub trait Iterable {
    /// Returns an iterator over borrows of the elements, which live as long
    /// as `self` is borrowed (`'b`) and borrow data for `'v`.
    fn iter<'b, 'v>(&'b self) -> Box<dyn Iterator<'b, 'v> + 'b>
    where
        Self: 'v;
}

/// The iterator an [`Iterable`] returns.
///
/// It lends out `&'b (dyn Val + 'v)` rather than owned values, so iterating
/// copies nothing.
pub trait Iterator<'b, 'v> {
    /// Returns the next element, or `None` once there are no more.
    fn next(&mut self) -> Option<&'b (dyn Val + 'v)>;
}

/// `self % rhs`: obtained with [`Val::as_modder`] on the left-hand operand.
pub trait Modder {
    /// Returns the remainder of `self / rhs`.
    ///
    /// # Errors
    ///
    /// Fails when `rhs` can't divide `self`, e.g. with
    /// [`ExecutionError::UnsupportedBinaryOperator`], or when the division is
    /// by zero or overflows.
    fn modulo<'b, 'v>(&'b self, _rhs: &(dyn Val + 'v)) -> Result<CowVal<'b, 'v>, ExecutionError>
    where
        Self: 'v;
}

/// `self * rhs`: obtained with [`Val::as_multiplier`] on the left-hand
/// operand.
pub trait Multiplier {
    /// Returns `self * rhs`.
    ///
    /// # Errors
    ///
    /// Fails when `rhs` can't multiply `self`, e.g. with
    /// [`ExecutionError::UnsupportedBinaryOperator`], or when the product
    /// overflows.
    fn mul<'b, 'v>(&'b self, _rhs: &(dyn Val + 'v)) -> Result<CowVal<'b, 'v>, ExecutionError>
    where
        Self: 'v;
}

/// Unary `-self`: obtained with [`Val::as_negator`].
///
/// `int` and `double` are negators; `uint` and `bool` aren't, so `-1u` and
/// `-true` are errors. Logical `!` is not a trait: it takes a `bool`.
pub trait Negator {
    /// Returns `-self`, always as a new value.
    ///
    /// # Errors
    ///
    /// Fails when the negation overflows, as `-(-9223372036854775807 - 1)`
    /// does.
    fn negate<'v>(&self) -> Result<Box<dyn Val + 'v>, ExecutionError>
    where
        Self: 'v;
}

/// The size of a container: obtained with [`Val::as_sizer`].
///
/// The standard library's `size(x)` and `x.size()` overloads for `list` and
/// `map` call it, so any value whose [type](Val::get_type) is `list` or `map`
/// gets `size()` by implementing `Sizer`. `string` and `bytes` have overloads
/// of their own, which count code points and bytes respectively.
pub trait Sizer {
    /// Returns the number of elements, or entries, in `self`.
    fn size(&self) -> CelInt;
}

/// `self - rhs`: obtained with [`Val::as_subtractor`] on the left-hand
/// operand.
///
/// Besides numbers of the same type, the built-in types subtract a `duration`
/// from a `timestamp` or a `duration`, and a `timestamp` from another, which
/// yields a `duration`.
pub trait Subtractor {
    /// Returns `self - rhs`.
    ///
    /// # Errors
    ///
    /// Fails when `rhs` can't be subtracted from `self`, e.g. with
    /// [`ExecutionError::UnsupportedBinaryOperator`], or when the difference
    /// overflows, with [`ExecutionError::Overflow`].
    fn sub<'b, 'v>(&'b self, _rhs: &(dyn Val + 'v)) -> Result<CowVal<'b, 'v>, ExecutionError>
    where
        Self: 'v;
}

/// Whether a value is its type's zero value: `0`, `""`, `false`, an empty
/// list, and so on. Obtained with [`Val::as_zeroer`].
///
/// `optional.ofNonZeroValue(x)` calls it: it is `optional.none()` for a zero
/// value, and `optional.of(x)` otherwise, including for a value without a
/// `Zeroer`. It mirrors cel-go's `traits.Zeroer`.
pub trait Zeroer {
    /// Returns whether `self` is the zero value of its type.
    fn is_zero_value(&self) -> bool;
}

/// Indexing and field selection: obtained with [`Val::as_indexer`], or, on a
/// built-in container nothing else refers to, with [`Val::into_indexer`].
///
/// One trait serves all of these, on the operand `a`:
///
/// * `a[i]` looks up `i`: an `int`, `uint` or whole `double` position in a
///   `list`, a key in a `map`.
/// * `a.f` looks up the string `"f"`, a field name.
/// * `has(a.f)` looks up `"f"` too: it is `true` if [`get`](Self::get)
///   succeeds, and `false` if it fails with [`ExecutionError::NoSuchKey`].
///   Any other error is the result of `has()`.
/// * `a.?f` and `a[?i]`, of the optional syntax, yield `optional.of` the value
///   [`get`](Self::get) returns, and `optional.none()` when it fails.
///
/// A struct-like value that reports a missing field with
/// [`ExecutionError::no_such_key`] therefore supports `has()` with no further
/// work.
///
/// # Example
///
/// ```
/// use cel::common::traits::Indexer;
/// use cel::common::types::{CelInt, CelString, Type};
/// use cel::common::value::{CowVal, StaticVal, Val};
/// use cel::ExecutionError;
/// use std::any::Any;
///
/// static POINT: Type = Type::new_unspecified_type("acme.Point");
///
/// #[derive(Clone, Debug)]
/// struct Point {
///     x: CelInt,
///     y: CelInt,
/// }
///
/// impl Indexer for Point {
///     fn get<'b, 'v>(&'b self, field: &dyn Val) -> Result<CowVal<'b, 'v>, ExecutionError>
///     where
///         Self: 'v,
///     {
///         match field.downcast_ref::<CelString>().map(CelString::inner) {
///             // borrows the field rather than copying it
///             Some("x") => Ok(CowVal::Borrowed(&self.x)),
///             Some("y") => Ok(CowVal::Borrowed(&self.y)),
///             // what makes `has(p.z)` false rather than an error
///             _ => Err(ExecutionError::no_such_key("field")),
///         }
///     }
///
///     fn steal<'v>(self: Box<Self>, field: &dyn Val) -> Result<Box<dyn Val + 'v>, ExecutionError>
///     where
///         Self: 'v,
///     {
///         // only called on the built-in containers: a copy will do
///         self.get(field).map(CowVal::into_owned)
///     }
/// }
///
/// impl Val for Point {
///     fn get_type(&self) -> &Type {
///         &POINT
///     }
///     fn cel_type() -> &'static Type {
///         &POINT
///     }
///     fn as_indexer<'b, 'v>(&'b self) -> Option<&'b (dyn Indexer + 'v)>
///     where
///         Self: 'v,
///     {
///         Some(self)
///     }
///     fn clone_as_boxed<'v>(&self) -> Box<dyn Val + 'v> {
///         Box::new(self.clone())
///     }
///     fn as_any(&self) -> Option<&dyn Any> {
///         Some(self)
///     }
/// }
/// impl StaticVal for Point {}
///
/// let p = Point { x: CelInt::from(1), y: CelInt::from(2) };
/// let indexer = p.as_indexer().unwrap();
/// // `p.y`
/// let y = indexer.get(&CelString::from("y")).unwrap();
/// assert_eq!(y.downcast_ref::<CelInt>(), Some(&CelInt::from(2)));
/// // `has(p.z)` is false
/// assert!(matches!(
///     indexer.get(&CelString::from("z")),
///     Err(ExecutionError::NoSuchKey(_))
/// ));
/// ```
pub trait Indexer {
    /// Returns the value at `_idx`, borrowed from `self` where possible.
    ///
    /// # Errors
    ///
    /// Fails with [`ExecutionError::NoSuchKey`] when there is nothing at
    /// `_idx`, which `has()` reports as `false`; a `list` fails with
    /// [`ExecutionError::IndexOutOfBounds`] instead. An index of a type `self`
    /// can't be indexed with should fail with
    /// [`ExecutionError::UnexpectedType`], which the interpreter reports as
    /// a [`NoSuchOverload`](ExecutionError::NoSuchOverload) for the operator.
    fn get<'b, 'v>(&'b self, _idx: &dyn Val) -> Result<CowVal<'b, 'v>, ExecutionError>
    where
        Self: 'v;

    /// Like [`get`](Self::get), on a `self` that is dropped afterwards: the
    /// value can be moved out of it rather than cloned. The interpreter calls
    /// it, through [`Val::into_indexer`], to index a built-in container
    /// nothing else refers to, such as a function's result; any other value,
    /// it indexes with [`get`](Self::get) and copies the element out. An
    /// application's type can implement it as
    /// `self.get(idx).map(CowVal::into_owned)`.
    ///
    /// # Errors
    ///
    /// As [`get`](Self::get).
    fn steal<'v>(self: Box<Self>, _idx: &dyn Val) -> Result<Box<dyn Val + 'v>, ExecutionError>
    where
        Self: 'v;
}

pub(crate) mod adapter {
    use crate::{common::value::CowVal, ExecutionError};

    pub fn sizer_size<'b, 'v>(args: Vec<CowVal<'b, 'v>>) -> Result<CowVal<'b, 'v>, ExecutionError> {
        let target = &args[0];
        match target.as_sizer() {
            None => Err(ExecutionError::UnexpectedType {
                got: target.get_type().name().to_owned(),
                want: "missing trait Sizer".to_owned(),
            }),
            Some(sizer) => Ok(CowVal::owned(sizer.size())),
        }
    }
}
