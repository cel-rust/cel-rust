#[macro_export]
macro_rules! impl_conversions {
    // Capture pairs separated by commas, where each pair is separated by =>
    ($($target_type:ty => $value_variant:path),* $(,)?) => {
        $(
            impl FromValue for $target_type {
                fn from_value(expr: &Value) -> Result<Self, ExecutionError> {
                    if let $value_variant(v) = expr {
                        Ok(v.clone())
                    } else {
                        Err(ExecutionError::UnexpectedType {
                            got: format!("{:?}", expr),
                            want: stringify!($target_type).to_string(),
                        })
                    }
                }
            }

            impl FromValue for Option<$target_type> {
                fn from_value(expr: &Value) -> Result<Self, ExecutionError> {
                    match expr {
                        Value::Null => Ok(None),
                        $value_variant(v) => Ok(Some(v.clone())),
                        _ => Err(ExecutionError::UnexpectedType {
                            got: format!("{:?}", expr),
                            want: stringify!($target_type).to_string(),
                        }),
                    }
                }
            }

            impl From<$target_type> for Value {
                fn from(value: $target_type) -> Self {
                    $value_variant(value)
                }
            }

            impl $crate::magic::IntoResolveResult for $target_type {
                fn into_resolve_result(self) -> ResolveResult {
                    Ok($value_variant(self))
                }
            }

            impl $crate::magic::IntoResolveResult for Result<$target_type, ExecutionError> {
                fn into_resolve_result(self) -> ResolveResult {
                    self.map($value_variant)
                }
            }

            impl<'a, 'context, 'call> FromContext<'a, 'context, 'call> for $target_type {
                fn from_context(ctx: &'a mut FunctionContext<'context, 'call>) -> Result<Self, ExecutionError>
                where
                    Self: Sized,
                {
                    arg_value_from_context(ctx).and_then(|v| FromValue::from_value(&v))
                }
            }
        )*
    }
}

#[macro_export]
macro_rules! impl_handler {
    ($($t:ty),*) => {
        pastey::paste! {
            impl<F, $($t,)* R> IntoFunction<($($t,)*)> for F
            where
                F: Fn($($t,)*) -> R + Send + Sync + 'static,
                $($t: for<'a, 'context, 'call> $crate::FromContext<'a, 'context, 'call>,)*
                R: IntoResolveResult,
            {
                fn into_function(self) -> Function {
                    Box::new(move |_ftx| {
                        $(
                            let [<arg_ $t:lower>] = $t::from_context(_ftx)?;
                        )*
                        self($([<arg_ $t:lower>],)*).into_resolve_result()
                    })
                }
            }

            impl<F, $($t,)* R> IntoFunction<(WithFunctionContext, $($t,)*)> for F
            where
                F: Fn(&FunctionContext, $($t,)*) -> R + Send + Sync + 'static,
                $($t: for<'a, 'context, 'call> $crate::FromContext<'a, 'context, 'call>,)*
                R: IntoResolveResult,
            {
                fn into_function(self) -> Function {
                    Box::new(move |_ftx| {
                        $(
                            let [<arg_ $t:lower>] = $t::from_context(_ftx)?;
                        )*
                        self(_ftx, $([<arg_ $t:lower>],)*).into_resolve_result()
                    })
                }
            }
        }
    };
}

pub(crate) use impl_conversions;

/// Converts a Rust `snake_case` identifier to CEL `camelCase`.
///
/// Used by [`add_overload!`](crate::add_overload) and
/// [`add_member_overload!`](crate::add_member_overload) to derive the default
/// CEL name from the Rust fn ident. Leading and trailing underscores are
/// dropped; internal runs of underscores collapse (i.e. `foo__bar` becomes
/// `fooBar`). Non-alphabetic characters are left as-is.
#[doc(hidden)]
pub fn to_camel_case(snake: &str) -> String {
    let mut out = String::with_capacity(snake.len());
    let mut upper_next = false;
    for c in snake.chars() {
        if c == '_' {
            upper_next = !out.is_empty();
        } else if upper_next {
            out.push(c.to_ascii_uppercase());
            upper_next = false;
        } else {
            out.push(c);
        }
    }
    out
}

/// Register a member-function overload on an `Env` from a typed Rust `fn`
/// item, generating the arg-downcast wrapper at expansion time.
///
/// The syntax carries the CEL name (defaults to the Rust fn ident converted
/// from `snake_case` to `camelCase`, so `fn ends_with` becomes `endsWith`),
/// the overload id (defaults to `"{receiver}.{name}({rest_arg_types})"`,
/// matching cel-cpp's `MakeOverloadSignature` format), the receiver +
/// argument types (Rust types that implement
/// [`Val`](crate::common::value::Val)), and the return type.
///
/// # Shape
///
/// ```ignore
/// add_member_overload!(
///     env,
///     fn <fn_ident>: (<Receiver>[, <Arg>]*) -> <Ret>
///     [, name = "<cel-name>"]
///     [, id   = "<overload-id>"]
/// );
/// ```
///
/// The referenced fn must have the signature
/// `for<'a> fn(Cow<'a, Receiver>[, Cow<'a, Arg>]*) -> Result<Cow<'a, Ret>, ExecutionError>`.
///
/// # Example
///
/// ```ignore
/// fn matches<'a>(
///     this: Cow<'a, String>,
///     re: Cow<'a, String>,
/// ) -> Result<Cow<'a, CelBool>, ExecutionError> { … }
///
/// add_member_overload!(env, fn matches: (String, String) -> CelBool);
/// // → registers CEL name "matches", overload id "string.matches(string)".
/// ```
///
/// # Optional overrides
///
/// Both `name` and `id` may be given as trailing key-value args, in either
/// order. When only `name` is overridden the default id is built from the
/// resolved name, so `name = "endsWith"` yields id `"string.endsWith(string)"`.
///
/// ```ignore
/// add_member_overload!(env, fn regex_matches: (String, String) -> CelBool,
///     name = "matches", id = "matches_regex");
/// ```
#[macro_export]
macro_rules! add_member_overload {
    (
        $env:expr,
        fn $fn:ident : ( $this:ty $(, $other:ty )* $(,)? ) -> $ret:ty
        $(, $key:ident = $val:literal )*
        $(,)?
    ) => {{
        // The wrapper: unpacks `Vec<Cow<dyn Val>>` into typed args, calls
        // the target fn, and upcasts the return back to `Cow<dyn Val>`.
        fn __wrapper<'a>(
            args: ::std::vec::Vec<
                ::std::borrow::Cow<'a, dyn $crate::common::value::Val>,
            >,
        ) -> ::std::result::Result<
            ::std::borrow::Cow<'a, dyn $crate::common::value::Val>,
            $crate::ExecutionError,
        > {
            let mut __iter = args.into_iter();
            let __result: ::std::borrow::Cow<'a, $ret> = $fn(
                $crate::__member_overload_extract!(__iter, $this)
                $(, $crate::__member_overload_extract!(__iter, $other) )*
            )?;
            ::std::result::Result::Ok(match __result {
                ::std::borrow::Cow::Borrowed(v) =>
                    ::std::borrow::Cow::<'a, dyn $crate::common::value::Val>::Borrowed(
                        v as &dyn $crate::common::value::Val,
                    ),
                ::std::borrow::Cow::Owned(v) => {
                    let __boxed: ::std::boxed::Box<
                        dyn $crate::common::value::Val,
                    > = ::std::boxed::Box::new(v);
                    ::std::borrow::Cow::<'a, dyn $crate::common::value::Val>::Owned(__boxed)
                }
            })
        }

        // CEL name defaults to the fn ident. `name = "..."` overrides apply
        // first so the id default sees the resolved name (matching cel-cpp).
        let __name: ::std::string::String =
            $crate::to_camel_case(::std::stringify!($fn));
        $( $crate::__overload_name_override!(__name, $key = $val); )*

        // Id default follows cel-cpp `MakeOverloadSignature` for member fns:
        // `receiver.name(rest_arg_types_comma_separated)`.
        let __rest_types: ::std::vec::Vec<&str> = ::std::vec![
            $( <$other as $crate::common::value::Val>::cel_type().name() ),*
        ];
        let __id: ::std::string::String = ::std::format!(
            "{}.{}({})",
            <$this as $crate::common::value::Val>::cel_type().name(),
            __name,
            __rest_types.join(","),
        );
        $( $crate::__overload_id_override!(__id, $key = $val); )*

        $env.add_member_overload(
            &__name,
            &__id,
            <$this as $crate::common::value::Val>::cel_type().to_owned(),
            ::std::vec![
                $( <$other as $crate::common::value::Val>::cel_type().to_owned() ),*
            ],
            __wrapper,
        )
        .expect("Must be unique id");
    }};
}

