// For the DynamicType derive macro to work inside the cel crate itself,
// we need to alias the crate so ::cel:: paths resolve correctly
extern crate self as cel;

use std::alloc::System;
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock};

use serde::Serialize;
use serde_json::json;

use crate::context::VariableResolver;
use crate::magic::Function;
use crate::objects::{KeyRef, MapValue, ObjectType, ObjectValue, StringValue};
use crate::parser::Expression;
use crate::types::dynamic::{DynamicType, DynamicValueVtable, Vtable};
use crate::{Context, FunctionContext, Program, Value, to_value, types};

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct HttpRequest {
    method: String,
    path: String,
    headers: HashMap<String, String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct RequestOpaque<'a>(&'a HttpRequest);

fn custom_function() -> &'static Function {
    static CUSTOM_FN: OnceLock<Function> = OnceLock::new();
    CUSTOM_FN.get_or_init(|| {
        Box::new(|_ftx: &mut FunctionContext| Ok(Value::String(StringValue::Borrowed("YES"))))
    })
}
crate::register_type!(RequestOpaque);

impl<'a> ObjectType<'a> for RequestOpaque<'a> {
    fn type_name(&self) -> &'static str {
        "request_opaque"
    }

    fn resolve_function(&self, name: &str) -> Option<&Function> {
        match name {
            "custom" => Some(custom_function()),
            _ => None,
        }
    }

    fn get_member(&self, name: &str) -> Option<Value<'a>> {
        match name {
            "path" => Some(Value::String(self.0.path.as_str().into())),
            "method" => Some(Value::String(self.0.method.as_str().into())),
            "headers" => None, // TODO
            _ => None,
        }
    }

    fn json(&self) -> Option<serde_json::Value> {
        serde_json::to_value(self.0).ok()
    }
}

#[derive(Serialize)]
pub struct Resolver<'a> {
    request: &'a HttpRequest,
}

impl<'a> VariableResolver<'a> for Resolver<'a> {
    fn resolve_member(&self, expr: &str, member: &str) -> Option<Value<'a>> {
        match expr {
            "request" => RequestOpaque(self.request).get_member(member),
            _ => None,
        }
    }
    fn resolve(&self, variable: &str) -> Option<Value<'a>> {
        match variable {
            "request" => Some(Value::Object(ObjectValue::new(RequestOpaque(self.request)))),
            _ => None,
        }
    }

    fn all(&self) -> &[&'static str] {
        &["request"]
    }
}

impl<'a> DynamicType for serde_json::Value {
    fn materialize(&self) -> Value<'_> {
        to_value(self).unwrap()
    }
    fn auto_materialize(&self) -> bool {
        false
    }

    fn field(&self, field: &str) -> Option<Value<'_>> {
        match self {
            serde_json::Value::Object(m) => {
                let v = m.get(field)?;
                Some(types::dynamic::maybe_materialize(v))
            }
            _ => None,
        }
    }
}
crate::impl_dynamic_vtable!(serde_json::Value);

// Also implement for &serde_json::Value so we can use it as a reference in structs
impl<'a> DynamicType for &'a serde_json::Value {
    fn materialize(&self) -> Value<'_> {
        to_value(*self).unwrap()
    }
    fn auto_materialize(&self) -> bool {
        false
    }

    fn field(&self, field: &str) -> Option<Value<'_>> {
        match *self {
            serde_json::Value::Object(m) => {
                let v = m.get(field)?;
                Some(types::dynamic::maybe_materialize(v))
            }
            _ => None,
        }
    }
}
crate::impl_dynamic_vtable!(&serde_json::Value);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, DynamicType)]
pub struct HttpRequestRef<'a> {
    method: &'a str,
    path: &'a str,
    headers: &'a HashMap<String, String>,
    claims: &'a serde_json::Value,
}

#[derive(Debug, Clone, Serialize, DynamicType)]
pub struct DynResolver<'a> {
    request: HttpRequestRef<'a>,
}
impl<'a> VariableResolver<'a> for DynResolver<'a> {
    fn resolve(&self, variable: &str) -> Option<Value<'a>> {
        match variable {
            "request" => {
                // SAFETY: This transmute is sound under the following conditions:
                // 1. self.request contains references with lifetime 'a (HttpRequestRef<'a>)
                // 2. The caller must ensure that `self` (the DynResolver) lives at least
                //    as long as the returned Value<'a> is used
                // 3. The DynamicValue will dereference the pointer to call methods on
                //    HttpRequestRef, which then access the internal 'a references
                // 4. If DynResolver is dropped while the Value is still alive, this creates UB
                //
                // This is a limitation of the VariableResolver API: it returns Value<'a>
                // but only has &self available, creating a lifetime mismatch. Callers MUST
                // ensure the resolver outlives the returned Value.
                let req_ref: &'a HttpRequestRef<'a> = unsafe { std::mem::transmute(&self.request) };
                Some(Value::Dynamic(crate::types::dynamic::DynamicValue::new(
                    req_ref,
                )))
            }
            _ => None,
        }
    }

    fn all(&self) -> &[&'static str] {
        &["request"]
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
    fn all(&self) -> &[&'static str] {
        self.base.all()
    }

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
        name: ident,
        val: this.clone(),
    };
    let v = Value::resolve(expr, ftx.ptx, &resolver)?;
    Ok(v)
}

