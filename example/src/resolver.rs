use cel::common::types::CelString;
use cel::common::value::{BorrowedVal, Val};
use cel::context::VariableResolver;
use cel::parser::Parser;
use cel::{Context, Value};
use std::borrow::Cow;

/// Whatever the variable resolved, we always return a `CelString` pointing to a single `str`
struct MonotonicResolver<'a> {
    some_ref: &'a str,
    val: BorrowedVal<'a, CelString>,
}

impl<'a> VariableResolver for MonotonicResolver<'a> {
    fn resolve(&self, _: &str) -> Option<Value> {
        unreachable!("The interpreter should not call into this!")
    }

    fn resolve_val<'b>(&'b self, _variable: &str) -> Option<Cow<'b, dyn Val>> {
        Some(Cow::Borrowed(self.val.inner()))
    }
}

fn main() {
    let parser = Parser::default();

    // try replacing `==` with `!=`
    let expr = "foo == 'bar' ? bar : 'bar'";

    let ast = parser.parse(expr).unwrap();
    let mut context = Context::default();
    let bar = String::from("bar");
    let resolver = MonotonicResolver {
        some_ref: bar.as_str(),
        val: BorrowedVal::from(bar.as_str()),
    };
    context.set_variable_resolver(&resolver);
    let value = Value::resolve_val(&ast, &context).unwrap();

    // This should always pass
    assert_eq!(
        value.as_ref().downcast_ref::<CelString>(),
        Some(&CelString::from("bar"))
    );

    // But with when `foo != 'bar'`, we return a new CelString `"bar"`, the one borrowed from the AST
    assert!(std::ptr::eq(
        resolver.some_ref,
        value.as_ref().downcast_ref::<CelString>().unwrap().inner()
    ))
}
