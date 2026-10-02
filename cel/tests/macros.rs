use std::sync::Arc;

use cel::common::ast::{ComprehensionExpr, Expr, ListExpr, LiteralValue};
use cel::parser::{Macro, MacroExprHelper};
use cel::{Context, Env, IdedExpr, ParseError, Value};

/// `bind(var, init, expr)` evaluates `expr` with `var` bound to `init`.
///
/// Expands as cel-go's `cel.bind` does (`ext/bindings.go`): a comprehension
/// over an empty list whose accumulator is `var`, so only `accu_init` and
/// `result` are ever evaluated.
fn bind(
    helper: &mut MacroExprHelper<'_>,
    _target: Option<IdedExpr>,
    mut args: Vec<IdedExpr>,
) -> Result<IdedExpr, ParseError> {
    let result = args.pop().unwrap();
    let init = args.pop().unwrap();
    let name = args.pop().unwrap();
    let Expr::Ident(var) = name.expr else {
        return Err(helper.new_error(name.id, "bind() variable names must be simple identifiers"));
    };
    let iter_range = helper.next_expr(Expr::List(ListExpr::new(vec![])));
    let loop_cond = helper.next_expr(Expr::Literal(LiteralValue::Boolean(false.into())));
    let loop_step = helper.next_expr(Expr::Ident(var.clone()));
    Ok(
        helper.next_expr(Expr::Comprehension(Box::new(ComprehensionExpr {
            iter_range,
            iter_var: "#unused".to_string(),
            iter_var2: None,
            accu_var: var,
            accu_init: init,
            loop_cond,
            loop_step,
            result,
        }))),
    )
}

#[test]
fn a_macro_added_to_the_env_expands_the_calls_it_matches() {
    let mut env = Env::stdlib();
    env.add_macro(Macro::global("bind", 3, bind)).unwrap();
    let expr = env.parser().parse("bind(x, 2, x * x)").unwrap();
    let context = Context::with_env(Arc::new(env));
    assert_eq!(Value::resolve(&expr, &context), Ok(Value::Int(4)));
}
