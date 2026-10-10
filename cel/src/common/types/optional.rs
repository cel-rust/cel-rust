use crate::common::ast::{operators, CallExpr, Expr, IdedExpr};
use crate::common::traits::Zeroer;
use crate::common::types::{self, CelBool, Type, OPTIONAL_TYPE};
use crate::common::value::{Builtin, BuiltinRef, CowVal, Val};
use crate::parser::{Macro, MacroExprHelper, ParseError};
use crate::ExecutionError;
use std::mem;
use std::sync::Arc;

/// The variable `optMap` and `optFlatMap` bind a target that isn't an
/// identifier to, so that it is evaluated once.
const TARGET: &str = "@target";

/// A CEL optional whose value may borrow data for `'v`.
///
/// Of type [`OPTIONAL_TYPE`](super::OPTIONAL_TYPE), `optional_type`, whatever
/// its value: `optional.of(x)` holds `x`, `optional.none()` holds nothing. It
/// supports no operator of its own; its methods, `hasValue()`, `value()`,
/// `orValue()`, `or()` and the like, are overloads of the standard library,
/// registered while the environment supports optional values, as it does by
/// default: see [`Env::with_optional_support`](crate::Env::with_optional_support).
/// Selecting a field of an optional, or indexing one, applies to the value it
/// holds, and yields another optional. Two optionals are equal when both are
/// empty, or both hold equal values.
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
        <Self as Val>::cel_type()
    }

    fn cel_type() -> &'static Type {
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
            Some(val) => Box::new(Optional(Some(OptionalInternal::Box(val.clone_as_boxed())))),
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
    /// `optional.none()`, which holds no value.
    pub fn none() -> Self {
        Optional(None)
    }

    /// `optional.of(val)`.
    pub fn of(val: Box<dyn Val + 'v>) -> Self {
        Optional(Some(OptionalInternal::Box(val)))
    }

    /// An optional of `f` applied to the value, or `optional.none()` if this
    /// holds none.
    pub fn map(&self, f: impl FnOnce(&(dyn Val + 'v)) -> Box<dyn Val + 'v>) -> Self {
        self.0
            .as_ref()
            .map(|val| Optional(Some(OptionalInternal::Box(f(val.as_val())))))
            .unwrap_or(Optional(None))
    }

    /// The value, if there is one.
    pub fn option<'b>(&'b self) -> Option<&'b (dyn Val + 'v)> {
        self.0.as_ref().map(OptionalInternal::as_val)
    }

    /// The value, if there is one; the same as [`option`](Self::option).
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

/// Like [`unwrap_optional`], erroring when the receiver is not an optional.
///
/// The error names `function` and the runtime types of the receiver followed by
/// the remaining arguments `rest`; it is only built when there is an error.
fn expect_optional<'b, 'v>(
    function: &str,
    this: CowVal<'b, 'v>,
    rest: &[CowVal<'b, 'v>],
) -> Result<Option<CowVal<'b, 'v>>, ExecutionError> {
    match unwrap_optional(this) {
        Unwrapped::NotOptional(this) => Err(ExecutionError::overload_for_values(
            function,
            std::iter::once(this.as_ref()).chain(rest.iter().map(|arg| arg.as_ref())),
            true,
        )),
        Unwrapped::Some(v) => Ok(Some(v)),
        Unwrapped::None => Ok(None),
    }
}

fn optional_value<'b, 'v>(mut args: Vec<CowVal<'b, 'v>>) -> Result<CowVal<'b, 'v>, ExecutionError> {
    let this = args.remove(0);
    expect_optional("value", this, &args)?
        .ok_or_else(|| ExecutionError::function_error("value", "optional.none() dereference"))
}

fn optional_has_value<'b, 'v>(args: Vec<CowVal<'b, 'v>>) -> Result<CowVal<'b, 'v>, ExecutionError> {
    let has = args[0]
        .downcast_ref::<Optional>()
        .ok_or_else(|| {
            ExecutionError::overload_for_values(
                "hasValue",
                args.iter().map(|arg| arg.as_ref()),
                true,
            )
        })?
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
        .ok_or_else(|| {
            ExecutionError::overload_for_values("or", [this.as_ref(), other.as_ref()], true)
        })?
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
    let this = args.remove(0);
    Ok(expect_optional("orValue", this, std::slice::from_ref(&other))?.unwrap_or(other))
}

pub(crate) fn stdlib(env: &mut crate::Env) {
    env.add_type(types::OPTIONAL_TYPE).expect("Must be unique");
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
    env.add_macro(Macro::receiver("optMap", 2, |helper, target, args| {
        opt_map_macro(helper, target, args, true)
    }))
    .expect("Must be unique");
    env.add_macro(Macro::receiver("optFlatMap", 2, |helper, target, args| {
        opt_map_macro(helper, target, args, false)
    }))
    .expect("Must be unique");
}

