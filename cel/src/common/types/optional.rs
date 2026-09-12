use crate::common::traits::Zeroer;
use crate::common::types::{self, CelBool, Type, OPTIONAL_TYPE};
use crate::common::value::{Builtin, BuiltinRef, CowVal, Val};
use crate::ExecutionError;
use std::sync::Arc;

/// A CEL optional whose value may borrow data for `'v`.
#[derive(Debug)]
pub struct Optional<'v>(Option<OptionalInternal<'v>>);

#[derive(Debug)]
enum OptionalInternal<'v> {
    Box(Box<dyn Val + 'v>),
    Arc(Arc<dyn Val + 'v>),
}

impl<'v> OptionalInternal<'v> {
    fn clone_as_boxed<'w>(&self) -> Box<dyn Val + 'w>
    where
        'v: 'w,
    {
        match self {
            OptionalInternal::Box(val) => val.clone_as_boxed(),
            OptionalInternal::Arc(val) => val.clone_as_boxed(),
        }
    }

    fn as_val<'b>(&'b self) -> &'b (dyn Val + 'v) {
        match self {
            OptionalInternal::Box(b) => b.as_ref(),
            OptionalInternal::Arc(a) => a.as_ref(),
        }
    }
}

impl<'v> Val for Optional<'v> {
    fn get_type(&self) -> &Type {
        &super::OPTIONAL_TYPE
    }

    fn equals(&self, other: &dyn Val) -> bool {
        let Some(other) = other.downcast_ref::<Optional>() else {
            return false;
        };
        match (self.option(), other.option()) {
            (None, None) => true,
            (Some(a), Some(b)) => a.equals(b),
            _ => false,
        }
    }

    fn clone_as_boxed<'w>(&self) -> Box<dyn Val + 'w>
    where
        Self: 'w,
    {
        match &self.0 {
            None => Box::new(Optional(None)),
            Some(val) => val.clone_as_boxed(),
        }
    }

    fn as_builtin<'b, 'w>(&'b self) -> BuiltinRef<'b, 'w>
    where
        Self: 'w,
    {
        BuiltinRef::Optional(self)
    }

    fn into_builtin<'w>(self: Box<Self>) -> Option<Builtin<'w>>
    where
        Self: 'w,
    {
        Some(Builtin::Optional(*self))
    }
}

impl<'v> Optional<'v> {
    pub fn none() -> Self {
        Optional(None)
    }

    pub fn of(val: Box<dyn Val + 'v>) -> Self {
        Optional(Some(OptionalInternal::Box(val)))
    }

    pub fn map(&self, f: impl FnOnce(&(dyn Val + 'v)) -> Box<dyn Val + 'v>) -> Self {
        self.0
            .as_ref()
            .map(|val| Optional(Some(OptionalInternal::Box(f(val.as_val())))))
            .unwrap_or(Optional(None))
    }

    pub fn option<'b>(&'b self) -> Option<&'b (dyn Val + 'v)> {
        self.0.as_ref().map(OptionalInternal::as_val)
    }

    pub fn inner<'b>(&'b self) -> Option<&'b (dyn Val + 'v)> {
        self.option()
    }
}

impl<'v> From<Option<Box<dyn Val + 'v>>> for Optional<'v> {
    fn from(val: Option<Box<dyn Val + 'v>>) -> Self {
        Optional(val.map(OptionalInternal::Box))
    }
}

impl<'v> From<Box<dyn Val + 'v>> for Optional<'v> {
    fn from(val: Box<dyn Val + 'v>) -> Self {
        Optional(Some(OptionalInternal::Box(val)))
    }
}

impl<'v> From<Option<Arc<dyn Val + 'v>>> for Optional<'v> {
    fn from(val: Option<Arc<dyn Val + 'v>>) -> Self {
        Optional(val.map(OptionalInternal::Arc))
    }
}

impl<'v> From<Optional<'v>> for Option<Box<dyn Val + 'v>> {
    fn from(val: Optional<'v>) -> Option<Box<dyn Val + 'v>> {
        val.0.map(|val| match val {
            OptionalInternal::Box(b) => b,
            OptionalInternal::Arc(a) => a.clone_as_boxed(),
        })
    }
}

