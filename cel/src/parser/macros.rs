use crate::common::ast::{
    operators, CallExpr, ComprehensionExpr, Expr, IdedExpr, ListExpr, LiteralValue,
};
use crate::parser::{MacroExprHelper, ParseError};

const OPT_MAP: &str = "optMap";
const OPT_FLAT_MAP: &str = "optFlatMap";

pub type MacroExpander = fn(
    helper: &mut MacroExprHelper,
    target: Option<IdedExpr>,
    args: Vec<IdedExpr>,
) -> Result<IdedExpr, ParseError>;

pub fn find_expander(
    func_name: &str,
    target: Option<&IdedExpr>,
    args: &[IdedExpr],
    optional_syntax: bool,
) -> Option<MacroExpander> {
    match func_name {
        OPT_MAP if optional_syntax && args.len() == 2 && target.is_some() => {
            Some(opt_map_macro_expander)
        }
        OPT_FLAT_MAP if optional_syntax && args.len() == 2 && target.is_some() => {
            Some(opt_flat_map_macro_expander)
        }
        operators::HAS if args.len() == 1 && target.is_none() => Some(has_macro_expander),
        operators::EXISTS if args.len() == 2 && target.is_some() => Some(exists_macro_expander),
        operators::ALL if args.len() == 2 && target.is_some() => Some(all_macro_expander),
        operators::EXISTS_ONE | "existsOne" if args.len() == 2 && target.is_some() => {
            Some(exists_one_macro_expander)
        }
        operators::MAP if (args.len() == 2 || args.len() == 3) && target.is_some() => {
            Some(map_macro_expander)
        }
        operators::FILTER if args.len() == 2 && target.is_some() => Some(filter_macro_expander),
        _ => None,
    }
}

fn has_macro_expander(
    helper: &mut MacroExprHelper,
    target: Option<IdedExpr>,
    mut args: Vec<IdedExpr>,
) -> Result<IdedExpr, ParseError> {
    if target.is_some() {
        unreachable!("Got a target when expecting `None`!")
    }
    if args.len() != 1 {
        unreachable!("Expected a single arg!")
    }

    let ided_expr = args.remove(0);
    match ided_expr.expr {
        Expr::Select(mut select) => {
            select.test = true;
            Ok(helper.next_expr(Expr::Select(select)))
        }
        _ => Err(ParseError {
            source: None,
            pos: helper.pos_for(ided_expr.id).unwrap_or_default(),
            msg: "invalid argument to has() macro".to_string(),
            expr_id: 0,
            source_info: None,
        }),
    }
}

fn exists_macro_expander(
    helper: &mut MacroExprHelper,
    target: Option<IdedExpr>,
    mut args: Vec<IdedExpr>,
) -> Result<IdedExpr, ParseError> {
    if target.is_none() {
        unreachable!("Expected a target, but got `None`!")
    }
    if args.len() != 2 {
        unreachable!("Expected two args!")
    }

    let mut arguments = vec![args.remove(1)];
    let v = extract_ident(args.remove(0), helper)?;

    let init = helper.next_expr(Expr::Literal(LiteralValue::Boolean(false.into())));
    let result_binding = "@result".to_string();
    let accu_ident = helper.next_expr(Expr::Ident(result_binding.clone()));
    let arg = helper.next_expr(Expr::Call(CallExpr {
        func_name: operators::LOGICAL_NOT.to_string(),
        target: None,
        args: vec![accu_ident],
    }));
    let condition = helper.next_expr(Expr::Call(CallExpr {
        func_name: operators::NOT_STRICTLY_FALSE.to_string(),
        target: None,
        args: vec![arg],
    }));

    arguments.insert(0, helper.next_expr(Expr::Ident(result_binding.clone())));
    let step = helper.next_expr(Expr::Call(CallExpr {
        func_name: operators::LOGICAL_OR.to_string(),
        target: None,
        args: arguments,
    }));

    let result = helper.next_expr(Expr::Ident(result_binding.clone()));

    Ok(
        helper.next_expr(Expr::Comprehension(Box::new(ComprehensionExpr {
            iter_range: target.unwrap(),
            iter_var: v,
            iter_var2: None,
            accu_var: result_binding,
            accu_init: init,
            loop_cond: condition,
            loop_step: step,
            result,
        }))),
    )
}
fn all_macro_expander(
    helper: &mut MacroExprHelper,
    target: Option<IdedExpr>,
    mut args: Vec<IdedExpr>,
) -> Result<IdedExpr, ParseError> {
    if target.is_none() {
        unreachable!("Expected a target, but got `None`!")
    }
    if args.len() != 2 {
        unreachable!("Expected two args!")
    }

    let mut arguments = vec![args.remove(1)];
    let v = extract_ident(args.remove(0), helper)?;

    let init = helper.next_expr(Expr::Literal(LiteralValue::Boolean(true.into())));
    let result_binding = "@result".to_string();
    let accu_ident = helper.next_expr(Expr::Ident(result_binding.clone()));
    let condition = helper.next_expr(Expr::Call(CallExpr {
        func_name: operators::NOT_STRICTLY_FALSE.to_string(),
        target: None,
        args: vec![accu_ident],
    }));

    arguments.insert(0, helper.next_expr(Expr::Ident(result_binding.clone())));
    let step = helper.next_expr(Expr::Call(CallExpr {
        func_name: operators::LOGICAL_AND.to_string(),
        target: None,
        args: arguments,
    }));

    let result = helper.next_expr(Expr::Ident(result_binding.clone()));

    Ok(
        helper.next_expr(Expr::Comprehension(Box::new(ComprehensionExpr {
            iter_range: target.unwrap(),
            iter_var: v,
            iter_var2: None,
            accu_var: result_binding,
            accu_init: init,
            loop_cond: condition,
            loop_step: step,
            result,
        }))),
    )
}