/// Internal helper for [`add_member_overload!`]: extracts one arg from the
/// iterator, downcasting through a `Cow<dyn Val>` into a `Cow<T>` on either
/// branch. Not for direct use.
#[doc(hidden)]
#[macro_export]
macro_rules! __member_overload_extract {
    ($iter:ident, $ty:ty) => {{
        let __arg = $iter.next().ok_or($crate::ExecutionError::NoSuchOverload)?;
        match __arg {
            ::std::borrow::Cow::Borrowed(v) => ::std::borrow::Cow::Borrowed(
                v.downcast_ref::<$ty>()
                    .ok_or($crate::ExecutionError::NoSuchOverload)?,
            ),
            ::std::borrow::Cow::Owned(b) => ::std::borrow::Cow::Owned(*<
                ::std::boxed::Box<dyn $crate::common::value::Val>
                    as $crate::common::value::Downcast
            >::downcast::<$ty>(b)
            .map_err(|_| $crate::ExecutionError::NoSuchOverload)?),
        }
    }};
}

/// Internal helper: applies only `name = "..."` overrides (silently ignoring
/// any `id = ...`) so name resolution happens before id defaulting.
#[doc(hidden)]
#[macro_export]
macro_rules! __overload_name_override {
    ($bind:ident, name = $val:literal) => {
        let $bind: ::std::string::String = ::std::string::String::from($val);
    };
    ($bind:ident, id = $val:literal) => {};
}

