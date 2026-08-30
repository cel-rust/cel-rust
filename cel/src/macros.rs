#[macro_export]
macro_rules! impl_conversions {
    // Capture triples separated by commas: the Rust type a function signature can
    // use, the `Value` variant it corresponds to at the library boundary, and the
    // concrete `Val` implementation that backs it internally. The latter lets both
    // argument extraction and return-value construction go straight to/from the
    // `Val`, without materializing an intermediate `Value`.
    ($($target_type:ty => $value_variant:path as $cel_type:ty),* $(,)?) => {
        $(
            impl From<$target_type> for Value {
                fn from(value: $target_type) -> Self {
                    $value_variant(value)
                }
            }

            impl<'context, 'call> $crate::magic::IntoResolveResult<'context, 'call> for $target_type {
                fn into_resolve_result(self) -> Result<$crate::common::value::CowVal<'context, 'call>, ExecutionError> {
                    Ok($crate::common::value::CowVal::owned(<$cel_type>::from(self)))
                }
            }

            impl<'context, 'call> $crate::magic::IntoResolveResult<'context, 'call> for Result<$target_type, ExecutionError> {
                fn into_resolve_result(self) -> Result<$crate::common::value::CowVal<'context, 'call>, ExecutionError> {
                    $crate::magic::IntoResolveResult::into_resolve_result(self?)
                }
            }

            impl<'a, 'context, 'call> FromContext<'a, 'context, 'call> for $target_type {
                fn from_context(ctx: &'a mut FunctionContext<'context, 'call>) -> Result<Self, ExecutionError>
                where
                    Self: Sized,
                {
                    $crate::magic::arg_val_from_context(ctx)
                        .and_then(|v| $crate::magic::FromVal::from_val(v.as_ref()))
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
                R: for<'context, 'call> IntoResolveResult<'context, 'call>,
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
                R: for<'context, 'call> IntoResolveResult<'context, 'call>,
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
/// overload id (defaults to `"{fn_ident}_{receiver_cel_type_name}"`), the
/// receiver + argument types (Rust types that implement
/// [`CelValType`](crate::common::types::CelValType)), and the CEL type of the
/// result.
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
/// `for<'b, 'v> fn(&Receiver[, &Arg]*) -> Result<CowVal<'b, 'v>, ExecutionError>`.
/// It receives its arguments by reference, downcast from the call's
/// [`CowVal`](crate::common::value::CowVal)s, and so cannot return a value
/// borrowing from them; register a plain
/// [`Function`](crate::common::functions::Function) by hand for that.
///
/// # Example
///
/// ```ignore
/// fn matches<'b, 'v>(
///     this: &CelString<'_>,
///     re: &CelString<'_>,
/// ) -> Result<CowVal<'b, 'v>, ExecutionError> { … }
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
        // The wrapper: downcasts each `CowVal` to its declared Rust type and
        // hands the references to the target fn.
        fn __wrapper<'b, 'v>(
            args: ::std::vec::Vec<$crate::common::value::CowVal<'b, 'v>>,
        ) -> ::std::result::Result<
            $crate::common::value::CowVal<'b, 'v>,
            $crate::ExecutionError,
        > {
            // Built only on the failure path, and reports the same context a
            // hand-written overload would.
            let __no_overload = || {
                $crate::ExecutionError::no_such_member_overload(
                    ::std::stringify!($fn),
                    args.iter()
                        .map(|a| a.get_type().name().to_owned())
                        .collect(),
                )
            };
            let mut __at = 0usize;
            let __result: $crate::common::value::CowVal<'b, 'v> = $fn(
                $crate::__member_overload_extract!(args, __at, $this, __no_overload)
                $(, $crate::__member_overload_extract!(args, __at, $other, __no_overload) )*
            )?;
            // The declared return type is what the overload is registered as
            // answering to; a mismatch is a bug in the fn, not in the call.
            ::std::debug_assert_eq!(
                __result.get_type(),
                <$ret as $crate::common::types::CelValType>::cel_type(),
                "`{}` returned a {}",
                ::std::stringify!($fn),
                __result.get_type().name(),
            );
            ::std::result::Result::Ok(__result)
        }

        // Defaults for CEL name + overload id — overridden below if provided.
        let __name: ::std::string::String =
            ::std::string::String::from(::std::stringify!($fn));
        let __id: ::std::string::String = ::std::format!(
            "{}_{}",
            ::std::stringify!($fn),
            <$this as $crate::common::types::CelValType>::cel_type().name(),
        );

        // Apply overrides. Each trailing `key = "value"` rebinds one local.
        // Order-independent — unknown keys are a compile error via the inner
        // dispatcher macro.
        $( $crate::__member_overload_option!(__name, __id, $key = $val); )*

        $env.add_member_overload(
            &__name,
            &__id,
            <$this as $crate::common::types::CelValType>::cel_type().to_owned(),
            ::std::vec![
                $( <$other as $crate::common::types::CelValType>::cel_type().to_owned() ),*
            ],
            __wrapper,
        )
        .expect("Must be unique id");
    }};
}

/// Internal helper for [`add_member_overload!`]: borrows the argument at
/// `$idx` and downcasts it to `$ty`, advancing `$idx`. Not for direct use.
#[doc(hidden)]
#[macro_export]
macro_rules! __member_overload_extract {
    ($args:ident, $idx:ident, $ty:ty, $err:ident) => {{
        let __arg = $args.get($idx).ok_or_else($err)?.as_ref();
        $idx += 1;
        __arg.downcast_ref::<$ty>().ok_or_else($err)?
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
pub(crate) use impl_handler;
