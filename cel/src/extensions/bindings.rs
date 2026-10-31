//! The bindings extension library: the `cel.bind` macro, following cel-go's
//! `ext.Bindings()`.

use crate::common::ast::{Expr, IdedExpr};
use crate::parser::{Macro, MacroExprHelper, ParseError};
use crate::{DeclarationError, Env, Library};
use std::mem;

/// The name of the bindings extension library.
pub(crate) const NAME: &str = "cel.lib.ext.cel.bindings";

/// The internal `cel.@block(slots, expr)` call the bindings extension
/// evaluates: `expr` with each of the list `slots` bound as `@index0`,
/// `@index1`, …, a slot evaluated on its first reference and kept for the rest
/// of the block. A slot may only reference the slots before it. Only an
/// optimized AST spells it, as `@` can't start an identifier in source.
pub(crate) const BLOCK: &str = "cel.@block";

/// Adds the bindings extension library to `env`: [`Bindings`], with its
/// default settings.
pub fn extension(env: &mut Env) -> Result<(), DeclarationError> {
    env.add_library(Bindings::default())
}

/// The bindings extension library, `cel.lib.ext.cel.bindings`: registers its
/// `cel.bind` macro on the [`Env`] it is added to.
///
/// `cel.bind(var, init, expr)` evaluates `expr` with `var` bound to `init`,
/// which is evaluated once. It is only expanded by [`Env::compile`] and
/// [`Env::parser`], not by [`Program::compile`](crate::Program::compile).
#[derive(Clone, Debug, Default)]
#[non_exhaustive]
pub struct Bindings {}

impl Library for Bindings {
    fn name(&self) -> Option<&str> {
        Some(NAME)
    }

    fn register(&self, env: &mut Env) -> Result<(), DeclarationError> {
        env.add_macro(Macro::receiver("bind", 3, bind))
    }
}

/// `cel.bind(var, init, expr)`: `var` bound to `init` in `expr`. Declines any
/// target but `cel`.
fn bind(
    helper: &mut MacroExprHelper<'_>,
    target: &mut Option<IdedExpr>,
    args: &mut Vec<IdedExpr>,
) -> Result<Option<IdedExpr>, ParseError> {
    let on_cel = |t: &IdedExpr| matches!(&t.expr, Expr::Ident(namespace) if namespace == "cel");
    if !target.as_ref().is_some_and(on_cel) {
        return Ok(None);
    }
    let [var, init, expr] =
        <[IdedExpr; 3]>::try_from(mem::take(args)).expect("the macro matched three arguments");
    let Expr::Ident(var) = var.expr else {
        return Err(helper.new_error(
            var.id,
            "cel.bind() variable names must be simple identifiers",
        ));
    };
    Ok(Some(helper.bind(&var, init, expr)))
}

#[cfg(test)]
mod tests {
    use crate::common::ast::{CallExpr, Expr, ListExpr, LiteralValue};
    use crate::extractors::Arguments;
    use crate::parser::Macro;
    use crate::{Context, Env, ExecutionError, FunctionContext, IdedExpr, Value};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    fn env() -> Env {
        let mut env = Env::stdlib();
        env.add_extension(crate::extensions::bindings)
            .expect("We can't test the extension, if we can't register it");
        env
    }

    fn eval(expr: &str) -> Value {
        let env = env();
        let program = env.compile(expr).expect("This must be valid CEL");
        let mut context = Context::with_env(Arc::new(env));
        context.add_variable_from_value("x", Value::Int(-1));
        program.execute(&context).expect("This must evaluate")
    }

    #[test]
    fn adding_it_twice_registers_it_once() {
        let mut env = env();
        assert_eq!(env.add_extension(crate::extensions::bindings), Ok(()));
        let program = env.compile("cel.bind(a, 1, a + 1)").unwrap();
        assert_eq!(
            program.execute(&Context::with_env(Arc::new(env))),
            Ok(Value::Int(2))
        );
    }

    #[test]
    fn bind_evaluates_the_expression_with_the_variable_bound() {
        assert_eq!(eval("cel.bind(a, 1, a + 1)"), Value::Int(2));
        assert_eq!(
            eval("cel.bind(a, [1, 2], a.map(e, e * 2))"),
            Value::List(Arc::new(vec![Value::Int(2), Value::Int(4)]))
        );
    }

    #[test]
    fn binds_nest() {
        assert_eq!(
            eval("cel.bind(a, 1, cel.bind(b, a + 1, a + b))"),
            Value::Int(3)
        );
        assert_eq!(eval("cel.bind(a, 1, cel.bind(a, a + 1, a))"), Value::Int(2));
    }