/// Internal helper: applies only `id = "..."` overrides (silently ignoring
/// any `name = ...`).
#[doc(hidden)]
#[macro_export]
macro_rules! __overload_id_override {
    ($bind:ident, id = $val:literal) => {
        let $bind: ::std::string::String = ::std::string::String::from($val);
    };
    ($bind:ident, name = $val:literal) => {};
}

/// Register a global (non-member) function overload on an `Env` from a typed
/// Rust `fn` item, generating the arg-downcast wrapper at expansion time.
///
/// Mirrors [`add_member_overload!`] but delegates to `Env::add_overload` and
/// treats every parameter as a regular arg (no `this`-receiver split).
///
/// # Shape
///
/// ```ignore
/// add_overload!(
///     env,
///     fn <fn_ident>: (<Arg>*) -> <Ret>
///     [, name = "<cel-name>"]
///     [, id   = "<overload-id>"]
/// );
/// ```
///
/// The referenced fn must have the signature
/// `for<'a> fn(Cow<'a, Arg>*) -> Result<Cow<'a, Ret>, ExecutionError>`.
///
/// Zero-argument overloads are supported: use `()` for the parameter list.
///
/// # Default naming
///
/// * `name` defaults to the fn ident converted from `snake_case` to
///   `camelCase` — `fn ends_with` yields the CEL name `"endsWith"`, a
///   single-word ident like `fn matches` stays `"matches"`.
/// * `id` defaults to cel-cpp's `MakeOverloadSignature` format:
///   `"{name}({arg_types_comma_separated})"` — e.g. `size(string)` for a
///   one-arg fn, `matches(string,string)` for two args, `now()` when there
///   are no args. When `name = "..."` is overridden the id default uses the
///   resolved name.
///
/// Either may be overridden via trailing `name = "..."` / `id = "..."`
/// key-value args, in either order.
#[macro_export]
macro_rules! add_overload {
    // Non-empty arg list.
    (
        $env:expr,
        fn $fn:ident : ( $first:ty $(, $rest:ty )* $(,)? ) -> $ret:ty
        $(, $key:ident = $val:literal )*
        $(,)?
    ) => {{
        fn __wrapper<'a>(
            args: ::std::vec::Vec<
                ::std::borrow::Cow<'a, dyn $crate::common::value::Val>,
            >,
        ) -> ::std::result::Result<
            ::std::borrow::Cow<'a, dyn $crate::common::value::Val>,
            $crate::ExecutionError,
        > {
            let mut __iter = args.into_iter();
            let __result: ::std::borrow::Cow<'a, $ret> = $fn(
                $crate::__member_overload_extract!(__iter, $first)
                $(, $crate::__member_overload_extract!(__iter, $rest) )*
            )?;
            ::std::result::Result::Ok(match __result {
                ::std::borrow::Cow::Borrowed(v) =>
                    ::std::borrow::Cow::<'a, dyn $crate::common::value::Val>::Borrowed(
                        v as &dyn $crate::common::value::Val,
                    ),
                ::std::borrow::Cow::Owned(v) => {
                    let __boxed: ::std::boxed::Box<
                        dyn $crate::common::value::Val,
                    > = ::std::boxed::Box::new(v);
                    ::std::borrow::Cow::<'a, dyn $crate::common::value::Val>::Owned(__boxed)
                }
            })
        }

        let __name: ::std::string::String =
            $crate::to_camel_case(::std::stringify!($fn));
        $( $crate::__overload_name_override!(__name, $key = $val); )*

        // Id default: `name(arg_types_comma_separated)` — cel-cpp format.
        let __arg_types: ::std::vec::Vec<&str> = ::std::vec![
            <$first as $crate::common::value::Val>::cel_type().name()
            $(, <$rest as $crate::common::value::Val>::cel_type().name() )*
        ];
        let __id: ::std::string::String =
            ::std::format!("{}({})", __name, __arg_types.join(","));
        $( $crate::__overload_id_override!(__id, $key = $val); )*

        $env.add_overload(
            &__name,
            &__id,
            ::std::vec![
                <$first as $crate::common::value::Val>::cel_type().to_owned()
                $(, <$rest as $crate::common::value::Val>::cel_type().to_owned() )*
            ],
            __wrapper,
        )
        .expect("Must be unique id");
    }};

    // Zero-argument overload.
    (
        $env:expr,
        fn $fn:ident : ( ) -> $ret:ty
        $(, $key:ident = $val:literal )*
        $(,)?
    ) => {{
        fn __wrapper<'a>(
            _args: ::std::vec::Vec<
                ::std::borrow::Cow<'a, dyn $crate::common::value::Val>,
            >,
        ) -> ::std::result::Result<
            ::std::borrow::Cow<'a, dyn $crate::common::value::Val>,
            $crate::ExecutionError,
        > {
            let __result: ::std::borrow::Cow<'a, $ret> = $fn()?;
            ::std::result::Result::Ok(match __result {
                ::std::borrow::Cow::Borrowed(v) =>
                    ::std::borrow::Cow::<'a, dyn $crate::common::value::Val>::Borrowed(
                        v as &dyn $crate::common::value::Val,
                    ),
                ::std::borrow::Cow::Owned(v) => {
                    let __boxed: ::std::boxed::Box<
                        dyn $crate::common::value::Val,
                    > = ::std::boxed::Box::new(v);
                    ::std::borrow::Cow::<'a, dyn $crate::common::value::Val>::Owned(__boxed)
                }
            })
        }

        let __name: ::std::string::String =
            $crate::to_camel_case(::std::stringify!($fn));
        $( $crate::__overload_name_override!(__name, $key = $val); )*

        // Zero-arg id default: `name()` — cel-cpp format.
        let __id: ::std::string::String = ::std::format!("{}()", __name);
        $( $crate::__overload_id_override!(__id, $key = $val); )*

        $env.add_overload(&__name, &__id, ::std::vec::Vec::new(), __wrapper)
            .expect("Must be unique id");
    }};
}

