//! The lists extension library: member functions on `list`, `lists.range`
//! and the `sortBy` macro, following cel-go's `ext.Lists()`.

use super::{arg, iterable};
use crate::common::ast::{CallExpr, Expr, IdedExpr};
use crate::common::types::{CelInt, CelList, Kind, INT_TYPE, LIST_TYPE};
use crate::common::value::{CowVal, Val};
use crate::parser::{map_macro_expander, Macro, MacroExprHelper, ParseError};
use crate::{DeclarationError, Env, ExecutionError};
use std::cmp::Ordering;

/// The most elements `lists.range` creates, cel-go's default.
const MAX_RANGE_SIZE: i64 = 1_000_000;

/// The variable `sortBy` binds its target to, so that it is evaluated once.
const SORT_BY_INPUT: &str = "@__sortBy_input__";

/// The overload `sortBy` expands to: named so that CEL can't call it.
const SORT_BY_KEYS: &str = "@sortByAssociatedKeys";

/// Registers the lists extension's overloads, and its `sortBy` macro, on
/// `env`.
///
/// `sortBy` is only expanded by [`Env::compile`] and [`Env::parser`], not by
/// [`Program::compile`](crate::Program::compile).
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
    env.add_member_overload(
        SORT_BY_KEYS,
        "list.@sortByAssociatedKeys(list)",
        LIST_TYPE,
        vec![LIST_TYPE],
        sort_by_associated_keys,
    )?;
    crate::add_overload!(env, fn range: (CelInt) -> Result<CelList>, name = "lists.range")?;
    env.add_macro(Macro::receiver("sortBy", 2, sort_by_macro))?;
    Ok(())
}

/// Expands `list.sortBy(e, key)`, as cel-go does, into
/// `cel.bind(@__sortBy_input__, list,
/// @__sortBy_input__.@sortByAssociatedKeys(@__sortBy_input__.map(e, key)))`.
fn sort_by_macro(
    helper: &mut MacroExprHelper<'_>,
    target: &mut Option<IdedExpr>,
    args: &mut Vec<IdedExpr>,
) -> Result<Option<IdedExpr>, ParseError> {
    let list = target.take().expect("sortBy is a receiver macro");
    if matches!(list.expr, Expr::Literal(_) | Expr::Map(_) | Expr::Struct(_)) {
        return Err(helper.new_error(
            list.id,
            "sortBy can only be applied to a list, identifier, comprehension, call or select expression",
        ));
    }
    let input =
        |helper: &mut MacroExprHelper<'_>| helper.next_expr(Expr::Ident(SORT_BY_INPUT.to_owned()));
    let mut mapped = Some(input(helper));
    let keys = map_macro_expander(helper, &mut mapped, args)?;
    let target = input(helper);
    let sorted = helper.next_expr(Expr::Call(CallExpr {
        func_name: SORT_BY_KEYS.to_owned(),
        target: Some(Box::new(target)),
        args: vec![keys],
    }));
    Ok(Some(helper.bind(SORT_BY_INPUT, list, sorted)))
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
    sort_by_key(&mut elements, |element| element.as_ref(), "sort")?;
    Ok(owned_list(elements))
}

/// The elements of `list` in the ascending order of `keys`, the key of each
/// element at its index: the expansion of `sortBy`. Elements with equal keys
/// keep their order.
fn sort_by_associated_keys<'b, 'v>(
    args: Vec<CowVal<'b, 'v>>,
) -> Result<CowVal<'b, 'v>, ExecutionError> {
    let [list, keys] = take_args(args)?;
    let elements = into_elements(list)?;
    let mut items = iterable(keys.as_ref())?.iter();
    let mut keys = Vec::with_capacity(elements.len());
    while let Some(key) = items.next() {
        keys.push(key);
    }
    if keys.len() != elements.len() {
        return Err(ExecutionError::function_error(
            "sortBy",
            format!(
                "{SORT_BY_KEYS}() expected a list of the same size as the associated keys list, \
                 but got {} and {} elements respectively.",
                elements.len(),
                keys.len()
            ),
        ));
    }
    let mut keyed: Vec<_> = keys.into_iter().zip(elements).collect();
    sort_by_key(&mut keyed, |(key, _)| *key, "sortBy")?;
    Ok(owned_list(
        keyed.into_iter().map(|(_, element)| element).collect(),
    ))
}

