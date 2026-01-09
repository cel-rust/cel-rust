use crate::common::ast::{
    operators, BindExpr, CallExpr, ComprehensionExpr, Expr, IdedExpr, ListExpr,
};
use crate::common::value::CelVal::{Boolean, Int};
use crate::parser::{MacroExprHelper, ParseError};

pub type MacroExpander = fn(
    helper: &mut MacroExprHelper,
    target: Option<IdedExpr>,
    args: Vec<IdedExpr>,
) -> Result<IdedExpr, ParseError>;

pub fn find_expander(
    func_name: &str,
    target: Option<&IdedExpr>,
    args: &[IdedExpr],
) -> Option<MacroExpander> {
    match func_name {
        operators::HAS if args.len() == 1 && target.is_none() => Some(has_macro_expander),
        operators::EXISTS if (args.len() == 2 || args.len() == 3) && target.is_some() => {
            Some(exists_macro_expander)
        }
        operators::ALL if (args.len() == 2 || args.len() == 3) && target.is_some() => {
            Some(all_macro_expander)
        }
        operators::EXISTS_ONE | "existsOne"
            if (args.len() == 2 || args.len() == 3) && target.is_some() =>
        {
            Some(exists_one_macro_expander)
        }
        operators::MAP if (args.len() == 2 || args.len() == 3) && target.is_some() => {
            Some(map_macro_expander)
        }
        operators::FILTER if args.len() == 2 && target.is_some() => Some(filter_macro_expander),
        "transformList" if (args.len() == 3 || args.len() == 4) && target.is_some() => {
            Some(transform_list_macro_expander)
        }
        "transformMap" if (args.len() == 3 || args.len() == 4) && target.is_some() => {
            Some(transform_map_macro_expander)
        }
        "cel.bind" if args.len() == 3 && target.is_none() => Some(bind_macro_expander),
        "block" if args.len() == 2 && target.is_some() => Some(block_macro_expander),
        "index" if args.len() == 1 && target.is_some() => Some(index_macro_expander),
        "iterVar" if args.len() == 2 && target.is_some() => Some(iter_var_macro_expander),
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
    if args.len() != 2 && args.len() != 3 {
        unreachable!("Expected two or three args!")
    }

    let (iter_var, iter_var2, condition) = if args.len() == 3 {
        // 3-parameter form: exists(index_var, value_var, condition)
        let condition = args.remove(2);
        let value_var = extract_ident(args.remove(1), helper)?;
        let index_var = extract_ident(args.remove(0), helper)?;
        (index_var, Some(value_var), condition)
    } else {
        // 2-parameter form: exists(var, condition)
        let condition = args.remove(1);
        let var = extract_ident(args.remove(0), helper)?;
        (var, None, condition)
    };

    let mut arguments = vec![condition];

    let init = helper.next_expr(Expr::Literal(Boolean(false)));
    let result_binding = "@result".to_string();
    let accu_ident = helper.next_expr(Expr::Ident(result_binding.clone()));
    let arg = helper.next_expr(Expr::Call(CallExpr {
        func_name: operators::LOGICAL_NOT.to_string(),
        target: None,
        args: vec![accu_ident],
    }));
    let loop_condition = helper.next_expr(Expr::Call(CallExpr {
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
            iter_var,
            iter_var2,
            accu_var: result_binding,
            accu_init: init,
            loop_cond: loop_condition,
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
    if args.len() != 2 && args.len() != 3 {
        unreachable!("Expected two or three args!")
    }

    let (iter_var, iter_var2, condition) = if args.len() == 3 {
        // 3-parameter form: all(index_var, value_var, condition)
        let condition = args.remove(2);
        let value_var = extract_ident(args.remove(1), helper)?;
        let index_var = extract_ident(args.remove(0), helper)?;
        (index_var, Some(value_var), condition)
    } else {
        // 2-parameter form: all(var, condition)
        let condition = args.remove(1);
        let var = extract_ident(args.remove(0), helper)?;
        (var, None, condition)
    };

    let mut arguments = vec![condition];

    let init = helper.next_expr(Expr::Literal(Boolean(true)));
    let result_binding = "@result".to_string();
    let accu_ident = helper.next_expr(Expr::Ident(result_binding.clone()));
    let loop_condition = helper.next_expr(Expr::Call(CallExpr {
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
            iter_var,
            iter_var2,
            accu_var: result_binding,
            accu_init: init,
            loop_cond: loop_condition,
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
    if args.len() != 2 && args.len() != 3 {
        unreachable!("Expected two or three args!")
    }

    let (iter_var, iter_var2, condition) = if args.len() == 3 {
        // 3-parameter form: existsOne(index_var, value_var, condition)
        let condition = args.remove(2);
        let value_var = extract_ident(args.remove(1), helper)?;
        let index_var = extract_ident(args.remove(0), helper)?;
        (index_var, Some(value_var), condition)
    } else {
        // 2-parameter form: existsOne(var, condition)
        let condition = args.remove(1);
        let var = extract_ident(args.remove(0), helper)?;
        (var, None, condition)
    };

    let mut arguments = vec![condition];

    let init = helper.next_expr(Expr::Literal(Int(0)));
    let result_binding = "@result".to_string();
    let condition = helper.next_expr(Expr::Literal(Boolean(true)));

    let args = vec![
        helper.next_expr(Expr::Ident(result_binding.clone())),
        helper.next_expr(Expr::Literal(Int(1))),
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
    let one = helper.next_expr(Expr::Literal(Int(1)));
    let result = helper.next_expr(Expr::Call(CallExpr {
        func_name: operators::EQUALS.to_string(),
        target: None,
        args: vec![accu, one],
    }));

    Ok(
        helper.next_expr(Expr::Comprehension(Box::new(ComprehensionExpr {
            iter_range: target.unwrap(),
            iter_var,
            iter_var2,
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
    let condition = helper.next_expr(Expr::Literal(Boolean(true)));

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
    let condition = helper.next_expr(Expr::Literal(Boolean(true)));

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

fn bind_macro_expander(
    helper: &mut MacroExprHelper,
    target: Option<IdedExpr>,
    mut args: Vec<IdedExpr>,
) -> Result<IdedExpr, ParseError> {
    if target.is_some() {
        unreachable!("cel.bind should not have a target")
    }
    if args.len() != 3 {
        unreachable!("cel.bind should have exactly 3 arguments")
    }

    let var_name_expr = args.remove(0);
    let value_expr = args.remove(0);
    let body_expr = args.remove(0);

    // Extract the variable name from the first argument
    let var_name = extract_ident(var_name_expr, helper)?;

    // Create a comprehension that iterates over a single-element list containing the value
    // The body_expr is evaluated with var_name bound to that value
    // This is: [value_expr].map(var_name, body_expr)[0]
    let list_expr = helper.next_expr(Expr::List(ListExpr {
        elements: vec![value_expr],
        optional_indices: vec![],
    }));

    let result_var = "__result__".to_string();
    let comprehension = ComprehensionExpr {
        iter_var: var_name,
        iter_var2: None,
        iter_range: list_expr,
        accu_var: result_var.clone(),
        // Dummy init value - will be overwritten in the first (and only) iteration
        accu_init: helper.next_expr(Expr::Literal(Boolean(false))),
        // Always execute the loop body once
        loop_cond: helper.next_expr(Expr::Literal(Boolean(true))),
        // Evaluate body_expr with var_name in scope, store result in accumulator
        loop_step: body_expr,
        // Return the accumulator value
        result: helper.next_expr(Expr::Ident(result_var)),
    };

    Ok(helper.next_expr(Expr::Comprehension(Box::new(comprehension))))
}

fn transform_list_macro_expander(
    helper: &mut MacroExprHelper,
    target: Option<IdedExpr>,
    mut args: Vec<IdedExpr>,
) -> Result<IdedExpr, ParseError> {
    if target.is_none() {
        unreachable!("Expected a target, but got `None`!")
    }
    if args.len() != 3 && args.len() != 4 {
        unreachable!("Expected three or four args!")
    }

    // Extract variables and expressions
    let (index_var, value_var, filter, transform) = if args.len() == 4 {
        // 4-arg form: transformList(index_var, value_var, filter, transform)
        let transform = args.remove(3);
        let filter = Some(args.remove(2));
        let value_var = extract_ident(args.remove(1), helper)?;
        let index_var = extract_ident(args.remove(0), helper)?;
        (index_var, value_var, filter, transform)
    } else {
        // 3-arg form: transformList(index_var, value_var, transform)
        let transform = args.remove(2);
        let value_var = extract_ident(args.remove(1), helper)?;
        let index_var = extract_ident(args.remove(0), helper)?;
        (index_var, value_var, None, transform)
    };

    // Build comprehension: accumulator starts as empty list, appends transformed elements
    let init = helper.next_expr(Expr::List(ListExpr::new(Vec::default())));
    let result_binding = "@result".to_string();
    let condition = helper.next_expr(Expr::Literal(Boolean(true)));

    // Step: append transformed element to result list
    let add_args = vec![
        helper.next_expr(Expr::Ident(result_binding.clone())),
        helper.next_expr(Expr::List(ListExpr::new(vec![transform]))),
    ];
    let step = helper.next_expr(Expr::Call(CallExpr {
        func_name: operators::ADD.to_string(),
        target: None,
        args: add_args,
    }));

    // If there's a filter, wrap step in conditional
    let step = match filter {
        Some(filter_expr) => {
            let accu = helper.next_expr(Expr::Ident(result_binding.clone()));
            helper.next_expr(Expr::Call(CallExpr {
                func_name: operators::CONDITIONAL.to_string(),
                target: None,
                args: vec![filter_expr, step, accu],
            }))
        }
        None => step,
    };

    let result = helper.next_expr(Expr::Ident(result_binding.clone()));

    Ok(
        helper.next_expr(Expr::Comprehension(Box::new(ComprehensionExpr {
            iter_range: target.unwrap(),
            iter_var: index_var,
            iter_var2: Some(value_var),
            accu_var: result_binding,
            accu_init: init,
            loop_cond: condition,
            loop_step: step,
            result,
        }))),
    )
}

fn transform_map_macro_expander(
    helper: &mut MacroExprHelper,
    target: Option<IdedExpr>,
    mut args: Vec<IdedExpr>,
) -> Result<IdedExpr, ParseError> {
    if target.is_none() {
        unreachable!("Expected a target, but got `None`!")
    }
    if args.len() != 3 && args.len() != 4 {
        unreachable!("Expected three or four args!")
    }

    // Extract variables and expressions
    let (key_var, value_var, filter, transform) = if args.len() == 4 {
        // 4-arg form: transformMap(key_var, value_var, filter, transform)
        let transform = args.remove(3);
        let filter = Some(args.remove(2));
        let value_var = extract_ident(args.remove(1), helper)?;
        let key_var = extract_ident(args.remove(0), helper)?;
        (key_var, value_var, filter, transform)
    } else {
        // 3-arg form: transformMap(key_var, value_var, transform)
        let transform = args.remove(2);
        let value_var = extract_ident(args.remove(1), helper)?;
        let key_var = extract_ident(args.remove(0), helper)?;
        (key_var, value_var, None, transform)
    };

    use crate::common::ast::{EntryExpr, IdedEntryExpr, MapEntryExpr, MapExpr};

    // Build comprehension: accumulator starts as empty map
    let init = helper.next_expr(Expr::Map(MapExpr {
        entries: Vec::default(),
    }));
    let result_binding = "@result".to_string();
    let condition = helper.next_expr(Expr::Literal(Boolean(true)));

    // Step: add transformed entry to result map
    // The transform should produce a value, and we use key_var as the key
    let key_ided_expr = helper.next_expr(Expr::Ident(key_var.clone()));
    let new_entry = IdedEntryExpr {
        id: key_ided_expr.id,
        expr: EntryExpr::MapEntry(MapEntryExpr {
            key: key_ided_expr,
            value: transform,
            optional: false,
        }),
    };
    let single_entry_map = helper.next_expr(Expr::Map(MapExpr {
        entries: vec![new_entry],
    }));

    let add_args = vec![
        helper.next_expr(Expr::Ident(result_binding.clone())),
        single_entry_map,
    ];
    let step = helper.next_expr(Expr::Call(CallExpr {
        func_name: operators::ADD.to_string(),
        target: None,
        args: add_args,
    }));

    // If there's a filter, wrap step in conditional
    let step = match filter {
        Some(filter_expr) => {
            let accu = helper.next_expr(Expr::Ident(result_binding.clone()));
            helper.next_expr(Expr::Call(CallExpr {
                func_name: operators::CONDITIONAL.to_string(),
                target: None,
                args: vec![filter_expr, step, accu],
            }))
        }
        None => step,
    };

    let result = helper.next_expr(Expr::Ident(result_binding.clone()));

    Ok(
        helper.next_expr(Expr::Comprehension(Box::new(ComprehensionExpr {
            iter_range: target.unwrap(),
            iter_var: key_var,
            iter_var2: Some(value_var),
            accu_var: result_binding,
            accu_init: init,
            loop_cond: condition,
            loop_step: step,
            result,
        }))),
    )
}

fn block_macro_expander(
    helper: &mut MacroExprHelper,
    target: Option<IdedExpr>,
    mut args: Vec<IdedExpr>,
) -> Result<IdedExpr, ParseError> {
    if target.is_none() {
        unreachable!("Expected a target (cel), but got `None`!")
    }
    if args.len() != 2 {
        unreachable!("Expected two args!")
    }

    // Validate that target is the identifier "cel"
    let target = target.unwrap();
    match &target.expr {
        Expr::Ident(name) if name == "cel" => {
            // Valid cel.block(...) call
        }
        _ => {
            return Err(ParseError {
                source: None,
                pos: helper.pos_for(target.id).unwrap_or_default(),
                msg: "block macro must be called as cel.block(...)".to_string(),
                expr_id: 0,
                source_info: None,
            });
        }
    }

    // Extract the bindings list and result expression
    let bindings_expr = args.remove(0);
    let result_expr = args.remove(0);

    // The first argument must be a list
    let bindings = match bindings_expr.expr {
        Expr::List(list) => list.elements,
        _ => {
            return Err(ParseError {
                source: None,
                pos: helper.pos_for(bindings_expr.id).unwrap_or_default(),
                msg: "first argument to cel.block must be a list".to_string(),
                expr_id: 0,
                source_info: None,
            });
        }
    };

    // Build nested bindings from right to left (starting from the result)
    // cel.block([a, b, c], result) becomes:
    // cel.bind(@index0, a, cel.bind(@index1, b, cel.bind(@index2, c, result)))
    let mut current_expr = result_expr;
    for (i, binding) in bindings.into_iter().enumerate().rev() {
        let var_name = format!("@index{}", i);
        current_expr = helper.next_expr(Expr::Bind(Box::new(BindExpr {
            var: var_name,
            init: binding,
            result: current_expr,
        })));
    }

    Ok(current_expr)
}

fn index_macro_expander(
    helper: &mut MacroExprHelper,
    target: Option<IdedExpr>,
    mut args: Vec<IdedExpr>,
) -> Result<IdedExpr, ParseError> {
    if target.is_none() {
        unreachable!("Expected a target (cel), but got `None`!")
    }
    if args.len() != 1 {
        unreachable!("Expected one arg!")
    }

    // Validate that target is the identifier "cel"
    let target = target.unwrap();
    match &target.expr {
        Expr::Ident(name) if name == "cel" => {
            // Valid cel.index(...) call
        }
        _ => {
            return Err(ParseError {
                source: None,
                pos: helper.pos_for(target.id).unwrap_or_default(),
                msg: "index macro must be called as cel.index(...)".to_string(),
                expr_id: 0,
                source_info: None,
            });
        }
    }

    // Extract the index argument
    let index_expr = args.remove(0);

    // The argument must be an integer literal
    let index = match index_expr.expr {
        Expr::Literal(Int(n)) => n,
        _ => {
            return Err(ParseError {
                source: None,
                pos: helper.pos_for(index_expr.id).unwrap_or_default(),
                msg: "argument to cel.index must be an integer literal".to_string(),
                expr_id: 0,
                source_info: None,
            });
        }
    };

    // Rewrite cel.index(N) to @indexN
    let ident = format!("@index{}", index);
    Ok(helper.next_expr(Expr::Ident(ident)))
}

fn iter_var_macro_expander(
    helper: &mut MacroExprHelper,
    target: Option<IdedExpr>,
    mut args: Vec<IdedExpr>,
) -> Result<IdedExpr, ParseError> {
    if target.is_none() {
        unreachable!("Expected a target (cel), but got `None`!")
    }
    if args.len() != 2 {
        unreachable!("Expected two args!")
    }

    // Validate that target is the identifier "cel"
    let target = target.unwrap();
    match &target.expr {
        Expr::Ident(name) if name == "cel" => {
            // Valid cel.iterVar(...) call
        }
        _ => {
            return Err(ParseError {
                source: None,
                pos: helper.pos_for(target.id).unwrap_or_default(),
                msg: "iterVar macro must be called as cel.iterVar(...)".to_string(),
                expr_id: 0,
                source_info: None,
            });
        }
    }

    // Extract the two arguments
    let slot_expr = args.remove(0);
    let index_expr = args.remove(0);

    // Both arguments must be integer literals
    let slot = match slot_expr.expr {
        Expr::Literal(Int(n)) => n,
        _ => {
            return Err(ParseError {
                source: None,
                pos: helper.pos_for(slot_expr.id).unwrap_or_default(),
                msg: "first argument to cel.iterVar must be an integer literal".to_string(),
                expr_id: 0,
                source_info: None,
            });
        }
    };

    let index = match index_expr.expr {
        Expr::Literal(Int(n)) => n,
        _ => {
            return Err(ParseError {
                source: None,
                pos: helper.pos_for(index_expr.id).unwrap_or_default(),
                msg: "second argument to cel.iterVar must be an integer literal".to_string(),
                expr_id: 0,
                source_info: None,
            });
        }
    };

    // Rewrite cel.iterVar(N, M) to @it:N:M
    let ident = format!("@it:{}:{}", slot, index);
    Ok(helper.next_expr(Expr::Ident(ident)))
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