fn exists_one_macro_expander(
    helper: &mut MacroExprHelper,
    target: Option<IdedExpr>,
    mut args: Vec<IdedExpr>,
) -> Result<IdedExpr, ParseError> {
    if target.is_none() {
        unreachable!("Expected a target, but got `None`!")
    }
    if args.len() != 2 {
        unreachable!("Expected two args!")
    }

    let mut arguments = vec![args.remove(1)];
    let v = extract_ident(args.remove(0), helper)?;

    let init = helper.next_expr(Expr::Literal(LiteralValue::Int(0.into())));
    let result_binding = "@result".to_string();
    let condition = helper.next_expr(Expr::Literal(LiteralValue::Boolean(true.into())));

    let args = vec![
        helper.next_expr(Expr::Ident(result_binding.clone())),
        helper.next_expr(Expr::Literal(LiteralValue::Int(1.into()))),
    ];
    arguments.push(helper.next_expr(Expr::Call(CallExpr {
        func_name: operators::ADD.to_string(),
        target: None,
        args,
    })));
    arguments.push(helper.next_expr(Expr::Ident(result_binding.clone())));

    let step = helper.next_expr(Expr::Call(CallExpr {
        func_name: operators::CONDITIONAL.to_string(),
        target: None,
        args: arguments,
    }));

    let accu = helper.next_expr(Expr::Ident(result_binding.clone()));
    let one = helper.next_expr(Expr::Literal(LiteralValue::Int(1.into())));
    let result = helper.next_expr(Expr::Call(CallExpr {
        func_name: operators::EQUALS.to_string(),
        target: None,
        args: vec![accu, one],
    }));

    Ok(
        helper.next_expr(Expr::Comprehension(Box::new(ComprehensionExpr {
            iter_range: target.unwrap(),
            iter_var: v,
            iter_var2: None,
            accu_var: result_binding,
            accu_init: init,
            loop_cond: condition,
            loop_step: step,
            result,
        }))),
    )
}

fn map_macro_expander(
    helper: &mut MacroExprHelper,
    target: Option<IdedExpr>,
    mut args: Vec<IdedExpr>,
) -> Result<IdedExpr, ParseError> {
    if target.is_none() {
        unreachable!("Expected a target, but got `None`!")
    }
    if args.len() != 2 && args.len() != 3 {
        unreachable!("Expected two or three args!")
    }

    let func = args.pop().unwrap();
    let v = extract_ident(args.remove(0), helper)?;

    let init = helper.next_expr(Expr::List(ListExpr::new(Vec::default())));
    let result_binding = "@result".to_string();
    let condition = helper.next_expr(Expr::Literal(LiteralValue::Boolean(true.into())));

    let filter = args.pop();

    let args = vec![
        helper.next_expr(Expr::Ident(result_binding.clone())),
        helper.next_expr(Expr::List(ListExpr::new(vec![func]))),
    ];
    let step = helper.next_expr(Expr::Call(CallExpr {
        func_name: operators::ADD.to_string(),
        target: None,
        args,
    }));

    let step = match filter {
        Some(filter) => {
            let accu = helper.next_expr(Expr::Ident(result_binding.clone()));
            helper.next_expr(Expr::Call(CallExpr {
                func_name: operators::CONDITIONAL.to_string(),
                target: None,
                args: vec![filter, step, accu],
            }))
        }
        None => step,
    };

    let result = helper.next_expr(Expr::Ident(result_binding.clone()));

    Ok(
        helper.next_expr(Expr::Comprehension(Box::new(ComprehensionExpr {
            iter_range: target.unwrap(),
            iter_var: v,
            iter_var2: None,
            accu_var: result_binding,
            accu_init: init,
            loop_cond: condition,
            loop_step: step,
            result,
        }))),
    )
}

