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
use crate::{to_value, types, Context, FunctionContext, Program, Value};

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

impl<'a> DynamicValueVtable for &'a str {
    fn vtable() -> &'static Vtable {
        todo!()
    }
}
impl<'a> DynamicType for &'a str {
    fn materialize(&self) -> Value<'_> {
        Value::from(*self)
    }
    fn auto_materialize(&self) -> bool {
        true
    }
}

impl DynamicValueVtable for serde_json::Value {
        fn vtable() -> &'static Vtable {
            static VTABLE: OnceLock<Vtable> = OnceLock::new();
            VTABLE.get_or_init(|| {
                unsafe fn materialize_impl(ptr: *const ()) -> Value<'static> {
                    unsafe {
                        let this = &*(ptr as *const serde_json::Value);
                        std::mem::transmute(this.materialize())
                    }
                }

                unsafe fn field_impl(ptr: *const (), field: &str) -> Option<Value<'static>> {
                    unsafe {
                        let this = &*(ptr as *const serde_json::Value);
                        std::mem::transmute(this.field(field))
                    }
                }

                unsafe fn debug_impl(
                    ptr: *const (),
                    f: &mut std::fmt::Formatter<'_>,
                ) -> std::fmt::Result {
                    unsafe {
                        let this = &*(ptr as *const serde_json::Value);
                        std::fmt::Debug::fmt(this, f)
                    }
                }

                Vtable {
                    materialize: materialize_impl,
                    field: field_impl,
                    debug: debug_impl,
                }
            })
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct HttpRequestRef<'a> {
    method: &'a str,
    path: &'a str,
    headers: &'a HashMap<String, String>,
    claims: &'a serde_json::Value
}

impl DynamicType for HttpRequestRef<'_> {
    fn materialize(&self) -> Value<'_> {
        let mut m = vector_map::VecMap::with_capacity(3);
        m.insert(KeyRef::from("method"), types::dynamic::maybe_materialize(&self.method));
        m.insert(KeyRef::from("path"), types::dynamic::maybe_materialize(&self.path));
        // m.insert(
        //     KeyRef::from("headers"),
        //     Value::from_iter(self.headers.iter()),
        // );
        m.insert(KeyRef::from("claims"), types::dynamic::maybe_materialize(self.claims));
        Value::Map(MapValue::Borrow(m))
    }

    fn field(&self, field: &str) -> Option<Value<'_>> {
        Some(match field {
            "method" => types::dynamic::maybe_materialize(&self.method),
            "path" => types::dynamic::maybe_materialize(&self.path),
            // "headers" => types::dynamic::maybe_materialize(&self.headers),
            "claims" => types::dynamic::maybe_materialize(self.claims),
            _ => return None,
        })
    }
}

impl DynamicValueVtable for HttpRequestRef<'_> {
    fn vtable() -> &'static Vtable {
        use std::sync::OnceLock;
        static VTABLE: OnceLock<Vtable> = OnceLock::new();
        VTABLE.get_or_init(|| {
            unsafe fn materialize_impl(ptr: *const ()) -> Value<'static> {
                unsafe {
                    let this = &*(ptr as *const HttpRequestRef);
                    std::mem::transmute(this.materialize())
                }
            }

            unsafe fn field_impl(ptr: *const (), field: &str) -> Option<Value<'static>> {
                unsafe {
                    let this = &*(ptr as *const HttpRequestRef);
                    std::mem::transmute(this.field(field))
                }
            }

            unsafe fn debug_impl(
                ptr: *const (),
                f: &mut std::fmt::Formatter<'_>,
            ) -> std::fmt::Result {
                unsafe {
                    let this = &*(ptr as *const HttpRequestRef);
                    std::fmt::Debug::fmt(this, f)
                }
            }

            Vtable {
                materialize: materialize_impl,
                field: field_impl,
                debug: debug_impl,
            }
        })
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct DynResolver<'a> {
    request: HttpRequestRef<'a>,
}
impl<'a> VariableResolver<'a> for DynResolver<'a> {
    fn resolve(&self, variable: &str) -> Option<Value<'a>> {
        match variable {
            "request" => {
                // SAFETY: self.request has lifetime 'a (it's HttpRequestRef<'a>)
                // We're creating a reference to it with the correct lifetime
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
impl<'a> DynamicType for DynResolver<'a> {
    fn materialize(&self) -> Value<'_> {
        let mut m = vector_map::VecMap::with_capacity(1);
        m.insert(KeyRef::from("request"), self.request.materialize());
        Value::Map(MapValue::Borrow(m))
    }

    fn field(&self, field: &str) -> Option<Value<'_>> {
        match field {
            "request" => {
                let v: Value<'_> = self.request.materialize();
                Some(v)
            }
            _ => None,
        }
    }
}

impl DynamicValueVtable for DynResolver<'_> {
    fn vtable() -> &'static Vtable {
        static VTABLE: OnceLock<Vtable> = OnceLock::new();
        VTABLE.get_or_init(|| {
            unsafe fn materialize_impl(ptr: *const ()) -> Value<'static> {
                unsafe {
                    let this = &*(ptr as *const DynResolver);
                    std::mem::transmute(this.materialize())
                }
            }

            unsafe fn field_impl(ptr: *const (), field: &str) -> Option<Value<'static>> {
                unsafe {
                    let this = &*(ptr as *const DynResolver);
                    std::mem::transmute(this.field(field))
                }
            }

            unsafe fn debug_impl(
                ptr: *const (),
                f: &mut std::fmt::Formatter<'_>,
            ) -> std::fmt::Result {
                unsafe {
                    let this = &*(ptr as *const DynResolver);
                    std::fmt::Debug::fmt(this, f)
                }
            }

            Vtable {
                materialize: materialize_impl,
                field: field_impl,
                debug: debug_impl,
            }
        })
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
    let req = HttpRequestRef {
        method: "GET",
        path: "/foo",
        headers: &Default::default(),
        claims: &json!({"sub": "me@example.com"}),
    };
    let p = Program::compile("request.claims.sub + 'hi'").unwrap().optimized().expression;

    // AllocationRegistry::enable_tracking();
    let resolver = DynResolver { request: req };
    let res = Value::resolve(&p, &pctx, &resolver).unwrap();
    // AllocationRegistry::disable_tracking();
    assert_eq!(
        res.json().unwrap(),
        json!("me@example.com")
    );
    // let Value::Dynamic(_ob) = res else { panic!() };
    // let req = ob.downcast_ref::<RequestOpaque>().unwrap().0;
    // assert_eq!(req.method, "GET");
}

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
