//! Shows what a registered function can avoid doing once it stops being forced
//! through `Value`: a request is bound to the interpreter as a CEL struct whose
//! fields *borrow* from the handler's own `HttpRequest`, and the functions below
//! hand back slices of those very bytes.
//!
//! Nothing is copied on the way in: `CelString::from(&str)` borrows, so the
//! struct's `path` field is the request's own path, for as long as the request
//! lives. Nothing is copied on the way out either: `stripVersion` reads that field
//! when it is called, strips the version prefix off the `&str`, and returns a
//! `CelString` over the remaining slice - the same bytes, not a copy of them.
//!
//! That works because a registered function is handed `CowVal<'context, 'call>`s:
//! `'context` is the borrow of the call itself and `'call` bounds the data the
//! values borrow (here, the request). A function may return anything bounded by
//! those, so a `CelString<'call>` over a sub-slice of the request's path is fine,
//! and the interpreter can hand it to the caller with the same bound.
//!
//! Run with `cargo run -p example --bin example-no-copy --features structs`.
use cel::common::ast::{Expr, LiteralValue};
use cel::common::types::{CelMap, CelMapKey, CelString, CelStruct};
use cel::common::value::{CowVal, Val};
use cel::parser::Parser;
use cel::{Context, ExecutionError, FunctionContext, Value};
use std::collections::HashMap;

/// Plain application data - nothing CEL-aware about it. This is what a request
/// handler already has lying around before it wants to evaluate a policy against it.
struct HttpRequest {
    method: &'static str,
    path: String,
    headers: Vec<(String, String)>,
}

const VERSION_PREFIXES: [&str; 2] = ["/v1", "/v2"];

/// Exposes the request to the interpreter as a CEL struct.
///
/// Every field borrows from `req`, so building this copies no request bytes (only
/// the struct's own bookkeeping is allocated, once, here). The struct is what gets
/// bound with [`Context::add_variable_as_val`], so `request.method`, `request.path`
/// and `request.headers` work as plain field accesses, and its lifetime ties the
/// context to the request: the request has to outlive it.
fn request_struct(req: &HttpRequest) -> CelStruct<'_> {
    let headers: HashMap<CelMapKey<'_>, Box<dyn Val + '_>> = req
        .headers
        .iter()
        .map(|(k, v)| {
            let value: Box<dyn Val + '_> = Box::new(CelString::from(v.as_str()));
            (CelMapKey::from(k.as_str()), value)
        })
        .collect();

    let mut request = CelStruct::new(REQUEST_TYPE.to_owned());
    request.add_field_value(
        "method".to_owned(),
        CowVal::owned(CelString::from(req.method)),
    );
    request.add_field_value(
        "path".to_owned(),
        CowVal::owned(CelString::from(req.path.as_str())),
    );
    request.add_field_value("headers".to_owned(), CowVal::owned(CelMap::from(headers)));
    request
}

const REQUEST_TYPE: &str = "HttpRequest";

/// The raw shape a registered function actually has (`Context::add_function`'s
/// typed closures are sugar that builds this same type). Reaching for it directly,
/// instead of a plain `fn(...) -> R` closure, is what lets a function hand back a
/// [`CowVal`] tied to the call's own [`FunctionContext`] lifetimes, instead of being
/// limited to an owned value.
type RawFunction = Box<
    dyn for<'context, 'call> Fn(
            &mut FunctionContext<'context, 'call>,
        ) -> Result<CowVal<'context, 'call>, ExecutionError>
        + Send
        + Sync,
>;

/// Extracts `this` as the request struct, requiring it to already be a borrow (e.g.
/// `request` resolved as a plain identifier) rather than a freshly computed value.
/// Only a borrow can be handed on for `'context`, which is what lets the functions
/// below return slices of the request's own fields instead of copies.
fn this_request<'context, 'call>(
    ftx: &FunctionContext<'context, 'call>,
) -> Result<&'context CelStruct<'call>, ExecutionError> {
    match &ftx.this {
        Some(CowVal::Borrowed(this)) => {
            let this: &'context (dyn Val + 'call) = *this;
            this.downcast_ref::<CelStruct>()
                .filter(|request| request.name() == REQUEST_TYPE)
                .ok_or_else(|| ftx.error("not an HttpRequest"))
        }
        _ => Err(ftx.error("must be called on a borrowed HttpRequest")),
    }
}

/// `request.stripVersion()` - the request's path without its `/v1` or `/v2` prefix.
///
/// The path is read from the struct's field at call time, and the result is a
/// `CelString` over the tail of *that same `&str`*: `path.strip_prefix(..)` only
/// moves the start pointer. The bytes are never copied. The one allocation left is
/// the `Box` around the 24-byte `CelString` itself, which is what makes it an
/// owned `CowVal` (a `CowVal::Borrowed` needs a value that already exists to point
/// at, and this one is computed here).
///
/// [`CelString::as_borrowed`] is what makes this possible: it hands back the
/// field's `&'call str` itself, where `inner()` would only give a `&str` tied to
/// this call, too short-lived to return.
fn strip_version<'context, 'call>(
    ftx: &mut FunctionContext<'context, 'call>,
) -> Result<CowVal<'context, 'call>, ExecutionError> {
    let request = this_request(ftx)?;
    let path: &'call str = request
        .field_value("path")
        .and_then(|path| path.downcast_ref::<CelString>())
        .and_then(CelString::as_borrowed)
        .ok_or_else(|| ftx.error("`path` must be a borrowed string"))?;

    let stripped = VERSION_PREFIXES
        .iter()
        .find_map(|prefix| {
            path.strip_prefix(prefix)
                // `/v10/..` is not `/v1` + `0/..`: only strip whole segments
                .filter(|rest| rest.is_empty() || rest.starts_with('/'))
        })
        .unwrap_or(path);
    Ok(CowVal::owned(CelString::from(stripped)))
}

