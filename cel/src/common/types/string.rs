use crate::common::traits::{Adder, Comparer};
use crate::common::types::Type;
use crate::common::value::{BorrowedVal, Val};
use crate::ExecutionError;
use std::borrow::Cow;
use std::cmp::Ordering;
use std::ops::Deref;
use std::string::String as StdString;

#[derive(Debug, Default, Eq, Hash, PartialEq, PartialOrd, Ord)]
pub struct String(
    // Safety invariant: the `'static` on this `Cow` may be a lie. The only
    // constructor that stores the `Borrowed` variant is
    // `<BorrowedVal<'a, String> as From<&'a str>>` in this file, which
    // launders an `&'a str` to `&'static str` via `leak_ref`. Such a
    // value is sound to read only while the enclosing
    // `BorrowedVal<'a, String>` is alive, i.e. for `'a`.
    //
    // Consequently, no code may move or copy the `Borrowed` variant out
    // of a `&String` into a value that is not itself bounded by `'a`.
    // Every function that produces an owned `String`, `Box<dyn Val>`,
    // `StdString`, or `Cow<'static, str>` from `&self` must go through
    // `Cow::Owned` (see `Clone`, `Val::clone_as_boxed`, and
    // `into_inner` in this file). Returning `&str` bounded by `&self`
    // is fine. Adding a new such producer without re-auditing this
    // invariant reintroduces a use-after-free reachable from safe code.
    //
    // Non-laundered values are always `Cow::Owned`; every other
    // constructor in this file allocates.
    Cow<'static, str>,
);

impl String {
    pub fn into_inner(self) -> StdString {
        self.clone().into()
    }

    pub fn inner(&self) -> &str {
        self.0.as_ref()
    }
}

impl Clone for String {
    // Upholds the Safety invariant on the field: the clone is always
    // `Cow::Owned`, never a copy of a (possibly laundered) `Borrowed`.
    fn clone(&self) -> Self {
        Self(Cow::Owned(StdString::from(self.inner())))
    }
}

impl Deref for String {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.inner()
    }
}

impl Val for String {
    fn get_type(&self) -> Type<'_> {
        super::STRING_TYPE
    }

    fn as_adder(&self) -> Option<&dyn Adder> {
        Some(self)
    }

    fn as_comparer(&self) -> Option<&dyn Comparer> {
        Some(self)
    }

    fn equals(&self, other: &dyn Val) -> bool {
        other
            .downcast_ref::<Self>()
            .is_some_and(|other| self.0 == other.0)
    }

    fn clone_as_boxed(&self) -> Box<dyn Val> {
        // Upholds the Safety invariant on the field: `Box<dyn Val>` is
        // `'static`, so the boxed value must not carry a laundered
        // `Borrowed`. `self.clone()` always produces `Cow::Owned`.
        // `String(self.0.clone())` would copy the `Borrowed` pointer.
        Box::new(self.clone())
    }
}

impl Adder for String {
    fn add<'a>(&'a self, rhs: &dyn Val) -> Result<Cow<'a, dyn Val>, ExecutionError> {
        if let Some(rhs) = rhs.downcast_ref::<Self>() {
            let mut s = StdString::with_capacity(rhs.0.len() + self.0.len());
            s.push_str(&self.0);
            s.push_str(&rhs.0);
            Ok(Cow::<dyn Val>::Owned(Box::new(Self(s.to_owned().into()))))
        } else {
            Err(ExecutionError::UnsupportedBinaryOperator(
                "add",
                (self as &dyn Val).try_into()?,
                rhs.try_into()?,
            ))
        }
    }
}

impl Comparer for String {
    fn compare(&self, rhs: &dyn Val) -> Result<Ordering, ExecutionError> {
        if let Some(rhs) = rhs.downcast_ref::<Self>() {
            Ok(self.0.cmp(&rhs.0))
        } else {
            Err(ExecutionError::NoSuchOverload)
        }
    }
}

impl From<StdString> for String {
    fn from(v: StdString) -> Self {
        Self(Cow::Owned(v))
    }
}

impl From<String> for StdString {
    fn from(v: String) -> Self {
        v.0.into_owned()
    }
}

impl From<&str> for String {
    fn from(value: &str) -> Self {
        Self(StdString::from(value).to_owned().into())
    }
}

impl TryFrom<Box<dyn Val>> for StdString {
    type Error = Box<dyn Val>;

    fn try_from(value: Box<dyn Val>) -> Result<Self, Self::Error> {
        super::cast_boxed::<String>(value).map(|s| s.into_inner())
    }
}

impl<'a> TryFrom<&'a dyn Val> for &'a str {
    type Error = &'a dyn Val;
    fn try_from(value: &'a dyn Val) -> Result<Self, Self::Error> {
        if let Some(s) = value.downcast_ref::<String>() {
            return Ok(s.inner());
        }
        Err(value)
    }
}

