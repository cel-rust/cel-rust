use crate::common::ast::{
    operators, CallExpr, ComprehensionExpr, Expr, IdedExpr, ListExpr, LiteralValue,
};
use crate::parser::{MacroExprHelper, ParseError};
use crate::DeclarationError;
use std::collections::BTreeMap;
use std::fmt;
use std::sync::{Arc, LazyLock};

/// Rewrites a matched call, given its target (for a receiver call) and its
/// arguments, into the expression that replaces it.
type Expander = dyn Fn(
        &mut MacroExprHelper<'_>,
        &mut Option<IdedExpr>,
        &mut Vec<IdedExpr>,
    ) -> Result<Option<IdedExpr>, ParseError>
    + Send
    + Sync;

/// A parse-time rewrite of a call into another expression.
///
/// A macro matches a call on the function's name, on whether it is called on a
/// target (`x.f(..)`) or globally (`f(..)`), and on its argument count. The
/// parser then hands the call's target and arguments to the macro's expander,
/// and the `Some` expression it returns replaces the call in the parsed
/// expression:
///
/// - the target is `None` for a global call, and the `x` of `x.f(..)` for a
///   receiver call;
/// - the target and arguments are lent: the expander takes what it uses out
///   of them, e.g. with `args.pop()` or `target.take()`;
/// - new nodes are made with [`MacroExprHelper::next_expr`], so each gets its
///   own id and the call's place in the source;
/// - `Ok(None)` declines the call, which stays as written, as when no macro
///   matches it: e.g. for `cel.bind(..)`, a `bind` on any other target. The
///   expander must decline before taking anything from the target or the
///   arguments, or the parse fails;
/// - an `Err`, best made with [`MacroExprHelper::new_error`], fails the parse
///   with that error.
///
/// Macros are added to an [`Env`](crate::Env) with
/// [`Env::add_macro`](crate::Env::add_macro), and expanded by the parser it
/// builds with [`Env::parser`](crate::Env::parser).
#[derive(Clone)]
pub struct Macro {
    function: String,
    receiver_style: bool,
    arg_count: usize,
    expander: Arc<Expander>,
}

impl Macro {
    /// A macro for the global call `function(..)` with `arg_count` arguments.
    pub fn global(
        function: impl Into<String>,
        arg_count: usize,
        expander: impl Fn(
                &mut MacroExprHelper<'_>,
                &mut Option<IdedExpr>,
                &mut Vec<IdedExpr>,
            ) -> Result<Option<IdedExpr>, ParseError>
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
    pub fn receiver(
        function: impl Into<String>,
        arg_count: usize,
        expander: impl Fn(
                &mut MacroExprHelper<'_>,
                &mut Option<IdedExpr>,
                &mut Vec<IdedExpr>,
            ) -> Result<Option<IdedExpr>, ParseError>
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

    /// The expansion of the call whose `target` and `args` are lent, or `None`
    /// when the expander declines it, leaving them as they were.
    pub(crate) fn expand(
        &self,
        helper: &mut MacroExprHelper<'_>,
        target: &mut Option<IdedExpr>,
        args: &mut Vec<IdedExpr>,
    ) -> Result<Option<IdedExpr>, ParseError> {
        let lent = shape(target, args);
        let expansion = (self.expander)(helper, target, args)?;
        if expansion.is_none() && shape(target, args) != lent {
            return Err(helper.new_error(
                helper.id,
                format!(
                    "macro '{}' declined the call after taking from it",
                    self.function
                ),
            ));
        }
        Ok(expansion)
    }

    /// Whether this macro is for a call with `arg_count` arguments, on a
    /// target when `receiver_style`.
    fn matches(&self, receiver_style: bool, arg_count: usize) -> bool {
        self.receiver_style == receiver_style && self.arg_count == arg_count
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

/// What an expander can take out of a call's lent parts: the target, some
/// arguments, or the contents of either, which leaves a default node behind.
/// Default nodes are counted rather than ruled out, as the parser leaves one
/// for an argument it could not parse.
fn shape(target: &Option<IdedExpr>, args: &[IdedExpr]) -> (bool, usize, usize) {
    let defaults = target
        .iter()
        .chain(args)
        .filter(|e| e.id == 0 && matches!(e.expr, Expr::Unspecified))
        .count();
    (target.is_some(), args.len(), defaults)
}

/// A built-in expander, which never declines the call it matched.
type BuiltIn = fn(
    &mut MacroExprHelper<'_>,
    &mut Option<IdedExpr>,
    &mut Vec<IdedExpr>,
) -> Result<IdedExpr, ParseError>;

/// The expander of a built-in macro.
fn expanding(
    expander: BuiltIn,
) -> impl Fn(
    &mut MacroExprHelper<'_>,
    &mut Option<IdedExpr>,
    &mut Vec<IdedExpr>,
) -> Result<Option<IdedExpr>, ParseError> {
    move |helper, target, args| expander(helper, target, args).map(Some)
}

/// The macros of the CEL standard library, built once and shared by every
/// parser and [`Env`](crate::Env) that expands them.
static STANDARD: LazyLock<Arc<Macros>> = LazyLock::new(|| {
    Arc::new(
        [
            Macro::global(operators::HAS, 1, expanding(has_macro_expander)),
            Macro::receiver(operators::EXISTS, 2, expanding(exists_macro_expander)),
            Macro::receiver(operators::ALL, 2, expanding(all_macro_expander)),
            Macro::receiver(
                operators::EXISTS_ONE,
                2,
                expanding(exists_one_macro_expander),
            ),
            Macro::receiver("existsOne", 2, expanding(exists_one_macro_expander)),
            Macro::receiver(operators::MAP, 2, expanding(map_macro_expander)),
            Macro::receiver(operators::MAP, 3, expanding(map_macro_expander)),
            Macro::receiver(operators::FILTER, 2, expanding(filter_macro_expander)),
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
            .find(|m| m.matches(target.is_some(), args.len()))
    }

    /// Adds `m`, unless a macro for the same calls is already there.
    pub(crate) fn add(&mut self, m: Macro) -> Result<(), DeclarationError> {
        let same_function = self.by_function.entry(m.function.clone()).or_default();
        if same_function
            .iter()
            .any(|other| other.matches(m.receiver_style, m.arg_count))
        {
            return Err(DeclarationError::duplicate_macro(&m.function));
        }
        same_function.push(m);
        Ok(())
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
    target: &mut Option<IdedExpr>,
    args: &mut Vec<IdedExpr>,
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
    target: &mut Option<IdedExpr>,
    args: &mut Vec<IdedExpr>,
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
            iter_range: target.take().unwrap(),
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
    target: &mut Option<IdedExpr>,
    args: &mut Vec<IdedExpr>,
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
            iter_range: target.take().unwrap(),
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
    target: &mut Option<IdedExpr>,
    args: &mut Vec<IdedExpr>,
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
            iter_range: target.take().unwrap(),
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

/// Expands `target.map(v, f)`, or `target.map(v, p, f)`: also the key list of
/// the lists extension's `sortBy`.
pub(crate) fn map_macro_expander(
    helper: &mut MacroExprHelper,
    target: &mut Option<IdedExpr>,
    args: &mut Vec<IdedExpr>,
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
            iter_range: target.take().unwrap(),
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
    target: &mut Option<IdedExpr>,
    args: &mut Vec<IdedExpr>,
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
            iter_range: target.take().unwrap(),
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
