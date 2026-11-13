//! The two-variable comprehensions extension library: `all`, `exists`,
//! `existsOne`, `transformList`, `transformMap` and `transformMapEntry` over
//! an index or key and a value, following cel-go's
//! `ext.TwoVarComprehensions()`.

use crate::common::ast::{
    operators, CallExpr, ComprehensionExpr, Expr, IdedExpr, LiteralValue, MapExpr,
};
use crate::common::decls::Signature;
use crate::common::types::{MutableMap, MAP_OF_K_V, PARAM_K, PARAM_V};
use crate::common::value::CowVal;
use crate::parser::{
    all_comprehension, exists_comprehension, exists_one_comprehension, extract_ident,
    list_comprehension, Macro, MacroExprHelper, ParseError, ACCUMULATOR,
};
use crate::{DeclarationError, Env, ExecutionError, Library};
use std::mem;

/// Adds the two-variable comprehensions extension library to `env`:
/// [`TwoVarComprehensions`].
pub fn extension(env: &mut Env) -> Result<(), DeclarationError> {
    env.add_library(TwoVarComprehensions::default())
}

/// The two-variable comprehensions extension library,
/// `cel.lib.ext.comprev2`: registers its macros on the [`Env`] it is added
/// to.
///
/// Each macro binds two variables to every entry of its target, a list's
/// index and element, or a map's key and value:
///
/// - `all(i, v, p)`, `exists(i, v, p)` and `existsOne(i, v, p)` (or
///   `exists_one`) test the predicate `p` as their one-variable namesakes do;
/// - `transformList(i, v, [f,] t)` is the list of `t` for each entry, or each
///   one `f` holds for;
/// - `transformMap(i, v, [f,] t)` is the map of each such index or key to `t`;
/// - `transformMapEntry(i, v, [f,] e)` is the map of the entries of each such
///   map `e`, and fails on a key two of them share.
///
/// They are only expanded by [`Env::compile`] and [`Env::parser`], not by
/// [`Program::compile`](crate::Program::compile).
///
/// ```
/// use cel::{extensions, Context, Env, Value};
/// use std::sync::Arc;
///
/// let mut env = Env::stdlib();
/// env.add_extension(extensions::two_var_comprehensions).unwrap();
/// let program = env
///     .compile("{'a': 1, 'b': 2}.transformMap(k, v, v * 10) == {'a': 10, 'b': 20}")
///     .unwrap();
/// let context = Context::with_env(Arc::new(env));
/// assert_eq!(program.execute(&context), Ok(Value::Bool(true)));
/// ```
#[derive(Clone, Debug, Default)]
#[non_exhaustive]
pub struct TwoVarComprehensions {}

impl Library for TwoVarComprehensions {
    fn name(&self) -> Option<&str> {
        Some("cel.lib.ext.comprev2")
    }

    fn register(&self, env: &mut Env) -> Result<(), DeclarationError> {
        env.add_macro(Macro::receiver("all", 3, quantifier(all_comprehension)))?;
        env.add_macro(Macro::receiver(
            "exists",
            3,
            quantifier(exists_comprehension),
        ))?;
        for name in ["existsOne", "exists_one"] {
            env.add_macro(Macro::receiver(
                name,
                3,
                quantifier(exists_one_comprehension),
            ))?;
        }
        for arg_count in [3, 4] {
            env.add_macro(Macro::receiver("transformList", arg_count, transform_list))?;
            env.add_macro(Macro::receiver("transformMap", arg_count, transform_map))?;
            env.add_macro(Macro::receiver(
                "transformMapEntry",
                arg_count,
                transform_map_entry,
            ))?;
        }
        // What `transformMap` and `transformMapEntry` step with. The evaluator
        // inserts in place into the map they build, so these copying overloads
        // only run for a comprehension shaped otherwise.
        env.add_overload(
            operators::MAP_INSERT,
            "@mapInsert_map_key_value",
            Signature::new(vec![MAP_OF_K_V, PARAM_K, PARAM_V]).returns(MAP_OF_K_V),
            insert_entry,
        )?;
        env.add_overload(
            operators::MAP_INSERT,
            "@mapInsert_map_map",
            Signature::new(vec![MAP_OF_K_V, MAP_OF_K_V]).returns(MAP_OF_K_V),
            insert_entries,
        )?;
        Ok(())
    }
}

