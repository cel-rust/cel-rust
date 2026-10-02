use cel::{Context, Env};
use serde::Serialize;
use std::sync::Arc;

// An example struct that derives Serialize
#[derive(Serialize)]
struct MyStruct {
    a: i32,
    b: i32,
}

fn main() {
    let env = Arc::new(Env::stdlib());
    let program = env.compile("foo.a == foo.b").unwrap();
    let mut context = Context::with_env(env);

    // MyStruct will be implicitly serialized into the CEL appropriate types
    context
        .add_variable("foo", MyStruct { a: 1, b: 1 })
        .unwrap();

    let value = program.execute(&context).unwrap();
    assert_eq!(value, true.into());
}
