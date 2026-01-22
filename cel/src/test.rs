// For the DynamicType derive macro to work inside the cel crate itself,
// we need to alias the crate so ::cel:: paths resolve correctly
extern crate self as cel;

use std::alloc::System;
use std::collections::HashMap;
use std::fmt::Display;
use std::marker::PhantomPinned;
use std::pin::Pin;
use std::ptr::NonNull;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use serde::{Serialize, Serializer};
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
    claims: Claims,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Http2Request {
    #[serde(serialize_with = "ser_display")]
    method: http::Method,
    path: String,
    headers: HashMap<String, String>,
    claims: Claims,
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
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Claims(serde_json::Value);

// Implement Default for Claims to allow easy initialization in tests
impl Default for Claims {
    fn default() -> Self {
        Claims(serde_json::Value::Object(Default::default()))
    }
}

// Helper function to extract the inner value from Claims
fn claims_inner<'a>(c: &'a &'a Claims) -> &'a serde_json::Value {
    &c.0
}

// Generic helper to convert any AsRef<str> to &str
// Works with http::Method, String, and other AsRef<str> types
fn as_str<'a, T: AsRef<str>>(c: &'a &'a T) -> Value<'a> {
    Value::String(c.as_ref().into())
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, DynamicType)]
pub struct HttpRequestRef<'a> {
    // Use with_value to convert http::Method to Value directly
    #[dynamic(with_value = "as_str")]
    #[serde(serialize_with = "ser_display")]
    method: &'a http::Method,
    path: &'a str,
    headers: &'a HashMap<String, String>,
    // Use with to unwrap the Claims newtype
    #[dynamic(with = "claims_inner")]
    claims: &'a Claims,
}

pub fn ser_display<S: Serializer, T: Display>(t: &T, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(&t.to_string())
}