fn filter_macro_expander(
    helper: &mut MacroExprHelper,
    target: Option<IdedExpr>,
    mut args: Vec<IdedExpr>,
) -> Result<IdedExpr, ParseError> {
    if target.is_none() {
        unreachable!("Expected a target, but got `None`!")
    }
    if args.len() != 2 {
        unreachable!("Expected two args!")
    }

    let var = args.remove(0);
    let v = extract_ident(var.clone(), helper)?;
    let filter = args.pop().unwrap();

    let init = helper.next_expr(Expr::List(ListExpr::new(Vec::default())));
    let result_binding = "@result".to_string();
    let condition = helper.next_expr(Expr::Literal(LiteralValue::Boolean(true.into())));

    let args = vec![
        helper.next_expr(Expr::Ident(result_binding.clone())),
        helper.next_expr(Expr::List(ListExpr::new(vec![var]))),
    ];
    let step = helper.next_expr(Expr::Call(CallExpr {
        func_name: operators::ADD.to_string(),
        target: None,
        args,
    }));

    let accu = helper.next_expr(Expr::Ident(result_binding.clone()));
    let step = helper.next_expr(Expr::Call(CallExpr {
        func_name: operators::CONDITIONAL.to_string(),
        target: None,
        args: vec![filter, step, accu],
    }));

    let result = helper.next_expr(Expr::Ident(result_binding.clone()));

    Ok(
        helper.next_expr(Expr::Comprehension(Box::new(ComprehensionExpr {
            iter_range: target.unwrap(),
            iter_var: v,
            iter_var2: None,
            accu_var: result_binding,
            accu_init: init,
            loop_cond: condition,
            loop_step: step,
            result,
        }))),
    )
}

fn opt_map_macro_expander(
    helper: &mut MacroExprHelper,
    target: Option<IdedExpr>,
    args: Vec<IdedExpr>,
) -> Result<IdedExpr, ParseError> {
    opt_macro_expander(helper, target, args, true)
}

fn opt_flat_map_macro_expander(
    helper: &mut MacroExprHelper,
    target: Option<IdedExpr>,
    args: Vec<IdedExpr>,
) -> Result<IdedExpr, ParseError> {
    opt_macro_expander(helper, target, args, false)
}

/// `t.hasValue() ? [optional.of(]<expr>[)] with var = t.value() : optional.none()`,
/// binding a non-identifier target once, as cel-go's optMap/optFlatMap do.
fn opt_macro_expander(
    helper: &mut MacroExprHelper,
    target: Option<IdedExpr>,
    mut args: Vec<IdedExpr>,
    wrap: bool,
) -> Result<IdedExpr, ParseError> {
    let target = target.expect("Expected a target, but got `None`!");
    let map_expr = args.pop().expect("Expected two args!");
    let var = extract_ident(args.pop().expect("Expected two args!"), helper)?;

    let unused = "#unused".to_string();
    let target_var = match &target.expr {
        Expr::Ident(name) => name.clone(),
        _ => "@target".to_string(),
    };

    let ident = |helper: &mut MacroExprHelper| helper.next_expr(Expr::Ident(target_var.clone()));
    let receiver = ident(helper);
    let has_value = helper.next_expr(Expr::Call(CallExpr {
        func_name: "hasValue".to_string(),
        target: Some(Box::new(receiver)),
        args: vec![],
    }));
    let receiver = ident(helper);
    let value = helper.next_expr(Expr::Call(CallExpr {
        func_name: "value".to_string(),
        target: Some(Box::new(receiver)),
        args: vec![],
    }));

    let comprehension = |helper: &mut MacroExprHelper, var: &str, init, result| {
        let iter_range = helper.next_expr(Expr::List(ListExpr::new(Vec::default())));
        let loop_cond = helper.next_expr(Expr::Literal(LiteralValue::Boolean(false.into())));
        let loop_step = helper.next_expr(Expr::Ident(var.to_string()));
        helper.next_expr(Expr::Comprehension(Box::new(ComprehensionExpr {
            iter_range,
            iter_var: unused.clone(),
            iter_var2: None,
            accu_var: var.to_string(),
            accu_init: init,
            loop_cond,
            loop_step,
            result,
        })))
    };

    let mut mapped = comprehension(helper, &var, value, map_expr);
    if wrap {
        let optional = helper.next_expr(Expr::Ident("optional".to_string()));
        mapped = helper.next_expr(Expr::Call(CallExpr {
            func_name: "of".to_string(),
            target: Some(Box::new(optional)),
            args: vec![mapped],
        }));
    }
    let optional = helper.next_expr(Expr::Ident("optional".to_string()));
    let none = helper.next_expr(Expr::Call(CallExpr {
        func_name: "none".to_string(),
        target: Some(Box::new(optional)),
        args: vec![],
    }));
    let result = helper.next_expr(Expr::Call(CallExpr {
        func_name: operators::CONDITIONAL.to_string(),
        target: None,
        args: vec![has_value, mapped, none],
    }));

    if matches!(target.expr, Expr::Ident(_)) {
        Ok(result)
    } else {
        Ok(comprehension(helper, &target_var, target, result))
    }
}

fn extract_ident(expr: IdedExpr, helper: &mut MacroExprHelper) -> Result<String, ParseError> {
    match expr.expr {
        Expr::Ident(ident) => Ok(ident),
        _ => Err(ParseError {
            source: None,
            pos: helper.pos_for(expr.id).unwrap_or_default(),
            msg: "argument must be a simple name".to_string(),
            expr_id: 0,
            source_info: None,
        }),
    }
}
