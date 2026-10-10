mod bindings;
#[cfg(feature = "ext_encoders")]
mod encoders;
mod lists;
mod math;
mod strings;

pub use bindings::extension as bindings;
#[cfg(feature = "ext_encoders")]
pub use encoders::extension as encoders;
pub use lists::extension as lists;
pub use math::extension as math;
pub use strings::extension as strings;

use crate::common::traits::Iterable;
use crate::common::value::{FromVal, Val};
use crate::ExecutionError;

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