/// `t.optMap(v, e)` is `t.hasValue() ? optional.of(<e with v = t.value()>) :
/// optional.none()`, and `t.optFlatMap(v, e)` is the same without the
/// `optional.of`, as `wrap` says. A target that isn't an identifier is bound
/// to `@target` first, so it is evaluated once.
fn opt_map_macro(
    helper: &mut MacroExprHelper<'_>,
    target: &mut Option<IdedExpr>,
    args: &mut Vec<IdedExpr>,
    wrap: bool,
) -> Result<Option<IdedExpr>, ParseError> {
    let target = target.take().expect("a receiver macro has a target");
    let [var, mapping] =
        <[IdedExpr; 2]>::try_from(mem::take(args)).expect("the macro matched two arguments");
    let Expr::Ident(var) = var.expr else {
        return Err(helper.new_error(var.id, "argument must be a simple name"));
    };

    let (name, bound) = match &target.expr {
        Expr::Ident(name) => (name.clone(), None),
        _ => (TARGET.to_string(), Some(target)),
    };
    let ident = |helper: &mut MacroExprHelper<'_>| helper.next_expr(Expr::Ident(name.clone()));

    let receiver = ident(helper);
    let has_value = member_call(helper, receiver, "hasValue");
    let receiver = ident(helper);
    let value = member_call(helper, receiver, "value");
    let mapped = helper.bind(&var, value, mapping);
    let some = if wrap {
        optional_call(helper, "of", vec![mapped])
    } else {
        mapped
    };
    let none = optional_call(helper, "none", vec![]);
    let result = helper.next_expr(Expr::Call(CallExpr {
        func_name: operators::CONDITIONAL.to_string(),
        target: None,
        args: vec![has_value, some, none],
    }));

    Ok(Some(match bound {
        Some(target) => helper.bind(TARGET, target, result),
        None => result,
    }))
}

/// `<target>.<func_name>()`.
fn member_call(helper: &mut MacroExprHelper<'_>, target: IdedExpr, func_name: &str) -> IdedExpr {
    helper.next_expr(Expr::Call(CallExpr {
        func_name: func_name.to_string(),
        target: Some(Box::new(target)),
        args: vec![],
    }))
}

/// `optional.<func_name>(<args>)`.
fn optional_call(
    helper: &mut MacroExprHelper<'_>,
    func_name: &str,
    args: Vec<IdedExpr>,
) -> IdedExpr {
    let namespace = helper.next_expr(Expr::Ident("optional".to_string()));
    helper.next_expr(Expr::Call(CallExpr {
        func_name: func_name.to_string(),
        target: Some(Box::new(namespace)),
        args,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::types::{self, CelInt, CelString};
    use crate::{Context, Env, Value};

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
        let ExecutionError::NoSuchOverload(overload) = err else {
            panic!("expected a no-such-overload error");
        };
        assert_eq!(overload.function(), "value");
        assert_eq!(overload.argument_types(), ["int"]);
        assert!(overload.is_member_function());
    }

    #[test]
    fn optional_has_value_rejects_non_optional_receiver() {
        let err = optional_has_value(vec![non_optional()]).unwrap_err();
        let ExecutionError::NoSuchOverload(overload) = err else {
            panic!("expected a no-such-overload error");
        };
        assert_eq!(overload.function(), "hasValue");
        assert_eq!(overload.argument_types(), ["int"]);
        assert!(overload.is_member_function());
    }

    #[test]
    fn optional_or_optional_rejects_non_optional_receiver() {
        let err = optional_or_optional(vec![non_optional(), some_optional()]).unwrap_err();
        let ExecutionError::NoSuchOverload(overload) = err else {
            panic!("expected a no-such-overload error");
        };
        assert_eq!(overload.function(), "or");
        assert_eq!(overload.argument_types(), ["int", "optional_type"]);
        assert!(overload.is_member_function());
    }

    #[test]
    fn optional_or_value_rejects_non_optional_receiver() {
        let err = optional_or_value(vec![non_optional(), non_optional()]).unwrap_err();
        let ExecutionError::NoSuchOverload(overload) = err else {
            panic!("expected a no-such-overload error");
        };
        assert_eq!(overload.function(), "orValue");
        assert_eq!(overload.argument_types(), ["int", "int"]);
        assert!(overload.is_member_function());
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

    fn eval(source: &str) -> Result<Value, String> {
        let env = Env::stdlib();
        let expr = env
            .parser()
            .enable_optional_syntax(true)
            .parse(source)
            .map_err(|e| e.to_string())?;
        let context = Context::with_env(Arc::new(env));
        Value::resolve(&expr, &context).map_err(|e| e.to_string())
    }

    #[test]
    fn opt_map_and_opt_flat_map_map_the_value_of_an_optional() {
        for (source, want) in [
            ("optional.of(42).optMap(y, y + 1).value()", Value::Int(43)),
            (
                "optional.none().optMap(y, y + 1).hasValue()",
                Value::Bool(false),
            ),
            ("optional.of(0).optMap(y, y).hasValue()", Value::Bool(true)),
            (
                "{'k': {'s': 'v'}}.?k.optFlatMap(k, k.?s).value()",
                Value::from("v"),
            ),
            (
                "{'k': {}}.?k.optFlatMap(k, k.?s).hasValue()",
                Value::Bool(false),
            ),
            ("{}.?k.optFlatMap(k, k.?s).hasValue()", Value::Bool(false)),
            (
                "optional.of(1).optFlatMap(x, optional.ofNonZeroValue(x - 1)).hasValue()",
                Value::Bool(false),
            ),
        ] {
            assert_eq!(eval(source), Ok(want), "{source}");
        }
    }

    #[test]
    fn opt_map_binds_a_variable_named_as_a_namespace() {
        let source = "optional.of(3).optMap(optional, optional * 2).value()";
        assert_eq!(eval(source), Ok(Value::Int(6)));
    }

    #[test]
    fn opt_map_needs_a_simple_name() {
        let err = eval("optional.of(1).optMap(1 + 1, 2)").unwrap_err();
        assert!(err.contains("argument must be a simple name"), "{err}");
    }

    #[test]
    fn opt_map_is_left_as_written_without_the_optional_library() {
        let expr = Env::default().parser().parse("a.optMap(x, x)").unwrap();
        assert!(matches!(expr.expr, Expr::Call(call) if call.func_name == "optMap"));
    }
}
