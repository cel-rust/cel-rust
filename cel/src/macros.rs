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
pub(crate) use impl_handler;
