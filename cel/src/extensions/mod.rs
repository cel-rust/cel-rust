mod bindings;
#[cfg(feature = "ext_encoders")]
mod encoders;
mod lists;
mod math;
mod strings;

pub use bindings::{extension as bindings, Bindings};
#[cfg(feature = "ext_encoders")]
pub use encoders::{extension as encoders, Encoders};
pub use lists::{extension as lists, Lists};
pub use math::{extension as math, Math};
pub use strings::{extension as strings, Strings};

use crate::common::traits::Iterable;
use crate::common::value::{FromVal, Val};
use crate::{DeclarationError, Env, ExecutionError};

/// A library of types, overloads and macros, added to an [`Env`] with
/// [`Env::add_library`]: the extensions of this module, such as [`Strings`],
/// are libraries.
///
/// A library with a [name](Library::name) is added once to an environment:
/// adding it again changes nothing.
///
/// A library holds its settings, if it has any, and reads them as it
/// registers: what it declares can depend on them. An overload itself is a
/// plain [`fn`](crate::common::functions::Function), which captures nothing.
///
/// # Example
///
/// ```
/// use cel::common::types::{CelBool, CelString};
/// use cel::{Context, DeclarationError, Env, Library, Value};
/// use std::sync::Arc;
///
/// struct Emptiness;
///
/// fn is_empty(this: &CelString<'_>) -> CelBool {
///     CelBool::from(this.inner().is_empty())
/// }
///
/// impl Library for Emptiness {
///     fn name(&self) -> Option<&str> {
///         Some("acme.lib.emptiness")
///     }
///
///     fn register(&self, env: &mut Env) -> Result<(), DeclarationError> {
///         cel::add_member_overload!(env, fn is_empty: (CelString) -> CelBool)
///     }
/// }
///
/// let mut env = Env::stdlib();
/// assert_eq!(env.add_library(Emptiness), Ok(()));
/// assert_eq!(env.add_library(Emptiness), Ok(()));
/// assert!(env.has_library("acme.lib.emptiness"));
///
/// let program = env.compile("''.isEmpty()").unwrap();
/// let context = Context::with_env(Arc::new(env));
/// assert_eq!(program.execute(&context), Ok(Value::Bool(true)));
/// ```
pub trait Library {
    /// The name this library is added under, once: a library of a name
    /// already added to an [`Env`] is not registered, whatever its settings,
    /// so the first one added stays. `None`, the default, for a library
    /// registered every time it is added.
    ///
    /// Names are qualified, as cel-go's are: `cel.lib.ext.strings`.
    fn name(&self) -> Option<&str> {
        None
    }

    /// Registers the types, overloads and macros of this library on `env`.
    ///
    /// # Errors
    ///
    /// Fails with the [`DeclarationError`] of what `env` rejects, e.g. an
    /// overload it already declares.
    fn register(&self, env: &mut Env) -> Result<(), DeclarationError>;
}

/// `list` as an iterable, for an overload that takes any `list`, not just a
/// `CelList`.
fn iterable<'b, 'v>(list: &'b (dyn Val + 'v)) -> Result<&'b (dyn Iterable + 'v), ExecutionError> {
    list.as_iterable()
        .ok_or_else(|| ExecutionError::UnexpectedType {
            got: list.get_type().name().to_owned(),
            want: "iterable".to_owned(),
        })
}

/// `val` downcast to the `T` its overload was registered with.
fn arg<'b, 'v, T: FromVal<'b, 'v> + Val>(val: &'b (dyn Val + 'v)) -> Result<&'b T, ExecutionError> {
    val.downcast_ref::<T>()
        .ok_or_else(|| ExecutionError::UnexpectedType {
            got: val.get_type().name().to_owned(),
            want: T::cel_type().name().to_owned(),
        })
}

#[cfg(test)]
mod tests {
    use crate::common::traits::{self, Iterable};
    use crate::common::types::{Type, LIST_TYPE};
    use crate::common::value::Val;

    /// A `list` that is not a `CelList`, as a downstream crate would define
    /// one: reachable only through `Val`'s trait accessors.
    #[derive(Clone, Debug)]
    pub(super) struct OtherList<T>(pub(super) Vec<T>);

    struct OtherListIter<'b, T>(std::slice::Iter<'b, T>);

    impl<'b, 'v, T: Val + 'v> traits::Iterator<'b, 'v> for OtherListIter<'b, T> {
        fn next(&mut self) -> Option<&'b (dyn Val + 'v)> {
            self.0.next().map(|v| v as &dyn Val)
        }
    }

    impl<T: Val> Iterable for OtherList<T> {
        fn iter<'b, 'v>(&'b self) -> Box<dyn traits::Iterator<'b, 'v> + 'b>
        where
            Self: 'v,
        {
            Box::new(OtherListIter(self.0.iter()))
        }
    }

    impl<T: Val + Clone> Val for OtherList<T> {
        fn get_type(&self) -> &Type {
            &LIST_TYPE
        }

        fn cel_type() -> &'static Type {
            &LIST_TYPE
        }

        fn as_iterable<'b, 'v>(&'b self) -> Option<&'b (dyn Iterable + 'v)>
        where
            Self: 'v,
        {
            Some(self)
        }

        fn clone_as_boxed<'v>(&self) -> Box<dyn Val + 'v>
        where
            Self: 'v,
        {
            Box::new(self.clone())
        }
    }
}
