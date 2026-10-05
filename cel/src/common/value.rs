//! Values: the [`Val`] trait every CEL value implements, and [`CowVal`], a
//! value either borrowed or owned.
//!
//! # Values are trait objects
//!
//! The interpreter handles every value as a `dyn Val`. The built-in types, in
//! [`types`](crate::common::types), implement it, and so can any type of an
//! application: bound with
//! [`Context::add_variable_as_val`](crate::Context::add_variable_as_val), or
//! returned by a function, it is a CEL value like any other.
//!
//! A value reports its CEL [`Type`] with [`Val::get_type`]: function overloads
//! are picked by it, and `type(x)` evaluates to it. Anything else a value can
//! do, it hands out through its `as_*` accessors: each returns one of the
//! capabilities of [`traits`](crate::common::traits), or `None` when the value
//! doesn't have it, and each serves one or more operators, `+` for
//! [`Val::as_adder`]. Equality is [`Val::equals`].
//!
//! # Borrowing: `'b` and `'v`
//!
//! A value may borrow the data it stands for rather than own a copy of it:
//! [`CelString::from`] a `&str` borrows the `str`. The trait object carries that
//! borrow as its lifetime bound: a `dyn Val + 'v` is a value whose data lives
//! for `'v`. The value itself is often handled by reference too, for a shorter
//! `'b`: as a `&'b (dyn Val + 'v)`, or as a [`CowVal<'b, 'v>`](CowVal), which
//! may own it instead.
//!
//! Keeping the two apart is what lets data go through an evaluation uncopied:
//! indexing a list borrowed for `'b` hands out the element borrowed for `'b`,
//! and a function given a string that borrows a request's bytes for `'v` can
//! return a substring of those same bytes, still valid for `'v` once the call
//! returns. The `zero_copy` and `resolver` examples of the repository show
//! both.
//!
//! # Recovering the concrete type
//!
//! `downcast_ref`, on a `dyn Val`, returns the Rust type behind it. It works for
//! the built-in types, and for an application's `'static` types that implement
//! [`StaticVal`]: see [`FromVal`].
//!
//! # `Val` and `Value`
//!
//! [`Value`](crate::Value) is the owned enum the rest of the crate's API deals
//! in: [`Program::execute`](crate::Program::execute) returns one, and
//! [`Context::add_variable`](crate::Context::add_variable) takes anything that
//! converts to one. Converting a `Val` to a `Value` copies it, and goes by its
//! type's [`Kind`](crate::common::types::Kind): only the built-in types have a
//! `Value` counterpart. An expression that evaluates to a value of an
//! application's own type fails to convert; use
//! [`Value::resolve_val`](crate::Value::resolve_val), which returns the
//! `CowVal` itself.
//!
//! # Example
//!
//! ```
//! use cel::common::types::CelString;
//! use cel::common::value::CowVal;
//!
//! let request_path = String::from("/api/v1");
//!
//! // a `CelString` borrowing `request_path`, owned by the `CowVal`
//! let value: CowVal<'_, '_> = CowVal::owned(CelString::from(request_path.as_str()));
//! assert!(value.is_owned());
//!
//! // `CowVal` derefs to `dyn Val`, which `downcast_ref` turns back into a `CelString`
//! let path: &CelString = value.downcast_ref::<CelString>().unwrap();
//! assert!(std::ptr::eq(path.inner(), request_path.as_str()), "not a copy");
//! ```
use crate::common::traits::{
    Adder, Comparer, Container, Divider, Indexer, Iterable, Modder, Multiplier, Negator, Sizer,
    Subtractor, Zeroer,
};
use crate::common::types::CelStruct;
use crate::common::types::{CelBytes, CelList, CelMap, CelOptional, CelString, Type};
use std::any::Any;
use std::fmt::Debug;
use std::ops::Deref;

