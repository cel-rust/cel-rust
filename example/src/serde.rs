use cel::{Context, Program};
use serde::Serialize;

// An example struct that derives Serialize
#[derive(Serialize)]
struct MyStruct {
    a: i32,
    b: i32,
}

fn main() {
    let program = Program::compile("foo.a == foo.b").unwrap();
    let mut context = Context::default();

    // MyStruct will be implicitly serialized into the CEL appropriate types
    context
        .add_variable("foo", MyStruct { a: 1, b: 1 })
        .unwrap();

    let value = program.execute(&context).unwrap();
    assert_eq!(value, true.into());
}
