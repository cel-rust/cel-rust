//! The lists extension library: member functions on `list`, and
//! `lists.range`, following cel-go's `ext.Lists()`.
//!
//! `sortBy`, a macro in cel-go, is not available.

use super::{arg, iterable};
use crate::common::types::{CelInt, CelList, Kind, INT_TYPE, LIST_TYPE};
use crate::common::value::{CowVal, Val};
use crate::{DeclarationError, Env, ExecutionError};
use std::cmp::Ordering;

/// The most elements `lists.range` creates, cel-go's default.
const MAX_RANGE_SIZE: i64 = 1_000_000;

/// Registers the lists extension's overloads on `env`.
pub fn extension(env: &mut Env) -> Result<(), DeclarationError> {
    // Registered by hand, as the macro would downcast the receiver to a
    // `CelList`: these take any `list`, like the stdlib's list overloads.
    env.add_member_overload(
        "slice",
        "list.slice(int,int)",
        LIST_TYPE,
        vec![INT_TYPE, INT_TYPE],
        slice,
    )?;
    env.add_member_overload("flatten", "list.flatten()", LIST_TYPE, vec![], flatten)?;
    env.add_member_overload(
        "flatten",
        "list.flatten(int)",
        LIST_TYPE,
        vec![INT_TYPE],
        flatten_depth,
    )?;
    env.add_member_overload("distinct", "list.distinct()", LIST_TYPE, vec![], distinct)?;
    env.add_member_overload("reverse", "list.reverse()", LIST_TYPE, vec![], reverse)?;
    env.add_member_overload("sort", "list.sort()", LIST_TYPE, vec![], sort)?;
    crate::add_overload!(env, fn range: (CelInt) -> Result<CelList>, name = "lists.range")?;
    Ok(())
}

/// `args` as the `N` arguments of an overload.
fn take_args<'b, 'v, const N: usize>(
    args: Vec<CowVal<'b, 'v>>,
) -> Result<[CowVal<'b, 'v>; N], ExecutionError> {
    args.try_into()
        .map_err(|args: Vec<_>| ExecutionError::invalid_argument_count(N, args.len()))
}

/// The elements of `list`: moved out of an owned `CelList`, cloned otherwise.
fn into_elements<'v>(list: CowVal<'_, 'v>) -> Result<Vec<Box<dyn Val + 'v>>, ExecutionError> {
    let list = match list {
        CowVal::Owned(list) => match Vec::try_from(list) {
            Ok(items) => return Ok(items),
            Err(list) => CowVal::Owned(list),
        },
        borrowed => borrowed,
    };
    let mut items = iterable(list.as_ref())?.iter();
    let mut elements = Vec::new();
    while let Some(item) = items.next() {
        elements.push(item.clone_as_boxed());
    }
    Ok(elements)
}

fn owned_list<'b, 'v>(elements: Vec<Box<dyn Val + 'v>>) -> CowVal<'b, 'v> {
    CowVal::owned(CelList::from(elements))
}

/// The elements from index `start` up to, but excluding, `end`.
fn slice<'b, 'v>(args: Vec<CowVal<'b, 'v>>) -> Result<CowVal<'b, 'v>, ExecutionError> {
    let [list, start, end] = take_args(args)?;
    let (start, end) = (
        *arg::<CelInt>(start.as_ref())?.inner(),
        *arg::<CelInt>(end.as_ref())?.inner(),
    );
    let invalid = |reason: String| {
        ExecutionError::function_error("slice", format!("cannot slice({start}, {end}), {reason}"))
    };
    if start < 0 || end < 0 {
        return Err(invalid("negative indexes not supported".to_owned()));
    }
    if start > end {
        return Err(invalid(
            "start index must be less than or equal to end index".to_owned(),
        ));
    }
    // Only the elements kept are cloned, so a borrowed list is walked rather
    // than copied whole.
    let mut items = iterable(list.as_ref())?.iter();
    let mut sliced = Vec::new();
    let mut len = 0;
    while let Some(item) = items.next() {
        if (start..end).contains(&len) {
            sliced.push(item.clone_as_boxed());
        }
        len += 1;
    }
    if len < end {
        return Err(invalid(format!("list is length {len}")));
    }
    Ok(owned_list(sliced))
}