/// A builder of the comprehension a one-variable macro expands to, given a
/// second variable: see [`all_comprehension`].
type Quantifier =
    fn(&mut MacroExprHelper<'_>, IdedExpr, String, Option<String>, IdedExpr) -> IdedExpr;

/// The expander of `range.<macro>(i, v, predicate)`, which `build` makes the
/// comprehension of.
fn quantifier(
    build: Quantifier,
) -> impl Fn(
    &mut MacroExprHelper<'_>,
    &mut Option<IdedExpr>,
    &mut Vec<IdedExpr>,
) -> Result<Option<IdedExpr>, ParseError>
       + Send
       + Sync
       + 'static {
    move |helper, target, args| {
        let [var, var2, predicate] =
            <[IdedExpr; 3]>::try_from(mem::take(args)).expect("the macro matched three arguments");
        let (iter_var, iter_var2) = iter_vars(helper, var, var2)?;
        let range = target.take().expect("a receiver macro has a target");
        Ok(Some(build(
            helper,
            range,
            iter_var,
            Some(iter_var2),
            predicate,
        )))
    }
}

/// `range.transformList(i, v, [filter,] transform)`.
fn transform_list(
    helper: &mut MacroExprHelper<'_>,
    target: &mut Option<IdedExpr>,
    args: &mut Vec<IdedExpr>,
) -> Result<Option<IdedExpr>, ParseError> {
    let Transform {
        iter_var,
        iter_var2,
        filter,
        transform,
    } = Transform::of(helper, args)?;
    let range = target.take().expect("a receiver macro has a target");
    Ok(Some(list_comprehension(
        helper,
        range,
        iter_var,
        Some(iter_var2),
        filter,
        transform,
    )))
}

/// `range.transformMap(i, v, [filter,] transform)`: steps with
/// `cel.@mapInsert(@result, i, transform)`.
fn transform_map(
    helper: &mut MacroExprHelper<'_>,
    target: &mut Option<IdedExpr>,
    args: &mut Vec<IdedExpr>,
) -> Result<Option<IdedExpr>, ParseError> {
    let Transform {
        iter_var,
        iter_var2,
        filter,
        transform,
    } = Transform::of(helper, args)?;
    let key = helper.next_expr(Expr::Ident(iter_var.clone()));
    let range = target.take().expect("a receiver macro has a target");
    Ok(Some(map_comprehension(
        helper,
        range,
        iter_var,
        iter_var2,
        filter,
        vec![key, transform],
    )))
}

/// `range.transformMapEntry(i, v, [filter,] transform)`: steps with
/// `cel.@mapInsert(@result, transform)`.
fn transform_map_entry(
    helper: &mut MacroExprHelper<'_>,
    target: &mut Option<IdedExpr>,
    args: &mut Vec<IdedExpr>,
) -> Result<Option<IdedExpr>, ParseError> {
    let Transform {
        iter_var,
        iter_var2,
        filter,
        transform,
    } = Transform::of(helper, args)?;
    let range = target.take().expect("a receiver macro has a target");
    Ok(Some(map_comprehension(
        helper,
        range,
        iter_var,
        iter_var2,
        filter,
        vec![transform],
    )))
}

/// The arguments of a `transform*` macro: `(i, v, transform)` or
/// `(i, v, filter, transform)`.
struct Transform {
    iter_var: String,
    iter_var2: String,
    filter: Option<IdedExpr>,
    transform: IdedExpr,
}

impl Transform {
    fn of(helper: &mut MacroExprHelper<'_>, args: &mut Vec<IdedExpr>) -> Result<Self, ParseError> {
        let mut args = mem::take(args).into_iter();
        let expected = "the macro matched three or four arguments";
        let (var, var2) = (args.next().expect(expected), args.next().expect(expected));
        let (iter_var, iter_var2) = iter_vars(helper, var, var2)?;
        let transform = args.next_back().expect(expected);
        Ok(Self {
            iter_var,
            iter_var2,
            filter: args.next(),
            transform,
        })
    }
}