/// Sorts `items` in the ascending order of their keys, which must all be of
/// one type, and comparable. Errors are reported as `function`'s.
fn sort_by_key<T>(
    items: &mut [T],
    key: impl Fn(&T) -> &dyn Val,
    function: &str,
) -> Result<(), ExecutionError> {
    // The sort may panic on comparisons that are not a total order, so each
    // key is compared to the first one up front: this rules out mixed types,
    // and a NaN, which compares to nothing.
    if let Some(first) = items.first().map(&key) {
        for item in items.iter() {
            if key(item).get_type() != first.get_type() {
                return Err(ExecutionError::function_error(
                    function,
                    "list elements must have the same type",
                ));
            }
            compare(first, key(item), function)?;
        }
    }
    let mut error = None;
    items.sort_by(|a, b| {
        compare(key(a), key(b), function).unwrap_or_else(|e| {
            error.get_or_insert(e);
            Ordering::Equal
        })
    });
    error.map_or(Ok(()), Err)
}

fn compare(a: &dyn Val, b: &dyn Val, function: &str) -> Result<Ordering, ExecutionError> {
    a.as_comparer()
        .ok_or_else(|| {
            ExecutionError::function_error(function, "list elements must be comparable")
        })?
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
    use crate::{Context, DeclarationError, Env, ExecutionError, Value};
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
        ctx.env()
            .compile(expr)
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
    fn sort_by() {
        assert_true("[-3, 1, -5, -2, 4].sortBy(e, -(e * e)) == [-5, 4, -3, -2, 1]");
        assert_true("[1, 2, 3].sortBy(e, -e) == [3, 2, 1]");
        assert_true(
            "['a', 'c', 'b', 'first'].sortBy(e, e == 'first' ? '' : e) \
             == ['first', 'a', 'b', 'c']",
        );
        assert_true(
            "[{'name': 'foo', 'score': 0}, {'name': 'bar', 'score': -10}, \
              {'name': 'baz', 'score': 1000}].sortBy(e, e.score).map(e, e.name) \
             == ['bar', 'foo', 'baz']",
        );
        assert_true("[].sortBy(e, e) == []");
        assert_true("['a'].sortBy(e, e) == ['a']");
        assert_true("[[1], [2]].sortBy(e, e[0]) == [[1], [2]]");
    }

    #[test]
    fn sort_by_is_stable() {
        assert_true("[3, 1, 2, 4, 5].sortBy(e, e % 2) == [2, 4, 3, 1, 5]");
        // Long enough not to be insertion sorted, which is stable either way.
        assert_true(
            "lists.range(100).sortBy(e, e % 2) \
             == lists.range(50).map(i, i * 2) + lists.range(50).map(i, i * 2 + 1)",
        );
        assert_true("['b', 'a', 'c'].sortBy(e, 0) == ['b', 'a', 'c']");
    }

    #[test]
    fn sort_by_any_target() {
        assert_true("[3, 1, 2].map(x, x * 2).sortBy(e, e) == [2, 4, 6]");
        assert_true("{'l': [2, 1]}.l.sortBy(e, e) == [1, 2]");
        assert_true("[2, 1].reverse().sortBy(e, -e) == [2, 1]");
        assert_true("[[2], [3, 1]].sortBy(l, l.sortBy(e, e)[0]) == [[3, 1], [2]]");
    }

    #[test]
    fn sort_by_errs_on_its_keys() {
        assert_error(
            "[[1], ['a']].sortBy(e, e[0])",
            "list elements must have the same type",
        );
        assert_error(
            "[[1], [2]].sortBy(e, e)",
            "list elements must be comparable",
        );
        assert!(matches!(
            eval("[1.0, 2.0].sortBy(e, e == 1.0 ? double('NaN') : e)"),
            Err(ExecutionError::ValuesNotComparable(..))
        ));
    }

    #[test]
    fn sort_by_on_a_map_is_an_error() {
        let mut ctx = context();
        ctx.add_variable(
            "m",
            Value::Map(std::collections::HashMap::from([(1, 2)]).into()),
        );
        assert!(matches!(
            eval_in(&ctx, "m.sortBy(k, k)"),
            Err(ExecutionError::NoSuchOverload { .. })
        ));
    }

    #[test]
    fn sort_by_parse_errors() {
        let ctx = context();
        for (expr, message) in [
            (
                "'abc'.sortBy(e, e)",
                "sortBy can only be applied to a list, identifier, comprehension, call or select expression",
            ),
            (
                "{1: 2}.sortBy(e, e)",
                "sortBy can only be applied to a list, identifier, comprehension, call or select expression",
            ),
            ("[1].sortBy(e.f, e)", "argument must be a simple name"),
        ] {
            let errors = ctx.env().compile(expr).expect_err(expr).errors;
            assert_eq!(errors.len(), 1, "{expr}");
            assert_eq!(errors[0].msg, message, "{expr}");
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
        assert_eq!(eval("ints.sortBy(e, -e)"), Ok(vec![3, 3, 2, 1].into()));
        assert_eq!(eval("nested.flatten()"), Ok(vec!["a", "b", "c"].into()));
    }
}
