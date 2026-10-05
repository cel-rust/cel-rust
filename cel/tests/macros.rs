use std::sync::Arc;

use cel::common::ast::{ComprehensionExpr, Expr, ListExpr, LiteralValue};
use cel::parser::{Macro, MacroExprHelper};
use cel::{Context, Env, IdedExpr, ParseError, Program, Value};

/// `bind(var, init, expr)` evaluates `expr` with `var` bound to `init`.
///
/// Expands as cel-go's `cel.bind` does (`ext/bindings.go`): a comprehension
/// over an empty list whose accumulator is `var`, so only `accu_init` and
/// `result` are ever evaluated.
fn bind(
    helper: &mut MacroExprHelper<'_>,
    _target: &mut Option<IdedExpr>,
    args: &mut Vec<IdedExpr>,
) -> Result<Option<IdedExpr>, ParseError> {
    let result = args.pop().unwrap();
    let init = args.pop().unwrap();
    let name = args.pop().unwrap();
    let Expr::Ident(var) = name.expr else {
        return Err(helper.new_error(name.id, "bind() variable names must be simple identifiers"));
    };
    let iter_range = helper.next_expr(Expr::List(ListExpr::new(vec![])));
    let loop_cond = helper.next_expr(Expr::Literal(LiteralValue::Boolean(false.into())));
    let loop_step = helper.next_expr(Expr::Ident(var.clone()));
    Ok(Some(helper.next_expr(Expr::Comprehension(Box::new(
        ComprehensionExpr {
            iter_range,
            iter_var: "#unused".to_string(),
            iter_var2: None,
            accu_var: var,
            accu_init: init,
            loop_cond,
            loop_step,
            result,
        },
    )))))
}

/// `cel.bind(..)`: `bind` on the `cel` namespace only, as cel-go's
/// `ext/bindings.go` declines any other target.
fn cel_bind(
    helper: &mut MacroExprHelper<'_>,
    target: &mut Option<IdedExpr>,
    args: &mut Vec<IdedExpr>,
) -> Result<Option<IdedExpr>, ParseError> {
    match target.as_ref().map(|t| &t.expr) {
        Some(Expr::Ident(namespace)) if namespace == "cel" => bind(helper, target, args),
        _ => Ok(None),
    }
}

#[test]
fn a_macro_added_to_the_env_expands_the_calls_it_matches() {
    let mut env = Env::stdlib();
    env.add_macro(Macro::global("bind", 3, bind)).unwrap();
    let expr = env.parser().parse("bind(x, 2, x * x)").unwrap();
    let context = Context::with_env(Arc::new(env));
    assert_eq!(Value::resolve(&expr, &context), Ok(Value::Int(4)));
}

#[test]
fn a_macro_leaves_the_calls_it_declines_as_written() {
    let mut env = Env::stdlib();
    env.add_macro(Macro::receiver("bind", 3, cel_bind)).unwrap();
    let declined = env.parser().parse("m.bind(x, 2, x * x)").unwrap();
    let without_macro = Env::stdlib().parser().parse("m.bind(x, 2, x * x)").unwrap();
    assert_eq!(declined, without_macro);
    let expanded = env.parser().parse("cel.bind(x, 2, x * x)").unwrap();
    let context = Context::with_env(Arc::new(env));
    assert_eq!(Value::resolve(&expanded, &context), Ok(Value::Int(4)));
}

/// `count(..)`: how many arguments the call has.
#[allow(clippy::ptr_arg)] // The signature of an expander.
fn count(
    helper: &mut MacroExprHelper<'_>,
    _target: &mut Option<IdedExpr>,
    args: &mut Vec<IdedExpr>,
) -> Result<Option<IdedExpr>, ParseError> {
    let count = LiteralValue::Int((args.len() as i64).into());
    Ok(Some(helper.next_expr(Expr::Literal(count))))
}

#[test]
fn a_var_arg_macro_expands_calls_with_any_number_of_arguments() {
    let mut env = Env::stdlib();
    env.add_macro(Macro::receiver_var_arg("count", count))
        .unwrap();
    env.add_macro(Macro::global_var_arg("count", count))
        .unwrap();
    let program = env
        .compile("[m.count(), m.count(a), m.count(a, b, c), count(), count(a, b)]")
        .unwrap();
    let context = Context::with_env(Arc::new(env));
    assert_eq!(
        program.execute(&context),
        Ok(Value::List(Arc::new(vec![
            Value::Int(0),
            Value::Int(1),
            Value::Int(3),
            Value::Int(0),
            Value::Int(2),
        ])))
    );
}

#[test]
fn a_macro_for_the_exact_argument_count_wins_over_a_var_arg_one() {
    let mut env = Env::stdlib();
    env.add_macro(Macro::global_var_arg("count", count))
        .unwrap();
    env.add_macro(Macro::global("count", 2, |helper, _, _| {
        Ok(Some(
            helper.next_expr(Expr::Literal(LiteralValue::Int((-2).into()))),
        ))
    }))
    .unwrap();
    let program = env.compile("[count(a), count(a, b)]").unwrap();
    let context = Context::with_env(Arc::new(env));
    assert_eq!(
        program.execute(&context),
        Ok(Value::List(Arc::new(vec![Value::Int(1), Value::Int(-2)])))
    );
}

/// The flow of extensions: add to an `Env`, compile with it, run with it.
#[test]
fn a_program_compiled_by_the_env_expands_its_macros() {
    let mut env = Env::stdlib();
    env.add_macro(Macro::global("first", 2, |_, _, args| {
        Ok(Some(args.remove(0)))
    }))
    .unwrap();
    let program = env.compile("first(1, 2)").unwrap();
    let context = Context::with_env(Arc::new(env));
    assert_eq!(program.execute(&context), Ok(Value::Int(1)));
}

#[test]
fn program_compile_expands_the_macros_of_the_standard_env() {
    let source = "[1, 2].exists(x, x > 1) && has(a.b)";
    let program = Program::compile(source).unwrap();
    let with_env = Env::stdlib().compile(source).unwrap();
    assert_eq!(program.expression(), with_env.expression());
}
