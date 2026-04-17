use crate::common::traits::Zeroer;
use crate::common::types::{self, CelBool, Type, OPTIONAL_TYPE};
use crate::common::value::Val;
use crate::ExecutionError;
use std::borrow::Cow;
use std::sync::Arc;

#[derive(Debug)]
pub struct Optional(Option<OptionalInternal>);

#[derive(Debug)]
enum OptionalInternal {
    Box(Box<dyn Val>),
    Arc(Arc<dyn Val>),
}

impl OptionalInternal {
    fn clone_as_boxed(&self) -> Box<dyn Val> {
        match self {
            OptionalInternal::Box(val) => val.clone_as_boxed(),
            OptionalInternal::Arc(val) => val.clone_as_boxed(),
        }
    }
}

impl Val for Optional {
    fn get_type(&self) -> &Type {
        &super::OPTIONAL_TYPE
    }

    fn clone_as_boxed(&self) -> Box<dyn Val> {
        match &self.0 {
            None => Box::new(Optional(None)),
            Some(val) => val.clone_as_boxed(),
        }
    }
}

impl Optional {
    pub fn none() -> Self {
        Optional(None)
    }

    pub fn of(val: Box<dyn Val>) -> Self {
        Optional(Some(OptionalInternal::Box(val)))
    }

    pub fn map(&self, f: impl FnOnce(&dyn Val) -> Box<dyn Val>) -> Self {
        self.0
            .as_ref()
            .map(|val| {
                let m = match val {
                    OptionalInternal::Box(b) => f(b.as_ref()),
                    OptionalInternal::Arc(a) => f(a.as_ref()),
                };
                Optional(Some(OptionalInternal::Box(m)))
            })
            .unwrap_or(Optional(None))
    }

    pub fn option(&self) -> Option<&dyn Val> {
        self.0.as_ref().map(|val| match val {
            OptionalInternal::Box(b) => b.as_ref(),
            OptionalInternal::Arc(a) => a.as_ref(),
        })
    }

    pub fn inner(&self) -> Option<&dyn Val> {
        self.0.as_ref().map(|val| match val {
            OptionalInternal::Box(b) => b.as_ref(),
            OptionalInternal::Arc(a) => a.as_ref(),
        })
    }
}

impl From<Option<Box<dyn Val>>> for Optional {
    fn from(val: Option<Box<dyn Val>>) -> Self {
        Optional(val.map(OptionalInternal::Box))
    }
}

impl From<Box<dyn Val>> for Optional {
    fn from(val: Box<dyn Val>) -> Self {
        Optional(Some(OptionalInternal::Box(val)))
    }
}

impl From<Option<Arc<dyn Val>>> for Optional {
    fn from(val: Option<Arc<dyn Val>>) -> Self {
        Optional(val.map(OptionalInternal::Arc))
    }
}

impl From<Optional> for Option<Box<dyn Val>> {
    fn from(val: Optional) -> Option<Box<dyn Val>> {
        val.0.map(|val| val.clone_as_boxed())
    }
}

impl From<Optional> for Option<Arc<dyn Val>> {
    fn from(val: Optional) -> Option<Arc<dyn Val>> {
        val.0.map(|i| match i {
            OptionalInternal::Arc(a) => a,
            OptionalInternal::Box(b) => Arc::from(b),
        })
    }
}

fn optional_none<'a>(_args: Vec<Cow<'a, dyn Val>>) -> Result<Cow<'a, dyn Val>, ExecutionError> {
    Ok(Cow::<dyn Val>::Owned(Box::new(Optional::none())))
}

fn optional_of<'a>(args: Vec<Cow<'a, dyn Val>>) -> Result<Cow<'a, dyn Val>, ExecutionError> {
    let mut args = args;
    let value = args.remove(0);
    Ok(Cow::<dyn Val>::Owned(Box::new(Optional::of(
        value.into_owned(),
    ))))
}

fn optional_of_non_zero_value<'a>(
    args: Vec<Cow<'a, dyn Val>>,
) -> Result<Cow<'a, dyn Val>, ExecutionError> {
    match args[0].as_zeroer().is_some_and(Zeroer::is_zero_value) {
        true => optional_none(args),
        false => optional_of(args),
    }
}

fn optional_value<'a>(args: Vec<Cow<'a, dyn Val>>) -> Result<Cow<'a, dyn Val>, ExecutionError> {
    // TODO: This can be optimized to avoid cloning and "just" pass the `Cow`
    // but we either need to deal with the `Arc` case or wait until that's all ripped out!
    let mut args = args;
    args.remove(0)
        .downcast_ref::<Optional>()
        .expect("must be `CelOptional`")
        .option()
        .map(|v| Cow::Owned(v.to_owned()))
        .ok_or_else(|| ExecutionError::function_error("value", "optional.none() dereference"))
}

fn optional_has_value<'a>(args: Vec<Cow<'a, dyn Val>>) -> Result<Cow<'a, dyn Val>, ExecutionError> {
    super::unary_fn(args, OPTIONAL_TYPE, |opt: &Optional| {
        Ok(Box::new(CelBool::from(opt.option().is_some())))
    })
}

fn optional_or_optional<'a>(
    args: Vec<Cow<'a, dyn Val>>,
) -> Result<Cow<'a, dyn Val>, ExecutionError> {
    let mut args = args;
    let other = args.remove(1);
    let this = args.remove(0);
    if this
        .downcast_ref::<Optional>()
        .expect("Must be an `Optional`")
        .option()
        .is_some()
    {
        Ok(this)
    } else {
        Ok(other)
    }
}

fn optional_or_value<'a>(args: Vec<Cow<'a, dyn Val>>) -> Result<Cow<'a, dyn Val>, ExecutionError> {
    // TODO: This can be optimized to avoid cloning and "just" pass the `Cow`
    // but we either need to deal with the `Arc` case or wait until that's all ripped out!
    let mut args = args;
    let other = args.remove(1);
    Ok(args
        .remove(0)
        .downcast_ref::<Optional>()
        .expect("must be `CelOptional`")
        .option()
        .map(|v| Cow::Owned(v.to_owned()))
        .unwrap_or(other))
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
    use crate::common::types::{self, CelInt, CelString};

    #[test]
    fn is_assignable() {
        let s = CelString::from("foo");
        assert!(types::OPTIONAL_TYPE.is_assignable(&s));
        let i = CelInt::from(42);
        assert!(types::OPTIONAL_TYPE.is_assignable(&i));
    }
}