/// The map built by inserting `insertion` (a key and a value, or a map of
/// entries) for each entry of `range`, or for each one `filter` holds for:
/// the comprehension stepping with `cel.@mapInsert(@result, ..insertion)` from
/// `{}`.
fn map_comprehension(
    helper: &mut MacroExprHelper<'_>,
    range: IdedExpr,
    iter_var: String,
    iter_var2: String,
    filter: Option<IdedExpr>,
    insertion: Vec<IdedExpr>,
) -> IdedExpr {
    let init = helper.next_expr(Expr::Map(MapExpr::default()));
    let condition = helper.next_expr(Expr::Literal(LiteralValue::Boolean(true.into())));
    let accu = helper.next_expr(Expr::Ident(ACCUMULATOR.to_string()));
    let step = helper.next_expr(Expr::Call(CallExpr {
        func_name: operators::MAP_INSERT.to_string(),
        target: None,
        args: std::iter::once(accu).chain(insertion).collect(),
    }));
    let step = match filter {
        Some(filter) => {
            let accu = helper.next_expr(Expr::Ident(ACCUMULATOR.to_string()));
            helper.next_expr(Expr::Call(CallExpr {
                func_name: operators::CONDITIONAL.to_string(),
                target: None,
                args: vec![filter, step, accu],
            }))
        }
        None => step,
    };
    let result = helper.next_expr(Expr::Ident(ACCUMULATOR.to_string()));
    helper.next_expr(Expr::Comprehension(Box::new(ComprehensionExpr {
        iter_range: range,
        iter_var,
        iter_var2: Some(iter_var2),
        accu_var: ACCUMULATOR.to_string(),
        accu_init: init,
        loop_cond: condition,
        loop_step: step,
        result,
    })))
}

/// The names of the iteration variables `var` and `var2`, checked as cel-go's
/// `extractIterVars` does.
///
/// # Errors
///
/// Fails when either isn't a simple name, when they are the same name, or
/// when either is an accumulator's name: `@result`, or `__result__`, which
/// cel-go's parser named it before.
fn iter_vars(
    helper: &mut MacroExprHelper<'_>,
    var: IdedExpr,
    var2: IdedExpr,
) -> Result<(String, String), ParseError> {
    let (id, id2) = (var.id, var2.id);
    let var = extract_ident(var, helper)?;
    let var2 = extract_ident(var2, helper)?;
    if var == var2 {
        return Err(helper.new_error(id2, format!("duplicate variable name: {var}")));
    }
    for (name, id) in [(&var, id), (&var2, id2)] {
        if name == ACCUMULATOR || name == "__result__" {
            return Err(helper.new_error(id, "iteration variable overwrites accumulator variable"));
        }
    }
    Ok((var, var2))
}

/// `cel.@mapInsert(map, key, value)`: a copy of `map`, with `value` at `key`.
/// Fails on a key `map` already has.
fn insert_entry<'b, 'v>(args: Vec<CowVal<'b, 'v>>) -> Result<CowVal<'b, 'v>, ExecutionError> {
    let [map, key, value] = <[CowVal; 3]>::try_from(args)
        .map_err(|args| ExecutionError::invalid_argument_count(3, args.len()))?;
    let mut copy = MutableMap::default();
    copy.extend(map.as_ref())?;
    copy.insert(key, value)?;
    Ok(CowVal::owned(copy.to_immutable()))
}

/// `cel.@mapInsert(map, entries)`: a copy of `map`, with the entries of the
/// map `entries`. Fails on a key `map` already has.
fn insert_entries<'b, 'v>(args: Vec<CowVal<'b, 'v>>) -> Result<CowVal<'b, 'v>, ExecutionError> {
    let [map, entries] = <[CowVal; 2]>::try_from(args)
        .map_err(|args| ExecutionError::invalid_argument_count(2, args.len()))?;
    let mut copy = MutableMap::default();
    copy.extend(map.as_ref())?;
    copy.extend(entries.as_ref())?;
    Ok(CowVal::owned(copy.to_immutable()))
}