fn flatten<'b, 'v>(args: Vec<CowVal<'b, 'v>>) -> Result<CowVal<'b, 'v>, ExecutionError> {
    let [list] = take_args(args)?;
    flatten_to(list, 1)
}

fn flatten_depth<'b, 'v>(args: Vec<CowVal<'b, 'v>>) -> Result<CowVal<'b, 'v>, ExecutionError> {
    let [list, depth] = take_args(args)?;
    let depth = *arg::<CelInt>(depth.as_ref())?.inner();
    flatten_to(list, depth)
}

/// `list` with the elements of its nested lists spliced in, `depth` levels
/// deep.
fn flatten_to<'b, 'v>(list: CowVal<'b, 'v>, depth: i64) -> Result<CowVal<'b, 'v>, ExecutionError> {
    let depth = u64::try_from(depth)
        .map_err(|_| ExecutionError::function_error("flatten", "level must be non-negative"))?;
    let mut flat = Vec::new();
    flatten_into(into_elements(list)?, depth, &mut flat)?;
    Ok(owned_list(flat))
}

fn flatten_into<'v>(
    elements: Vec<Box<dyn Val + 'v>>,
    depth: u64,
    flat: &mut Vec<Box<dyn Val + 'v>>,
) -> Result<(), ExecutionError> {
    for element in elements {
        // Only lists are flattened: a map is iterable too, but kept whole.
        if depth > 0 && element.get_type().kind() == Kind::List {
            flatten_into(into_elements(CowVal::Owned(element))?, depth - 1, flat)?;
        } else {
            flat.push(element);
        }
    }
    Ok(())
}

/// The first of each set of equal elements, in order. Equality is CEL's, so
/// `1`, `1u` and `1.0` are one element.
fn distinct<'b, 'v>(args: Vec<CowVal<'b, 'v>>) -> Result<CowVal<'b, 'v>, ExecutionError> {
    let [list] = take_args(args)?;
    let mut items = iterable(list.as_ref())?.iter();
    let mut unique: Vec<&dyn Val> = Vec::new();
    while let Some(item) = items.next() {
        if !unique.iter().any(|seen| item.equals(*seen)) {
            unique.push(item);
        }
    }
    Ok(owned_list(
        unique
            .into_iter()
            .map(|item| item.clone_as_boxed())
            .collect(),
    ))
}

fn reverse<'b, 'v>(args: Vec<CowVal<'b, 'v>>) -> Result<CowVal<'b, 'v>, ExecutionError> {
    let [list] = take_args(args)?;
    let mut elements = into_elements(list)?;
    elements.reverse();
    Ok(owned_list(elements))
}

/// The elements in ascending order. They must all be of one type, and
/// comparable: `[1, 1u].sort()` is an error, as in cel-go.
fn sort<'b, 'v>(args: Vec<CowVal<'b, 'v>>) -> Result<CowVal<'b, 'v>, ExecutionError> {
    let [list] = take_args(args)?;
    let mut elements = into_elements(list)?;
    // The sort may panic on comparisons that are not a total order, so each
    // element is compared to the first one up front: this rules out mixed
    // types, and a NaN, which compares to nothing.
    if let Some(first) = elements.first() {
        for element in &elements {
            if element.get_type() != first.get_type() {
                return Err(ExecutionError::function_error(
                    "sort",
                    "list elements must have the same type",
                ));
            }
            compare(first.as_ref(), element.as_ref())?;
        }
    }
    let mut error = None;
    elements.sort_by(|a, b| {
        compare(a.as_ref(), b.as_ref()).unwrap_or_else(|e| {
            error.get_or_insert(e);
            Ordering::Equal
        })
    });
    match error {
        Some(error) => Err(error),
        None => Ok(owned_list(elements)),
    }
}

fn compare(a: &dyn Val, b: &dyn Val) -> Result<Ordering, ExecutionError> {
    a.as_comparer()
        .ok_or_else(|| ExecutionError::function_error("sort", "list elements must be comparable"))?
        .compare(b)
}