#[derive(Debug, Clone)]
pub struct DynResolverRef<'a> {
    rf: &'a DynResolver<'a>,
}
#[derive(Debug, Clone, Serialize, DynamicType)]
pub struct DynResolver<'a> {
    request: Option<HttpRequestRef<'a>>,
}
impl<'a> DynResolver<'a> {
    pub fn eval(&'a self, ctx: &'a Context, expr: &'a Expression) -> Value<'a> {
        let resolver2 = DynResolverRef { rf: self };
        let res = Value::resolve(expr, &ctx, &resolver2).unwrap();
        res
    }
}
impl<'a> VariableResolver<'a> for DynResolverRef<'a> {
    fn resolve(&self, variable: &str) -> Option<Value<'a>> {
        self.rf.field(variable)
    }
}
impl<'a> DynResolver<'a> {
    pub fn new_from_request(req: &'a Http2Request) -> Self {
        Self {
            request: Some(HttpRequestRef {
                method: &req.method,
                path: req.path.as_str(),
                headers: &req.headers,
                claims: &req.claims,
            }),
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
    let (lock, count) = get_alloc_counter();
    let _guard = lock.lock().unwrap_or_else(|e| e.into_inner());

    let mut pctx = Context::default();
    pctx.add_function("with", with);
    let req = HttpRequest {
        method: "GET".to_string(),
        path: "/foo".to_string(),
        headers: Default::default(),
        claims: Default::default(),
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
    let (lock, count) = get_alloc_counter();
    let _guard = lock.lock().unwrap_or_else(|e| e.into_inner());

    let mut pctx = Context::default();
    pctx.add_function("with", with);
    let req = HttpRequest {
        method: "GET".to_string(),
        path: "/foo".to_string(),
        headers: Default::default(),
        claims: Default::default(),
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
    let (lock, _count) = get_alloc_counter();
    let _guard = lock.lock().unwrap_or_else(|e| e.into_inner());

    let mut pctx = Context::default();
    pctx.add_function("with", with);
    let req = HttpRequest {
        method: "GET".to_string(),
        path: "/foo".to_string(),
        headers: Default::default(),
        claims: Default::default(),
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
        claims: Default::default(),
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

fn run(expr: &str, req: Http2Request, f: impl FnOnce(Value)) {
    let (lock, count) = get_alloc_counter();
    let _guard = lock.lock().unwrap_or_else(|e| e.into_inner());

    let mut pctx = Context::default();
    pctx.add_function("with", with);
    let headers = HashMap::from([("k".to_string(), "v".to_string())]);
    let claims = Claims(json!({"sub": "me@example.com"}));
    let p = Program::compile(expr).unwrap();

    AllocationRegistry::enable_tracking();
    let resolver = DynResolver::new_from_request(&req);
    let res = resolver.eval(&pctx, &p.expression);
    AllocationRegistry::disable_tracking();
    f(res);
}

#[test]
fn dyn_val() {
    let headers = HashMap::from([("k".to_string(), "v".to_string())]);
    let claims = Claims(json!({"sub": "me@example.com"}));
    run(
        "request.claims.sub + request.method + request.path + request.headers['k']",
        Http2Request {
            method: http::Method::GET,
            path: "/foo".to_string(),
            headers,
            claims,
        },
        |res| {
            assert_eq!(res.json().unwrap(), json!("me@example.comGET/foov"));
        },
    );
}

#[test]
fn test_dynamic_with_attribute() {
    let pctx = Context::default();
    let headers = HashMap::new();
    let claims = Claims(json!({"sub": "user@example.com", "role": "admin"}));

    let req = Http2Request {
        method: http::Method::GET,
        path: "/api/data".to_string(),
        headers,
        claims,
    };

    let resolver = DynResolver::new_from_request(&req);

    // Test accessing claims field which should use the with attribute
    // Access through the derived DynResolver struct, not directly on the Option
    let request_val = resolver.field("request").unwrap();
    let request_dv = match request_val {
        Value::Dynamic(dv) => dv,
        _ => panic!("Expected dynamic value for request"),
    };
    let claims_val = request_dv.field("claims").unwrap();

    // The with attribute transforms &Claims to &serde_json::Value via |c| &c.0
    // So we should be able to access the inner value directly
    let dv = match claims_val {
        Value::Dynamic(dv) => dv,
        _ => panic!("Expected dynamic value for claims"),
    };

    // Access a field in the transformed value
    let sub_val = dv.field("sub").unwrap();
    assert_eq!(sub_val.json().unwrap(), json!("user@example.com"));

    let role_val = dv.field("role").unwrap();
    assert_eq!(role_val.json().unwrap(), json!("admin"));

    // Test via CEL expression
    let p = Program::compile("request.claims.sub").unwrap();
    let res = resolver.eval(&pctx, &p.expression);
    assert_eq!(res.json().unwrap(), json!("user@example.com"));
}

#[test]
fn test_dynamic_with_value_attribute() {
    // Test the with_value attribute which directly returns Value
    let pctx = Context::default();
    let headers = HashMap::new();
    let claims = Claims(json!({"sub": "user@example.com"}));

    let req = Http2Request {
        method: http::Method::POST,
        path: "/api/users".to_string(),
        headers,
        claims,
    };

    let resolver = DynResolver::new_from_request(&req);

    // Test accessing method field which uses with_value attribute
    // Access through the derived DynResolver struct
    let request_val = resolver.field("request").unwrap();
    let request_dv = match request_val {
        Value::Dynamic(dv) => dv,
        _ => panic!("Expected dynamic value for request"),
    };
    let method_val = request_dv.field("method").unwrap();

    // with_value should return Value::String directly
    match method_val {
        Value::String(s) => {
            assert_eq!(s.as_ref(), "POST");
        }
        _ => panic!("Expected String value for method, got {:?}", method_val),
    }

    // Test via materialize
    let materialized = resolver.materialize();
    if let Value::Map(map) = materialized {
        // Get the request field (which is an Option<HttpRequestRef>)
        let request_val = map.get(&KeyRef::from("request")).unwrap();
        // Since it's Some, it should be a Map
        if let Value::Map(request_map) = request_val {
            let method_from_map = request_map.get(&KeyRef::from("method")).unwrap();
            match method_from_map {
                Value::String(s) => {
                    assert_eq!(s.as_ref(), "POST");
                }
                _ => panic!("Expected String value in map"),
            }
        } else {
            panic!("Expected Map for request field");
        }
    } else {
        panic!("Expected Map from materialize");
    }

    // Test via CEL expression
    let p = Program::compile("request.method").unwrap();
    let res = resolver.eval(&pctx, &p.expression);
    assert_eq!(res.json().unwrap(), json!("POST"));
}

#[test]
fn test_option_dynamic_type() {
    // Test Option<T> fields via maybe_materialize_optional
    // Note: Option<T> does NOT implement DynamicValueVtable directly to avoid the static vtable issue.
    // Instead, when using #[derive(DynamicType)] on a struct with Option<T> fields,
    // the derive macro will automatically use maybe_materialize_optional for those fields.
    use crate::types::dynamic::{DynamicType, maybe_materialize_optional};

    // Test Some(value)
    let some_string: Option<String> = Some("hello".to_string());
    let val = maybe_materialize_optional(&some_string);
    assert_eq!(val, Value::String("hello".into()));

    // Test None
    let none_string: Option<String> = None;
    let val = maybe_materialize_optional(&none_string);
    assert_eq!(val, Value::Null);

    // Test with non-auto-materializing type (HashMap)
    let some_map: Option<std::collections::HashMap<String, String>> = Some({
        let mut m = std::collections::HashMap::new();
        m.insert("key".to_string(), "value".to_string());
        m
    });

    let val = maybe_materialize_optional(&some_map);
    // The Some(HashMap) should materialize to a Dynamic value since HashMap doesn't auto-materialize
    match val {
        Value::Dynamic(dv) => {
            // Can access fields through the dynamic value
            let field_val = dv.field("key").unwrap();
            assert_eq!(field_val, Value::String("value".into()));
        }
        _ => panic!("Expected Dynamic value for HashMap"),
    }

    // Test None for HashMap
    let none_map: Option<std::collections::HashMap<String, String>> = None;
    let val = maybe_materialize_optional(&none_map);
    assert_eq!(val, Value::Null);
}

#[test]
fn test_option_in_derived_struct() {
    // Test Option<T> fields in a derived struct
    use crate::types::dynamic::DynamicType;

    #[derive(Debug, DynamicType)]
    struct MyStruct<'a> {
        required: &'a str,
        optional: Option<i64>,
    }

    let with_value = MyStruct {
        required: "test",
        optional: Some(42),
    };

    // Materialize the struct
    let materialized = with_value.materialize();
    if let Value::Map(map) = materialized {
        // Check required field
        let req = map.get(&KeyRef::from("required")).unwrap();
        assert_eq!(req, &Value::String("test".into()));

        // Check optional field with Some value
        let opt = map.get(&KeyRef::from("optional")).unwrap();
        assert_eq!(opt, &Value::Int(42));
    } else {
        panic!("Expected Map");
    }

    let without_value = MyStruct {
        required: "test2",
        optional: None,
    };

    // Materialize the struct with None
    let materialized = without_value.materialize();
    if let Value::Map(map) = materialized {
        // Check required field
        let req = map.get(&KeyRef::from("required")).unwrap();
        assert_eq!(req, &Value::String("test2".into()));

        // Check optional field with None value
        let opt = map.get(&KeyRef::from("optional")).unwrap();
        assert_eq!(opt, &Value::Null);
    } else {
        panic!("Expected Map");
    }
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

// Global allocation tracking
static GLOBAL_ALLOC_COUNTER: OnceLock<(Mutex<()>, Arc<AtomicUsize>)> = OnceLock::new();

fn get_alloc_counter() -> (&'static Mutex<()>, &'static Arc<AtomicUsize>) {
    let (lock, counter) = GLOBAL_ALLOC_COUNTER.get_or_init(|| {
        let counter = Arc::new(AtomicUsize::new(0));
        let _ = AllocationRegistry::set_global_tracker(Counter(counter.clone()));
        (Mutex::new(()), counter)
    });
    counter.store(0, Ordering::SeqCst);
    (lock, counter)
}