#[cfg(test)]
mod tests {
    use super::{insert_entries, insert_entry};
    use crate::common::value::{CowVal, Val};
    use crate::{Context, Env, ExecutionError, Value};
    use std::collections::HashMap;
    use std::sync::Arc;

    fn env() -> Env {
        let mut env = Env::stdlib();
        env.add_extension(crate::extensions::two_var_comprehensions)
            .expect("We can't test the extension, if we can't register it");
        env
    }

    fn eval(expr: &str) -> Result<Value, ExecutionError> {
        let env = env();
        let program = env.compile(expr).expect("This must be valid CEL");
        program.execute(&Context::with_env(Arc::new(env)))
    }

    /// The `macros2` conformance cases that expect an error only check that
    /// there is one: these are the errors they name, but for one.
    #[test]
    fn errors_are_those_the_conformance_suite_names() {
        let divide_by_zero = [
            "[1, 2, 3].exists(i, v, v / i == 17)",
            "[1, 2, 3].all(i, v, v / i != 17)",
            "[3, 2, 1, 0].existsOne(i, v, v / i > 1)",
            "[2, 1, 0].transformList(i, v, v / i)",
            "[2, 1, 0].transformList(i, v, v / i > 0, v)",
            "{'foo': 2, 'bar': 1, 'baz': 0}.transformMap(k, v, 4 / v)",
            "{'foo': 2, 'bar': 1, 'baz': 0}.transformMap(k, v, k == 'baz' && 4 / v == 0, v)",
        ];
        for expr in divide_by_zero {
            assert!(
                matches!(eval(expr), Err(ExecutionError::DivisionByZero(_))),
                "{expr}: {:?}",
                eval(expr)
            );
        }
        // cel-spec names `no_such_overload` here. cel-rust's `%` reports any
        // operand it can't take as `UnsupportedBinaryOperator`, in the
        // one-variable `all` too: what matters here is that the error of the
        // `'foo'` element isn't absorbed by the elements after it.
        assert_eq!(
            eval("[0, 'foo', 5].all(i, v, v % 3 == i)"),
            Err(ExecutionError::UnsupportedBinaryOperator(
                "rem",
                Value::from("foo"),
                Value::Int(3)
            ))
        );
    }

    /// cel-go's examples (`ext/comprehensions.go`), over lists and maps.
    #[test]
    fn the_quantifiers_bind_the_index_or_key_and_the_value() {
        for (expr, expected) in [
            ("[1, 2, 3].all(i, j, i < j)", true),
            ("{'hello': 'world', 'taco': 'taco'}.all(k, v, k != v)", false),
            (
                "{'h': ['hello', 'hi'], 'j': ['joke', 'jog']}.all(k, vals, vals.all(v, v.startsWith(k)))",
                true,
            ),
            (
                "{'greeting': 'hello', 'farewell': 'goodbye'}.exists(k, v, k.startsWith('good') || v.endsWith('bye'))",
                true,
            ),
            ("[1, 2, 4, 8, 16].exists(i, v, v == 1024 && i == 10)", false),
            ("[1, 2, 1, 3, 1, 4].existsOne(i, v, i == 1 || v == 1)", false),
            ("[1, 1, 2, 2, 3, 3].existsOne(i, v, i == 2 && v == 2)", true),
            ("{'i': 0, 'j': 1, 'k': 2}.existsOne(i, v, i == 'l' || v == 1)", true),
            ("[1, 1, 2, 2, 3, 3].exists_one(i, v, i == 2 && v == 2)", true),
        ] {
            assert_eq!(eval(expr), Ok(Value::Bool(expected)), "{expr}");
        }
    }

