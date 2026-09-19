use crate::common::traits::{
    Adder, Comparer, Container, Divider, Indexer, Iterable, Modder, Multiplier, Negator, Sizer,
    Subtractor, Zeroer,
};
#[cfg(feature = "structs")]
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
/// [`CelBytes`], and the containers can borrow.
///
/// # Implementing `Val` for a `'static` type
///
/// Return `Some(self)` from [`Val::as_any`] and implement the [`StaticVal`]
/// marker so that `downcast_ref` can recover the
/// concrete type:
///
/// ```
/// use cel::common::types::Type;
/// use cel::common::value::{StaticVal, Val};
/// use std::any::Any;
///
/// #[derive(Debug)]
/// struct Ip(u32);
///
/// impl Val for Ip {
///     fn get_type(&self) -> &Type {
///         static IP: Type = Type::new_unspecified_type("ip");
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
/// ```
pub trait Val: Debug + Send + Sync {
    fn get_type(&self) -> &Type;

    // Accessors for operators that produce values carry a `'v` that `Self`
    // outlives, so that the operator's result can be bounded by the
    // operand's own lifetime rather than by the borrow `'b`.

    fn as_adder<'b, 'v>(&'b self) -> Option<&'b (dyn Adder + 'v)>
    where
        Self: 'v,
    {
        None
    }

    fn as_comparer(&self) -> Option<&dyn Comparer> {
        None
    }

    fn as_container(&self) -> Option<&dyn Container> {
        None
    }

    fn as_divider<'b, 'v>(&'b self) -> Option<&'b (dyn Divider + 'v)>
    where
        Self: 'v,
    {
        None
    }

    fn as_indexer<'b, 'v>(&'b self) -> Option<&'b (dyn Indexer + 'v)>
    where
        Self: 'v,
    {
        None
    }

    fn into_indexer<'v>(self: Box<Self>) -> Option<Box<dyn Indexer + 'v>>
    where
        Self: 'v,
    {
        None
    }

    fn as_iterable<'b, 'v>(&'b self) -> Option<&'b (dyn Iterable + 'v)>
    where
        Self: 'v,
    {
        None
    }

    fn as_modder<'b, 'v>(&'b self) -> Option<&'b (dyn Modder + 'v)>
    where
        Self: 'v,
    {
        None
    }

    fn as_multiplier<'b, 'v>(&'b self) -> Option<&'b (dyn Multiplier + 'v)>
    where
        Self: 'v,
    {
        None
    }

    fn as_negator<'b, 'v>(&'b self) -> Option<&'b (dyn Negator + 'v)>
    where
        Self: 'v,
    {
        None
    }

    fn as_sizer(&self) -> Option<&dyn Sizer> {
        None
    }

    fn as_subtractor<'b, 'v>(&'b self) -> Option<&'b (dyn Subtractor + 'v)>
    where
        Self: 'v,
    {
        None
    }

    fn as_zeroer(&self) -> Option<&dyn Zeroer> {
        None
    }

    fn equals(&self, _other: &dyn Val) -> bool {
        false
    }

    /// Clones the value into a box whose trait-object lifetime `'v` is any
    /// lifetime `Self` outlives. Implementations must not shorten a borrow:
    /// a value borrowing for `'a` clones into a value borrowing for `'a`.
    fn clone_as_boxed<'v>(&self) -> Box<dyn Val + 'v>
    where
        Self: 'v;

    /// `'static` implementations return `Some(self)`; this is what backs
    /// `downcast_ref` for them. Values that borrow
    /// cannot be `Any` and keep the default `None`.
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
pub trait StaticVal: Val + 'static {}

/// A type that can be recovered by reference from a `&'b (dyn Val + 'v)`.
///
/// Implemented for every [`StaticVal`] through [`Val::as_any`], and for the
/// built-in borrowing types through [`Val::as_builtin`]. A borrowing type
/// is recovered as `T<'v>`: the downcast keeps the value's own lifetime
/// bound rather than shortening it to the borrow `'b`.
pub trait FromVal<'b, 'v>: Sized {
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
    #[cfg(feature = "structs")]
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
    #[cfg(feature = "structs")]
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

#[cfg(feature = "structs")]
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
pub enum CowVal<'b, 'v> {
    Borrowed(&'b (dyn Val + 'v)),
    Owned(Box<dyn Val + 'v>),
}

impl<'b, 'v> CowVal<'b, 'v> {
    /// Boxes an owned value.
    pub fn owned<T: Val + 'v>(val: T) -> Self {
        CowVal::Owned(Box::new(val))
    }

    pub fn is_borrowed(&self) -> bool {
        matches!(self, CowVal::Borrowed(_))
    }

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