// #[global_allocator]
// static GLOBAL: Allocator<System> = Allocator::system();

#[test]
fn zero_alloc() {
    let count = Arc::new(AtomicUsize::new(0));
    let _ = AllocationRegistry::set_global_tracker(Counter(count.clone()));
    let mut pctx = Context::default();
    pctx.add_function("with", with);
    let req = HttpRequest {
        method: "GET".to_string(),
        path: "/foo".to_string(),
        headers: Default::default(),
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
fn header_lookup() {
    let count = Arc::new(AtomicUsize::new(0));
    let _ = AllocationRegistry::set_global_tracker(Counter(count.clone()));
    let mut pctx = Context::default();
    pctx.add_function("with", with);
    let req = HttpRequest {
        method: "GET".to_string(),
        path: "/foo".to_string(),
        headers: Default::default(),
    };
    let p = Program::compile("jwt.sub").unwrap().optimized().expression;
    dbg!(&p);
    let p = Program::compile("jwt['sub']")
        .unwrap()
        .optimized()
        .expression;
    dbg!(&p);

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
fn get_struct() {
    let count = Arc::new(AtomicUsize::new(0));
    let _ = AllocationRegistry::set_global_tracker(Counter(count.clone()));
    let mut pctx = Context::default();
    pctx.add_function("with", with);
    let req = HttpRequest {
        method: "GET".to_string(),
        path: "/foo".to_string(),
        headers: Default::default(),
    };
    let p = Program::compile("request").unwrap().optimized().expression;

    let resolver = Resolver { request: &req };
    let res = Value::resolve(&p, &pctx, &resolver).unwrap();
    assert_eq!(
        res.json().unwrap(),
        json!({"method": "GET", "path": "/foo", "headers": {}})
    );
    let Value::Object(ob) = res else { panic!() };
    let req = ob.downcast_ref::<RequestOpaque>().unwrap().0;
    assert_eq!(req.method, "GET");
}

#[test]
fn struct_function() {
    let mut pctx = Context::default();
    pctx.add_function("with", with);
    let req = HttpRequest {
        method: "GET".to_string(),
        path: "/foo".to_string(),
        headers: Default::default(),
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

#[test]
fn dyn_val() {
    let count = Arc::new(AtomicUsize::new(0));
    let _ = AllocationRegistry::set_global_tracker(Counter(count.clone()));
    let mut pctx = Context::default();
    pctx.add_function("with", with);
    let headers = HashMap::new();
    let claims = json!({"sub": "me@example.com"});
    let p = Program::compile("request.claims").unwrap();
    let dv = {
        let req = HttpRequestRef {
            method: "GET",
            path: "/foo",
            headers: &headers,
            claims: &claims,
        };

        AllocationRegistry::enable_tracking();
        let resolver = DynResolver { request: req };
        let res = Value::resolve(&p.expression, &pctx, &resolver).unwrap();
        drop(resolver);
        AllocationRegistry::disable_tracking();
        // assert_eq!(res.json().unwrap(), json!("me@example.com"));
        match res {
            Value::Dynamic(dv) => dv,
            _ => panic!("Expected dynamic value"),
        }
    };
    dbg!(types::dynamic::always_materialize(dv.field("sub").unwrap()));
    // let Value::Dynamic(_ob) = res else { panic!() };
    // let req = ob.downcast_ref::<RequestOpaque>().unwrap().0;
    // assert_eq!(req.method, "GET");
}

use cel_derive::DynamicType;
use tracking_allocator::{AllocationGroupId, AllocationRegistry, AllocationTracker, Allocator};

#[derive(Default, Clone, Debug)]
struct Counter(Arc<AtomicUsize>);

impl AllocationTracker for Counter {
    fn allocated(
        &self,
        addr: usize,
        object_size: usize,
        wrapped_size: usize,
        group_id: AllocationGroupId,
    ) {
        self.0.fetch_add(1, Ordering::SeqCst);
        println!(
            "allocation -> addr=0x{:0x} object_size={} wrapped_size={} group_id={:?}",
            addr, object_size, wrapped_size, group_id
        );
    }

    fn deallocated(
        &self,
        addr: usize,
        object_size: usize,
        wrapped_size: usize,
        source_group_id: AllocationGroupId,
        current_group_id: AllocationGroupId,
    ) {
        println!(
            "deallocation -> addr=0x{:0x} object_size={} wrapped_size={} source_group_id={:?} current_group_id={:?}",
            addr, object_size, wrapped_size, source_group_id, current_group_id
        );
    }
}