/// A CEL runtime value.
///
/// `Val` is object-safe and carries no `'static` requirement: a value may
/// borrow data, and that borrow is tracked by the trait-object lifetime
/// bound (`dyn Val + 'v`). Built-in scalars are `'static`; [`CelString`],
/// [`CelBytes`], and the containers can borrow. See the
/// [module documentation](self) for how values are used.
///
/// An implementation provides:
///
/// * its CEL type: [`get_type`](Val::get_type), and
///   [`cel_type`](Val::cel_type) when every value of the Rust type has the
///   same CEL type;
/// * a way to copy it: [`clone_as_boxed`](Val::clone_as_boxed);
/// * equality: [`equals`](Val::equals), which otherwise is always `false`;
/// * one `as_*` accessor per capability it has, e.g.
///   [`as_comparer`](Val::as_comparer) for `<` and friends, each returning
///   `Some(self)` and backed by the matching trait of
///   [`traits`](crate::common::traits). The others keep their default, `None`;
/// * for a `'static` type, [`as_any`](Val::as_any) and [`StaticVal`], which make
///   it possible to downcast to it, and to declare overloads taking it with
///   [`add_overload!`](crate::add_overload).
///
/// # Implementing `Val` for a `'static` type
///
/// Return `Some(self)` from [`Val::as_any`] and implement the [`StaticVal`]
/// marker so that `downcast_ref` can recover the
/// concrete type. Register its type with [`Env::add_type`](crate::Env::add_type)
/// for expressions to name it, e.g. `type(addr) == ip`:
///
/// ```
/// use cel::common::types::Type;
/// use cel::common::value::{StaticVal, Val};
/// use cel::{Context, Env, Value};
/// use std::any::Any;
/// use std::sync::Arc;
///
/// static IP: Type = Type::new_unspecified_type("ip");
///
/// #[derive(Debug)]
/// struct Ip(u32);
///
/// impl Val for Ip {
///     fn get_type(&self) -> &Type {
///         <Self as Val>::cel_type()
///     }
///     fn cel_type() -> &'static Type {
///         &IP
///     }
///     fn equals(&self, other: &dyn Val) -> bool {
///         other.downcast_ref::<Ip>().is_some_and(|o| o.0 == self.0)
///     }
///     fn clone_as_boxed<'v>(&self) -> Box<dyn Val + 'v> {
///         Box::new(Ip(self.0))
///     }
///     fn as_any(&self) -> Option<&dyn Any> {
///         Some(self)
///     }
/// }
/// impl StaticVal for Ip {}
///
/// let mut env = Env::stdlib();
/// env.add_type(IP.to_owned()).unwrap();
/// let program = env.compile("type(addr) == ip").unwrap();
/// let mut context = Context::with_env(Arc::new(env));
/// context.add_variable_as_val("addr", Box::new(Ip(0x7f000001)));
///
/// assert_eq!(program.execute(&context), Ok(Value::Bool(true)));
/// ```
pub trait Val: Debug + Send + Sync {
    /// The value's CEL type.
    ///
    /// Function overloads are picked by it (see
    /// [`Type::is_assignable`]), `type(x)` evaluates to a type value of its
    /// name, and the conversion to [`Value`](crate::Value) goes by its
    /// [`Kind`](crate::common::types::Kind). A value keeps the same type for as
    /// long as it lives. When all values of the Rust type have the same CEL
    /// type, return [`cel_type`](Val::cel_type).
    fn get_type(&self) -> &Type;

    /// Returns the runtime `Type` of this Val as a statically-callable
    /// associated function (no `self`).
    ///
    /// Used by the [`add_overload!`](crate::add_overload) and
    /// [`add_member_overload!`](crate::add_member_overload) macros to
    /// derive the argument / return types of a registered overload from
    /// the Rust type of a fn parameter.
    ///
    /// Every `Val` implementation must provide this. A type whose runtime
    /// `Type` varies per-instance (e.g. `Struct`, whose type name depends on
    /// the value) has no static answer to give and should panic here; such a
    /// type cannot be named in those macros, and callers should reach for
    /// [`get_type`](Val::get_type) on a value instead.
    fn cel_type() -> &'static Type
    where
        Self: Sized;

