use crate::common::ast::{
    CallExpr, ComprehensionExpr, EntryExpr, Expr, IdedEntryExpr, ListExpr, MapEntryExpr, MapExpr,
    SelectExpr,
};
use crate::magic::This;
use crate::objects::{Key, Map, Opaque};
use crate::parser::Expression;
use crate::{ExecutionError, IdedExpr, Value};
use std::sync::Arc;

fn is_lit(e: &Expr) -> bool {
    matches!(e, Expr::Literal(_) | Expr::Inline(_) | Expr::Map(_))
}

fn as_value(e: IdedExpr) -> Value {
    assert!(is_lit(&e.expr));
    match e.expr {
        Expr::Literal(l) => Value::from(l),
        Expr::Inline(l) => l,
        _ => unreachable!(),
    }
}

#[derive(Debug)]
pub struct PrecompileRegex(regex::Regex);

impl PartialEq for PrecompileRegex {
    fn eq(&self, other: &Self) -> bool {
        self.0.as_str() == other.0.as_str()
    }
}
impl Eq for PrecompileRegex {}

impl Opaque for PrecompileRegex {
    fn runtime_type_name(&self) -> &str {
        "regex"
    }

    // fn json(&self) -> Option<serde_json::Value> {
    //     todo!()
    // }
}

impl PrecompileRegex {
    pub fn precompiled_matches(
        this: This<Arc<dyn Opaque>>,
        val: Arc<String>,
    ) -> Result<bool, ExecutionError> {
        let Some(rgx) = this.0.downcast_ref::<Self>() else {
            return Err(ExecutionError::UnexpectedType {
                got: this.0.runtime_type_name().to_string(),
                want: "regex".to_string(),
            });
        };
        // panic!("");
        Ok(rgx.0.is_match(&val))
    }
}

fn specialize_call(c: CallExpr) -> CallExpr {
    match c.func_name.as_str() {
        "matches" if c.args.len() == 1 && c.target.is_some() => {
            let t = c.target.unwrap();
            let arg = c.args.into_iter().next().unwrap();
            let id = arg.id;
            // TODO: do not panic
            let Value::String(arg) = as_value(arg) else {
                panic!("todo")
            };

            let opaque = Value::Opaque(Arc::new(PrecompileRegex(
                regex::Regex::new(&arg).expect("TODO unwrap is wrong here"),
            )));
            let id_expr = IdedExpr {
                id,
                expr: Expr::Inline(opaque),
            };
            // We invert this to be 'regex.precompiled_matches(string)'
            // instead of 'string.matches(regex)'
            CallExpr {
                func_name: "precompiled_matches".to_string(),
                target: Some(Box::new(id_expr)),
                args: vec![*t],
            }
        }
        _ => c,
    }
}

pub fn optimize(expr: Expression) -> Expression {
    let id = expr.id;
    let with_id = |expr: Expr| Expression { id, expr };
    match expr.expr {
        Expr::Call(c) => {
            let target = c.target.map(|t| Box::new(optimize(*t)));
            let args = c.args.into_iter().map(optimize).collect::<Vec<_>>();
            let call = CallExpr {
                target,
                args,
                func_name: c.func_name,
            };
            with_id(Expr::Call(specialize_call(call)))
        }
        Expr::Comprehension(c) => with_id(Expr::Comprehension(Box::new(ComprehensionExpr {
            iter_range: optimize(c.iter_range),
            iter_var: c.iter_var,
            iter_var2: c.iter_var2,
            accu_var: c.accu_var,
            accu_init: optimize(c.accu_init),
            loop_cond: optimize(c.loop_cond),
            loop_step: optimize(c.loop_step),
            result: optimize(c.result),
        }))),
        Expr::Select(s) => with_id(Expr::Select(SelectExpr {
            operand: Box::new(optimize(*s.operand)),
            field: s.field,
            test: s.test,
        })),
        Expr::Struct(_) => {
            todo!()
        }
        Expr::List(v) => {
            let nl: Vec<IdedExpr> = v.elements.into_iter().map(optimize).collect();
            if nl.iter().all(|nl| is_lit(&nl.expr)) {
                with_id(Expr::Inline(Value::List(Arc::new(
                    nl.into_iter().map(as_value).collect(),
                ))))
            } else {
                with_id(Expr::List(ListExpr { elements: nl }))
            }
        }
        Expr::Map(m) => {
            let ne: Vec<IdedEntryExpr> = m
                .entries
                .into_iter()
                .map(|e| match e.expr {
                    EntryExpr::MapEntry(me) => {
                        let value = optimize(me.value);
                        let key = optimize(me.key);
                        let ne = MapEntryExpr {
                            value,
                            key,
                            optional: me.optional,
                        };
                        IdedEntryExpr {
                            id: e.id,
                            expr: EntryExpr::MapEntry(ne),
                        }
                    }
                    _ => unreachable!(),
                })
                .collect();
            if ne.iter().all(|nl| match &nl.expr {
                EntryExpr::MapEntry(me) => is_lit(&me.key.expr) && is_lit(&me.value.expr),
                _ => unreachable!(),
            }) {
                let m: std::collections::HashMap<Key, Value> = ne
                    .into_iter()
                    .map(|e| match e.expr {
                        EntryExpr::MapEntry(me) => (
                            as_value(me.key).try_into().expect("must be a valid key"),
                            as_value(me.value),
                        ),
                        _ => unreachable!(),
                    })
                    .collect();
                with_id(Expr::Inline(Value::Map(Map { map: Arc::new(m) })))
            } else {
                with_id(Expr::Map(MapExpr { entries: ne }))
            }
        }
        Expr::Literal(value) => with_id(Expr::Inline(Value::from(value))),
        // Expr::Unspecified => {}
        // Expr::Ident(_) => {}
        // Expr::Inline(_) => {}
        _ => expr,
    }
}