/// The ints from `0` up to, but excluding, `n`.
fn range(n: &CelInt) -> Result<CelList<'static>, ExecutionError> {
    let n = *n.inner();
    if n < 0 {
        return Err(ExecutionError::function_error(
            "lists.range",
            format!("size must be non-negative, got {n}"),
        ));
    }
    if n > MAX_RANGE_SIZE {
        return Err(ExecutionError::function_error(
            "lists.range",
            format!("size {n} exceeds maximum allowed ({MAX_RANGE_SIZE})"),
        ));
    }
    Ok((0..n)
        .map(|i| Box::new(CelInt::from(i)) as Box<dyn Val>)
        .collect::<Vec<_>>()
        .into())
}

#[cfg(test)]
mod tests {
    use crate::common::types::{CelInt, CelString};
    use crate::extensions::tests::OtherList;
    use crate::{Context, DeclarationError, Env, ExecutionError, Program, Value};
    use std::sync::Arc;

    #[test]
    fn registering_twice_is_an_error() {
        let mut env = Env::stdlib();
        assert_eq!(env.add_extension(crate::extensions::lists), Ok(()));
        assert_eq!(
            env.add_extension(crate::extensions::lists),
            Err(DeclarationError::duplicate_overload(
                "slice",
                "list.slice(int,int)"
            ))
        );
    }

    #[test]
    fn registers_alongside_the_strings_extension() {
        let mut env = Env::stdlib();
        assert_eq!(env.add_extension(crate::extensions::strings), Ok(()));
        assert_eq!(env.add_extension(crate::extensions::lists), Ok(()));
        let ctx = Context::with_env(Arc::new(env));
        assert_eq!(eval_in(&ctx, "'abc'.reverse()"), Ok("cba".into()));
        assert_eq!(eval_in(&ctx, "[1, 2].reverse()"), Ok(vec![2, 1].into()));
    }

    fn context() -> Context<'static, 'static> {
        let mut env = Env::stdlib();
        env.add_extension(crate::extensions::lists)
            .expect("We can't test the extension, if we can't register it");
        Context::with_env(Arc::new(env))
    }

    fn eval_in(ctx: &Context, expr: &str) -> Result<Value, ExecutionError> {
        Program::compile(expr)
            .expect("This must be valid CEL")
            .execute(ctx)
    }

    fn eval(expr: &str) -> Result<Value, ExecutionError> {
        eval_in(&context(), expr)
    }

    fn assert_true(expr: &str) {
        assert_eq!(eval(expr), Ok(Value::Bool(true)), "{expr}");
    }

    fn assert_error(expr: &str, message: &str) {
        match eval(expr) {
            Err(ExecutionError::FunctionError { message: m, .. }) => {
                assert_eq!(m, message, "{expr}")
            }
            other => panic!("{expr}: expected a function error, got {other:?}"),
        }
    }

    #[test]
    fn slice() {
        assert_true("[1, 2, 3, 4].slice(1, 3) == [2, 3]");
        assert_true("[1, 2, 3, 4].slice(0, 4) == [1, 2, 3, 4]");
        assert_true("[1, 2, 3, 4].slice(2, 2) == []");
        assert_true("[1, 2, 3, 4].slice(4, 4) == []");
        assert_error(
            "[1, 2, 3, 4].slice(3, 1)",
            "cannot slice(3, 1), start index must be less than or equal to end index",
        );
        assert_error(
            "[1, 2, 3, 4].slice(1, 5)",
            "cannot slice(1, 5), list is length 4",
        );
        assert_error(
            "[1, 2, 3, 4].slice(-1, 2)",
            "cannot slice(-1, 2), negative indexes not supported",
        );
        assert_error(
            "[1, 2, 3, 4].slice(1, -1)",
            "cannot slice(1, -1), negative indexes not supported",
        );
    }

    #[test]
    fn flatten() {
        assert_true("[1, [2, 3], [4]].flatten() == [1, 2, 3, 4]");
        assert_true("[1, [2, [3, 4]]].flatten() == [1, 2, [3, 4]]");
        assert_true("[1, 2, [], [], [3, 4]].flatten() == [1, 2, 3, 4]");
        assert_true("[1, [2, [3, [4]]]].flatten(2) == [1, 2, 3, [4]]");
        assert_true("[1, [2, 3]].flatten(0) == [1, [2, 3]]");
        assert_true("[[1], [[2]]].flatten(10) == [1, 2]");
        assert_true("[{'a': 1}, [{'b': 2}]].flatten() == [{'a': 1}, {'b': 2}]");
        assert_true("dyn([]).flatten() == []");
        assert_error("[1, [2, 3]].flatten(-1)", "level must be non-negative");
    }