impl<'a> From<&'a str> for BorrowedVal<'a, String> {
    fn from(value: &'a str) -> Self {
        // SAFETY:
        // Operation: `super::leak_ref::<'static, str>(value)`, the sole
        // unsafe call in this block. `BorrowedVal::new` is a safe fn.
        // Contract: `leak_ref`'s five `# Safety` preconditions must hold
        // for the caller-chosen lifetime, here `'static`. Because the
        // chosen lifetime exceeds the real one, `leak_ref`'s docs require
        // a named project-local invariant that bounds every use of the
        // returned reference. That invariant is the Safety invariant on
        // the `Cow<'static, str>` field of `String` in this file, plus
        // the Safety invariant on `BorrowedVal::phantom` in
        // `common::value.rs`.
        // Evidence:
        //   1–3. AXIOM (Rust Reference, reference validity): a live
        //        `&'a str` is non-null, aligned, and covers `len` bytes
        //        of initialized UTF-8 in one live allocation, with
        //        correct length metadata. The raw pointer decayed from
        //        `value` therefore satisfies (1), (2), and (3) for `'a`.
        //   4–5. TYPE FACT: `value: &'a str`, so the allocation is live
        //        and no `&mut` alias exists for all of `'a`. The leaked
        //        reference is only ever read within `'a` because:
        //        - INVARIANT (`BorrowedVal::phantom`): the returned
        //          `BorrowedVal<'a, String>` is bounded by `'a`, and its
        //          explicit `Drop` impl makes the borrow checker require
        //          `'a` to be live through the drop as well, so the
        //          `Box<String>` inside is dropped before `'a` ends.
        //        - INVARIANT (`String` field): every path from `&String`
        //          to an owned value goes through `Cow::Owned`, so the
        //          laundered `Borrowed` cannot be copied into a value
        //          that outlives the `BorrowedVal`. The only reads of
        //          the pointer are through `&str`s bounded by a borrow
        //          of the `BorrowedVal`, hence by `'a`.
        //        Therefore (4) and (5) hold at every read of the leaked
        //        reference, and no read or drop occurs after `'a`.
        // Postcondition: the returned `BorrowedVal<'a, String>` behaves
        // as a borrow of `value`; every `&str` reachable through it has
        // lifetime `<= 'a`, and every owned value derived from it copies
        // the bytes.
        unsafe {
            let leaked: &'static str = super::leak_ref(value);
            let val = String(Cow::Borrowed(leaked));
            BorrowedVal::new(val)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::StdString;
    use super::String;
    use crate::common::value::{BorrowedVal, Val};

    #[test]
    fn test_try_into_string() {
        let str: Box<dyn Val> = Box::new(String::from("cel-rust"));
        assert_eq!(Ok(StdString::from("cel-rust")), str.try_into())
    }

    #[test]
    fn test_try_into_str() {
        let str: Box<dyn Val> = Box::new(String::from("cel-rust"));
        assert_eq!(Ok("cel-rust"), str.as_ref().try_into())
    }

    #[test]
    fn test_str() {
        let string = StdString::from("cel-rust");
        let val = {
            let s = string.as_ref();
            BorrowedVal::from(s)
        };
        let r = val.inner();
        assert_eq!(string.as_str(), r.inner());
        assert!(std::ptr::eq(string.as_str(), r.inner()));
        assert!(std::ptr::eq(string.as_str(), r.inner()));
    }

    /// Regression: `clone_as_boxed` (and therefore `Cow::<dyn Val>::
    /// into_owned`) must copy the bytes out of a laundered `Borrowed`,
    /// never the pointer. Run under Miri to catch a use-after-free.
    #[test]
    fn test_clone_as_boxed_outlives_borrow() {
        let (boxed, owned): (Box<dyn Val>, Box<dyn Val>) = {
            let string = StdString::from("cel-rust");
            let val = BorrowedVal::from(string.as_str());
            let cow: std::borrow::Cow<dyn Val> = std::borrow::Cow::Borrowed(val.inner());
            (val.inner().clone_as_boxed(), cow.into_owned())
        };
        let boxed = boxed.downcast_ref::<String>().unwrap();
        let owned = owned.downcast_ref::<String>().unwrap();
        assert_eq!(boxed.inner(), "cel-rust");
        assert_eq!(owned.inner(), "cel-rust");
    }

    #[test]
    fn test_clone() {
        let s = {
            let string = StdString::from("cel-rust");
            let val = {
                let s = string.as_ref();
                BorrowedVal::from(s)
            };
            val.inner().clone()
        };
        assert_eq!(s, String::from("cel-rust"));
    }
}
