//! Implements functions of the Kubernetes list library.
//!
//! See:
//!
//! - https://kubernetes.io/docs/reference/using-api/cel/#kubernetes-list-library
//! - https://pkg.go.dev/k8s.io/apiextensions-apiserver/pkg/apiserver/schema/cel/library#Lists

use std::sync::Arc;

use crate::{functions::Result, magic::This, FunctionContext, Value};

/// Implements the `isSorted` function of the Kubernetes list library.
///
/// Returns whether the list is sorted and is supported on all comparable types.
///
/// See: https://github.com/kubernetes/apiextensions-apiserver/blob/ab2ddc498e31f2701200bff261e89120b3d929c3/pkg/apiserver/schema/cel/library/lists.go#L178
pub fn is_sorted(This(this): This<Arc<Vec<Value>>>) -> bool {
    this.is_sorted()
}

/// Implements the `indexOf` function of the Kubernetes list library.
///
/// Returns the first positional index of the provided element in the list.
///
/// See: https://github.com/kubernetes/apiextensions-apiserver/blob/ab2ddc498e31f2701200bff261e89120b3d929c3/pkg/apiserver/schema/cel/library/lists.go#L274
pub fn index_of(
    ftx: &FunctionContext,
    This(this): This<Arc<Vec<Value>>>,
    arg: Value,
) -> Result<i64> {
    find_position(ftx, this.iter(), &arg)
}

/// Implements the `lastIndexOf` function of the Kubernetes list library.
///
/// Returns the last positional index of the provided element in the list.
///
/// See: https://github.com/kubernetes/apiextensions-apiserver/blob/ab2ddc498e31f2701200bff261e89120b3d929c3/pkg/apiserver/schema/cel/library/lists.go#L288
pub fn last_index_of(
    ftx: &FunctionContext,
    This(this): This<Arc<Vec<Value>>>,
    arg: Value,
) -> Result<i64> {
    find_position(ftx, this.iter().rev(), &arg)
}

fn find_position<'a>(
    ftx: &FunctionContext,
    mut iter: impl Iterator<Item = &'a Value>,
    needle: &Value,
) -> Result<i64> {
    // Find the position (index) based on the equality of the list element and
    // the provided needle. This returns an Option, because the provided needle
    // might not be present in the list.
    let pos = iter.position(|element| element == needle);

    // If the position/index could not be found, -1 is returned. This is done in
    // accordance with the upstream Kubernetes implementation. See here:
    // https://github.com/kubernetes/apiextensions-apiserver/blob/ab2ddc498e31f2701200bff261e89120b3d929c3/pkg/apiserver/schema/cel/library/lists.go#L285
    // This further tries needs to convert the usize into a signed integer (i64)
    // which can fail. If this is the case, an execution error is returned.
    match pos {
        Some(pos) => i64::try_from(pos)
            .map_err(|err| ftx.error(format!("cannot convert usize into i64: {err}"))),
        None => Ok(-1i64),
    }
}

#[cfg(test)]
mod tests {
    use crate::test_utils::assert_script_eq;

    #[test]
    fn is_sorted() {
        #[rustfmt::skip]
        [
            ("is not alphabetically sorted", "['c', 'a', 'b'].isSorted()", false.into()),
            ("is alphabetically sorted", "['a', 'b', 'c'].isSorted()", true.into()),
            ("is not numerically sorted", "[3, 1, 2].isSorted()", false.into()),
            ("is numerically sorted", "[1, 2, 3].isSorted()", true.into()),
        ]
        .into_iter()
        .for_each(assert_script_eq);
    }

    #[test]
    fn index_of() {
        #[rustfmt::skip]
        [
            ("index of a is 0", "['a', 'b', 'a'].indexOf('a')", 0.into()),
            ("index of b is 0", "['a', 'b', 'a'].indexOf('b')", 1.into()),
        ]
        .into_iter()
        .for_each(assert_script_eq);
    }

    #[test]
    fn last_index_of() {
        #[rustfmt::skip]
        [
            ("last index of a is 0", "['a', 'b', 'a'].lastIndexOf('a')", 0.into()),
            ("last index of b is 1", "['a', 'b', 'a'].lastIndexOf('b')", 1.into()),
        ]
        .into_iter()
        .for_each(assert_script_eq);
    }
}