    #[test]
    fn a_bound_variable_shadows_one_of_the_context() {
        assert_eq!(eval("cel.bind(x, 0, x == 0)"), Value::Bool(true));
        assert_eq!(eval("cel.bind(a, 0, x)"), Value::Int(-1));
    }

    #[test]
    fn the_variable_must_be_a_simple_identifier() {
        for expr in ["cel.bind(a.b, 1, 2)", "cel.bind(1, 1, 2)"] {
            let errors = env().compile(expr).unwrap_err().errors;
            let messages: Vec<_> = errors.iter().map(|e| e.msg.as_str()).collect();
            assert_eq!(
                messages,
                ["cel.bind() variable names must be simple identifiers"],
                "{expr}"
            );
        }
    }

    /// Evaluates `expr` with the block macros, as the conformance test macros
    /// spell them.
    fn eval_block(env: Env, expr: &str) -> Result<Value, ExecutionError> {
        let env = with_block_macros(env);
        let program = env.compile(expr).expect("This must be valid CEL");
        let mut context = Context::with_env(Arc::new(env));
        context
            .add_function("count", |n: Arguments| {
                COUNT.set(COUNT.get() + 1);
                n.0[0].clone()
            })
            .unwrap();
        context
            .add_function("slot0", |ftx: &FunctionContext| {
                ftx.resolve(IdedExpr {
                    id: 0,
                    expr: Expr::Ident("@index0".to_owned()),
                })
            })
            .unwrap();
        context.add_variable_from_value("x", Value::Int(-1));
        program.execute(&context)
    }

    /// `env`, with `block(slots, expr)` spelling `cel.@block` and `index(n)`
    /// the slot `@indexn`.
    fn with_block_macros(mut env: Env) -> Env {
        env.add_macro(Macro::global("block", 2, |helper, _, args| {
            let args = std::mem::take(args);
            Ok(Some(helper.next_expr(Expr::Call(CallExpr {
                func_name: super::BLOCK.to_owned(),
                target: None,
                args,
            }))))
        }))
        .unwrap();
        env.add_macro(Macro::global("index", 1, |helper, _, args| {
            let Expr::Literal(LiteralValue::Int(n)) = &args[0].expr else {
                unreachable!()
            };
            let name = format!("@index{}", n.inner());
            Ok(Some(helper.next_expr(Expr::Ident(name))))
        }))
        .unwrap();
        env
    }

    thread_local! {
        /// The calls to `count` on this test's thread.
        static COUNT: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    }

    #[test]
    fn block_binds_its_slots() {
        assert_eq!(
            eval_block(env(), "block([1, index(0) + 1], index(1) * 10)"),
            Ok(Value::Int(20))
        );
        assert_eq!(
            eval_block(env(), "block([x], block([2], index(0)))"),
            Ok(Value::Int(2)),
            "an inner block's slots shadow the outer ones"
        );
    }

    #[test]
    fn a_slot_is_only_evaluated_when_referenced() {
        assert_eq!(
            eval_block(env(), "block([1 / 0, 2], index(1))"),
            Ok(Value::Int(2))
        );
        assert_eq!(
            eval_block(env(), "block([1 / 0], true || index(0))"),
            Ok(Value::Bool(true))
        );
        assert!(eval_block(env(), "block([1 / 0], index(0))").is_err());
    }

    #[test]
    fn a_slot_is_evaluated_once() {
        let before = COUNT.get();
        assert_eq!(
            eval_block(
                env(),
                "block([count(1)], [1, 2, 3].map(e, index(0) + e) == [2, 3, 4] && index(0) == 1)"
            ),
            Ok(Value::Bool(true))
        );
        assert_eq!(COUNT.get() - before, 1);
    }

    #[test]
    fn a_slot_s_error_is_evaluated_once() {
        let before = COUNT.get();
        assert_eq!(
            eval_block(
                env(),
                "block([count(1) / 0], (index(0) || true) && (index(0) || true))"
            ),
            Ok(Value::Bool(true))
        );
        assert_eq!(COUNT.get() - before, 1);
    }

    #[test]
    fn a_slot_is_not_copied_when_referenced() {
        use crate::common::types::{Type, DYN_TYPE};
        use crate::common::value::{CowVal, Val};
        use crate::context::VariableResolver;

        #[derive(Debug)]
        struct CountedVal(Arc<AtomicUsize>);

        impl Val for CountedVal {
            fn get_type(&self) -> &Type {
                &DYN_TYPE
            }

            fn cel_type() -> &'static Type {
                &DYN_TYPE
            }

            fn clone_as_boxed<'v>(&self) -> Box<dyn Val + 'v> {
                self.0.fetch_add(1, Ordering::SeqCst);
                Box::new(CountedVal(self.0.clone()))
            }
        }