pub(crate) use impl_handler;

#[cfg(test)]
mod tests {
    //! These tests lock down the default `id` heuristic of
    //! [`add_overload!`] and [`add_member_overload!`] to cel-cpp's
    //! `MakeOverloadSignature` format
    //! (see <https://github.com/cel-expr/cel-cpp/blob/master/common/signature_test.cc>).
    //!
    //! Rather than reaching into `Env`'s private overload registry, each
    //! test registers a fn via the macro, then tries to add a second
    //! overload with the **expected** id via the raw `Env::add_overload`
    //! api. `FunctionDecl::add_overload` rejects duplicate ids, so a
    //! matching default yields `Err(())`, and a mismatch yields `Ok(())`.
    //! A follow-up mismatched-id call then confirms the shape rejection
    //! is truly id-based (not a coincidence).
    use crate::common::types::{self, CelBool, CelInt, CelString};
    use crate::common::value::Val;
    use crate::{ExecutionError, Env};
    use std::borrow::Cow;

    // --- Fixture fns used across the tests below. -----------------------

    fn ping<'a>(_x: Cow<'a, CelString>) -> Result<Cow<'a, CelInt>, ExecutionError> {
        Ok(Cow::Owned(CelInt::from(0)))
    }
    fn ping2<'a>(
        _a: Cow<'a, CelString>,
        _b: Cow<'a, CelString>,
    ) -> Result<Cow<'a, CelInt>, ExecutionError> {
        Ok(Cow::Owned(CelInt::from(0)))
    }
    fn ping2_bool<'a>(
        _a: Cow<'a, CelString>,
        _b: Cow<'a, CelString>,
    ) -> Result<Cow<'a, CelBool>, ExecutionError> {
        Ok(Cow::Owned(CelBool::from(false)))
    }
    // Named to exercise snake_case → camelCase conversion of the CEL name.
    fn ends_with<'a>(
        _a: Cow<'a, CelString>,
        _b: Cow<'a, CelString>,
    ) -> Result<Cow<'a, CelBool>, ExecutionError> {
        Ok(Cow::Owned(CelBool::from(false)))
    }
    fn ping0<'a>() -> Result<Cow<'a, CelInt>, ExecutionError> {
        Ok(Cow::Owned(CelInt::from(0)))
    }

    fn noop(
        _args: Vec<Cow<'_, dyn Val>>,
    ) -> Result<Cow<'_, dyn Val>, ExecutionError> {
        let boxed: Box<dyn Val> = Box::new(CelInt::from(0));
        Ok(Cow::Owned(boxed))
    }

    // --- add_overload! default id -------------------------------------

    // --- to_camel_case ------------------------------------------------

    #[test]
    fn to_camel_case_single_word() {
        assert_eq!(super::to_camel_case("matches"), "matches");
    }

    #[test]
    fn to_camel_case_two_words() {
        assert_eq!(super::to_camel_case("ends_with"), "endsWith");
    }

    #[test]
    fn to_camel_case_three_words() {
        assert_eq!(super::to_camel_case("day_of_year"), "dayOfYear");
    }

    #[test]
    fn to_camel_case_leading_and_trailing_underscores_dropped() {
        assert_eq!(super::to_camel_case("_foo_bar_"), "fooBar");
    }

    #[test]
    fn to_camel_case_collapses_repeated_underscores() {
        assert_eq!(super::to_camel_case("foo__bar"), "fooBar");
    }

    // --- default CEL name via the macros ------------------------------

    /// Sniff the default CEL name a macro registered by trying to look the
    /// function up under that name via `find_overload` — it returns
    /// `Some(_)` iff the name was registered.
    fn name_registered(env: &Env, name: &str, arity: usize) -> bool {
        let args: Vec<Cow<'_, dyn Val>> = (0..arity)
            .map(|_| {
                let v: Box<dyn Val> = Box::new(CelString::from(""));
                Cow::Owned(v)
            })
            .collect();
        env.find_overload(name, &args).is_some()
    }
    fn member_name_registered(env: &Env, name: &str, arity: usize) -> bool {
        let args: Vec<Cow<'_, dyn Val>> = (0..arity)
            .map(|_| {
                let v: Box<dyn Val> = Box::new(CelString::from(""));
                Cow::Owned(v)
            })
            .collect();
        env.find_member_overload(name, &args).is_some()
    }

    #[test]
    fn add_overload_default_name_snake_to_camel() {
        let mut env = Env::default();
        crate::add_overload!(env, fn ends_with: (CelString, CelString) -> CelBool);
        assert!(name_registered(&env, "endsWith", 2));
        assert!(!name_registered(&env, "ends_with", 2));
    }

    #[test]
    fn add_overload_default_name_single_word_unchanged() {
        let mut env = Env::default();
        crate::add_overload!(env, fn ping: (CelString) -> CelInt);
        assert!(name_registered(&env, "ping", 1));
    }

    #[test]
    fn add_member_overload_default_name_snake_to_camel() {
        let mut env = Env::default();
        crate::add_member_overload!(env, fn ends_with: (CelString, CelString) -> CelBool);
        assert!(member_name_registered(&env, "endsWith", 2));
        assert!(!member_name_registered(&env, "ends_with", 2));
    }

    #[test]
    fn add_member_overload_id_default_uses_camel_cased_name() {
        // With the camelCase default, `fn ends_with` yields id
        // `"string.endsWith(string)"` without a `name = ...` override.
        let mut env = Env::default();
        crate::add_member_overload!(env, fn ends_with: (CelString, CelString) -> CelBool);
        assert!(env
            .add_member_overload(
                "endsWith",
                "string.endsWith(string)",
                types::STRING_TYPE,
                vec![types::STRING_TYPE],
                noop,
            )
            .is_err());
    }

    #[test]
    fn add_overload_default_id_matches_cel_cpp_one_arg() {
        let mut env = Env::default();
        crate::add_overload!(env, fn ping: (CelString) -> CelInt);
        // Expected cel-cpp signature: `ping(string)`
        assert!(
            env.add_overload("ping", "ping(string)", vec![types::STRING_TYPE], noop)
                .is_err(),
            "default id should be `ping(string)`",
        );
    }

    #[test]
    fn add_overload_default_id_matches_cel_cpp_two_args() {
        let mut env = Env::default();
        crate::add_overload!(env, fn ping2: (CelString, CelString) -> CelInt);
        // Expected cel-cpp signature: `ping2(string,string)`
        assert!(
            env.add_overload(
                "ping2",
                "ping2(string,string)",
                vec![types::STRING_TYPE, types::STRING_TYPE],
                noop,
            )
            .is_err(),
        );
    }

    #[test]
    fn add_overload_default_id_matches_cel_cpp_zero_args() {
        let mut env = Env::default();
        crate::add_overload!(env, fn ping0: () -> CelInt);
        // Expected cel-cpp signature: `ping0()`
        assert!(
            env.add_overload("ping0", "ping0()", vec![], noop).is_err(),
        );
    }

    #[test]
    fn add_overload_id_default_uses_resolved_name_override() {
        let mut env = Env::default();
        // name override changes the id default's function-name portion.
        crate::add_overload!(env, fn ping: (CelString) -> CelInt, name = "renamed");
        assert!(
            env.add_overload("renamed", "renamed(string)", vec![types::STRING_TYPE], noop)
                .is_err(),
            "id default should follow the overridden name",
        );
        // Sanity: the un-renamed id `ping(string)` is NOT registered under
        // this name. Use disjoint arg types so the only possible collision
        // vector is the id string itself.
        assert!(
            env.add_overload("renamed", "ping(string)", vec![types::INT_TYPE], noop)
                .is_ok(),
        );
    }

    #[test]
    fn add_overload_explicit_id_wins_over_default() {
        let mut env = Env::default();
        crate::add_overload!(env, fn ping: (CelString) -> CelInt, id = "explicit");
        assert!(
            env.add_overload("ping", "explicit", vec![types::STRING_TYPE], noop)
                .is_err(),
        );
    }

    // --- add_member_overload! default id ------------------------------

    #[test]
    fn add_member_overload_default_id_matches_cel_cpp_no_extra_args() {
        let mut env = Env::default();
        crate::add_member_overload!(env, fn ping: (CelString) -> CelInt);
        // Expected cel-cpp signature: `string.ping()`
        assert!(
            env.add_member_overload("ping", "string.ping()", types::STRING_TYPE, vec![], noop)
                .is_err(),
        );
    }

    #[test]
    fn add_member_overload_default_id_matches_cel_cpp_one_extra_arg() {
        let mut env = Env::default();
        crate::add_member_overload!(env, fn ping2: (CelString, CelString) -> CelInt);
        // Expected cel-cpp signature: `string.ping2(string)`
        assert!(
            env.add_member_overload(
                "ping2",
                "string.ping2(string)",
                types::STRING_TYPE,
                vec![types::STRING_TYPE],
                noop,
            )
            .is_err(),
        );
    }

    #[test]
    fn add_member_overload_id_default_uses_resolved_name_override() {
        let mut env = Env::default();
        crate::add_member_overload!(env, fn ping2_bool: (CelString, CelString) -> CelBool,
            name = "endsWith");
        // Expected cel-cpp signature: `string.endsWith(string)`
        assert!(
            env.add_member_overload(
                "endsWith",
                "string.endsWith(string)",
                types::STRING_TYPE,
                vec![types::STRING_TYPE],
                noop,
            )
            .is_err(),
        );
    }

    #[test]
    fn add_member_overload_explicit_id_wins_over_default() {
        let mut env = Env::default();
        crate::add_member_overload!(env, fn ping: (CelString) -> CelInt, id = "explicit");
        assert!(
            env.add_member_overload("ping", "explicit", types::STRING_TYPE, vec![], noop)
                .is_err(),
        );
    }
}
