use crate::context::VariableResolver;
use crate::magic::Function;
use crate::objects::{OpaqueBox, StringValue, StructValue};
use crate::parser::Expression;
use crate::{Context, FunctionContext, Program, Value};
use std::alloc::System;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock};

#[derive(Clone, Debug, Default, Eq, Hash, PartialEq, PartialOrd, Ord)]
pub struct HttpRequest {
    method: String,
    path: String,
}

#[derive(Clone, Debug)]
pub struct RequestOpaque<'a> {
    pub request: &'a HttpRequest,
}

fn custom_function() -> &'static Function {
    static CUSTOM_FN: OnceLock<Function> = OnceLock::new();
    CUSTOM_FN.get_or_init(|| {
        Box::new(|_ftx: &mut FunctionContext| Ok(Value::String(StringValue::Borrowed("YES"))))
    })
}
impl<'a> StructValue<'a> for RequestOpaque<'a> {
    fn resolve_function(&self, name: &str) -> Option<&Function> {
        match name {
            "custom" => Some(custom_function()),
            _ => None,
        }
    }
    fn get_member(&self, name: &str) -> Option<Value<'a>> {
        match name {
            "path" => Some(Value::String(self.request.path.as_str().into())),
            "method" => Some(Value::String(self.request.method.as_str().into())),
            _ => None,
        }
    }
}

pub struct Resolver<'a> {
    request: &'a HttpRequest,
}

impl<'a> VariableResolver<'a> for Resolver<'a> {
    fn resolve_member(&self, expr: &str, member: &str) -> Option<Value<'a>> {
        match expr {
            "request" => RequestOpaque {
                request: self.request,
            }
            .get_member(member),
            _ => None,
        }
    }
    fn resolve(&self, variable: &str) -> Option<Value<'a>> {
        match variable {
            "request" => Some(Value::Struct(OpaqueBox::new(RequestOpaque {
                request: self.request,
            }))),
            _ => None,
        }
    }
}
fn execute_with_mut_request<'a>(
    ctx: &'a Context,
    expression: &'a Expression,
    req: &'a Resolver<'a>,
) -> StringValue<'a> {
    let res = Value::resolve(expression, ctx, req).unwrap();
    let Value::String(s) = res else { panic!() };
    assert_eq!(s.as_ref(), "YES");
    s
}

struct CompositeResolver<'a, 'rf> {
    base: &'rf dyn VariableResolver<'a>,
    name: &'a str,
    val: Value<'a>,
}

impl<'a, 'rf> VariableResolver<'a> for CompositeResolver<'a, 'rf> {
    fn resolve(&self, expr: &str) -> Option<Value<'a>> {
        if expr == self.name {
            Some(self.val.clone())
        } else {
            self.base.resolve(expr)
        }
    }
}
fn with<'a, 'rf, 'b>(ftx: &'b mut crate::FunctionContext<'a, 'rf>) -> crate::ResolveResult<'a> {
    let this = ftx.this.as_ref().unwrap();
    let ident = ftx.ident(0)?;
    let expr: &'a Expression = ftx.expr(1)?;
    let x: &'rf dyn VariableResolver<'a> = ftx.vars();
    let resolver = CompositeResolver::<'a, 'rf> {
        base: x,
        // name: todo!(),
        name: ident,
        // val: todo!(),
        val: this.clone(),
    };
    let v = Value::resolve(expr, ftx.ptx, &resolver)?;
    Ok(v)
}

#[global_allocator]
static GLOBAL: Allocator<System> = Allocator::system();
#[test]
fn zero_alloc() {
    let count = Arc::new(AtomicUsize::new(0));
    let _ = AllocationRegistry::set_global_tracker(Counter(count.clone()));
    let mut pctx = Context::default();
    pctx.add_function("with", with);
    let req = HttpRequest {
        method: "GET".to_string(),
        path: "/foo".to_string(),
    };
    let p = Program::compile("request.path.with(p, p == '/foo' ? 'YES' : 'NO')")
        .unwrap()
        .optimized()
        .expression;

    let resolver = Resolver { request: &req };
    execute_with_mut_request(&pctx, &p, &resolver);
    AllocationRegistry::enable_tracking();
    for _ in 0..2 {
        execute_with_mut_request(&pctx, &p, &resolver);
    }
    AllocationRegistry::disable_tracking();
    assert_eq!(count.load(Ordering::SeqCst), 0);
}

#[test]
fn struct_function() {
    let mut pctx = Context::default();
    pctx.add_function("with", with);
    let req = HttpRequest {
        method: "GET".to_string(),
        path: "/foo".to_string(),
    };
    let p = Program::compile("request.path.with(p, p == '/foo' ? request.custom() : 'NO')")
        .unwrap()
        .optimized()
        .expression;

    let resolver = Resolver { request: &req };
    execute_with_mut_request(&pctx, &p, &resolver);
    for _ in 0..2 {
        let _ = execute_with_mut_request(&pctx, &p, &resolver);
    }
}

use tracking_allocator::{AllocationGroupId, AllocationRegistry, AllocationTracker, Allocator};

#[derive(Default, Clone, Debug)]
struct Counter(Arc<AtomicUsize>);

impl AllocationTracker for Counter {
    fn allocated(
        &self,
        _addr: usize,
        _object_size: usize,
        _wrapped_size: usize,
        _group_id: AllocationGroupId,
    ) {
        self.0.fetch_add(1, Ordering::SeqCst);
        // println!(
        //     "allocation -> addr=0x{:0x} object_size={} wrapped_size={} group_id={:?}",
        //     addr, object_size, wrapped_size, group_id
        // );
    }

    fn deallocated(
        &self,
        _addr: usize,
        _object_size: usize,
        _wrapped_size: usize,
        _source_group_id: AllocationGroupId,
        _current_group_id: AllocationGroupId,
    ) {
        // println!(
        //     "deallocation -> addr=0x{:0x} object_size={} wrapped_size={} source_group_id={:?} current_group_id={:?}",
        //     addr, object_size, wrapped_size, source_group_id, current_group_id
        // );
    }
}