        struct BorrowResolver(Box<dyn Val>);

        impl VariableResolver for BorrowResolver {
            fn resolve<'b>(&'b self, variable: &str) -> Option<CowVal<'b, 'b>> {
                (variable == "counted").then(|| CowVal::Borrowed(self.0.as_ref()))
            }
        }

        let clones = Arc::new(AtomicUsize::new(0));
        let resolver = BorrowResolver(Box::new(CountedVal(clones.clone())));
        let env = with_block_macros(env());
        let program = env
            .compile("block([counted], index(0) == index(0) && index(0) == index(0))")
            .unwrap();
        let mut context = Context::with_env(Arc::new(env));
        context.set_variable_resolver(&resolver);
        program.execute(&context).unwrap();
        // Kept once, then borrowed: a per-reference copy would add to it.
        assert_eq!(clones.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn a_slot_is_evaluated_in_the_scope_of_the_block() {
        assert_eq!(
            eval_block(env(), "block([x], [1, 2].map(x, index(0)))"),
            Ok(Value::List(Arc::new(vec![Value::Int(-1), Value::Int(-1)])))
        );
    }

    #[test]
    fn a_slot_may_only_reference_the_slots_before_it() {
        for expr in [
            "block([index(1), 1], index(0))",
            "block([index(0)], index(0))",
        ] {
            assert!(
                eval_block(env(), expr)
                    .is_err_and(|e| e.to_string().contains("referenced before its definition")),
                "{expr}"
            );
        }
    }

    #[test]
    fn a_slot_past_an_inner_block_s_is_an_outer_one() {
        assert_eq!(
            eval_block(env(), "block([7], block([], index(0)))"),
            Ok(Value::Int(7))
        );
        assert_eq!(
            eval_block(env(), "block([7, 8], block([1], index(0) + index(1)))"),
            Ok(Value::Int(9))
        );
    }

    #[test]
    fn a_slot_can_t_reference_itself_through_an_inner_block() {
        assert!(eval_block(env(), "block([block([], index(0))], index(0))")
            .is_err_and(|e| e.to_string().contains("referenced before its definition")));
    }

    #[test]
    fn a_name_spelling_no_slot_is_resolved_from_the_context() {
        let env = with_block_macros(env());
        let program = env.compile("block([1], index(9).a)").unwrap();
        let mut context = Context::with_env(Arc::new(env));
        context.add_variable_from_value("@index9.a", Value::Int(42));
        assert_eq!(program.execute(&context), Ok(Value::Int(42)));
    }

    #[test]
    fn a_function_resolving_a_slot_does_not_see_the_block() {
        // The slots are kept alongside the context, not on it.
        assert!(matches!(
            eval_block(env(), "block([x * 2], slot0())"),
            Err(ExecutionError::UndeclaredReference(_))
        ));
    }

    #[test]
    fn block_needs_a_list_of_slots() {
        assert!(eval_block(env(), "block(x, 1)").is_err());
    }

    #[test]
    fn a_slot_cant_be_optional() {
        let expr = |optional_indices| IdedExpr {
            id: 0,
            expr: Expr::Call(CallExpr {
                func_name: super::BLOCK.to_owned(),
                target: None,
                args: vec![
                    IdedExpr {
                        id: 1,
                        expr: Expr::List(ListExpr {
                            elements: vec![IdedExpr {
                                id: 2,
                                expr: Expr::Literal(LiteralValue::Int(1.into())),
                            }],
                            optional_indices,
                        }),
                    },
                    IdedExpr {
                        id: 3,
                        expr: Expr::Ident("@index0".to_owned()),
                    },
                ],
            }),
        };
        let context = Context::with_env(Arc::new(env()));
        assert_eq!(Value::resolve(&expr(vec![]), &context), Ok(Value::Int(1)));
        assert!(Value::resolve(&expr(vec![0]), &context).is_err());
    }

    #[test]
    fn block_needs_the_extension() {
        assert!(matches!(
            eval_block(Env::stdlib(), "block([1], index(0))"),
            Err(ExecutionError::UndeclaredReference(_))
        ));
    }

    #[test]
    fn bind_is_only_a_macro_on_cel() {
        for expr in ["m.bind(a, 1, a)", "bind(a, 1, a)", "cel.x.bind(a, 1, a)"] {
            let parsed = env().parser().parse(expr).unwrap();
            assert!(
                matches!(&parsed.expr, Expr::Call(call) if call.func_name == "bind"),
                "{expr}: {parsed:?}"
            );
        }
    }
}