    #[test]
    fn distinct() {
        assert_true("[1, 2, 2, 3, 3, 3].distinct() == [1, 2, 3]");
        assert_true("['b', 'b', 'c', 'a', 'c'].distinct() == ['b', 'c', 'a']");
        assert_true("[1, 'b', 2, 'b'].distinct() == [1, 'b', 2]");
        assert_true("[1, 1u, 1.0, 2u].distinct() == [1, 2u]");
        assert_true("[[1], [1], [2]].distinct() == [[1], [2]]");
        assert_true("[].distinct() == []");
    }

    #[test]
    fn reverse() {
        assert_true("[5, 3, 1, 2].reverse() == [2, 1, 3, 5]");
        assert_true("[].reverse() == []");
        assert_true("[false, true, true].reverse().reverse() == [false, true, true]");
    }

    #[test]
    fn sort() {
        assert_true("[3, 2, 1].sort() == [1, 2, 3]");
        assert_true("[42u, 3u, 1337u].sort() == [3u, 42u, 1337u]");
        assert_true("[1.0, -1.5, 2.0].sort() == [-1.5, 1.0, 2.0]");
        assert_true("['b', 'c', 'a'].sort() == ['a', 'b', 'c']");
        assert_true("[b'd', b'a', b'aa'].sort() == [b'a', b'aa', b'd']");
        assert_true("[true, false, true].sort() == [false, true, true]");
        assert_true("[].sort() == []");
        assert_error("[1, 'b'].sort()", "list elements must have the same type");
        assert_error("[1, 1u].sort()", "list elements must have the same type");
        assert_error("[[1, 2, 3]].sort()", "list elements must be comparable");
    }

    #[test]
    #[cfg(feature = "chrono")]
    fn sort_durations_and_timestamps() {
        assert_true(
            "[duration('1m'), duration('2s'), duration('3h')].sort() \
             == [duration('2s'), duration('1m'), duration('3h')]",
        );
        assert_true(
            "[timestamp('2024-01-03T00:00:00Z'), timestamp('2024-01-01T00:00:00Z')].sort() \
             == [timestamp('2024-01-01T00:00:00Z'), timestamp('2024-01-03T00:00:00Z')]",
        );
    }

    #[test]
    fn sort_errs_on_nan() {
        for expr in [
            "[1.0, double('NaN'), 2.0].sort()",
            "[double('NaN'), 1.0].sort()",
            "[double('NaN')].sort()",
        ] {
            assert!(
                matches!(eval(expr), Err(ExecutionError::ValuesNotComparable(..))),
                "{expr}"
            );
        }
    }

    #[test]
    fn range() {
        assert_true("lists.range(5) == [0, 1, 2, 3, 4]");
        assert_true("lists.range(0) == []");
        assert_true("size(lists.range(1000000)) == 1000000");
        assert_error("lists.range(-1)", "size must be non-negative, got -1");
        assert_error(
            "lists.range(1000001)",
            "size 1000001 exceeds maximum allowed (1000000)",
        );
    }

    #[test]
    fn any_list() {
        let mut ctx = context();
        ctx.add_variable_as_val(
            "ints",
            Box::new(OtherList(vec![
                CelInt::from(3),
                CelInt::from(1),
                CelInt::from(3),
                CelInt::from(2),
            ])),
        );
        ctx.add_variable_as_val(
            "nested",
            Box::new(OtherList(vec![
                OtherList(vec![CelString::from("a")]),
                OtherList(vec![CelString::from("b"), CelString::from("c")]),
            ])),
        );
        let eval = |expr| eval_in(&ctx, expr);
        assert_eq!(eval("ints.slice(1, 3)"), Ok(vec![1, 3].into()));
        assert_eq!(eval("ints.distinct()"), Ok(vec![3, 1, 2].into()));
        assert_eq!(eval("ints.reverse()"), Ok(vec![2, 3, 1, 3].into()));
        assert_eq!(eval("ints.sort()"), Ok(vec![1, 2, 3, 3].into()));
        assert_eq!(eval("nested.flatten()"), Ok(vec!["a", "b", "c"].into()));
    }
}
