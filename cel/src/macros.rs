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

/// Register a member-function overload on an `Env` from a typed Rust `fn`
/// item, generating the arg-downcast wrapper at expansion time.
///
/// The syntax carries the CEL name (defaults to the Rust fn ident), the
/// overload id (defaults to `"{fn_ident}_{first_arg_cel_type_name}"`), the
/// receiver + argument types (Rust types that implement
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
/// // → registers CEL name "matches", overload id "matches_string".
/// ```
///
/// # Optional overrides
///
/// Both `name` and `id` may be given as trailing key-value args, in either
/// order:
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

        // Defaults for CEL name + overload id — overridden below if provided.
        let __name: ::std::string::String =
            ::std::string::String::from(::std::stringify!($fn));
        let __id: ::std::string::String = ::std::format!(
            "{}_{}",
            ::std::stringify!($fn),
            <$this as $crate::common::value::Val>::cel_type().name(),
        );

        // Apply overrides. Each trailing `key = "value"` rebinds one local.
        // Order-independent — unknown keys are a compile error via the inner
        // dispatcher macro.
        $( $crate::__member_overload_option!(__name, __id, $key = $val); )*

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

/// Internal helper for [`add_member_overload!`]: rebinds `__name` or `__id`
/// depending on which key was passed. Anything else is a compile error.
#[doc(hidden)]
#[macro_export]
macro_rules! __member_overload_option {
    ($name_bind:ident, $id_bind:ident, name = $val:literal) => {
        let $name_bind: ::std::string::String = ::std::string::String::from($val);
    };
    ($name_bind:ident, $id_bind:ident, id = $val:literal) => {
        let $id_bind: ::std::string::String = ::std::string::String::from($val);
    };
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
/// * `name` defaults to the fn ident.
/// * `id` defaults to `"{fn_ident}_{first_arg_cel_type_name}"` when the
///   overload takes at least one argument, and to `"{fn_ident}"` when it
///   takes none.
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
            ::std::string::String::from(::std::stringify!($fn));
        let __id: ::std::string::String = ::std::format!(
            "{}_{}",
            ::std::stringify!($fn),
            <$first as $crate::common::value::Val>::cel_type().name(),
        );
        $( $crate::__member_overload_option!(__name, __id, $key = $val); )*

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
            ::std::string::String::from(::std::stringify!($fn));
        let __id: ::std::string::String =
            ::std::string::String::from(::std::stringify!($fn));
        $( $crate::__member_overload_option!(__name, __id, $key = $val); )*

        $env.add_overload(&__name, &__id, ::std::vec::Vec::new(), __wrapper)
            .expect("Must be unique id");
    }};
}

pub(crate) use impl_handler;
