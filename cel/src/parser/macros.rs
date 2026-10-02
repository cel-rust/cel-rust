use crate::common::ast::{
    operators, CallExpr, ComprehensionExpr, Expr, IdedExpr, ListExpr, LiteralValue,
};
use crate::parser::{MacroExprHelper, ParseError};
use std::collections::BTreeMap;
use std::fmt;
use std::sync::{Arc, LazyLock};

/// Rewrites a matched call, given its target (for a receiver call) and its
/// arguments, into the expression that replaces it.
type Expander = dyn Fn(&mut MacroExprHelper<'_>, Option<IdedExpr>, Vec<IdedExpr>) -> Result<IdedExpr, ParseError>
    + Send
    + Sync;

/// A parse-time rewrite of a call into another expression.
///
/// A macro matches a call on the function's name, on whether it is called on a
/// target (`x.f(..)`) or globally (`f(..)`), and on its argument count.
#[derive(Clone)]
pub(crate) struct Macro {
    function: String,
    receiver_style: bool,
    arg_count: usize,
    expander: Arc<Expander>,
}

impl Macro {
    /// A macro for the global call `function(..)` with `arg_count` arguments.
    pub(crate) fn global(
        function: impl Into<String>,
        arg_count: usize,
        expander: impl Fn(
                &mut MacroExprHelper<'_>,
                Option<IdedExpr>,
                Vec<IdedExpr>,
            ) -> Result<IdedExpr, ParseError>
            + Send
            + Sync
            + 'static,
    ) -> Self {
        Self {
            function: function.into(),
            receiver_style: false,
            arg_count,
            expander: Arc::new(expander),
        }
    }

    /// A macro for the receiver call `target.function(..)` with `arg_count`
    /// arguments, the target not counted.
    pub(crate) fn receiver(
        function: impl Into<String>,
        arg_count: usize,
        expander: impl Fn(
                &mut MacroExprHelper<'_>,
                Option<IdedExpr>,
                Vec<IdedExpr>,
            ) -> Result<IdedExpr, ParseError>
            + Send
            + Sync
            + 'static,
    ) -> Self {
        Self {
            function: function.into(),
            receiver_style: true,
            arg_count,
            expander: Arc::new(expander),
        }
    }

    pub(crate) fn expand(
        &self,
        helper: &mut MacroExprHelper<'_>,
        target: Option<IdedExpr>,
        args: Vec<IdedExpr>,
    ) -> Result<IdedExpr, ParseError> {
        (self.expander)(helper, target, args)
    }
}

impl fmt::Debug for Macro {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Macro")
            .field("function", &self.function)
            .field("receiver_style", &self.receiver_style)
            .field("arg_count", &self.arg_count)
            .finish_non_exhaustive()
    }
}

/// The macros a parser expands.
#[derive(Clone, Debug, Default)]
pub(crate) struct Macros {
    by_function: BTreeMap<String, Vec<Macro>>,
}

/// The macros of the CEL standard library, built once and shared by every
/// parser and [`Env`](crate::Env) that expands them.
static STANDARD: LazyLock<Arc<Macros>> = LazyLock::new(|| {
    Arc::new(
        [
            Macro::global(operators::HAS, 1, has_macro_expander),
            Macro::receiver(operators::EXISTS, 2, exists_macro_expander),
            Macro::receiver(operators::ALL, 2, all_macro_expander),
            Macro::receiver(operators::EXISTS_ONE, 2, exists_one_macro_expander),
            Macro::receiver("existsOne", 2, exists_one_macro_expander),
            Macro::receiver(operators::MAP, 2, map_macro_expander),
            Macro::receiver(operators::MAP, 3, map_macro_expander),
            Macro::receiver(operators::FILTER, 2, filter_macro_expander),
        ]
        .into_iter()
        .collect(),
    )
});

impl Macros {
    /// The macros of the CEL standard library: `has`, `all`, `exists`,
    /// `exists_one` (and `existsOne`), `map` and `filter`.
    pub(crate) fn standard() -> Arc<Macros> {
        Arc::clone(&STANDARD)
    }

    /// The macro matching the call `function(args)`, or `target.function(args)`
    /// when there is a target.
    pub(crate) fn find(
        &self,
        function: &str,
        target: Option<&IdedExpr>,
        args: &[IdedExpr],
    ) -> Option<&Macro> {
        self.by_function
            .get(function)?
            .iter()
            .find(|m| m.receiver_style == target.is_some() && m.arg_count == args.len())
    }
}

impl FromIterator<Macro> for Macros {
    fn from_iter<I: IntoIterator<Item = Macro>>(macros: I) -> Self {
        let mut by_function = BTreeMap::<String, Vec<Macro>>::new();
        for m in macros {
            by_function.entry(m.function.clone()).or_default().push(m);
        }
        Self { by_function }
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
        _ => Err(helper.new_error(ided_expr.id, "invalid argument to has() macro")),
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

fn extract_ident(expr: IdedExpr, helper: &mut MacroExprHelper) -> Result<String, ParseError> {
    match expr.expr {
        Expr::Ident(ident) => Ok(ident),
        _ => Err(helper.new_error(expr.id, "argument must be a simple name")),
    }
}
