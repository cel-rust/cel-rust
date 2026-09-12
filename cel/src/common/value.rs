use crate::common::traits::{
    Adder, Comparer, Container, Divider, Indexer, Iterable, Modder, Multiplier, Negator, Subtractor,
};
use crate::common::types::Type;
use std::any::Any;
use std::fmt::Debug;
use std::marker::PhantomData;

pub trait Val: Any + Debug + Send + Sync {
    fn get_type(&self) -> Type<'_>;

    fn as_adder(&self) -> Option<&dyn Adder> {
        None
    }

    fn as_comparer(&self) -> Option<&dyn Comparer> {
        None
    }

    fn as_container(&self) -> Option<&dyn Container> {
        None
    }

    fn as_divider(&self) -> Option<&dyn Divider> {
        None
    }

    fn as_indexer(&self) -> Option<&dyn Indexer> {
        None
    }

    fn into_indexer(self: Box<Self>) -> Option<Box<dyn Indexer>> {
        None
    }

    fn as_iterable(&self) -> Option<&dyn Iterable> {
        None
    }

    fn as_modder(&self) -> Option<&dyn Modder> {
        None
    }

    fn as_multiplier(&self) -> Option<&dyn Multiplier> {
        None
    }

    fn as_negator(&self) -> Option<&dyn Negator> {
        None
    }

    fn as_subtractor(&self) -> Option<&dyn Subtractor> {
        None
    }

    fn equals(&self, _other: &dyn Val) -> bool {
        false
    }

    fn clone_as_boxed(&self) -> Box<dyn Val>;
}

impl dyn Val {
    pub fn downcast_ref<T: Val>(&self) -> Option<&T> {
        <dyn Any>::downcast_ref::<T>(self)
    }
}

impl ToOwned for dyn Val {
    type Owned = Box<dyn Val>;

    fn to_owned(&self) -> Self::Owned {
        self.clone_as_boxed()
    }
}

impl PartialEq for dyn Val {
    fn eq(&self, other: &Self) -> bool {
        self.equals(other)
    }
}

/// A `T: Val` that borrows data for `'a`, even though `T` itself carries
/// no lifetime.
///
/// The value cannot outlive `'a`, and because of its `Drop` impl it
/// cannot even be *dropped* after `'a` ends:
///
/// ```compile_fail,E0597
/// use cel::common::types::CelString;
/// use cel::common::value::BorrowedVal;
///
/// let bv: BorrowedVal<'_, CelString>;
/// {
///     let s = String::from("bar");
///     bv = BorrowedVal::from(s.as_str());
/// } // error[E0597]: `s` does not live long enough (bv dropped later)
/// ```
pub struct BorrowedVal<'a, T: Val> {
    val: Box<T>,
    // Safety invariant: this field is load-bearing. `Box<T>` does not
    // itself mention `'a`, so without this `PhantomData` the compiler
    // would not bound `BorrowedVal<'a, T>` by `'a`. That bound is what
    // keeps every borrow reachable through the value, including a
    // lifetime-laundered interior `&'static` produced inside `T` (see
    // `<BorrowedVal<'a, String> as From<&'a str>>` in
    // `common::types::string.rs`), bounded at `<= 'a` and prevented from
    // outliving the borrow this wrapper represents. Do not remove this
    // field, and do not weaken its variance (e.g. to `PhantomData<fn()
    // -> &'a ()>` for contravariance, or `PhantomData<*const &'a ()>`
    // for invariance without a borrow) without re-auditing every unsafe
    // `From` / constructor that produces a `BorrowedVal`.
    //
    // The `PhantomData` alone does not make drop-check require `'a` to
    // be live when the `BorrowedVal` is dropped; the explicit `Drop`
    // impl below does. Do not remove that impl either: without it a
    // `BorrowedVal` can be dropped after its referent is freed, and the
    // drop glue of `T` then runs over a dangling laundered reference.
    phantom: PhantomData<&'a ()>,
}

// This impl exists solely for drop-check. A type with a `Drop` impl is
// considered to access every lifetime in its type when dropped, so the
// borrow checker requires `'a` to be live at the drop of a
// `BorrowedVal<'a, T>`. That guarantees the inner `Box<T>` (and any
// laundered `&'a` it holds, see the Safety invariant on `phantom`) is
// dropped before the referent it borrows. The `compile_fail` doctest on
// `BorrowedVal` pins this behaviour.
impl<'a, T: Val> Drop for BorrowedVal<'a, T> {
    fn drop(&mut self) {}
}

impl<'a, T: Val> BorrowedVal<'a, T> {
    pub fn new(val: T) -> Self {
        Self {
            val: Box::new(val),
            phantom: PhantomData,
        }
    }

    pub fn inner(&self) -> &T {
        self.val.as_ref()
    }
}

#[cfg(test)]
mod test {
    use crate::common::types;
    use crate::common::value::Val;
    use std::borrow::Cow;

    fn test(val: &dyn Val) -> bool {
        val.get_type() == types::STRING_TYPE
    }

    #[test]
    fn test_cow() {
        let s1 = types::CelString::from("cel");
        let s2 = types::CelString::from("cel");
        let b: Box<dyn Val> = Box::new(s1);
        let cow: Cow<dyn Val> = Cow::Owned(b);
        let borrowed: Cow<dyn Val> = Cow::Borrowed(&s2);
        assert!(test(borrowed.as_ref()));
        assert!(test(cow.as_ref()));
        assert!(test(borrowed.clone().as_ref()));
    }
}