    /// The transforms over the other shape than the conformance suite's: a
    /// map from a list, a list from a map (one entry: a map's order is
    /// unspecified), and `transformMapEntry`, which the suite doesn't cover.
    #[test]
    fn the_transforms_build_lists_and_maps_from_either() {
        for expr in [
            "[1, 2, 3].transformMap(i, v, (i * v) + v) == {0: 1, 1: 4, 2: 9}",
            "[1, 2, 3].transformMap(i, v, i % 2 == 0, (i * v) + v) == {0: 1, 2: 9}",
            "{'greeting': 'hello'}.transformList(k, v, k + ': ' + v) == ['greeting: hello']",
            "{'greeting': 'hello'}.transformMapEntry(k, v, {v: k}) == {'hello': 'greeting'}",
            "[1, 2, 3].transformMapEntry(i, v, {v: i}) == {1: 0, 2: 1, 3: 2}",
            "{'a': 1, 'b': 2}.transformMapEntry(k, v, v > 1, {v: k}) == {2: 'b'}",
            "[].transformMapEntry(i, v, {v: i}) == {}",
        ] {
            assert_eq!(eval(expr), Ok(Value::Bool(true)), "{expr}");
        }
    }

    /// Keys compare as numbers, as in a map literal: `1u` is the key `1`.
    #[test]
    fn transform_map_entry_fails_on_a_key_two_entries_share() {
        assert_eq!(
            eval("{'greeting': 'aloha', 'farewell': 'aloha'}.transformMapEntry(k, v, {v: k})"),
            Err(ExecutionError::DuplicateKey(Value::from("aloha")))
        );
        assert_eq!(
            eval("[1, 1u].transformMapEntry(i, v, {v: i})"),
            Err(ExecutionError::DuplicateKey(Value::UInt(1)))
        );
    }

    #[test]
    fn the_iteration_variables_are_two_simple_names_but_the_accumulator() {
        for (expr, message) in [
            ("[1].all(i.j, v, true)", "argument must be a simple name"),
            (
                "[1].transformList(i, 1, v)",
                "argument must be a simple name",
            ),
            ("[1].exists(i, i, true)", "duplicate variable name: i"),
            (
                "[1].transformMap(__result__, v, v)",
                "iteration variable overwrites accumulator variable",
            ),
            (
                "[1].existsOne(i, __result__, true)",
                "iteration variable overwrites accumulator variable",
            ),
        ] {
            let errors = env().compile(expr).unwrap_err().errors;
            let messages: Vec<_> = errors.iter().map(|e| e.msg.as_str()).collect();
            assert_eq!(messages, [message], "{expr}");
        }
    }

    /// The overloads the evaluator falls back to when it doesn't insert in
    /// place: they copy the map, and insert as it does.
    #[test]
    fn map_insert_copies_and_inserts() {
        let val = |value: Value| CowVal::Owned(Box::<dyn Val>::try_from(value).unwrap());
        let map = |entries: &[(i64, &str)]| {
            Value::Map(HashMap::from_iter(entries.iter().copied()).into())
        };
        let as_value = |result: Result<CowVal, ExecutionError>| {
            result.and_then(|v| Value::try_from(v.as_ref()))
        };

        assert_eq!(
            as_value(insert_entry(vec![
                val(map(&[(1, "a")])),
                val(Value::Int(2)),
                val(Value::from("b")),
            ])),
            Ok(map(&[(1, "a"), (2, "b")]))
        );
        assert_eq!(
            as_value(insert_entry(vec![
                val(map(&[(1, "a")])),
                val(Value::UInt(1)),
                val(Value::from("b")),
            ])),
            Err(ExecutionError::DuplicateKey(Value::UInt(1)))
        );
        assert_eq!(
            as_value(insert_entries(vec![
                val(map(&[(1, "a")])),
                val(map(&[(2, "b")]))
            ])),
            Ok(map(&[(1, "a"), (2, "b")]))
        );
        assert_eq!(
            as_value(insert_entries(vec![
                val(map(&[(1, "a")])),
                val(map(&[(1, "b")]))
            ])),
            Err(ExecutionError::DuplicateKey(Value::Int(1)))
        );
    }
}