impl<'v> From<Optional<'v>> for Option<Arc<dyn Val + 'v>> {
    fn from(val: Optional<'v>) -> Option<Arc<dyn Val + 'v>> {
        val.0.map(|i| match i {
            OptionalInternal::Arc(a) => a,
            OptionalInternal::Box(b) => Arc::from(b),
        })
    }
}

fn optional_none<'b, 'v>(_args: Vec<CowVal<'b, 'v>>) -> Result<CowVal<'b, 'v>, ExecutionError> {
    Ok(CowVal::owned(Optional::none()))
}

fn optional_of<'b, 'v>(mut args: Vec<CowVal<'b, 'v>>) -> Result<CowVal<'b, 'v>, ExecutionError> {
    let value = args.remove(0);
    Ok(CowVal::owned(Optional::of(value.into_owned())))
}

fn optional_of_non_zero_value<'b, 'v>(
    args: Vec<CowVal<'b, 'v>>,
) -> Result<CowVal<'b, 'v>, ExecutionError> {
    match args[0].as_zeroer().is_some_and(Zeroer::is_zero_value) {
        true => optional_none(args),
        false => optional_of(args),
    }
}

/// The outcome of [`unwrap_optional`].
pub(crate) enum Unwrapped<'b, 'v> {
    /// The value was not an optional; handed back unchanged.
    NotOptional(CowVal<'b, 'v>),
    /// An optional holding this value.
    Some(CowVal<'b, 'v>),
    /// An empty optional.
    None,
}

/// Unwraps an optional without copying its value: a borrowed optional
/// yields a borrow of its value, an owned one moves the value out.
pub(crate) fn unwrap_optional<'b, 'v>(value: CowVal<'b, 'v>) -> Unwrapped<'b, 'v> {
    match value {
        CowVal::Borrowed(v) => match v.downcast_ref::<Optional>() {
            None => Unwrapped::NotOptional(CowVal::Borrowed(v)),
            Some(opt) => match opt.option() {
                Some(inner) => Unwrapped::Some(CowVal::Borrowed(inner)),
                None => Unwrapped::None,
            },
        },
        CowVal::Owned(b) => {
            if b.downcast_ref::<Optional>().is_none() {
                return Unwrapped::NotOptional(CowVal::Owned(b));
            }
            match super::into_builtin(b) {
                Ok(Builtin::Optional(opt)) => match Option::<Box<dyn Val + 'v>>::from(opt) {
                    Some(inner) => Unwrapped::Some(CowVal::Owned(inner)),
                    None => Unwrapped::None,
                },
                _ => unreachable!("checked to be an `Optional` above"),
            }
        }
    }
}

/// Like [`unwrap_optional`], erroring when the value is not an optional.
fn expect_optional<'b, 'v>(this: CowVal<'b, 'v>) -> Result<Option<CowVal<'b, 'v>>, ExecutionError> {
    match unwrap_optional(this) {
        Unwrapped::NotOptional(_) => Err(ExecutionError::NoSuchOverload),
        Unwrapped::Some(v) => Ok(Some(v)),
        Unwrapped::None => Ok(None),
    }
}

fn optional_value<'b, 'v>(mut args: Vec<CowVal<'b, 'v>>) -> Result<CowVal<'b, 'v>, ExecutionError> {
    expect_optional(args.remove(0))?
        .ok_or_else(|| ExecutionError::function_error("value", "optional.none() dereference"))
}

fn optional_has_value<'b, 'v>(args: Vec<CowVal<'b, 'v>>) -> Result<CowVal<'b, 'v>, ExecutionError> {
    let has = args[0]
        .downcast_ref::<Optional>()
        .ok_or(ExecutionError::NoSuchOverload)?
        .option()
        .is_some();
    Ok(CowVal::owned(CelBool::from(has)))
}

fn optional_or_optional<'b, 'v>(
    mut args: Vec<CowVal<'b, 'v>>,
) -> Result<CowVal<'b, 'v>, ExecutionError> {
    let other = args.remove(1);
    let this = args.remove(0);
    if this
        .downcast_ref::<Optional>()
        .ok_or(ExecutionError::NoSuchOverload)?
        .option()
        .is_some()
    {
        Ok(this)
    } else {
        Ok(other)
    }
}

fn optional_or_value<'b, 'v>(
    mut args: Vec<CowVal<'b, 'v>>,
) -> Result<CowVal<'b, 'v>, ExecutionError> {
    let other = args.remove(1);
    Ok(expect_optional(args.remove(0))?.unwrap_or(other))
}