    // Accessors for operators that produce values carry a `'v` that `Self`
    // outlives, so that the operator's result can be bounded by the
    // operand's own lifetime rather than by the borrow `'b`.

    /// `self + rhs`: `Some(self)` if the value implements [`Adder`].
    ///
    /// The default, `None`, makes `+` an error with this value on the left.
    fn as_adder<'b, 'v>(&'b self) -> Option<&'b (dyn Adder + 'v)>
    where
        Self: 'v,
    {
        None
    }

    /// `self < rhs`, `self <= rhs`, `self > rhs` and `self >= rhs`:
    /// `Some(self)` if the value implements [`Comparer`].
    ///
    /// The default, `None`, makes those comparisons errors with this value on
    /// the left.
    fn as_comparer(&self) -> Option<&dyn Comparer> {
        None
    }

    /// `value in self`: `Some(self)` if the value implements [`Container`].
    ///
    /// The default, `None`, makes `in` an error with this value on the
    /// **right**.
    fn as_container(&self) -> Option<&dyn Container> {
        None
    }

    /// `self / rhs`: `Some(self)` if the value implements [`Divider`].
    ///
    /// The default, `None`, makes `/` an error with this value on the left.
    fn as_divider<'b, 'v>(&'b self) -> Option<&'b (dyn Divider + 'v)>
    where
        Self: 'v,
    {
        None
    }

    /// `self[i]`, `self.f`, `has(self.f)`, `self.?f` and `self[?i]`:
    /// `Some(self)` if the value implements [`Indexer`].
    ///
    /// The default, `None`, makes the first three errors, and the optional
    /// ones `optional.none()`.
    fn as_indexer<'b, 'v>(&'b self) -> Option<&'b (dyn Indexer + 'v)>
    where
        Self: 'v,
    {
        None
    }

    /// `self[i]`, `self.f` and the like on a value nothing else refers to,
    /// such as a function's result: `Some(self)` if the value implements
    /// [`Indexer`], so that [`Indexer::steal`] can move the element out rather
    /// than clone it.
    ///
    /// The interpreter only calls it on the built-in containers: it indexes
    /// any other value with [`as_indexer`](Val::as_indexer), and copies the
    /// element out. An application's type can keep the default, `None`.
    fn into_indexer<'v>(self: Box<Self>) -> Option<Box<dyn Indexer + 'v>>
    where
        Self: 'v,
    {
        None
    }

    /// The comprehensions, `self.all(x, ...)`, `self.map(x, ...)` and the
    /// like: `Some(self)` if the value implements [`Iterable`].
    ///
    /// The default, `None`, makes them errors on this value.
    fn as_iterable<'b, 'v>(&'b self) -> Option<&'b (dyn Iterable + 'v)>
    where
        Self: 'v,
    {
        None
    }

    /// `self % rhs`: `Some(self)` if the value implements [`Modder`].
    ///
    /// The default, `None`, makes `%` an error with this value on the left.
    fn as_modder<'b, 'v>(&'b self) -> Option<&'b (dyn Modder + 'v)>
    where
        Self: 'v,
    {
        None
    }

    /// `self * rhs`: `Some(self)` if the value implements [`Multiplier`].
    ///
    /// The default, `None`, makes `*` an error with this value on the left.
    fn as_multiplier<'b, 'v>(&'b self) -> Option<&'b (dyn Multiplier + 'v)>
    where
        Self: 'v,
    {
        None
    }

    /// `-self`: `Some(self)` if the value implements [`Negator`].
    ///
    /// The default, `None`, makes unary `-` an error on this value.
    fn as_negator<'b, 'v>(&'b self) -> Option<&'b (dyn Negator + 'v)>
    where
        Self: 'v,
    {
        None
    }

    /// `size(self)` and `self.size()`, for a value of type `list` or `map`:
    /// `Some(self)` if the value implements [`Sizer`].
    ///
    /// The standard library's `size` overloads for those two types call it;
    /// a value of another type needs `size` overloads of its own.
    fn as_sizer(&self) -> Option<&dyn Sizer> {
        None
    }

    /// `self - rhs`: `Some(self)` if the value implements [`Subtractor`].
    ///
    /// The default, `None`, makes binary `-` an error with this value on the
    /// left.
    fn as_subtractor<'b, 'v>(&'b self) -> Option<&'b (dyn Subtractor + 'v)>
    where
        Self: 'v,
    {
        None
    }

    /// `optional.ofNonZeroValue(self)`: `Some(self)` if the value implements
    /// [`Zeroer`].
    ///
    /// With the default, `None`, the value is never a zero value.
    fn as_zeroer(&self) -> Option<&dyn Zeroer> {
        None
    }

    /// `self == other`, and `self != other`; also how containers compare
    /// their elements, e.g. for `x in list`.
    ///
    /// The interpreter only calls it on the left-hand operand: implement it
    /// symmetrically with the types it compares equal to, as `1 == 1.0` and
    /// `1.0 == 1` both hold. The default returns `false`, so a value that
    /// keeps it isn't even equal to itself.
    fn equals(&self, _other: &dyn Val) -> bool {
        false
    }

    /// Clones the value into a box whose trait-object lifetime `'v` is any
    /// lifetime `Self` outlives. Implementations must not shorten a borrow:
    /// a value borrowing for `'a` clones into a value borrowing for `'a`.
    ///
    /// The interpreter clones a value to own one it only borrows, e.g. to
    /// bind the elements of a list to a comprehension's variable, or to put
    /// a variable's value in a list literal. The clone must be the same value,
    /// of the same type: the clone of an optional is an optional, not the
    /// value it holds.
    fn clone_as_boxed<'v>(&self) -> Box<dyn Val + 'v>
    where
        Self: 'v;

    /// `'static` implementations return `Some(self)`; this is what backs
    /// `downcast_ref` for them. Values that borrow
    /// cannot be `Any` and keep the default `None`.
    ///
    /// Implement [`StaticVal`] along with it.
    fn as_any(&self) -> Option<&dyn Any> {
        None
    }

    /// Crate-internal: borrowed built-in types identify themselves here so
    /// that they can be downcast without `Any`.
    #[doc(hidden)]
    fn as_builtin<'b, 'v>(&'b self) -> BuiltinRef<'b, 'v>
    where
        Self: 'v,
    {
        BuiltinRef::Other
    }

    /// Crate-internal: by-value counterpart of [`Val::as_builtin`]. Returns
    /// `None` for anything that is not a built-in, dropping the box, so
    /// check [`Val::as_builtin`] first when the box must be kept.
    #[doc(hidden)]
    fn into_builtin<'v>(self: Box<Self>) -> Option<Builtin<'v>>
    where
        Self: 'v,
    {
        None
    }
}

/// Marker for `Val` implementations that are `'static` and return
/// `Some(self)` from [`Val::as_any`]. Enables the generic
/// `downcast_ref` for the type.
///
/// It is what an application's own type implements to be recovered from a
/// `dyn Val`, through [`FromVal`]: by a function receiving it, and by the
/// [`add_overload!`](crate::add_overload) and
/// [`add_member_overload!`](crate::add_member_overload) macros, which downcast
/// each argument to the Rust type of the parameter.
pub trait StaticVal: Val + 'static {}

/// A type that can be recovered by reference from a `&'b (dyn Val + 'v)`.
///
/// Implemented for every [`StaticVal`] through [`Val::as_any`], and for the
/// built-in borrowing types through [`Val::as_builtin`]. A borrowing type
/// is recovered as `T<'v>`: the downcast keeps the value's own lifetime
/// bound rather than shortening it to the borrow `'b`.
pub trait FromVal<'b, 'v>: Sized {
    /// Returns `val` as a `Self`, or `None` if it isn't one.
    fn from_val(val: &'b (dyn Val + 'v)) -> Option<&'b Self>;
}

impl<'b, 'v, T: StaticVal> FromVal<'b, 'v> for T {
    fn from_val(val: &'b (dyn Val + 'v)) -> Option<&'b Self> {
        val.as_any()?.downcast_ref::<T>()
    }
}

/// Borrowed view of a built-in value, see [`Val::as_builtin`].
#[doc(hidden)]
#[non_exhaustive]
pub enum BuiltinRef<'b, 'v> {
    String(&'b CelString<'v>),
    Bytes(&'b CelBytes<'v>),
    List(&'b CelList<'v>),
    Map(&'b CelMap<'v>),
    Optional(&'b CelOptional<'v>),
    Struct(&'b CelStruct<'v>),
    Other,
}

/// Owned built-in value, see [`Val::into_builtin`].
#[doc(hidden)]
#[non_exhaustive]
pub enum Builtin<'v> {
    String(CelString<'v>),
    Bytes(CelBytes<'v>),
    List(CelList<'v>),
    Map(CelMap<'v>),
    Optional(CelOptional<'v>),
    Struct(CelStruct<'v>),
}

macro_rules! builtin_from_val {
    ($($ty:ident => $variant:ident),* $(,)?) => {
        $(
            impl<'b, 'v> FromVal<'b, 'v> for $ty<'v> {
                fn from_val(val: &'b (dyn Val + 'v)) -> Option<&'b Self> {
                    match val.as_builtin() {
                        BuiltinRef::$variant(v) => Some(v),
                        _ => None,
                    }
                }
            }
        )*
    };
}

builtin_from_val! {
    CelString => String,
    CelBytes => Bytes,
    CelList => List,
    CelMap => Map,
    CelOptional => Optional,
}

builtin_from_val! {
    CelStruct => Struct,
}

impl<'v> dyn Val + 'v {
    /// Recovers the concrete type behind this value, if it is `T`.
    ///
    /// For a borrowing type such as [`CelString`], the recovered reference
    /// keeps the value's lifetime bound: on a `&'b (dyn Val + 'v)`,
    /// `val.downcast_ref::<CelString>()` yields a `&'b CelString<'v>`.
    pub fn downcast_ref<'b, T: FromVal<'b, 'v>>(&'b self) -> Option<&'b T> {
        T::from_val(self)
    }
}

impl<'v> Clone for Box<dyn Val + 'v> {
    fn clone(&self) -> Self {
        (**self).clone_as_boxed()
    }
}

impl<'v> PartialEq for dyn Val + 'v {
    fn eq(&self, other: &Self) -> bool {
        self.equals(other)
    }
}

impl<'v> Eq for dyn Val + 'v {}

/// A clone-on-write `dyn Val`.
///
/// `'b` is the borrow, `'v` is the lifetime bound of the value itself: the
/// data a value may borrow (a resolver's `&str`, a context variable) lives
/// for `'v`, which outlives `'b`. Both are covariant, so a `CowVal` can
/// always be shortened.
///
/// Evaluating an expression yields a `CowVal`, and so does every operator and
/// function: a variable, a list's element, or a function's argument handed
/// back unchanged is `Borrowed`, while a value computed anew is `Owned`.
/// It derefs to `dyn Val`, so a `CowVal` is used like the value it holds.
pub enum CowVal<'b, 'v> {
    /// A value that lives elsewhere, e.g. in a variable or in the expression.
    Borrowed(&'b (dyn Val + 'v)),
    /// A value of its own, e.g. the result of an arithmetic operator.
    Owned(Box<dyn Val + 'v>),
}

impl<'b, 'v> CowVal<'b, 'v> {
    /// Boxes an owned value.
    pub fn owned<T: Val + 'v>(val: T) -> Self {
        CowVal::Owned(Box::new(val))
    }

    /// Whether this is [`CowVal::Borrowed`].
    pub fn is_borrowed(&self) -> bool {
        matches!(self, CowVal::Borrowed(_))
    }

    /// Whether this is [`CowVal::Owned`].
    pub fn is_owned(&self) -> bool {
        matches!(self, CowVal::Owned(_))
    }

    /// Extracts the owned value, cloning it if it was borrowed.
    pub fn into_owned(self) -> Box<dyn Val + 'v> {
        match self {
            CowVal::Borrowed(b) => b.clone_as_boxed(),
            CowVal::Owned(o) => o,
        }
    }
}

impl<'b, 'v> Deref for CowVal<'b, 'v> {
    type Target = dyn Val + 'v;

    fn deref(&self) -> &Self::Target {
        match self {
            CowVal::Borrowed(b) => *b,
            CowVal::Owned(o) => o.as_ref(),
        }
    }
}

impl<'b, 'v> AsRef<dyn Val + 'v> for CowVal<'b, 'v> {
    fn as_ref(&self) -> &(dyn Val + 'v) {
        &**self
    }
}

impl<'b, 'v> Clone for CowVal<'b, 'v> {
    fn clone(&self) -> Self {
        match self {
            CowVal::Borrowed(b) => CowVal::Borrowed(*b),
            CowVal::Owned(o) => CowVal::Owned(o.clone()),
        }
    }
}

impl<'b, 'v> Debug for CowVal<'b, 'v> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CowVal::Borrowed(b) => f.debug_tuple("Borrowed").field(b).finish(),
            CowVal::Owned(o) => f.debug_tuple("Owned").field(o).finish(),
        }
    }
}

impl<'b, 'v> PartialEq for CowVal<'b, 'v> {
    fn eq(&self, other: &Self) -> bool {
        self.as_ref().equals(other.as_ref())
    }
}

impl<'b, 'v> Eq for CowVal<'b, 'v> {}

impl<'b, 'v> From<Box<dyn Val + 'v>> for CowVal<'b, 'v> {
    fn from(val: Box<dyn Val + 'v>) -> Self {
        CowVal::Owned(val)
    }
}

impl<'b, 'v> From<&'b (dyn Val + 'v)> for CowVal<'b, 'v> {
    fn from(val: &'b (dyn Val + 'v)) -> Self {
        CowVal::Borrowed(val)
    }
}

#[cfg(test)]
mod test {
    use crate::common::types;
    use crate::common::types::{CelInt, CelString};
    use crate::common::value::{CowVal, Val};

    fn test(val: &dyn Val) -> bool {
        *val.get_type() == types::STRING_TYPE
    }

    #[test]
    fn test_cow() {
        let s1 = types::CelString::from("cel");
        let s2 = types::CelString::from("cel");
        let b: Box<dyn Val> = Box::new(s1);
        let cow: CowVal<'_, '_> = CowVal::Owned(b);
        let borrowed: CowVal<'_, '_> = CowVal::Borrowed(&s2);
        assert!(test(borrowed.as_ref()));
        assert!(test(cow.as_ref()));
        assert!(test(borrowed.clone().as_ref()));
        assert_eq!(cow.downcast_ref::<CelString>().unwrap().inner(), "cel");
        let boxed = cow.into_owned();
        let s: &CelString = boxed.downcast_ref::<CelString>().unwrap();
        assert_eq!(s.inner(), "cel");
        assert!(boxed.downcast_ref::<CelInt>().is_none());
    }

    #[test]
    fn borrowed_string_is_downcastable_and_keeps_its_pointer() {
        let owned = String::from("cel-rust");
        let borrowed: CowVal<'_, '_> = {
            let s = CelString::from(owned.as_str());
            CowVal::owned(s)
        };
        let s = borrowed.downcast_ref::<CelString>().unwrap();
        assert!(std::ptr::eq(s.inner(), owned.as_str()));
        // cloning keeps the borrow, no copy of the bytes
        let cloned = borrowed.clone().into_owned();
        let c = cloned.downcast_ref::<CelString>().unwrap();
        assert!(std::ptr::eq(c.inner(), owned.as_str()));
    }

    /// A `CowVal` bounded by `'v` cannot outlive the data it borrows.
    /// ```compile_fail,E0597
    /// use cel::common::types::CelString;
    /// use cel::common::value::CowVal;
    /// let escaped: CowVal<'static, 'static> = {
    ///     let s = String::from("cel");
    ///     CowVal::owned(CelString::from(s.as_str()))
    /// };
    /// ```
    fn _doc_only() {}
}
