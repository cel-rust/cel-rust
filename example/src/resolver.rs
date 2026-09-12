//! A [`VariableResolver`] that hands the interpreter a *borrowed* string, and
//! an expression whose result is that very borrow: evaluating it copies no
//! bytes.
//!
//! `CelString::from(&str)` borrows, and the interpreter passes borrowed values
//! through untouched wherever it can (variables, conditionals, `string(x)`,
//! `optional.value()`, field and index access on borrowed containers, ...).
//! The result of [`Value::resolve_val`] is a [`CowVal`] bounded by the
//! resolver's lifetime, so the borrow cannot outlive the data.
use cel::common::types::CelString;
use cel::common::value::CowVal;
use cel::context::VariableResolver;
use cel::parser::Parser;
use cel::{Context, Value};

/// Something that owns the request data for the duration of a request.
struct Request<'a> {
    path: &'a str,
}

impl VariableResolver for Request<'_> {
    fn resolve_val<'b>(&'b self, variable: &str) -> Option<CowVal<'b, 'b>> {
        match variable {
            // no copy: the `CelString` borrows `self.path`
            "path" => Some(CowVal::owned(CelString::from(self.path))),
            _ => None,
        }
    }
}

fn main() {
    let parser = Parser::default();
    let expr = parser
        .parse("path.startsWith('/api') ? path : '/'")
        .unwrap();

    let raw = String::from("/api/v1/things");
    let request = Request { path: raw.as_str() };

    let mut ctx = Context::default();
    ctx.set_variable_resolver(&request);

    // `result` is bounded by the borrows of `expr` and `ctx`: it cannot escape them.
    let result = Value::resolve_val(&expr, &ctx).unwrap();
    let path = result.downcast_ref::<CelString>().unwrap();
    assert!(
        std::ptr::eq(path.inner(), raw.as_str()),
        "the result must borrow the request's bytes"
    );
    println!("path = {} (same bytes as the request: yes)", path.inner());

    // To keep a value beyond the evaluation, copy it out: either as a `Value`
    // ...
    let owned: Value = result.as_ref().try_into().unwrap();
    // ... or as a `CelString` that owns its bytes.
    let owned_string = path.clone().into_static();
    drop(result);
    drop(ctx);
    drop(raw);
    println!("copied out: {owned:?} / {}", owned_string.inner());
}