pub(crate) fn stdlib(env: &mut crate::Env) {
    env.add_overload("optional.none", "optional_none", vec![], optional_none)
        .expect("Must be unique");
    env.add_overload(
        "optional.of",
        "optional_of",
        vec![types::DYN_TYPE],
        optional_of,
    )
    .expect("Must be unique");
    env.add_overload(
        "optional.ofNonZeroValue",
        "optional_ofNonZeroValue",
        vec![types::DYN_TYPE],
        optional_of_non_zero_value,
    )
    .expect("Must be unique");
    env.add_member_overload(
        "value",
        "optional_value",
        OPTIONAL_TYPE,
        vec![],
        optional_value,
    )
    .expect("Must be unique");
    env.add_member_overload(
        "hasValue",
        "optional_has_value",
        OPTIONAL_TYPE,
        vec![],
        optional_has_value,
    )
    .expect("Must be unique");
    env.add_member_overload(
        "or",
        "optional_or_optional",
        OPTIONAL_TYPE,
        vec![OPTIONAL_TYPE],
        optional_or_optional,
    )
    .expect("Must be unique");
    env.add_member_overload(
        "orValue",
        "optional_or_value",
        OPTIONAL_TYPE,
        vec![types::DYN_TYPE],
        optional_or_value,
    )
    .expect("Must be unique");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::types::{self, CelInt, CelString};

    #[test]
    fn is_assignable() {
        let s = CelString::from("foo");
        assert!(types::OPTIONAL_TYPE.is_assignable(&s));
        let i = CelInt::from(42);
        assert!(types::OPTIONAL_TYPE.is_assignable(&i));
    }

    fn non_optional() -> CowVal<'static, 'static> {
        CowVal::owned(CelInt::from(42))
    }

    fn some_optional() -> CowVal<'static, 'static> {
        CowVal::owned(Optional::of(Box::new(CelInt::from(1))))
    }

    #[test]
    fn optional_value_rejects_non_optional_receiver() {
        let err = optional_value(vec![non_optional()]).unwrap_err();
        assert!(matches!(err, ExecutionError::NoSuchOverload));
    }

    #[test]
    fn optional_has_value_rejects_non_optional_receiver() {
        let err = optional_has_value(vec![non_optional()]).unwrap_err();
        assert!(matches!(err, ExecutionError::NoSuchOverload));
    }

    #[test]
    fn optional_or_optional_rejects_non_optional_receiver() {
        let err = optional_or_optional(vec![non_optional(), some_optional()]).unwrap_err();
        assert!(matches!(err, ExecutionError::NoSuchOverload));
    }

    #[test]
    fn optional_or_value_rejects_non_optional_receiver() {
        let err = optional_or_value(vec![non_optional(), non_optional()]).unwrap_err();
        assert!(matches!(err, ExecutionError::NoSuchOverload));
    }

    #[test]
    fn optional_value_borrows_through() {
        let owned = String::from("cel");
        let opt = Optional::of(Box::new(CelString::from(owned.as_str())));
        let out = optional_value(vec![CowVal::Borrowed(&opt)]).unwrap();
        assert!(out.is_borrowed());
        let s = out.downcast_ref::<CelString>().unwrap();
        assert!(std::ptr::eq(s.inner(), owned.as_str()));
    }

    #[test]
    fn equals_two_nones() {
        assert!(Optional::none().equals(&Optional::none()));
    }

    #[test]
    fn equals_same_some() {
        let a = Optional::of(Box::new(CelInt::from(1)));
        let b = Optional::of(Box::new(CelInt::from(1)));
        assert!(a.equals(&b));
    }

    #[test]
    fn not_equals_none_vs_some() {
        let a = Optional::none();
        let b = Optional::of(Box::new(CelInt::from(1)));
        assert!(!a.equals(&b));
        assert!(!b.equals(&a));
    }

    #[test]
    fn not_equals_different_somes() {
        let a = Optional::of(Box::new(CelInt::from(1)));
        let b = Optional::of(Box::new(CelInt::from(2)));
        assert!(!a.equals(&b));
    }

    #[test]
    fn not_equals_non_optional() {
        let a = Optional::of(Box::new(CelInt::from(1)));
        let b = CelInt::from(1);
        assert!(!a.equals(&b));
    }
}