/// `request.header(name)` - looks a header up case-insensitively and hands back a
/// borrow of the value already stored in the request's `headers` map, rather than
/// allocating a fresh string for it.
fn header<'context, 'call>(
    ftx: &mut FunctionContext<'context, 'call>,
) -> Result<CowVal<'context, 'call>, ExecutionError> {
    let request = this_request(ftx)?;
    let name = ftx
        .args
        .first()
        .and_then(|name| name.downcast_ref::<CelString>())
        .ok_or_else(|| ftx.error("header() takes a string argument"))?;
    let headers = request
        .field_value("headers")
        .and_then(|headers| headers.downcast_ref::<CelMap>())
        .ok_or_else(|| ftx.error("`headers` must be a map"))?;

    headers
        .inner()
        .iter()
        .find_map(|(key, value)| match key {
            CelMapKey::String(key) if key.inner().eq_ignore_ascii_case(name.inner()) => {
                Some(CowVal::Borrowed(value.as_ref()))
            }
            _ => None,
        })
        .ok_or_else(|| ftx.error(format!("no such header: {}", name.inner())))
}

fn main() {
    let request = HttpRequest {
        method: "GET",
        path: "/v2/users/42".to_string(),
        headers: vec![("Host".to_string(), "api.example.com".to_string())],
    };

    // The request must outlive the context: the struct bound below borrows from it.
    let mut context = Context::default();
    context.add_variable_as_val("request", Box::new(request_struct(&request)));
    context
        .add_function("stripVersion", Box::new(strip_version) as RawFunction)
        .unwrap();
    context
        .add_function("header", Box::new(header) as RawFunction)
        .unwrap();

    // Rolled out manually rather than via `Program`/`execute`, so the result stays
    // a `CowVal` all the way out - the same thing the functions above return -
    // instead of being converted to `Value` at the last step.
    let evaluate = |expr: &str| {
        let ast = Parser::default().parse(expr).unwrap();
        Value::resolve_val(&ast, &context)
            .unwrap()
            .downcast_ref::<CelString>()
            .expect("a string")
            .as_borrowed()
            .expect("the result to borrow from the request")
    };

    // Plain field access on the injected struct: the request's own method bytes.
    let method = evaluate("request.method");
    println!("method: {method}");
    assert_eq!(method, "GET");
    assert!(std::ptr::eq(method.as_ptr(), request.method.as_ptr()));

    // The version-stripped path points into the request's path, past the `/v2`
    // prefix: not a copy of it.
    let stripped = evaluate("request.stripVersion()");
    println!("stripped path: {stripped}");
    assert_eq!(stripped, "/users/42");
    assert!(std::ptr::eq(stripped.as_ptr(), request.path[3..].as_ptr()));

    // A path without a version prefix comes back whole - still the same bytes.
    let unversioned = HttpRequest {
        method: "GET",
        path: "/v10/users".to_string(),
        headers: vec![],
    };
    let mut other = Context::default();
    other.add_variable_as_val("request", Box::new(request_struct(&unversioned)));
    other
        .add_function("stripVersion", Box::new(strip_version) as RawFunction)
        .unwrap();
    let ast = Parser::default().parse("request.stripVersion()").unwrap();
    let result = Value::resolve_val(&ast, &other).unwrap();
    let path = result.downcast_ref::<CelString>().unwrap().as_borrowed();
    assert_eq!(path, Some("/v10/users"));
    assert!(std::ptr::eq(
        path.unwrap().as_ptr(),
        unversioned.path.as_ptr()
    ));

    // A header value is a borrow of the one stored in the request's headers.
    let host = evaluate("request.header('host')");
    println!("host header: {host}");
    assert_eq!(host, "api.example.com");
    assert!(std::ptr::eq(host.as_ptr(), request.headers[0].1.as_ptr()));

    // Nothing here needs the request: a literal in the expression is evaluated as a
    // borrow of the AST itself, so the string handed back is the very one the
    // parser stored, not a copy of it. (The AST owns that string, so this borrow is
    // bounded by the `ast` and the `result` rather than by the context's data, and
    // `as_borrowed()` does not apply: `inner()` is the right accessor.)
    let ast = Parser::default().parse("'foo'").unwrap();
    let Expr::Literal(LiteralValue::String(parsed)) = &ast.expr else {
        panic!(
            "`'foo'` should parse to a string literal, got {:?}",
            ast.expr
        );
    };
    let result = Value::resolve_val(&ast, &context).unwrap();
    let literal = result.downcast_ref::<CelString>().unwrap();
    assert_eq!(literal.inner(), "foo");
    assert!(std::ptr::eq(
        literal.inner().as_ptr(),
        parsed.inner().as_ptr()
    ));
}
